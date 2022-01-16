use super::drawable::*;
use crate::cg_support::Transformation;
use super::model;
use std::rc::Rc;
use std::cell::RefCell;
use super::shader;
pub trait AbstractEntity {
    fn transformations(&self) -> &[Rc<RefCell<dyn Transformation>>];
    fn drawable(&mut self) -> &mut dyn Drawable;
    fn should_render(&self, pass: shader::RenderPassType) -> bool;
}

pub struct Entity {
    pub geometry: Box<dyn Drawable>,
    pub locations: Vec<Rc<RefCell<dyn Transformation>>>,
    pub render_passes: Vec<shader::RenderPassType>,
}

impl std::ops::Deref for Entity {
    type Target = dyn Drawable;

    fn deref(&self) -> &Self::Target {
        &*self.geometry
    }
}

impl std::ops::DerefMut for Entity {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.geometry
    }
}

impl AbstractEntity for Entity {
    fn transformations(&self) -> &[Rc<RefCell<dyn Transformation>>] {
        &self.locations
    }
    fn drawable(&mut self) -> &mut dyn Drawable {
        &mut *self.geometry
    }
    fn should_render(&self, pass: shader::RenderPassType) -> bool {
        self.render_passes.iter().any(|x| *x == pass)
    }
}

pub struct ModelEntity {
    pub geometry: Box<model::Model>,
    pub locations: Vec<Rc<RefCell<dyn Transformation>>>,
    pub render_passes: Vec<shader::RenderPassType>,
}

impl AbstractEntity for ModelEntity {
    fn transformations(&self) -> &[Rc<RefCell<dyn Transformation>>] {
        &self.locations
    }
    fn drawable(&mut self) -> &mut dyn Drawable {
        &mut *self.geometry
    }
    fn should_render(&self, pass: shader::RenderPassType) -> bool {
        self.render_passes.iter().any(|x| *x == pass)
    }
}

/// Renders the entity to the given surface
pub fn render_entity<S : glium::Surface>(entity: &mut dyn AbstractEntity, surface: &mut S, scene_data: &shader::SceneData, 
    cache: &shader::PipelineCache, shader: &shader::ShaderManager) 
{
    let matrices : Vec<[[f32; 4]; 4]> 
        = entity.transformations().iter().map(|x| x.borrow().as_transform().cast().unwrap().into()).collect();
    super::drawable::render_drawable(entity.drawable(), Some(&matrices), surface, scene_data, cache, shader)
      
}