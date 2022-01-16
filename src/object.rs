use crate::graphics_engine::model;
use crate::cg_support::node;
use crate::graphics_engine::entity::*;
use std::rc::Rc;
use std::cell::RefCell;
use crate::collisions;
use crate::graphics_engine::shader;
/// The transformation data for an entity
pub struct ObjectInstanceData {
    pub transform: Rc<RefCell<node::Node>>,
    pub velocity: cgmath::Vector3<f64>,
    pub collider: Option<collisions::CollisionObject>,
}

/// A game object is a renderable geometry with a node in the
/// scene transformation heirarchy and collision and physics
/// behavior.
pub struct GameObject {
    pub data: ObjectInstanceData,
    entity: Rc<RefCell<ModelEntity>>,
}

impl GameObject {
    #[allow(dead_code)]
    pub fn new(model: model::Model) -> GameObject {
        GameObject::from(model, node::Node::default())
    }

    pub fn from(model: model::Model, transform: node::Node) -> GameObject {
        let transform = Rc::new(RefCell::new(transform));
        GameObject {
            data: ObjectInstanceData {
                transform: transform.clone(),
                velocity: cgmath::vec3(0., 0., 0.),
                collider: None,
            },
            entity: Rc::new(RefCell::new(ModelEntity {
                geometry: Box::new(model),
                locations: vec![transform],
                render_passes: vec![shader::RenderPassType::Visual],
            })),
        }
    }

    pub fn with_depth(self) -> Self {
        self.entity.borrow_mut().render_passes.push(shader::RenderPassType::Depth);
        self
    }

    #[inline(always)]
    pub fn start_anim(&mut self, name: &str, do_loop: bool) {
        (&mut *self.entity.borrow_mut()).geometry.get_animator().start(name, do_loop)
    }

    #[inline(always)]
    pub fn as_entity(&self) -> Rc<RefCell<ModelEntity>> {
        self.entity.clone()
    }
}

/// Entities with shared geometry
/// 
/// TODO: free unnecessary instances
/// TODO: make private
pub struct GameObjects {
    pub instances: Vec<ObjectInstanceData>,
    entity: Rc<RefCell<Entity>>,
}

impl GameObjects {
    pub fn new(model: model::Model) -> GameObjects {
        GameObjects {
            instances: Vec::<ObjectInstanceData>::new(),
            entity: Rc::new(RefCell::new(Entity {
                geometry: Box::new(model),
                locations: Vec::new(),
                render_passes: vec![shader::RenderPassType::Visual],
            })),
        }
    }

    pub fn with_depth(self) -> Self {
        self.entity.borrow_mut().render_passes.push(shader::RenderPassType::Depth);
        self
    }

    /// Creates a new instance
    pub fn new_instance(&mut self, instance: ObjectInstanceData) {
        self.entity.borrow_mut().locations.push(instance.transform.clone());
        self.instances.push(instance);
        
    }

    /// Moves all instances based on their current velocity
    /// 
    /// TODO: Combine into Physics engine
    pub fn instance_motion(&mut self, dt: f64) {
        for instance in &mut self.instances {
            instance.transform.borrow_mut().pos +=
                dt * instance.velocity;
        }
    }

    pub fn iter_positions<F : FnMut(&node::Node)>(&self, mut cb: F) {
        for instance in &self.instances {
            cb(&*instance.transform.borrow())
        }
    }

    pub fn as_entity(&self) -> Rc<RefCell<Entity>>
    {
        self.entity.clone()
    }
}

