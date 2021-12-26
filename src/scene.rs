use glium::Surface;
use crate::draw_traits;
use crate::shader;
use crate::camera;
use crate::node;
use crate::render_target;
use crate::render_pass::*;

use cgmath::*;

pub struct Scene {
    ibl_map: Option<glium::texture::Cubemap>,
}

impl Scene {
    pub fn new() -> Scene {
        Scene {
            ibl_map: None
        }
    }

    fn get_scene_data(&self, viewer: &dyn draw_traits::Viewer, aspect: f32) 
        -> shader::SceneData
    {
        let view = viewer.view_mat();
        let proj = viewer.proj_mat(aspect);
        shader::SceneData {
            viewproj: (proj * view).into(),
            view: view.into(),
            proj: proj.into(),
            cam_pos: viewer.cam_pos().into(),
            ibl_map: match &self.ibl_map {
                Some(map) => Some(&map),
                _ => None,
            },
        }
    }

    pub fn render<S : glium::Surface, F : Fn(&mut S, &shader::SceneData)>
    (&self, frame: &mut S, viewer: &dyn draw_traits::Viewer, aspect: f32, func: F)
    {
        let mats = self.get_scene_data(viewer, aspect);
        func(frame, &mats);
    }

    pub fn render_target<'a, F : Fn(&mut glium::framebuffer::SimpleFrameBuffer, &shader::SceneData)>
    (&self, frame: &'a mut dyn render_target::RenderTarget, viewer: &dyn draw_traits::Viewer, aspect: f32, func: F)
    -> render_target::TextureType<'a>
    {
        frame.draw(viewer, &|fbo, viewer| {
            let mats = self.get_scene_data(viewer, aspect);
            func(fbo, &mats);
        });
        frame.read()
    }

    pub fn do_pass<'a, F>(&self, pass: &'a mut RenderPass, viewer: &dyn draw_traits::Viewer, 
        aspect: f32, shader: &shader::ShaderManager, func: F)
        -> render_target::TextureType<'a> where F : Fn(&mut glium::framebuffer::SimpleFrameBuffer, &shader::SceneData)
    {
        pass.run_pass(viewer, shader, &|fbo, viewer| {
            let mats = self.get_scene_data(viewer, aspect);
            func(fbo, &mats);
        })
    }

    pub fn set_ibl_map(&mut self, map: glium::texture::Cubemap) {
        self.ibl_map = Some(map);
    }
}