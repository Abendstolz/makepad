
use crate::math::*;
use crate::cx_shared::*;
use crate::cxdrawing::*;
use crate::cxshaders_shared::*;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct InstanceArea{
    pub draw_list_id:usize,
    pub draw_call_id:usize,
    pub instance_offset:usize,
    pub instance_count:usize,
    pub instance_writer:usize
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct DrawListArea{
    pub draw_list_id:usize
}

#[derive(Clone, Debug, PartialEq)]
pub enum Area{
    Empty,
    All,
    Instance(InstanceArea),
    DrawList(DrawListArea)
}

impl Default for Area{
    fn default()->Area{
        Area::Empty
    }
}

impl Area{
    pub fn is_empty(&self)->bool{
        if let Area::Empty = self{
            return true
        }
        false
    }

    pub fn get_rect(&self, cd:&CxDrawing)->Rect{
        return match self{
            Area::Instance(inst)=>{
                if inst.instance_count == 0{
                    return Rect::zero()
                }
                let draw_list = &cd.draw_lists[inst.draw_list_id];
                let draw_call = &draw_list.draw_calls[inst.draw_call_id];
                let csh = &cd.compiled_shaders[draw_call.shader_id];
                // ok now we have to patch x/y/w/h into it
                if let Some(ix) = csh.rect_instance_props.x{
                    let x = draw_call.instance[inst.instance_offset + ix];
                    if let Some(iy) = csh.rect_instance_props.y{
                        let y = draw_call.instance[inst.instance_offset + iy];
                        if let Some(iw) = csh.rect_instance_props.w{
                            let w = draw_call.instance[inst.instance_offset + iw];
                            if let Some(ih) = csh.rect_instance_props.h{
                                let h = draw_call.instance[inst.instance_offset + ih];
                                return Rect{x:x,y:y,w:w,h:h}
                            }
                        }
                    }
                }
                Rect::zero()
            },
            Area::DrawList(draw_list_area)=>{
                let draw_list = &cd.draw_lists[draw_list_area.draw_list_id];
                draw_list.rect.clone()
            },
            _=>Rect::zero(),
        }
    }

    pub fn set_rect(&self, cd:&mut CxDrawing, rect:&Rect){
         match self{
            Area::Instance(inst)=>{
                let draw_list = &mut cd.draw_lists[inst.draw_list_id];
                let draw_call = &mut draw_list.draw_calls[inst.draw_call_id];
                let csh = &cd.compiled_shaders[draw_call.shader_id];        // ok now we have to patch x/y/w/h into it

                if let Some(ix) = csh.rect_instance_props.x{
                    draw_call.instance[inst.instance_offset + ix] = rect.x;
                }
                if let Some(iy) = csh.rect_instance_props.y{
                    draw_call.instance[inst.instance_offset + iy] = rect.y;
                }
                if let Some(iw) = csh.rect_instance_props.w{
                    draw_call.instance[inst.instance_offset + iw] = rect.w;
                }
                if let Some(ih) = csh.rect_instance_props.h{
                    draw_call.instance[inst.instance_offset + ih] = rect.h;
                }
            },
            Area::DrawList(draw_list_area)=>{
                let draw_list = &mut cd.draw_lists[draw_list_area.draw_list_id];
                draw_list.rect = rect.clone()
            },
            _=>()
         }
    }

    pub fn move_xy(&self, dx:f32, dy:f32, cd:&mut CxDrawing){
        return match self{
            Area::Instance(inst)=>{
                if inst.instance_count == 0{
                    return;
                }
                let draw_list = &mut cd.draw_lists[inst.draw_list_id];
                let draw_call = &mut draw_list.draw_calls[inst.draw_call_id];
                let csh = &cd.compiled_shaders[draw_call.shader_id];

                for i in 0..inst.instance_count{
                    if let Some(x) = csh.rect_instance_props.x{
                        draw_call.instance[inst.instance_offset + x + i * csh.instance_slots] += dx;
                    }
                    if let Some(y) = csh.rect_instance_props.y{
                        draw_call.instance[inst.instance_offset + y+ i * csh.instance_slots] += dy;
                    }
                }
            }
            _=>(),
        }
    }

    pub fn write_float(&self, cd:&mut CxDrawing, prop_name:&str, value:f32){
        match self{
            Area::Instance(inst)=>{
                let draw_list = &mut cd.draw_lists[inst.draw_list_id];
                let draw_call = &mut draw_list.draw_calls[inst.draw_call_id];
                let csh = &cd.compiled_shaders[draw_call.shader_id];

                for prop in &csh.named_instance_props.props{
                    if prop.name == prop_name{
                        cd.paint_dirty = true;
                        draw_call.instance[inst.instance_offset + prop.offset] = value;
                        return
                    }
                }
            },
            _=>(),
        }
    }

    pub fn read_float(&self, cd:&CxDrawing, prop_name:&str)->f32{
        match self{
            Area::Instance(inst)=>{
                let draw_list = &cd.draw_lists[inst.draw_list_id];
                let draw_call = &draw_list.draw_calls[inst.draw_call_id];
                let csh = &cd.compiled_shaders[draw_call.shader_id];

                for prop in &csh.named_instance_props.props{
                    if prop.name == prop_name{
                        return draw_call.instance[inst.instance_offset + prop.offset + 0]
                    }
                }
            }
            _=>(),
        }
        return 0.0;
    }

   pub fn write_vec2(&self, cd:&mut CxDrawing, prop_name:&str, value:Vec2){
        match self{
            Area::Instance(inst)=>{
                let draw_list = &mut cd.draw_lists[inst.draw_list_id];
                let draw_call = &mut draw_list.draw_calls[inst.draw_call_id];
                let csh = &cd.compiled_shaders[draw_call.shader_id];

                for prop in &csh.named_instance_props.props{
                    if prop.name == prop_name{
                        cd.paint_dirty = true;
                        let off = inst.instance_offset + prop.offset;
                        draw_call.instance[off + 0] = value.x;
                        draw_call.instance[off + 1] = value.y;
                        return
                    }
                }
            }
            _=>(),
        }
    }

    pub fn read_vec2(&self, cd:&CxDrawing, prop_name:&str)->Vec2{
        match self{
            Area::Instance(inst)=>{
                let draw_list = &cd.draw_lists[inst.draw_list_id];
                let draw_call = &draw_list.draw_calls[inst.draw_call_id];
                let csh = &cd.compiled_shaders[draw_call.shader_id];

                for prop in &csh.named_instance_props.props{
                    if prop.name == prop_name{
                        let off = inst.instance_offset + prop.offset;
                        return Vec2{
                            x:draw_call.instance[off + 0],
                            y:draw_call.instance[off + 1]
                        }
                    }
                }
            },
            _=>(),
        }
        return vec2(0.0,0.0);
    }

    pub fn write_vec3(&self, cd:&mut CxDrawing, prop_name:&str, value:Vec3){
        match self{
            Area::Instance(inst)=>{
                let draw_list = &mut cd.draw_lists[inst.draw_list_id];
                let draw_call = &mut draw_list.draw_calls[inst.draw_call_id];
                let csh = &cd.compiled_shaders[draw_call.shader_id];

                for prop in &csh.named_instance_props.props{
                    if prop.name == prop_name{
                        cd.paint_dirty = true;
                        let off = inst.instance_offset + prop.offset;
                        draw_call.instance[off + 0] = value.x;
                        draw_call.instance[off + 1] = value.y;
                        draw_call.instance[off + 2] = value.z;
                        return
                    }
                }
            },
            _=>(),
        }
    }

    pub fn read_vec3(&self, cd:&CxDrawing, prop_name:&str)->Vec3{
        match self{
            Area::Instance(inst)=>{
                let draw_list = &cd.draw_lists[inst.draw_list_id];
                let draw_call = &draw_list.draw_calls[inst.draw_call_id];
                let csh = &cd.compiled_shaders[draw_call.shader_id];

                for prop in &csh.named_instance_props.props{
                    if prop.name == prop_name{
                        let off = inst.instance_offset + prop.offset;
                        return Vec3{
                            x:draw_call.instance[off + 0],
                            y:draw_call.instance[off + 1],
                            z:draw_call.instance[off + 2]
                        }
                    }
                }
            }
            _=>(),
        }
        return vec3(0.0,0.0,0.0);
    }

    pub fn write_vec4(&self, cd:&mut CxDrawing, prop_name:&str, value:Vec4){
        match self{
            Area::Instance(inst)=>{
                let draw_list = &mut cd.draw_lists[inst.draw_list_id];
                let draw_call = &mut draw_list.draw_calls[inst.draw_call_id];
                let csh = &cd.compiled_shaders[draw_call.shader_id];

                for prop in &csh.named_instance_props.props{
                    if prop.name == prop_name{
                        cd.paint_dirty = true;
                        let off = inst.instance_offset + prop.offset;
                        draw_call.instance[off + 0] = value.x;
                        draw_call.instance[off + 1] = value.y;
                        draw_call.instance[off + 2] = value.z;
                        draw_call.instance[off + 3] = value.w;
                        return
                    }
                }
            },
            _=>(),
        }
    }

    pub fn read_vec4(&self, cd:&CxDrawing, prop_name:&str)->Vec4{
        match self{
            Area::Instance(inst)=>{
                let draw_list = &cd.draw_lists[inst.draw_list_id];
                let draw_call = &draw_list.draw_calls[inst.draw_call_id];
                let csh = &cd.compiled_shaders[draw_call.shader_id];

                for prop in &csh.named_instance_props.props{
                    if prop.name == prop_name{
                        let off = inst.instance_offset + prop.offset;
                        return Vec4{
                            x:draw_call.instance[off + 0],
                            y:draw_call.instance[off + 1],
                            z:draw_call.instance[off + 2],
                            w:draw_call.instance[off + 3]
                        }
                    }
                }
            },
            _=>(),
        }
        return vec4(0.0,0.0,0.0,0.0);
    }

    pub fn append_data(&self, cd:&mut CxDrawing, data:&[f32]){
        match self{
            Area::Instance(inst)=>{
                let draw_list = &mut cd.draw_lists[inst.draw_list_id];
                let draw_call = &mut draw_list.draw_calls[inst.draw_call_id];
                //let csh = &cx.shaders.compiled_shaders[draw_call.shader_id];
                draw_call.instance.extend_from_slice(data);
            },
            _=>(),
        }
    }
 
    pub fn need_uniforms_now(&self, cd:&mut CxDrawing)->bool{
        match self{
            Area::Instance(inst)=>{
                let draw_list = &mut cd.draw_lists[inst.draw_list_id];
                let draw_call = &mut draw_list.draw_calls[inst.draw_call_id];
                //let csh = &cx.shaders.compiled_shaders[draw_call.shader_id];
                return draw_call.need_uniforms_now
            },
            _=>(),
        }
        return false
    }

   pub fn uniform_texture(&self, cd:&mut CxDrawing, _name: &str, texture_id: usize){
        match self{
            Area::Instance(inst)=>{
                let draw_list = &mut cd.draw_lists[inst.draw_list_id];
                let draw_call = &mut draw_list.draw_calls[inst.draw_call_id]; 
                draw_call.textures.push(texture_id as u32);
            },
            _=>()
        }
    }

    pub fn uniform_float(&self, cd:&mut CxDrawing, _name: &str, v:f32){
        match self{
            Area::Instance(inst)=>{
                let draw_list = &mut cd.draw_lists[inst.draw_list_id];
                let draw_call = &mut draw_list.draw_calls[inst.draw_call_id]; 
                draw_call.uniforms.push(v);
            },
            _=>()
         }
    }

    pub fn uniform_vec2f(&self, cd:&mut CxDrawing, _name: &str, x:f32, y:f32){
        match self{
            Area::Instance(inst)=>{
                let draw_list = &mut cd.draw_lists[inst.draw_list_id];
                let draw_call = &mut draw_list.draw_calls[inst.draw_call_id]; 
                draw_call.uniforms.push(x);
                draw_call.uniforms.push(y);
            },
            _=>()
         }
    }

    pub fn uniform_vec3f(&mut self, cd:&mut CxDrawing, _name: &str, x:f32, y:f32, z:f32){
        match self{
            Area::Instance(inst)=>{
                let draw_list = &mut cd.draw_lists[inst.draw_list_id];
                let draw_call = &mut draw_list.draw_calls[inst.draw_call_id]; 
                draw_call.uniforms.push(x);
                draw_call.uniforms.push(y);
                draw_call.uniforms.push(z);
            },
            _=>()
        }
    }

    pub fn uniform_vec4f(&self, cd:&mut CxDrawing, _name: &str, x:f32, y:f32, z:f32, w:f32){
        match self{
            Area::Instance(inst)=>{
                let draw_list = &mut cd.draw_lists[inst.draw_list_id];
                let draw_call = &mut draw_list.draw_calls[inst.draw_call_id]; 
                draw_call.uniforms.push(x);
                draw_call.uniforms.push(y);
                draw_call.uniforms.push(z);
                draw_call.uniforms.push(w);
            },
            _=>()
        }
    }

    pub fn uniform_mat4(&self, cd:&mut CxDrawing, _name: &str, v:&Mat4){
        match self{
            Area::Instance(inst)=>{
                let draw_list = &mut cd.draw_lists[inst.draw_list_id];
                let draw_call = &mut draw_list.draw_calls[inst.draw_call_id]; 
                for i in 0..16{
                    draw_call.uniforms.push(v.v[i]);
                }
            },
            _=>()
        }
    }

    pub fn contains(&self, x:f32, y:f32, cd:&CxDrawing)->bool{
        let rect = self.get_rect(cd);

        return x >= rect.x && x <= rect.x + rect.w &&
            y >= rect.y && y <= rect.y + rect.h;
    }
}
