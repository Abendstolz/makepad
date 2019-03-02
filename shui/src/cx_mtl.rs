use std::mem;

use cocoa::base::id as cocoa_id;
use cocoa::foundation::{NSAutoreleasePool,NSUInteger, NSRange};
use cocoa::appkit::{NSWindow, NSView};
use core_graphics::geometry::CGSize;
use objc::runtime::YES;
use objc::{msg_send, sel, sel_impl};
use metal::*;
use winit::os::macos::WindowExt;
use time::precise_time_ns;
use crate::cx::*;

impl Cx{

    pub fn exec_draw_list(&mut self, draw_list_id: usize, device:&Device, encoder:&RenderCommandEncoderRef){
        
        // update draw list uniforms
        {
            let draw_list = &mut self.draw_lists[draw_list_id];
            draw_list.resources.uni_dl.update_with_f32_data(device, &draw_list.uniforms);
        }
        // tad ugly otherwise the borrow checker locks 'self' and we can't recur
        let draw_calls_len = self.draw_lists[draw_list_id].draw_calls_len;
        for draw_call_id in 0..draw_calls_len{
            let sub_list_id = self.draw_lists[draw_list_id].draw_calls[draw_call_id].sub_list_id;
            if sub_list_id != 0{
                self.exec_draw_list(sub_list_id, device, encoder);
            }
            else{
                let draw_list = &mut self.draw_lists[draw_list_id];
                let draw = &mut draw_list.draw_calls[draw_call_id];

                let sh = &self.shaders[draw.shader_id];
                let shc = &self.compiled_shaders[draw.shader_id];
                
                if draw.update_frame_id == self.frame_id{
                    // update the instance buffer data
                    draw.resources.inst_vbuf.update_with_f32_data(device, &draw.instance);
                    draw.resources.uni_dr.update_with_f32_data(device, &draw.uniforms);
                }

                let instances = (draw.instance.len() / shc.instance_slots
                ) as u64;
                if let Some(pipeline_state) = &shc.pipeline_state{
                    encoder.set_render_pipeline_state(pipeline_state);
                    if let Some(buf) = &shc.geom_vbuf.buffer{encoder.set_vertex_buffer(0, Some(&buf), 0);}
                    else{println!("Drawing error: geom_vbuf None")}
                    if let Some(buf) = &draw.resources.inst_vbuf.buffer{encoder.set_vertex_buffer(1, Some(&buf), 0);}
                    else{println!("Drawing error: inst_vbuf None")}
                    if let Some(buf) = &self.resources.uni_cx.buffer{encoder.set_vertex_buffer(2, Some(&buf), 0);}
                    else{println!("Drawing error: uni_cx None")}
                    if let Some(buf) = &draw_list.resources.uni_dl.buffer{encoder.set_vertex_buffer(3, Some(&buf), 0);}
                    else{println!("Drawing error: uni_dl None")}
                    if let Some(buf) = &draw.resources.uni_dr.buffer{encoder.set_vertex_buffer(4, Some(&buf), 0);}
                    else{println!("Drawing error: uni_dr None")}

                    if let Some(buf) = &self.resources.uni_cx.buffer{encoder.set_fragment_buffer(0, Some(&buf), 0);}
                    else{println!("Drawing error: uni_cx None")}
                    if let Some(buf) = &draw_list.resources.uni_dl.buffer{encoder.set_fragment_buffer(1, Some(&buf), 0);}
                    else{println!("Drawing error: uni_dl None")}
                    if let Some(buf) = &draw.resources.uni_dr.buffer{encoder.set_fragment_buffer(2, Some(&buf), 0);}
                    else{println!("Drawing error: uni_dr None")}

                    // lets set our textures
                    for (i, texture_id) in draw.textures_2d.iter().enumerate(){
                        let tex = &mut self.textures_2d[*texture_id as usize];
                        if tex.dirty{
                            tex.upload_to_device(device);
                        }
                        if let Some(mtltex) = &tex.mtltexture{
                            encoder.set_fragment_texture(i as NSUInteger, Some(&mtltex));
                            encoder.set_vertex_texture(i as NSUInteger, Some(&mtltex));
                        }
                    }

                    if let Some(buf) = &shc.geom_ibuf.buffer{
                        encoder.draw_indexed_primitives_instanced(
                            MTLPrimitiveType::Triangle,
                            sh.geometry_indices.len() as u64, // Index Count
                            MTLIndexType::UInt32, // indexType,
                            &buf, // index buffer
                            0, // index buffer offset
                            instances, // instance count
                        )
                   }
                    else{println!("Drawing error: geom_ibuf None")}
                }
            }
        }
    }
 
