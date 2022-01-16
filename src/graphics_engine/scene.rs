use super::drawable::*;
use super::{shader, pipeline, entity, cubes};
use crate::cg_support::ssbo;
use super::entity::AbstractEntity;
use std::rc::Rc;
use std::cell::{RefCell, Cell};

/// A Scene manages the scene parameters and
/// strings together multiple render passes
pub struct Scene {
    ibl_maps: Option<shader::PbrMaps>,
    lights: ssbo::SSBO<shader::LightData>,
    main_light_dir: Option<cgmath::Vector3<f32>>,
    entities: Vec<Rc<RefCell<dyn AbstractEntity>>>,
    pass: Cell<Option<pipeline::RenderPass>>,
    viewer: Rc<RefCell<dyn Viewer>>,
}

impl Scene {
    pub fn new(pass: pipeline::RenderPass, viewer: Rc<RefCell<dyn Viewer>>) -> Scene {
        Scene {
            ibl_maps: None, lights: ssbo::SSBO::<shader::LightData>::dynamic(None),
            main_light_dir: None,
            entities: Vec::new(),
            pass: Cell::new(Some(pass)),
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

    /// Renders the scene
    /// Returns either a texture result of the render or `None` if the result was rendered onto
    /// the screen
    pub fn render(&self, shader: &shader::ShaderManager)
    {
        let vd = viewer_data_from(&*self.viewer.borrow());
        let sd = Rc::new(RefCell::new(self.get_scene_data(vd, shader::RenderPassType::Visual)));
        let mut pass = self.pass.take().unwrap();
        pass.run_pass(&*self.viewer.borrow(), shader, sd.clone(),
        &mut |fbo, viewer, typ, cache, _| {
            {
                let mut sdm = sd.borrow_mut();
                sdm.viewer = viewer_data_from(viewer);
                sdm.pass_type = typ;
            }
            let sd = sd.borrow();
            for entity in &self.entities {
                if entity.borrow().should_render(typ) {
                    let mut entity = entity.borrow_mut();
                    entity::render_entity(&mut *entity, fbo, &*sd, cache, shader);
                }
            }
        });
        self.pass.set(Some(pass));
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

    pub fn set_entities(&mut self, entities: Vec<Rc<RefCell<dyn AbstractEntity>>>) {
        self.entities = entities;
    }

    #[allow(dead_code)]
    pub fn add_entity(&mut self, entity: Rc<RefCell<dyn AbstractEntity>>) {
        self.entities.push(entity);
    }
}

pub fn gen_ibl_from_hdr<F : glium::backend::Facade>(hdr_path: &str, shader_manager: &shader::ShaderManager, facade: &F) -> shader::PbrMaps 
{
    use super::{camera, drawable, textures};
    use pipeline::*;
    let cbo = cubes::gen_cubemap_from_sphere(hdr_path, 1024, shader_manager, facade);
    let cam = camera::PerspectiveCamera::default(1.);
    let mip_levels = 5;
    let mut rt = render_target::MipCubemapRenderTarget::new(128, mip_levels, 10., cgmath::point3(0., 0., 0.));
    let iterations = Cell::new(0);
    let mut cache = shader::PipelineCache::default();
    let mut skybox = cubes::Skybox::new(cubes::SkyboxTex::Sphere(
        textures::load_texture_hdr(hdr_path, facade)), facade);
    let res = rt.draw(&cam, None, &mut cache, &mut |fbo, viewer, _, cache, _| {
        let its = iterations.get();
        let mip_level = its / 6;
        skybox.set_mip_progress(Some(mip_level as f32 / (mip_levels - 1) as f32));
        let sd = drawable::default_scene_data(viewer);
        drawable::render_drawable(&mut skybox, None, fbo, &sd, &cache, shader_manager);
        iterations.set(its + 1);
    });
    skybox.set_mip_progress(None);
    let mut tp = texture_processor::GenLutProcessor::new(512, 512, facade);
    let brdf = tp.process(None, shader_manager, &mut cache, None);
    match (res, brdf) {
        (Some(TextureType::TexCube(Ownership::Own(spec))), Some(TextureType::Tex2d(Ownership::Own(brdf)))) =>
            shader::PbrMaps {
                diffuse_ibl: cbo,
                spec_ibl: spec,
                brdf_lut: brdf,
            },
        _ => panic!("Unexpected return from generating ibl textures"),
    }
}