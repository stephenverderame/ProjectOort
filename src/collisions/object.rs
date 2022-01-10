use crate::node;
use cgmath::*;
use std::rc::{Rc, Weak};
use super::octree::ONode;
use std::cell::{RefCell};
use super::collision_mesh;

/// Internal use to collisions module
pub struct Object {
    pub model: Rc<RefCell<node::Node>>,
    pub local_center: Point3<f64>,
    pub local_radius: f64,
    pub octree_cell: Weak<RefCell<ONode>>,
    pub mesh: Weak<RefCell<collision_mesh::CollisionMesh>>,
}

impl Object {
    pub fn center(&self) -> Point3<f64> {
        self.model.borrow().mat().transform_point(self.local_center)
    }

    pub fn radius(&self) -> f64 {
        let model = self.model.borrow();
        let max_extents = model.scale.x.max(model.scale.y.max(model.scale.z));
        self.local_radius * max_extents
    }

    pub fn new(transform: Rc<RefCell<node::Node>>, center: Point3<f64>, radius: f64) -> Object {
        Object {
            model: transform,
            local_center: center,
            local_radius: radius,
            octree_cell: Weak::new(),
            mesh: Weak::new(),
        }
    }

    pub fn with_mesh(transform: Rc<RefCell<node::Node>>, center: Point3<f64>, radius: f64,
        mesh: &Rc<RefCell<collision_mesh::CollisionMesh>>) -> Object {
        Object {
            model: transform,
            local_center: center,
            local_radius: radius,
            octree_cell: Weak::new(),
            mesh: Rc::downgrade(mesh),
        }
    }
}

impl std::fmt::Debug for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Object")
            .field("center", &self.local_center)
            .field("radius", &self.local_radius)
            .finish()
    }
}