    pub fn repaint(&mut self,layer:&CoreAnimationLayer, device:&Device, command_queue:&CommandQueue){
        let pool = unsafe { NSAutoreleasePool::new(cocoa::base::nil) };

        if let Some(drawable) = layer.next_drawable() {
            self.prepare_frame();
            
            let render_pass_descriptor = RenderPassDescriptor::new();

            let color_attachment = render_pass_descriptor.color_attachments().object_at(0).unwrap();
            color_attachment.set_texture(Some(drawable.texture()));
            color_attachment.set_load_action(MTLLoadAction::Clear);
            color_attachment.set_clear_color(MTLClearColor::new(
                self.clear_color.x as f64, self.clear_color.y as f64, self.clear_color.z as f64, self.clear_color.w as f64
            ));
            color_attachment.set_store_action(MTLStoreAction::Store);

            let command_buffer = command_queue.new_command_buffer();

            render_pass_descriptor.color_attachments().object_at(0).unwrap().set_load_action(MTLLoadAction::Clear);

            let parallel_encoder = command_buffer.new_parallel_render_command_encoder(&render_pass_descriptor);
            let encoder = parallel_encoder.render_command_encoder();

            self.resources.uni_cx.update_with_f32_data(&device, &self.uniforms);

            // ok now we should call our render thing
            self.exec_draw_list(0, &device, encoder);

            encoder.end_encoding();
            parallel_encoder.end_encoding();

            command_buffer.present_drawable(&drawable);
            command_buffer.commit();
        
            unsafe { 
                msg_send![pool, release];
                //pool = NSAutoreleasePool::new(cocoa::base::nil);
            }
        }
    }

    fn resize_layer_to_turtle(&mut self, layer:&CoreAnimationLayer){
        // resize drawable
        layer.set_drawable_size(CGSize::new(
            (self.target_size.x * self.target_dpi_factor) as f64,
             (self.target_size.y * self.target_dpi_factor) as f64));
    }

