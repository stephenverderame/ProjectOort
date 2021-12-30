use crate::draw_traits;
use crate::shader;
use crate::render_target;
use crate::render_pass::*;
use crate::ssbo;

/// A Scene manages the scene parameters and
/// strings together multiple render passes
pub struct Scene {
    ibl_maps: Option<shader::PbrMaps>,
    lights: ssbo::SSBO<shader::LightData>,
}

impl Scene {
    pub fn new() -> Scene {
        Scene {
            ibl_maps: None, lights: ssbo::SSBO::<shader::LightData>::new(None),
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
            ibl_maps: self.ibl_maps.as_ref(),
            lights: Some(&self.lights),
        }
    }

    /*pub fn render<S : glium::Surface, F : Fn(&mut S, &shader::SceneData)>
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
    }*/

    pub fn render_pass<'b, F>(&self, pass: &'b mut RenderPass, viewer: &dyn draw_traits::Viewer, 
        aspect: f32, shader: &shader::ShaderManager, func: F)
        -> render_target::TextureType<'b> where F : Fn(&mut glium::framebuffer::SimpleFrameBuffer, &shader::SceneData)
    {
        pass.run_pass(viewer, shader, &self.get_scene_data(viewer, aspect),
        &|fbo, viewer, _, _| {
            let mats = self.get_scene_data(viewer, aspect);
            func(fbo, &mats);
        })
    }

    pub fn set_ibl_maps(&mut self, maps: shader::PbrMaps) {
        self.ibl_maps = Some(maps);
    }

    pub fn set_lights(&mut self, lights: &Vec<shader::LightData>) {
        self.lights.update(lights)
    }
}