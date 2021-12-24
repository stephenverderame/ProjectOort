#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 2],
    tex_coords: [f32; 2],
}

glium::implement_vertex!(Vertex, pos, tex_coords);

use glium::Surface;
use crate::shader;
use crate::draw_traits;
use std::cell::RefCell;

pub struct RenderTarget<'a>{
    fbo: Option<glium::framebuffer::SimpleFrameBuffer<'a>>,
    depth_tex: Box<glium::texture::DepthTexture2dMultisample>,
    // box for heap allocation to prevent dangling pointers
    tex: Box<glium::texture::Texture2dMultisample>,
    vbo: glium::VertexBuffer<Vertex>,
    ebo: glium::IndexBuffer<u16>,
    height: u32,
    width: u32,
    out_tex: Box<glium::texture::Texture2d>,
    out_fbo: Option<glium::framebuffer::SimpleFrameBuffer<'a>>,
    bright_color_tex: Box<glium::texture::Texture2d>,
    bright_color_fbo: RefCell<glium::framebuffer::SimpleFrameBuffer<'a>>,
    pong_tex: Box<glium::texture::Texture2d>,
    pong_fbo: RefCell<glium::framebuffer::SimpleFrameBuffer<'a>>,
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
        let bright_color_tex = Box::new(glium::texture::Texture2d::empty_with_format(facade,
            glium::texture::UncompressedFloatFormat::F16F16F16F16, glium::texture::MipmapsOption::NoMipmap,
            width, height).unwrap());
        let pong_tex = Box::new(glium::texture::Texture2d::empty_with_format(facade,
            glium::texture::UncompressedFloatFormat::F16F16F16F16, glium::texture::MipmapsOption::NoMipmap,
            width, height).unwrap());
        unsafe {
            let bright_color_ptr = &*bright_color_tex as *const glium::texture::Texture2d;
            let bright_color_fbo = RefCell::new(glium::framebuffer::SimpleFrameBuffer::new(facade, &*bright_color_ptr).unwrap());
            let pong_ptr = &*pong_tex as *const glium::texture::Texture2d;
            let pong_fbo = RefCell::new(glium::framebuffer::SimpleFrameBuffer::new(facade, &*pong_ptr).unwrap());
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
                bright_color_fbo,
                bright_color_tex,
                pong_tex,
                pong_fbo,

            }

        }
    }
    pub fn resize_and_clear<F : glium::backend::Facade>(&mut self, width: u32, height: u32, samples: u32, facade: &F) {
        if self.fbo.is_some() {
            self.fbo = None;
            self.out_fbo = None;
            self.depth_tex = Box::new(glium::texture::DepthTexture2dMultisample::empty(facade, width, height, samples).unwrap());
            self.tex = Box::new(glium::texture::Texture2dMultisample::empty_with_format(facade, glium::texture::UncompressedFloatFormat::F16F16F16F16,
                glium::texture::MipmapsOption::NoMipmap, width, height, samples).unwrap());
            self.out_tex = Box::new(glium::texture::Texture2d::empty_with_format(facade, glium::texture::UncompressedFloatFormat::F16F16F16F16,
                glium::texture::MipmapsOption::NoMipmap, width, height).unwrap());
            println!("Finish resizing");
        }
    }

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

    
    fn extract_bright_color(&self, mats: &shader::SceneData, shader: &shader::ShaderManager) {
        let data = shader::UniformData {
            scene_data: mats,
            model: cgmath::Matrix4::from_scale(1f32).into(),
            diffuse_tex: None,
            roughness_map: None,
            metallic_map: None,
            normal_map: Some(&self.out_tex),
            emission_map: None,
            env_map: None,
        };
        let (program, params, uniform) = shader.use_shader("ui-bloom", &data);
        match uniform {
            shader::UniformType::BloomUniform(uniform) => {
                let fbo = &mut *self.bright_color_fbo.borrow_mut();
                fbo.clear_color(0., 0., 0., 1.);
                fbo.draw(&self.vbo, &self.ebo, program, &uniform, &params).unwrap()
            },
            _ => panic!("Invalid uniform type returned for RenderTarget"),
        }
    }

    fn blur_pass(&self, shader: &shader::ShaderManager, mats: &shader::SceneData, pass_count: i32) 
    {
        let mut data = shader::UniformData {
            scene_data: mats,
            model: cgmath::Matrix4::from_scale(1f32).into(),
            diffuse_tex: None,
            roughness_map: None,
            metallic_map: None,
            normal_map: None,
            emission_map: None,
            env_map: None,
        };
        let fbo : &RefCell<glium::framebuffer::SimpleFrameBuffer>;
        match pass_count {
            x if x % 2 == 0 => {
                fbo = &self.pong_fbo;
                data.normal_map = Some(&self.bright_color_tex);
                data.roughness_map = None;
            },
            _ => {
                fbo = &self.bright_color_fbo;
                data.roughness_map = Some(&self.pong_tex);
                data.normal_map = None;
            }
        }
        let (program, params, uniform) = shader.use_shader("ui-blur", &data);
        match uniform {
            shader::UniformType::BlurUniform(uniform) => {
                fbo.borrow_mut().draw(&self.vbo, &self.ebo, program, &uniform, params).unwrap();
            },
            _ => panic!("Invalid uniform type returned for RenderTarget"),
        }
    }

    fn gen_bloom_tex<'b>(&'b self, mats: &'b shader::SceneData, shader: &shader::ShaderManager) {
        self.extract_bright_color(mats, shader);
        let passes = 10;
        for i in 0 .. passes {
            self.blur_pass(shader, mats, i);
        }
        if passes % 2 == 1 {
            // ends on even number (last rendered to pong_fbo)
            let bt = glium::BlitTarget {
                left: 0,
                bottom: 0,
                width: self.height as i32,
                height: self.width as i32,
            };
            self.pong_fbo.borrow().blit_whole_color_to(&*self.bright_color_fbo.borrow(), 
                &bt, glium::uniforms::MagnifySamplerFilter::Linear);

        }
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
        self.gen_bloom_tex(mats, shader);
        let args = shader::UniformData {
            scene_data: mats,
            model: cgmath::Matrix4::from_scale(1f32).into(),
            diffuse_tex: None,
            roughness_map: Some(&self.bright_color_tex),
            metallic_map: None,
            normal_map: Some(&self.out_tex),
            emission_map: None,
            env_map: None,
        };
        let (program, params, uniform) = shader.use_shader("ui", &args);
        match uniform {
            shader::UniformType::UiUniform(uniform) =>
                frame.draw(&self.vbo, &self.ebo, program, &uniform, &params).unwrap(),
            _ => panic!("Invalid uniform type returned for RenderTarget"),
        }
    }
}