    pub fn event_loop<F>(&mut self, mut event_handler:F)
    where F: FnMut(&mut Cx, Event),
    { 

        let mut events_loop = winit::EventsLoop::new();
        let glutin_window = winit::WindowBuilder::new()
            .with_dimensions((800, 600).into())
            .with_title(format!("Metal - {}",self.title))
            .build(&events_loop).unwrap();

        let window: cocoa_id = unsafe { mem::transmute(glutin_window.get_nswindow()) };
        let device = Device::system_default();

        let layer = CoreAnimationLayer::new();
        layer.set_device(&device);
        layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        layer.set_presents_with_transaction(false);

        unsafe {
            let view = window.contentView();
            view.setWantsBestResolutionOpenGLSurface_(YES);
            view.setWantsLayer(YES);
            view.setLayer(mem::transmute(layer.as_ref()));
        }

        let draw_size = glutin_window.get_inner_size().unwrap();
        layer.set_drawable_size(CGSize::new(draw_size.width as f64, draw_size.height as f64));

        let command_queue = device.new_command_queue();

        glutin_window.set_position(winit::dpi::LogicalPosition::new(1920.0,400.0));
        
        self.compile_all_mtl_shaders(&device);

        self.load_binary_deps_from_file();

        let start_time = precise_time_ns();
        
        event_handler(self, Event::AppInit); 

        while self.running{
            // unfortunate duplication of code between poll and run_forever but i don't know how to put this in a closure
            // without borrowchecker hell
            events_loop.poll_events(|winit_event|{
                let event = self.map_winit_event(winit_event, &glutin_window);
                if let Event::Resized(_) = &event{ // do this here because mac
                    self.resize_layer_to_turtle(&layer);
                    event_handler(self, event); 
                    self.dirty_area = Area::Empty;
                    self.redraw_area = Area::All;
                    event_handler(self, Event::Redraw);
                    self.repaint(&layer, &device, &command_queue);
                }
                else{
                    event_handler(self, event); 
                }
            });
            if self.animations.len() != 0{
                let time_now = precise_time_ns();
                let time = (time_now - start_time) as f64 / 1_000_000_000.0; // keeps the error as low as possible
                event_handler(self, Event::Animate(AnimateEvent{time:time}));
                self.check_ended_animations(time);
                if self.ended_animations.len() > 0{
                    event_handler(self, Event::AnimationEnded(AnimateEvent{time:time}));
                }
            }
            // call redraw event
            if !self.dirty_area.is_empty(){
                self.dirty_area = Area::Empty;
                self.redraw_area = self.dirty_area.clone();
                self.frame_id += 1;
                event_handler(self, Event::Redraw);
                self.paint_dirty = true;
            }
            // repaint everything if we need to
            if self.paint_dirty{
                self.paint_dirty = false;
                self.repaint(&layer, &device, &command_queue);
            }
            
            // wait for the next event blockingly so it stops eating power
            if self.animations.len() == 0 && self.dirty_area.is_empty(){
                events_loop.run_forever(|winit_event|{
                    let event = self.map_winit_event(winit_event, &glutin_window);
                    if let Event::Resized(_) = &event{ // do this here because mac
                        self.resize_layer_to_turtle(&layer);
                        event_handler(self, event); 
                        self.dirty_area = Area::Empty;
                        self.redraw_area = Area::All;
                        event_handler(self, Event::Redraw);
                        self.repaint(&layer, &device, &command_queue);
                    }
                    else{
                        event_handler(self, event);
                    }
                    winit::ControlFlow::Break
                })
            }
        }
    }

       pub fn compile_all_mtl_shaders(&mut self, device:&Device){
        for sh in &self.shaders{
            let mtlsh = Self::compile_mtl_shader(&sh, device);
            if let Ok(mtlsh) = mtlsh{
                self.compiled_shaders.push(CompiledShader{
                    shader_id:self.compiled_shaders.len(),
                    ..mtlsh
                });
            }
            else if let Err(err) = mtlsh{
                println!("GOT ERROR: {}", err.msg);
                self.compiled_shaders.push(
                    CompiledShader{..Default::default()}
                )
            }
        };
    }
    pub fn type_to_packed_metal(ty:&str)->String{
        match ty.as_ref(){
            "float"=>"float".to_string(),
            "vec2"=>"packed_float2".to_string(),
            "vec3"=>"packed_float3".to_string(),
            "vec4"=>"packed_float4".to_string(),
            "mat2"=>"packed_float2x2".to_string(),
            "mat3"=>"packed_float3x3".to_string(),
            "mat4"=>"float4x4".to_string(),
            ty=>ty.to_string()
        }
    }

    pub fn type_to_metal(ty:&str)->String{
        match ty.as_ref(){
            "float"=>"float".to_string(),
            "vec2"=>"float2".to_string(),
            "vec3"=>"float3".to_string(),
            "vec4"=>"float4".to_string(),
            "mat2"=>"float2x2".to_string(),
            "mat3"=>"float3x3".to_string(),
            "mat4"=>"float4x4".to_string(),
            "texture2d"=>"texture2d<float>".to_string(),
            ty=>ty.to_string()
        }
    }

    pub fn assemble_struct(name:&str, vars:&Vec<ShVar>, packed:bool, field:&str)->String{
        let mut out = String::new();
        out.push_str("struct ");
        out.push_str(name);
        out.push_str("{\n");
        out.push_str(field);
        for var in vars{
            out.push_str("  ");
            out.push_str(
                &if packed{
                    Self::type_to_packed_metal(&var.ty)
                }
                else{
                    Self::type_to_metal(&var.ty)
                }
            );
            out.push_str(" ");
            out.push_str(&var.name);
            out.push_str(";\n")
        };
        out.push_str("};\n\n");
        out
    }

