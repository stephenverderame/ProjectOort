use super::drawable::*;
use crate::cg_support::Transformation;
use super::model;
use std::rc::Rc;
use std::cell::RefCell;
use super::shader;

/// Relative render order of an entity
#[derive(Copy, Clone, Hash)]
pub enum RenderOrder {
    /// Render this entity first, in the order they are specified
    First,
    /// Render this entity last, in the order they are specified
    Last,
    /// Entity is independent of render order
    Unordered,
}

/// An entity is a drawable combined with positional data
/// An entity can be in many positions at once
pub trait AbstractEntity {
    /// Gets the transformations for all locations for this entity
    fn transformations(&self) -> &[Rc<RefCell<dyn Transformation>>];

    /// Gets the drawable for this entity
    fn drawable(&mut self) -> &mut dyn Drawable;

    /// Determines if the entity should be drawn during `pass`
    fn should_render(&self, pass: shader::RenderPassType) -> bool;

    fn render_order(&self) -> RenderOrder;
}

/// An entity with any drawable
pub struct Entity {
    pub geometry: Box<dyn Drawable>,
    pub locations: Vec<Rc<RefCell<dyn Transformation>>>,
    pub render_passes: Vec<shader::RenderPassType>,
    pub order: RenderOrder,
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
        let base_bool = self.render_passes.iter().any(|x| *x == pass);
        if base_bool {
            match pass {
                shader::RenderPassType::Depth => 
                    self.geometry.transparency().map(|x| x <= f32::EPSILON).unwrap_or(true),
                shader::RenderPassType::TransparentDepth => 
                    self.geometry.transparency().map(|x| x > f32::EPSILON).unwrap_or(false),
                _ => base_bool,
            }
        } else { base_bool }

    }
    fn render_order(&self) -> RenderOrder {
        self.order
    }
}
/// An entity whose drawable is an externaly loaded model
pub struct ModelEntity {
    pub geometry: Box<model::Model>,
    pub locations: Vec<Rc<RefCell<dyn Transformation>>>,
    pub render_passes: Vec<shader::RenderPassType>,
    pub order: RenderOrder,
}

impl AbstractEntity for ModelEntity {
    fn transformations(&self) -> &[Rc<RefCell<dyn Transformation>>] {
        &self.locations
    }
    fn drawable(&mut self) -> &mut dyn Drawable {
        &mut *self.geometry
    }
    fn should_render(&self, pass: shader::RenderPassType) -> bool {
        let base_bool = self.render_passes.iter().any(|x| *x == pass);
        if base_bool {
            match pass {
                shader::RenderPassType::Depth => 
                    self.geometry.transparency().map(|x| x <= f32::EPSILON).unwrap_or(true),
                shader::RenderPassType::TransparentDepth => 
                    self.geometry.transparency().map(|x| x > f32::EPSILON).unwrap_or(false),
                _ => base_bool,
            }
        } else { base_bool }

    }
    fn render_order(&self) -> RenderOrder {
        self.order
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