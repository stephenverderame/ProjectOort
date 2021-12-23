#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 2],
    tex_coords: [f32; 2],
}

glium::implement_vertex!(Vertex, pos, tex_coords);

use glium::framebuffer::ToColorAttachment;
use glium::framebuffer::ToDepthAttachment;
use glium::Surface;
use crate::shader;
use crate::draw_traits;

pub struct RenderTarget<'a>{
    fbo: Option<glium::framebuffer::SimpleFrameBuffer<'a>>,
    depth_tex: Box<glium::texture::DepthTexture2dMultisample>,
    tex: Box<glium::texture::Texture2dMultisample>,
    vbo: glium::VertexBuffer<Vertex>,
    ebo: glium::IndexBuffer<u16>,
    height: u32,
    width: u32,
    out_tex: Box<glium::texture::Texture2d>,
    out_fbo: Option<glium::framebuffer::SimpleFrameBuffer<'a>>,
}

impl<'a> RenderTarget<'a> {
    pub fn new<F : glium::backend::Facade>(samples: u32, width: u32, height: u32, facade: &F) -> RenderTarget {
        let depth_tex = glium::texture::DepthTexture2dMultisample::empty(facade, width, height, samples).unwrap();
        let tex = glium::texture::Texture2dMultisample::empty_with_format(facade, glium::texture::UncompressedFloatFormat::F16F16F16F16,
            glium::texture::MipmapsOption::NoMipmap, width, height, samples).unwrap();
        let verts: [Vertex; 4] = [Vertex { pos: [1.0, -1.0], tex_coords: [1., 0.] },
            Vertex { pos: [1.0, 1.0], tex_coords: [1., 1.] },
            Vertex { pos: [-1.0, 1.0], tex_coords: [0., 1.] },
            Vertex { pos: [-1.0, -1.0], tex_coords: [0., 0.] }];
        let indices: [u16; 6] = [2, 3, 0, 2, 0, 1];
        let out_tex = glium::texture::Texture2d::empty_with_format(facade, glium::texture::UncompressedFloatFormat::F16F16F16F16,
            glium::texture::MipmapsOption::NoMipmap, width, height).unwrap();
        RenderTarget {
            fbo: None,
            out_fbo: None,
            depth_tex: Box::new(depth_tex),
            tex: Box::new(tex),
            vbo: glium::VertexBuffer::new(facade, &verts).unwrap(),
            ebo: glium::IndexBuffer::new(facade, glium::index::PrimitiveType::TrianglesList, &indices).unwrap(),
            width: width,
            height: height,
            out_tex: Box::new(out_tex),

        }

    }
    /*pub fn resize<F : glium::backend::Facade>(&'a mut self, width: u32, height: u32, samples: u32, facade: &'a F) {
        self.fbo = None;
        self.out_fbo = None;
        self.rbo = glium::framebuffer::DepthRenderBuffer::new(facade, 
            glium::texture::DepthFormat::F32, width, height).unwrap();
        self.tex = glium::texture::Texture2dMultisample::empty_with_format(facade, glium::texture::UncompressedFloatFormat::F16F16F16F16,
            glium::texture::MipmapsOption::NoMipmap, width, height, samples).unwrap();
        self.out_tex = glium::texture::Texture2d::empty_with_format(facade, glium::texture::UncompressedFloatFormat::F16F16F16F16,
            glium::texture::MipmapsOption::NoMipmap, width, height).unwrap();
    }*/

    pub fn draw<F : glium::backend::Facade>(&mut self, facade: &F) -> &mut glium::framebuffer::SimpleFrameBuffer<'a> {
        if self.fbo.is_none() {
            let ms_tex = &*self.tex as *const glium::texture::Texture2dMultisample;
            let rbo_ptr = &*self.depth_tex as *const glium::texture::DepthTexture2dMultisample;
            unsafe {
                self.fbo = Some(glium::framebuffer::SimpleFrameBuffer::with_depth_buffer(facade, 
                    &*ms_tex, &*rbo_ptr).unwrap());
            }
        }
        if self.out_fbo.is_none() {
            unsafe {
                let tex_ptr = &*self.out_tex as *const glium::texture::Texture2d;
                self.out_fbo = Some(glium::framebuffer::SimpleFrameBuffer::new(facade, &*tex_ptr).unwrap());
            }
        }
        self.fbo.as_mut().unwrap()
    }
    
}

impl<'a> draw_traits::Drawable for RenderTarget<'a> {
    fn render<S>(&self, frame: &mut S, mats: &shader::SceneData, shader: &shader::ShaderManager)
        where S : glium::Surface
    {

        let dst_target = glium::BlitTarget {
            left: 0,
            bottom: 0,
            width: self.width as i32,
            height: self.height as i32,
        };
        self.fbo.as_ref().unwrap().blit_whole_color_to(self.out_fbo.as_ref().unwrap(), 
            &dst_target, glium::uniforms::MagnifySamplerFilter::Linear);
        let args = shader::UniformData {
            scene_data: mats,
            model: cgmath::Matrix4::from_scale(1f32).into(),
            diffuse_tex: None,
            roughness_map: None,
            metallic_map: None,
            normal_map: Some(&self.out_tex),
            emission_map: None,
            env_map: None,
        };
        let shader_name = "ui-msaa";
        let (program, params, uniform) = shader.use_shader(shader_name, &args);
        match uniform {
            shader::UniformType::UiUniform(uniform) =>
                frame.draw(&self.vbo, &self.ebo, program, &uniform, &params).unwrap(),
            _ => panic!("Invalid uniform type returned for RenderTarget"),
        }
    }
}