    pub fn assemble_texture_slots(textures:&Vec<ShVar>)->String{
        let mut out = String::new();
        out.push_str("struct ");
        out.push_str("_Tex{\n");
        for (i, tex) in textures.iter().enumerate(){
            out.push_str("texture2d<float> ");
            out.push_str(&tex.name);
            out.push_str(&format!(" [[texture({})]];\n", i));
        };
        out.push_str("};\n\n");
        out
    }

    pub fn assemble_shader(sh:&Shader)->Result<AssembledMtlShader, SlErr>{
        
        let mut mtl_out = "#include <metal_stdlib>\nusing namespace metal;\n".to_string();

        // ok now define samplers from our sh. 
        let texture_slots = sh.flat_vars(ShVarStore::Texture);
        let geometries = sh.flat_vars(ShVarStore::Geometry);
        let instances = sh.flat_vars(ShVarStore::Instance);
        let mut varyings = sh.flat_vars(ShVarStore::Varying);
        let locals = sh.flat_vars(ShVarStore::Local);
        let uniforms_cx = sh.flat_vars(ShVarStore::UniformCx); 
        let uniforms_dl = sh.flat_vars(ShVarStore::UniformDl);
        let uniforms_dr = sh.flat_vars(ShVarStore::Uniform);

        // lets count the slots
        //let geometry_slots = sh.compute_slot_total(&geometries);
        let instance_slots = sh.compute_slot_total(&instances);
        //let varying_slots = sh.compute_slot_total(&varyings);

        mtl_out.push_str(&Self::assemble_struct("_Geom", &geometries, true, ""));
        mtl_out.push_str(&Self::assemble_struct("_Inst", &instances, true, ""));
        mtl_out.push_str(&Self::assemble_struct("_UniCx", &uniforms_cx, true, ""));
        mtl_out.push_str(&Self::assemble_struct("_UniDl", &uniforms_dl, true, ""));
        mtl_out.push_str(&Self::assemble_struct("_UniDr", &uniforms_dr, true, ""));
        mtl_out.push_str(&Self::assemble_struct("_Loc", &locals, false, ""));

        // we need to figure out which texture slots exist 
        mtl_out.push_str(&Self::assemble_texture_slots(&texture_slots));

        // we need to figure out which texture slots exist 
       // mtl_out.push_str(&Self::assemble_constants(&texture_slots));



        let mut vtx_cx = SlCx{
            depth:0,
            target:SlTarget::Vertex,
            defargs_fn:"_Tex _tex, thread _Loc &_loc, thread _Vary &_vary, thread _Geom &_geom, thread _Inst &_inst, device _UniCx &_uni_cx, device _UniDl &_uni_dl, device _UniDr &_uni_dr".to_string(),
            defargs_call:"_tex, _loc, _vary, _geom, _inst, _uni_cx, _uni_dl, _uni_dr".to_string(),
            call_prefix:"_".to_string(),
            shader:sh,
            scope:Vec::new(),
            fn_deps:vec!["vertex".to_string()],
            fn_done:Vec::new(),
            auto_vary:Vec::new()
        };
        let vtx_fns = assemble_fn_and_deps(sh, &mut vtx_cx)?;
        let mut pix_cx = SlCx{
            depth:0,
            target:SlTarget::Pixel,
            defargs_fn:"_Tex _tex, thread _Loc &_loc, thread _Vary &_vary, device _UniCx &_uni_cx, device _UniDl &_uni_dl, device _UniDr &_uni_dr".to_string(),
            defargs_call:"_tex, _loc, _vary, _uni_cx, _uni_dl, _uni_dr".to_string(),
            call_prefix:"_".to_string(),
            shader:sh,
            scope:Vec::new(),
            fn_deps:vec!["pixel".to_string()],
            fn_done:vtx_cx.fn_done,
            auto_vary:Vec::new()
        };        

        let pix_fns = assemble_fn_and_deps(sh, &mut pix_cx)?;

        // lets add the auto_vary ones to the varyings struct
        for auto in &pix_cx.auto_vary{
            varyings.push(auto.clone());
        }
        mtl_out.push_str(&Self::assemble_struct("_Vary", &varyings, false, "  float4 mtl_position [[position]];\n"));

        mtl_out.push_str("//Vertex shader\n");
        mtl_out.push_str(&vtx_fns);
        mtl_out.push_str("//Pixel shader\n");
        mtl_out.push_str(&pix_fns);

        // lets define the vertex shader
        mtl_out.push_str("vertex _Vary _vertex_shader(_Tex _tex, device _Geom *in_geometries [[buffer(0)]], device _Inst *in_instances [[buffer(1)]],\n");
        mtl_out.push_str("  device _UniCx &_uni_cx [[buffer(2)]], device _UniDl &_uni_dl [[buffer(3)]], device _UniDr &_uni_dr [[buffer(4)]],\n");
        mtl_out.push_str("  uint vtx_id [[vertex_id]], uint inst_id [[instance_id]]){\n");
        mtl_out.push_str("  _Loc _loc;\n");
        mtl_out.push_str("  _Vary _vary;\n");
        mtl_out.push_str("  _Geom _geom = in_geometries[vtx_id];\n");
        mtl_out.push_str("  _Inst _inst = in_instances[inst_id];\n");
        mtl_out.push_str("  _vary.mtl_position = _vertex(");
        mtl_out.push_str(&vtx_cx.defargs_call);
        mtl_out.push_str(");\n\n");

        for auto in pix_cx.auto_vary{
            if let ShVarStore::Geometry = auto.store{
              mtl_out.push_str("       _vary.");
              mtl_out.push_str(&auto.name);
              mtl_out.push_str(" = _geom.");
              mtl_out.push_str(&auto.name);
              mtl_out.push_str(";\n");
            }
            else if let ShVarStore::Instance = auto.store{
              mtl_out.push_str("       _vary.");
              mtl_out.push_str(&auto.name);
              mtl_out.push_str(" = _inst.");
              mtl_out.push_str(&auto.name);
              mtl_out.push_str(";\n");
            }
        }

        mtl_out.push_str("       return _vary;\n");
        mtl_out.push_str("};\n");
        // then the fragment shader
        mtl_out.push_str("fragment float4 _fragment_shader(_Vary _vary[[stage_in]],_Tex _tex,\n");
        mtl_out.push_str("  device _UniCx &_uni_cx [[buffer(0)]], device _UniDl &_uni_dl [[buffer(1)]], device _UniDr &_uni_dr [[buffer(2)]]){\n");
        mtl_out.push_str("  _Loc _loc;\n");
        mtl_out.push_str("  return _pixel(");
        mtl_out.push_str(&pix_cx.defargs_call);
        mtl_out.push_str(");\n};\n");

        if sh.log != 0{
            println!("---- Metal shader -----\n{}",mtl_out);
        }

         Ok(AssembledMtlShader{
            instance_slots:instance_slots,
            uniforms_dr:uniforms_dr,
            uniforms_dl:uniforms_dl,
            uniforms_cx:uniforms_cx,
            texture_slots:texture_slots,
            rect_instance_props:RectInstanceProps::construct(sh, &instances),
            named_instance_props:NamedInstanceProps::construct(sh, &instances),
            mtlsl:mtl_out
        })
    }

