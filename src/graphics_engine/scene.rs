use super::drawable::*;
use super::{shader, pipeline, entity, cubes};
use crate::cg_support::ssbo;
use super::entity::AbstractEntity;
use std::rc::Rc;
use std::cell::{RefCell, Cell};
use std::collections::BTreeMap;

pub trait AbstractScene {
    /// Renders the scene
    /// Returns either a texture result of the render or `None` 
    /// if the result was rendered onto the screen
    fn render(&mut self, inputs: Option<&Vec<pipeline::TextureType>>,
        shader: &shader::ShaderManager) 
        -> Option<pipeline::TextureType>;


    /// Sets the lights used for this scene
    fn set_lights(&mut self, lights: &Vec<shader::LightData>);
}

/// A Scene manages the scene parameters and
/// strings together multiple render passes
pub struct Scene {
    ibl_maps: Option<shader::PbrMaps>,
    lights: Option<ssbo::SSBO<shader::LightData>>,
    main_light_dir: Option<cgmath::Vector3<f32>>,
    entities: Vec<Rc<RefCell<dyn AbstractEntity>>>,
    pass: Option<pipeline::RenderPass>,
    viewer: Rc<RefCell<dyn Viewer>>,
}

impl Scene {
    pub fn new(pass: pipeline::RenderPass, viewer: Rc<RefCell<dyn Viewer>>) -> Scene {
        Scene {
            ibl_maps: None, lights: Some(ssbo::SSBO::dynamic(None)),
            main_light_dir: None,
            entities: Vec::new(),
            pass: Some(pass),
            viewer,
        }
    }

    fn get_scene_data<'a>(viewer: shader::ViewerData, pass: shader::RenderPassType,
        ibl_maps: &'a Option<shader::PbrMaps>, lights: &'a Option<ssbo::SSBO<shader::LightData>>,
        light_dir: &'a Option<cgmath::Vector3<f32>>) -> shader::SceneData<'a>
    {
        shader::SceneData {
            viewer,
            ibl_maps: ibl_maps.as_ref(),
            lights: lights.as_ref(),
            pass_type: pass,
            light_pos: light_dir.map(|x| x.into()),
        }
    }

    fn render_transparency(entities:  &[Rc<RefCell<dyn AbstractEntity>>], 
        obj: *const entity::Entity, viewer: &dyn Viewer, 
        scene_data: &shader::SceneData, cache: &shader::PipelineCache,
        fbo: &mut glium::framebuffer::SimpleFrameBuffer,
        shader: &shader::ShaderManager) 
    {
        use crate::cg_support::Transformation;
        use entity::*;
        use cgmath::*;
        let mut map = BTreeMap::new();
        let mut firsts = Vec::new();
        let mut lasts = Vec::new();
        let view_mat = viewer.view_mat().into_transform();
        for entity in entities {
            if entity.as_ptr() as *const entity::Entity != obj && 
                entity.borrow().should_render(shader::RenderPassType::Transparent(obj)) 
            {
                // NOTE: I think we just need to order the transparent viewpoints, not the 
                // objects when doing a transparency pass
                match entity.borrow().render_order() {
                    RenderOrder::Unordered => {
                        let cam_z = view_mat.transform_point(entity.borrow()
                            .transformations().map(|trans| {
                                trans[0].borrow().as_transform()
                                .transform_point(point3(0., 0., 0.))
                            }).unwrap_or(point3(0., 0., 0.))).z;
                        let mut fixpoint_depth = -((cam_z * 10f64.powi(8) + 0.5) as i64);
                        while map.get(&fixpoint_depth).is_some() { fixpoint_depth -= 1; }
                        map.insert(fixpoint_depth, entity.clone());
                    },
                    RenderOrder::First => firsts.push(entity.clone()),
                    RenderOrder::Last => lasts.push(entity.clone()),
                }
            }
        }

        for entity in firsts.into_iter()
            .chain(map.into_iter().map(|(_, entity)| entity)).chain(lasts.into_iter()) 
        {
            let mut entity = entity.borrow_mut();
            entity::render_entity(&mut *entity, fbo, scene_data, cache, shader)
        }
    }

