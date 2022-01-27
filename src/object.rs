use crate::graphics_engine::model;
use crate::cg_support::node;
use crate::graphics_engine::entity::*;
use std::rc::Rc;
use std::cell::RefCell;
use crate::collisions;
use crate::graphics_engine::shader;
use crate::physics::*;
use crate::cg_support::Transformation;

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum ObjectType {
    Laser, Ship, Asteroid, Any
}

/// A game object that only stores a model
/// and gives access to model animation 
pub struct AnimGameObject {
    pub data: RigidBody<ObjectType>,
    entity: Rc<RefCell<ModelEntity>>,
}

impl AnimGameObject {
    #[allow(dead_code)]
    pub fn new(model: model::Model) -> Self {
        AnimGameObject::from(model, node::Node::default())
    }

    pub fn from(model: model::Model, transform: node::Node) -> Self {
        let transform = Rc::new(RefCell::new(transform));
        Self {
            data: RigidBody::new(transform.clone(), None, BodyType::Dynamic, ObjectType::Any),
            entity: Rc::new(RefCell::new(ModelEntity {
                geometry: Box::new(model),
                locations: vec![transform],
                render_passes: vec![shader::RenderPassType::Visual, 
                    shader::RenderPassType::transparent_tag()],
            })),
        }
    }

    /// Indicates the entity should be rendered during a depth pass
    #[inline]
    pub fn with_depth(self) -> Self {
        self.entity.borrow_mut().render_passes.push(shader::RenderPassType::Depth);
        self
    }

    /// Indicates the object cannot move
    #[inline]
    #[allow(dead_code)]
    pub fn immobile(mut self) -> Self {
        self.data.body_type = BodyType::Static;
        self
    }

    /// Starts the animation with the given name
    /// See `Animator`
    #[inline(always)]
    pub fn start_anim(&mut self, name: &str, do_loop: bool) {
        (&mut *self.entity.borrow_mut()).geometry.get_animator().start(name, do_loop)
    }

    #[inline(always)]
    pub fn as_entity(&self) -> Rc<RefCell<ModelEntity>> {
        self.entity.clone()
    }

    /// Gets the transform of this object
    #[inline(always)]
    pub fn transform(&self) -> &Rc<RefCell<node::Node>>
    {
        &self.data.transform
    }
}

/// A game object is a renderable entity that has collision information and
/// a node in the scene graph
/// A game object is a flyweight
pub struct GameObject {
    instances: Vec<RigidBody<ObjectType>>,
    entity: Rc<RefCell<Entity>>,
    collision_prototype: Option<collisions::CollisionObject>,
    bod_type: BodyType,
    typ: ObjectType,
}

impl GameObject {
    /// Creates a new game object with the specified graphics model
    pub fn new(model: model::Model, typ: ObjectType) -> Self {
        Self {
            instances: Vec::<RigidBody<ObjectType>>::new(),
            entity: Rc::new(RefCell::new(Entity {
                geometry: Box::new(model),
                locations: Vec::new(),
                render_passes: vec![shader::RenderPassType::Visual,
                    shader::RenderPassType::transparent_tag()],
            })),
            collision_prototype: None,
            bod_type: BodyType::Dynamic,
            typ,
        }
    }

    /// Enables this object to interact with other rigid bodies
    pub fn with_collisions(mut self, collision_mesh: &str, tree_args: collisions::TreeStopCriteria) -> Self {
        self.collision_prototype = Some(collisions::CollisionObject::prototype(collision_mesh, tree_args));
        for body in &mut self.instances {
            body.collider = Some(
                collisions::CollisionObject::from(body.transform.clone(), self.collision_prototype.as_ref().unwrap()));
        }
        self
    }

    /// Enables this object to be rendered during a depth pass
    #[inline]
    pub fn with_depth(self) -> Self {
        self.entity.borrow_mut().render_passes.push(shader::RenderPassType::Depth);
        self
    }

    /// Sets the initial position of an instance of this object
    pub fn at_pos(mut self, transform: node::Node) -> Self {
        let transform = Rc::new(RefCell::new(transform));
        self.entity.borrow_mut().locations.push(transform.clone());
        self.instances.push(RigidBody::new(transform.clone(),
            self.collision_prototype.as_ref().map(|x| collisions::CollisionObject::from(transform, x)),
            self.bod_type, self.typ));
        self
    }

    /// Creates a new instance of this object
    pub fn new_instance(&mut self, transform: node::Node, initial_vel: Option<cgmath::Vector3<f64>>) {
        let transform = Rc::new(RefCell::new(transform));
        self.entity.borrow_mut().locations.push(transform.clone());
        self.instances.push(RigidBody::new(transform.clone(),
            self.collision_prototype.as_ref().map(|x| collisions::CollisionObject::from(transform, x)),
            self.bod_type, self.typ));  
        if let Some(vel) = initial_vel {
            self.instances.last_mut().unwrap().velocity = vel;
        }     
    }

    /// Makes the object unmoveable in physics simulations
    pub fn immobile(mut self) -> Self {
        self.bod_type = BodyType::Static;
        for body in &mut self.instances {
            body.body_type = self.bod_type;
        }
        self
    }

    pub fn iter_positions<F : FnMut(&node::Node)>(&self, mut cb: F) {
        for instance in &self.instances {
            cb(&*instance.transform.borrow())
        }
    }

    #[inline(always)]
    pub fn as_entity(&self) -> Rc<RefCell<Entity>>
    {
        self.entity.clone()
    }

    /// Assumes this object has at least one instance, gets the transform of the first instance
    /// Helper function for when the game object only represents on object
    #[inline(always)]
    #[allow(dead_code)]
    pub fn transform(&self) -> &Rc<RefCell<node::Node>>
    {
        &self.instances[0].transform
    }

    /// Gets a mutable reference to the rigid body at index `idx`
    /// Requires there are more instances than `idx`
    #[inline(always)]
    pub fn body(&mut self, idx: usize) -> &mut RigidBody<ObjectType>
    {
        &mut self.instances[idx]
    }

    /// Gets a vector of mutable references to the rigid bodies
    #[inline(always)]
    pub fn bodies_ref(&mut self) -> Vec<&mut RigidBody<ObjectType>> {
        self.instances.iter_mut().collect()
    }

    #[inline(always)]
    pub fn bodies(&mut self) -> &mut Vec<RigidBody<ObjectType>> {
        &mut self.instances
    }

    /// Retains all instances (both visual and rigid body) whose transformation pointer satisfies the given
    /// predicate
    /// 
    /// `pred` - takes a pointer to the object transformation and returns `false` to remove it
    pub fn retain<T : Fn(*const ()) -> bool>(&mut self, pred: T) {
        self.instances.retain(|body| pred(body.transform.as_ptr() as *const ()));
        // *const () to compare fat and thin pointers
        self.entity.borrow_mut().locations.retain(|model| {
            let r = pred(model.as_ptr() as *const ());
            r
        });
    }
}