    pub fn compile_mtl_shader(sh:&Shader, device: &Device)->Result<CompiledShader, SlErr>{
        let ash = Self::assemble_shader(sh)?;

        let options = CompileOptions::new();
        let library = device.new_library_with_source(&ash.mtlsl, &options);

        match library{
            Err(library)=>Err(SlErr{msg:library}),
            Ok(library)=>Ok(CompiledShader{
                shader_id:0,
                pipeline_state:{
                    let vert = library.get_function("_vertex_shader", None).unwrap();
                    let frag = library.get_function("_fragment_shader", None).unwrap();
                    let rpd = RenderPipelineDescriptor::new();
                    rpd.set_vertex_function(Some(&vert));
                    rpd.set_fragment_function(Some(&frag));
                    let color = rpd.color_attachments().object_at(0).unwrap();
                    color.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
                    color.set_blending_enabled(true);
                    color.set_source_rgb_blend_factor(MTLBlendFactor::One);
                    color.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
                    color.set_source_alpha_blend_factor(MTLBlendFactor::One);
                    color.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
                    color.set_rgb_blend_operation(MTLBlendOperation::Add);
                    color.set_alpha_blend_operation(MTLBlendOperation::Add);
                    Some(device.new_render_pipeline_state(&rpd).unwrap())
                },
                library:Some(library),
                instance_slots:ash.instance_slots,
                named_instance_props:ash.named_instance_props.clone(),
                rect_instance_props:ash.rect_instance_props.clone(),
                //assembled_shader:ash,
                geom_ibuf:{
                    let mut geom_ibuf = MetalBuffer{..Default::default()};
                    geom_ibuf.update_with_u32_data(device, &sh.geometry_indices);
                    geom_ibuf
                },
                geom_vbuf:{
                    let mut geom_vbuf = MetalBuffer{..Default::default()};
                    geom_vbuf.update_with_f32_data(device, &sh.geometry_vertices);
                    geom_vbuf
                }
            })
        }
    }
}