    fn render_entities(entities:  &[Rc<RefCell<dyn AbstractEntity>>], 
        viewer: &dyn Viewer, scene_data: &shader::SceneData, 
        pass: shader::RenderPassType, cache: &shader::PipelineCache,
        fbo: &mut glium::framebuffer::SimpleFrameBuffer,
        shader: &shader::ShaderManager) 
    {
        match pass {
            shader::RenderPassType::Transparent(ptr) => 
                Self::render_transparency(entities, ptr, viewer, 
                    scene_data, cache, fbo, shader),
            typ => {
                for entity in entities {
                    if entity.borrow().should_render(typ) {
                        let mut entity = entity.borrow_mut();
                        entity::render_entity(&mut *entity, fbo, scene_data, 
                            cache, shader);
                    }
                }
            },
        }
    }

    pub fn set_ibl_maps(&mut self, maps: shader::PbrMaps) {
        self.ibl_maps = Some(maps);
    }

    pub fn set_light_dir(&mut self, dir_light: cgmath::Vector3<f32>) {
        self.main_light_dir = Some(dir_light);
    }

    pub fn set_entities(&mut self, entities: Vec<Rc<RefCell<dyn AbstractEntity>>>) {
        self.entities = entities;
        use std::cmp::Ordering;
        use entity::RenderOrder as RO;
        self.entities.sort_by(|a, b| {
            match (a.borrow().render_order(), b.borrow().render_order()) {
                (RO::First, RO::First) | (RO::Last, RO::Last) |
                (RO::Unordered, RO::Unordered) => Ordering::Equal,
                (RO::First, _) | (_, RO::Last) => Ordering::Less,
                (RO::Last, _) | (_, RO::First) => Ordering::Greater,
            }
        });
    }

    #[allow(dead_code)]
    pub fn add_entity(&mut self, entity: Rc<RefCell<dyn AbstractEntity>>) {
        self.entities.push(entity);
    }
}
/// Generates an ibl from an hdr and skybox
/// 
/// `hdr_path` - the path to the hdr diffuse ibl image
/// 
/// `bg_skybox` - the skybox storing the texture to generate the specular ibl from
pub fn gen_ibl_from_hdr<F : glium::backend::Facade>(hdr_path: &str, 
    bg_skybox: &mut cubes::Skybox, shader_manager: &shader::ShaderManager, 
    facade: &F) -> shader::PbrMaps 
{
    use super::{camera, drawable};
    use pipeline::*;
    let cbo = cubes::gen_cubemap_from_sphere(hdr_path, 1024, shader_manager, facade);
    let cam = camera::PerspectiveCamera::default(1.);
    let mip_levels = 5;
    let pos_func = || cgmath::point3(0., 0., 0.);
    let mut rt = render_target::MipCubemapRenderTarget::new(128, mip_levels, 10., 
        Box::new(pos_func));
    let iterations = Cell::new(0);
    let mut cache = shader::PipelineCache::default();
    let res = rt.draw(&cam, None, &mut cache, &mut |fbo, viewer, _, cache, _, _| {
        let its = iterations.get();
        let mip_level = its;
        bg_skybox.set_mip_progress(Some(mip_level as f32 / (mip_levels - 1) as f32));
        let mut sd = drawable::default_scene_data(viewer);
        sd.pass_type = shader::RenderPassType::LayeredVisual;
        drawable::render_drawable(bg_skybox, None, fbo, &sd, &cache, shader_manager);
        iterations.set(its + 1);
    });
    bg_skybox.set_mip_progress(None);
    let mut tp = texture_processor::GenLutProcessor::new(512, 512, facade);
    let brdf = tp.process(None, shader_manager, &mut cache, None);
    match (res, brdf) {
        (Some(TextureType::TexCube(Ownership::Own(spec))), 
            Some(TextureType::Tex2d(Ownership::Own(brdf)))) =>
            shader::PbrMaps {
                diffuse_ibl: cbo,
                spec_ibl: spec,
                brdf_lut: brdf,
            },
        _ => panic!("Unexpected return from generating ibl textures"),
    }
}

