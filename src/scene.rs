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
            ibl_maps: None, lights: ssbo::SSBO::<shader::LightData>::dynamic(None),
        }
    }

    fn get_scene_data(&self, viewer: shader::ViewerData, pass: shader::RenderPassType) 
        -> shader::SceneData
    {
        shader::SceneData {
            viewer,
            ibl_maps: self.ibl_maps.as_ref(),
            lights: Some(&self.lights),
            tiles_x: None,
            depth_tex: None,
            pass_type: pass,
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
        -> Option<render_target::TextureType<'b>> where F : Fn(&mut glium::framebuffer::SimpleFrameBuffer, &shader::SceneData, shader::RenderPassType)
    {
        use std::rc::*;
        use std::cell::*;
        let vd = draw_traits::viewer_data_from(viewer, aspect);
        let sd = Rc::new(RefCell::new(self.get_scene_data(vd, shader::RenderPassType::Visual)));
        pass.run_pass(viewer, shader, sd.clone(),
        &|fbo, viewer, typ, _| {
            {
                let mut sdm = sd.borrow_mut();
                sdm.viewer = draw_traits::viewer_data_from(viewer, aspect);
                sdm.pass_type = typ;
            }
            let sd = sd.borrow();
            func(fbo, &*sd, typ);
        })
    }

    pub fn set_ibl_maps(&mut self, maps: shader::PbrMaps) {
        self.ibl_maps = Some(maps);
    }

    pub fn set_lights(&mut self, lights: &Vec<shader::LightData>) {
        self.lights.update(lights)
    }
}