#[derive(Clone, Default)]
pub struct CxResources{
    pub uni_cx:MetalBuffer,
    pub winit:CxWinit
}

#[derive(Clone, Default)]
pub struct DrawListResources{
     pub uni_dl:MetalBuffer
}

#[derive(Default,Clone)]
pub struct DrawCallResources{
    pub uni_dr:MetalBuffer,
    pub inst_vbuf:MetalBuffer
}



impl<'a> SlCx<'a>{
    pub fn map_call(&self, name:&str, args:&Vec<Sl>)->MapCallResult{
        match name{
            "sample2d"=>{ // transform call to
                let base = &args[0];
                let coord = &args[1];
                return MapCallResult::Rewrite(
                    format!("{}.sample(sampler(mag_filter::linear,min_filter::linear),{})", base.sl, coord.sl),
                    "vec4".to_string()
                )
            },
            _=>return MapCallResult::None
        }
    }    
    pub fn map_type(&self, ty:&str)->String{
        Cx::type_to_metal(ty)
    }

    pub fn map_var(&mut self, var:&ShVar)->String{
        let mty = Cx::type_to_metal(&var.ty);
        match var.store{
            ShVarStore::Uniform=>return format!("{}(_uni_dr.{})", mty, var.name),
            ShVarStore::UniformDl=>return format!("{}(_uni_dl.{})", mty, var.name),
            ShVarStore::UniformCx=>return format!("{}(_uni_cx.{})", mty, var.name),
            ShVarStore::Instance=>{
                if let SlTarget::Pixel = self.target{
                    if self.auto_vary.iter().find(|v|v.name == var.name).is_none(){
                        self.auto_vary.push(var.clone());
                    }
                    return format!("_vary.{}",var.name);
                }
                else{
                    return format!("{}(_inst.{})", mty, var.name);
                }
            },
            ShVarStore::Geometry=>{
                if let SlTarget::Pixel = self.target{
                    if self.auto_vary.iter().find(|v|v.name == var.name).is_none(){
                        self.auto_vary.push(var.clone());
                    }
                    return format!("_vary.{}",var.name);
                }
                else{
                    
                    return format!("{}(_geom.{})", mty, var.name);
                }
            },
            ShVarStore::Texture=>return format!("_tex.{}",var.name),
            ShVarStore::Local=>return format!("_loc.{}",var.name),
            ShVarStore::Varying=>return format!("_vary.{}",var.name),
        }
    }
}

