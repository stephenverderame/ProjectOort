use super::drawable::*;
use crate::cg_support::Transformation;
use super::model;
use std::rc::Rc;
use std::cell::RefCell;
use super::shader;

/// Relative render order of an entity
#[derive(Copy, Clone, Hash, PartialEq, Eq)]
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
    /// 
    /// Returns `None` or a slice of at least 1 Transformation
    fn transformations(&self) -> Option<&[Rc<RefCell<dyn Transformation>>]>;

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
    fn transformations(&self) -> Option<&[Rc<RefCell<dyn Transformation>>]> {
        if !self.locations.is_empty() { Some(&self.locations) }
        else { None }
    }
    fn drawable(&mut self) -> &mut dyn Drawable {
        &mut *self.geometry
    }
    fn should_render(&self, pass: shader::RenderPassType) -> bool {
        let base_bool = self.render_passes.iter().any(|x| *x == pass);
        if base_bool {
            match pass {
                shader::RenderPassType::Depth => 
                    self.geometry.transparency().map(|x| x <= f32::EPSILON)
                        .unwrap_or(true),
                shader::RenderPassType::TransparentDepth => 
                    self.geometry.transparency().map(|x| x > f32::EPSILON)
                        .unwrap_or(false),
                _ => base_bool,
            }
        } else { base_bool }

    }
    fn render_order(&self) -> RenderOrder {
        self.order
    }
}

/// Constructs a new Entity
pub struct EntityBuilder {
    drawable: Box<dyn Drawable>,
    locations: Vec<Rc<RefCell<dyn Transformation>>>,
    render_passes: Vec<shader::RenderPassType>,
    order: RenderOrder,
}

impl EntityBuilder {
    pub fn new<D : Drawable + 'static>(drawable: D) -> Self {
        Self {
            drawable: Box::new(drawable),
            locations: Vec::new(),
            render_passes: Vec::new(),
            order: RenderOrder::Unordered,
        }
    }

    /// Adds a location to the entity
    #[allow(unused)]
    pub fn at(mut self, pos: crate::cg_support::node::Node) -> Self {
        self.locations.push(Rc::new(RefCell::new(pos)));
        self
    }

    /// Sets the render passes for the entity
    pub fn with_pass(mut self, pass: shader::RenderPassType) -> Self {
        self.render_passes.push(pass);
        self
    }

    /// Sets the render order for the entity
    pub fn render_order(mut self, order: RenderOrder) -> Self {
        self.order = order;
        self
    }

    /// Builds the entity
    pub fn build(self) -> Entity {
        Entity {
            geometry: self.drawable,
            locations: self.locations,
            render_passes: self.render_passes,
            order: self.order,
        }
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
    fn transformations(&self) -> Option<&[Rc<RefCell<dyn Transformation>>]> {
        if !self.locations.is_empty() { Some(&self.locations) }
        else { None }
    }
    fn drawable(&mut self) -> &mut dyn Drawable {
        &mut *self.geometry
    }
    fn should_render(&self, pass: shader::RenderPassType) -> bool {
        let base_bool = self.render_passes.iter().any(|x| *x == pass);
        if base_bool {
            match pass {
                shader::RenderPassType::Depth => 
                    self.geometry.transparency().map(|x| x <= f32::EPSILON)
                    .unwrap_or(true),
                shader::RenderPassType::TransparentDepth => 
                    self.geometry.transparency().map(|x| x > f32::EPSILON)
                    .unwrap_or(false),
                _ => base_bool,
            }
        } else { base_bool }

    }
    fn render_order(&self) -> RenderOrder {
        self.order
    }

} 

/// Renders the entity to the given surface
pub fn render_entity<S : glium::Surface>(entity: &mut dyn AbstractEntity, 
    surface: &mut S, scene_data: &shader::SceneData, 
    cache: &shader::PipelineCache, shader: &shader::ShaderManager) 
{
    let matrices : Vec<[[f32; 4]; 4]> 
        = entity.transformations().map(|entities| {
            entities.iter().map(|x| x.borrow().as_transform()
            .cast().unwrap().into()).collect()
        }).unwrap_or_else(|| Vec::new());
    super::drawable::render_drawable(entity.drawable(), Some(&matrices), 
        surface, scene_data, cache, shader)
      
}