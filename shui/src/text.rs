use crate::shader::*;
use crate::cx::*;
use crate::cxdrawing::*;

pub enum Wrapping{
    Char,
    Word,
    Line,
    None
}

pub struct Text{
    pub font_id:usize,
    pub shader_id:usize,
    pub text:String,
    pub color: Vec4,
    pub font_size:f32,
    pub wrapping:Wrapping,
}

impl Style for Text{
    fn style(cx:&mut Cx)->Self{
        let mut sh = Shader::def(); 
        Self::def_shader(&mut sh);
        Self{
            shader_id:cx.shaders.add(sh),
            font_id:cx.fonts.load("fonts/ubuntu_regular_256.font", &mut cx.textures),
            text:"".to_string(),
            font_size:10.0,
            wrapping:Wrapping::Word,
            color:Vec4{x:1.0,y:1.0,z:1.0,w:1.0}
        }
    }
}

impl Text{
    pub fn def_shader(sh: &mut Shader){
        // lets add the draw shader lib
        Cx::def_shader(sh);
        sh.geometry_vertices = vec![
            0.0,0.0,
            1.0,0.0,
            1.0,1.0,
            0.0,1.0
        ];
        sh.geometry_indices = vec![
            0,1,2,
            0,3,2
        ];

        sh.add_ast(shader_ast!(||{
            let geom:vec2<Geometry>;
            let texture:texture2d<Texture>;
            let tex_size:vec2<Uniform>;
            let list_clip:vec4<Uniform>;
            let draw_clip:vec4<Instance>;
            let font_geom:vec4<Instance>;
            let font_tc:vec4<Instance>;
            let color:vec4<Instance>;
            let x:float<Instance>;
            let y:float<Instance>;
            let font_size:float<Instance>;
            let font_base:float<Instance>;
            let tex_coord:vec2<Varying>;

            fn pixel()->vec4{
                let s:vec4 = sample2d(texture, tex_coord.xy);
                let sig_dist:float =  max(min(s.r, s.g), min(max(s.r, s.g), s.b)) - 0.5;

                df_viewport(tex_coord * tex_size * 0.07);
                df_shape = -sig_dist - 0.5 / df_aa;

                return df_fill(color); 
            }
            
            fn vertex()->vec4{
                let shift:vec2 = vec2(0.0,0.0);
                let min_pos:vec2 = vec2(
                    x + font_size * font_geom.x,
                    y - font_size * font_geom.y + font_size * font_base
                );
                let max_pos:vec2 = vec2(
                    x + font_size * font_geom.z,
                    y - font_size * font_geom.w + font_size * font_base
                );
                
                let clipped:vec2 = clamp(
                    mix(min_pos, max_pos, geom) + shift,
                    max(draw_clip.xy, list_clip.xy),
                    min(draw_clip.zw, list_clip.zw)
                );
                let normalized:vec2 = (clipped - min_pos - shift) / (max_pos - min_pos);

                tex_coord = mix(
                    font_tc.xy,
                    font_tc.zw,
                    normalized.xy
                );

                return vec4(clipped,0.,1.) * camera_projection;
            }
        }));
    }

    pub fn draw_text(&mut self, cx:&mut Cx, x:Value, y:Value, text:&str){
        let dr = cx.drawing.instance(cx.shaders.get(self.shader_id));
        let font = cx.fonts.get(self.font_id);

        let mut wx = x.eval_width(&cx.turtle);
        let mut wy = y.eval_height(&cx.turtle);

        if dr.first{
            dr.texture("texture", font.texture_id);
            dr.uvec2f("tex_size", font.width as f32, font.height as f32);
            dr.uvec4f("list_clip", -50000.0,-50000.0,50000.0,50000.0);
        }

        if let Some(turtle) = &mut cx.turtle.turtles.last_mut(){
            if wx.is_nan(){
                wx = turtle.walk_x
            }
            if wy.is_nan(){
                wy = turtle.walk_y
            }
        }
        // lets draw 'str' from char a to z
        for c in text.chars(){
            
            if c >= '\u{10000}'{
                continue
            }
            // lets look up the glyph
            let slot = font.unicodes[c as usize];
            if slot == 0 {
                continue
            }
           
            let glyph = &font.glyphs[slot];

            dr.vec4f("draw_clip", -50000.0,-50000.0,50000.0,50000.0);
            dr.vec4f("font_geom",glyph.x1 ,glyph.y1 ,glyph.x2 ,glyph.y2);
            dr.vec4f("font_tc",glyph.tx1 ,glyph.ty1 ,glyph.tx2 ,glyph.ty2);
            dr.vec4f("color",1.0,1.0,1.0,1.0);
            dr.float("x", wx);
            dr.float("y", wy);
            dr.float("font_size", self.font_size);
            dr.float("font_base", 1.0);
             
            wx += glyph.advance * self.font_size;
        }
     
    }
}