#[derive(Default,Clone)]
pub struct AssembledMtlShader{
    pub instance_slots:usize,
    pub uniforms_dr: Vec<ShVar>,
    pub uniforms_dl: Vec<ShVar>,
    pub uniforms_cx: Vec<ShVar>,
    pub texture_slots:Vec<ShVar>,
    pub rect_instance_props: RectInstanceProps,
    pub named_instance_props: NamedInstanceProps,
    pub mtlsl:String,
}

#[derive(Default,Clone)]
pub struct MetalBuffer{
    pub buffer:Option<metal::Buffer>,
    pub size:usize,
    pub used:usize
}

impl MetalBuffer{
    pub fn update_with_f32_data(&mut self, device:&Device, data:&Vec<f32>){
        if self.size < data.len(){
            self.buffer = None;
        }
        if let None = &self.buffer{
            self.buffer = Some(
                device.new_buffer(
                    (data.len() * std::mem::size_of::<f32>()) as u64,
                    MTLResourceOptions::CPUCacheModeDefaultCache
                )
            );
            self.size = data.len()
        }
        if let Some(buffer) = &self.buffer{
            let p = buffer.contents(); 
            unsafe {
                std::ptr::copy(data.as_ptr(), p as *mut f32, data.len());
            }
            buffer.did_modify_range(NSRange::new(0 as u64, (data.len() * std::mem::size_of::<f32>()) as u64));
        }
        self.used = data.len()
    }

    pub fn update_with_u32_data(&mut self, device:&Device, data:&Vec<u32>){
        if self.size < data.len(){
            self.buffer = None;
        }
        if let None = &self.buffer{
            self.buffer = Some(
                device.new_buffer(
                    (data.len() * std::mem::size_of::<u32>()) as u64,
                    MTLResourceOptions::CPUCacheModeDefaultCache
                )
            );
            self.size = data.len()
        }
        if let Some(buffer) = &self.buffer{
            let p = buffer.contents(); 
            unsafe {
                std::ptr::copy(data.as_ptr(), p as *mut u32, data.len());
            }
            buffer.did_modify_range(NSRange::new(0 as u64, (data.len() * std::mem::size_of::<u32>()) as u64));
        }
        self.used = data.len()
    }
}

#[derive(Default,Clone)]
pub struct CompiledShader{
    pub library:Option<metal::Library>,
    pub pipeline_state:Option<metal::RenderPipelineState>,
    pub shader_id: usize,
    pub geom_vbuf:MetalBuffer,
    pub geom_ibuf:MetalBuffer,
    pub instance_slots:usize,
    pub rect_instance_props: RectInstanceProps,
    pub named_instance_props: NamedInstanceProps,
}


#[derive(Default,Clone)]
pub struct Texture2D{
    pub texture_id: usize,
    pub dirty:bool,
    pub image: Vec<u32>,
    pub width: usize,
    pub height:usize,
    pub mtltexture: Option<metal::Texture>
}

impl Texture2D{
    pub fn resize(&mut self, width:usize, height:usize){
        self.width = width;
        self.height = height;
        self.image.resize((width * height) as usize, 0);
        self.dirty = true;
    }

    pub fn upload_to_device(&mut self, device:&Device){
        let desc = TextureDescriptor::new();
        desc.set_texture_type(MTLTextureType::D2);
        desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        desc.set_width(self.width as u64);
        desc.set_height(self.height as u64);
        desc.set_storage_mode(MTLStorageMode::Managed);
        //desc.set_mipmap_level_count(1);
        //desc.set_depth(1);
        //desc.set_sample_count(4);
        let tex = device.new_texture(&desc);
    
        let region = MTLRegion{
            origin:MTLOrigin{x:0,y:0,z:0},
            size:MTLSize{width:self.width as u64, height:self.height as u64, depth:1}
        };
        tex.replace_region(region, 0, (self.width * mem::size_of::<u32>()) as u64, self.image.as_ptr() as *const std::ffi::c_void);

        //image_buf.did_modify_range(NSRange::new(0 as u64, (self.image.len() * mem::size_of::<u32>()) as u64));

        self.mtltexture = Some(tex);
        self.dirty = false;
      
    }
}