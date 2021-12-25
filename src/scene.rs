use glium::Surface;
use crate::draw_traits;
use crate::shader;
use crate::camera;
use crate::node;
use crate::render_target;

use cgmath::*;
use glium::framebuffer::ToDepthAttachment;
use glium::framebuffer::ToColorAttachment;

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
        -> (shader::SceneData, cgmath::Matrix4<f32>) 
    {
        let view = viewer.view_mat();
        let proj = viewer.proj_mat(aspect);
        (shader::SceneData {
            viewproj: (proj * view).into(),
            view: view.into(),
            proj: proj.into(),
            cam_pos: viewer.cam_pos().into(),
            ibl_map: match &self.ibl_map {
                Some(map) => Some(&map),
                _ => None,
            },
        }, proj)
    }

    pub fn render<S : glium::Surface, F : Fn(&mut S, &shader::SceneData)>
    (&self, frame: &mut S, viewer: &dyn draw_traits::Viewer, aspect: f32, func: F)
    {
        let (mats, _) = self.get_scene_data(viewer, aspect);
        func(frame, &mats);
    }

    pub fn render_target<'a, F : Fn(&mut glium::framebuffer::SimpleFrameBuffer, &shader::SceneData)>
    (&self, frame: &'a mut dyn render_target::RenderTarget, viewer: &dyn draw_traits::Viewer, aspect: f32, func: F)
    -> render_target::TextureType<'a>
    {
        let (mut mats, proj) = self.get_scene_data(viewer, aspect);
        frame.draw(viewer, &|fbo, viewer| {
            let view_mat = viewer.view_mat();
            mats.viewproj = (proj * view_mat).into();
            mats.view = view_mat.into();
            mats.cam_pos = viewer.cam_pos().into();
            func(fbo, &mats);
        });
        frame.read()
    }

    pub fn set_ibl_map(&mut self, map: glium::texture::Cubemap) {
        self.ibl_map = Some(map);
    }
}