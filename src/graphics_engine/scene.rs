use super::drawable::*;
use super::shader;
use super::render_target;
use super::render_pass::*;
use crate::cg_support::ssbo;
use super::entity::Entity;
use std::rc::Rc;
use std::cell::{RefCell, Cell};

/// A Scene manages the scene parameters and
/// strings together multiple render passes
pub struct Scene {
    ibl_maps: Option<shader::PbrMaps>,
    lights: ssbo::SSBO<shader::LightData>,
    main_light_dir: Option<cgmath::Vector3<f32>>,
    entities: Vec<Entity>,
    pass: Cell<RenderPass>,
    viewer: Rc<RefCell<dyn Viewer>>,
}

impl Scene {
    pub fn new(pass: RenderPass, viewer: Rc<RefCell<dyn Viewer>>) -> Scene {
        Scene {
            ibl_maps: None, lights: ssbo::SSBO::<shader::LightData>::dynamic(None),
            main_light_dir: None,
            entities: Vec::new(),
            pass,
            viewer,
        }
    }

    fn get_scene_data(&self, viewer: shader::ViewerData, pass: shader::RenderPassType) 
        -> shader::SceneData
    {
        shader::SceneData {
            viewer,
            ibl_maps: self.ibl_maps.as_ref(),
            lights: Some(&self.lights),
            pass_type: pass,
            light_pos: self.main_light_dir.map(|x| x.into()),
        }
    }

    fn surface_render<'a, S : glium::Surface, U : glium::uniforms::Uniforms>(vbo: VertexHolder<'a>, ebo: glium::index::IndicesSource<'a>,
        shader: &glium::Program, uniform: U, params: glium::DrawParameters, surface: &mut S) -> Result<(), glium::DrawError>
    {
        /*match vbo {
            VboType::Vertices2d(vbo) => 
                surface.draw(vbo, ebo, shader, &uniform, &params),
            VboType::Vertices3d(vbo) =>
                surface.draw(vbo, ebo, shader, &uniform, &params),
            VboType::VerticesPos(vbo) =>
                surface.draw(vbo, ebo, shader, &uniform, &params),
        }*/
        surface.draw(vbo, ebo, shader, &uniform, &params)
    }

    fn render_drawable<S : glium::Surface>(surface: &mut S, entity: &Entity, scene_data: &shader::SceneData, 
        local_data: &shader::PipelineCache, shader: &shader::ShaderManager) 
    {
        let matrices : Vec<[[f32; 4]; 4]> 
            = entity.locations.iter().map(|x| x.borrow().as_transform().cast().unwrap().into()).collect();
        for (args, vbo, ebo) in entity.render_args(&matrices).into_iter() {
            let (shader, params, uniform) = shader.use_shader(&args, Some(scene_data), Some(local_data));
            match uniform {
                shader::UniformType::LaserUniform(uniform) => 
                    Scene::surface_render(vbo, ebo, shader, uniform, params, surface),
                shader::UniformType::PbrUniform(uniform) => 
                    Scene::surface_render(vbo, ebo, shader, uniform, params, surface),
                shader::UniformType::DepthUniform(uniform) =>
                    Scene::surface_render(vbo, ebo, shader, uniform, params, surface),
                shader::UniformType::EqRectUniform(uniform) =>
                    Scene::surface_render(vbo, ebo, shader, uniform, params, surface),
                shader::UniformType::SkyboxUniform(uniform) =>
                    Scene::surface_render(vbo, ebo, shader, uniform, params, surface),
                shader::UniformType::UiUniform(uniform) =>
                    Scene::surface_render(vbo, ebo, shader, uniform, params, surface),
                shader::UniformType::SepConvUniform(uniform) =>
                    Scene::surface_render(vbo, ebo, shader, uniform, params, surface),
                shader::UniformType::ExtractBrightUniform(uniform) =>
                    Scene::surface_render(vbo, ebo, shader, uniform, params, surface),
                shader::UniformType::PrefilterHdrEnvUniform(uniform) =>
                    Scene::surface_render(vbo, ebo, shader, uniform, params, surface),
                shader::UniformType::BrdfLutUniform(uniform) =>
                    Scene::surface_render(vbo, ebo, shader, uniform, params, surface),
            }.unwrap()
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

    /// Executes the specified render pass and passes in Scene Data for this Scene
 /*   pub fn render_pass<'b, F>(&self, pass: &'b mut RenderPass, viewer: &dyn Viewer, 
        shader: &shader::ShaderManager) -> Option<render_target::TextureType<'b>> 
    {
        use std::rc::*;
        use std::cell::*;
        let vd = viewer_data_from(viewer);
        let sd = Rc::new(RefCell::new(self.get_scene_data(vd, shader::RenderPassType::Visual)));
        pass.run_pass(viewer, shader, sd.clone(),
        &mut |fbo, viewer, typ, cache, _| {
            {
                let mut sdm = sd.borrow_mut();
                sdm.viewer = viewer_data_from(viewer);
                sdm.pass_type = typ;
            }
            let sd = sd.borrow();
            for entity in &self.entities {
                if entity.should_render(typ) {
                    Scene::render_drawable(fbo, entity, &*sd, cache, shader);
                }
            }
        })
    }*/

    /// Renders the scene
    /// Returns either a texture result of the render or `None` if the result was rendered onto
    /// the screen
    pub fn render(&self, shader: &shader::ShaderManager) -> Option<render_target::TextureType>
    {
        let vd = viewer_data_from(&*self.viewer.borrow());
        let sd = Rc::new(RefCell::new(self.get_scene_data(vd, shader::RenderPassType::Visual)));
        let mut pass = self.pass.take();
        pass.run_pass(&*self.viewer.borrow(), shader, sd.clone(),
        &mut |fbo, viewer, typ, cache, _| {
            {
                let mut sdm = sd.borrow_mut();
                sdm.viewer = viewer_data_from(viewer);
                sdm.pass_type = typ;
            }
            let sd = sd.borrow();
            for entity in &self.entities {
                if entity.should_render(typ) {
                    Scene::render_drawable(fbo, entity, &*sd, cache, shader);
                }
            }
        })
    }

    pub fn set_ibl_maps(&mut self, maps: shader::PbrMaps) {
        self.ibl_maps = Some(maps);
    }

    pub fn set_lights(&mut self, lights: &Vec<shader::LightData>) {
        self.lights.update(lights)
    }

    pub fn set_light_dir(&mut self, dir_light: cgmath::Vector3<f32>) {
        self.main_light_dir = Some(dir_light);
    }
}