impl AbstractScene for Scene {
    fn render(&mut self, _inputs: Option<&Vec<pipeline::TextureType>>,
        shader: &shader::ShaderManager) 
        -> Option<pipeline::TextureType>
    {
        let vd = viewer_data_from(&*self.viewer.borrow());
        let sd = Rc::new(RefCell::new(Self::get_scene_data(vd, shader::RenderPassType::Visual,
            &self.ibl_maps, &self.lights, &self.main_light_dir)));
        let viewer = self.viewer.borrow();
        let entities = &self.entities;
        let res = self.pass.as_mut().unwrap().run_pass(&*viewer, shader, sd.clone(),
        &mut |fbo, viewer, typ, cache, _, _| {
            fbo.clear_color_and_depth((0., 0., 0., 1.), 1.);
            {
                let mut sdm = sd.borrow_mut();
                sdm.viewer = viewer_data_from(viewer);
                sdm.pass_type = typ;
            }
            let scene_data = sd.borrow();
            Self::render_entities(entities, viewer, &*scene_data, 
                typ, cache, fbo, shader);
        });
        res
    }

    fn set_lights(&mut self, lights: &Vec<shader::LightData>) {
        if let Some(this_lights) = self.lights.as_mut() {
            this_lights.update(lights)
        } else {
            self.lights = Some(ssbo::SSBO::dynamic(Some(&lights)))
        }
    }
}

use glium::*;
use pipeline::texture_processor::{BlitTextureProcessor, CompositorProcessor};
pub struct CompositorScene<S : Surface,
    SHolder : std::ops::DerefMut<Target = S>,
    GetSHolder : Fn() -> (SHolder, BlitTarget),
    CleanSHolder : Fn(SHolder)> 
{
    scenes: Vec<Box<dyn AbstractScene>>,
    compositor: CompositorProcessor,
    blitter: BlitTextureProcessor<S, SHolder, GetSHolder, CleanSHolder>,
    viewer: Rc<RefCell<dyn Viewer>>,
}

/// Creates a new `CompositorScene` which blends together all output textures 
/// from each scene in `scenes` and blits the result to the main framebuffer
/// 
/// `width` - width of input textures
/// 
/// `height` - height of input textures
/// 
/// `viewer` - viewer to get the projection and view matrices for compositing
pub fn compositor_scene_new<F : backend::Facade>(width : u32, height : u32, 
    viewer: Rc<RefCell<dyn Viewer>>, scenes: Vec<Box<dyn AbstractScene>>, 
    fac: &F) -> CompositorScene<glium::Frame, super::MutCtx, 
        impl Fn() -> (super::MutCtx, BlitTarget), 
        impl Fn(super::MutCtx)>
{
    CompositorScene {
        scenes,
        blitter: BlitTextureProcessor::new(width, height, move || { 
            let mut surface = super::get_active_ctx().as_surface();
            surface.clear_color_and_depth((1., 0., 0., 1.), 1.);
            (surface, BlitTarget {
                left: 0, bottom: 0,
                width: width as i32, height: height as i32,
            })
        }, |disp| disp.finish()),
        compositor: CompositorProcessor::new(width, height, fac),
        viewer,
    }
}

impl<S : Surface,
    SHolder : std::ops::DerefMut<Target = S>,
    GetSHolder : Fn() -> (SHolder, BlitTarget),
    CleanSHolder : Fn(SHolder)> 
    AbstractScene for CompositorScene<S, SHolder, GetSHolder, CleanSHolder>
{
    fn render(&mut self, inputs: Option<&Vec<pipeline::TextureType>>,
        shader: &shader::ShaderManager) 
        -> Option<pipeline::TextureType>
    {
        use super::shader::{PipelineCache, SceneData, RenderPassType};
        use super::pipeline::TextureProcessor;
        let mut new_inputs = Vec::new();
        for scene in &mut self.scenes {
            if let Some(comp) = scene.render(inputs, shader) {
                new_inputs.push(comp);
            }
        }
        let sd = SceneData {
            viewer: viewer_data_from(&*self.viewer.borrow()),
            lights: None,
            ibl_maps: None,
            pass_type: RenderPassType::Visual,
            light_pos: None,
        };
        let default = Vec::new();
        let final_inputs : Vec<_> = 
            inputs.unwrap_or(&default).iter().chain(new_inputs.iter()).collect();
        if let Some(tex @ pipeline::TextureType::Tex2d(_)) = 
            self.compositor.process(Some(final_inputs), shader, 
            &mut PipelineCache::default(), Some(&sd))
        {
            self.blitter.process(Some(vec![&tex]), shader, 
                &mut PipelineCache::default(), Some(&sd));
        } else {
            panic!("Invalid return from compositor in compositor scene")
        }
        
        None
    }

    fn set_lights(&mut self, lights: &Vec<shader::LightData>) {
        for scene in &mut self.scenes {
            scene.set_lights(lights);
        }
    }
}