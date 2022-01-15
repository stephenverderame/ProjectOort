use crate::cg_support::node;
use cgmath::*;
use std::rc::{Rc, Weak};
use super::octree::ONode;
use std::cell::{RefCell};
use super::collision_mesh;

/// Internal use to collisions module
pub struct Object {
    pub model: Rc<RefCell<node::Node>>,
    pub local_radius: f64,
    pub octree_cell: Weak<RefCell<ONode>>,
    pub mesh: Weak<RefCell<collision_mesh::CollisionMesh>>,
}

impl Object {
    pub fn center(&self) -> Point3<f64> {
        //let p = self.model.borrow().pos.to_vec() + self.local_center.to_vec();
        //point3(p.x, p.y, p.z)
        self.model.borrow().mat().transform_point(point3(0., 0., 0.))
        // center in local coordinates always 0, 0, 0 to be rotationally invariant
    }

    pub fn radius(&self) -> f64 {
        let model = self.model.borrow();
        let max_extents = model.scale.x.max(model.scale.y.max(model.scale.z));
        self.local_radius * max_extents
    }

    /// Testing helper function to make a new object
    #[allow(dead_code)]
    pub fn new(transform: Rc<RefCell<node::Node>>, radius: f64) -> Object {
        Object {
            model: transform,
            local_radius: radius,
            octree_cell: Weak::new(),
            mesh: Weak::new(),
        }
    }

    /// Constructs a new object with the given transformation and mesh
    /// 
    /// `radius` - the maximum extents of the mesh based around `center`
    pub fn with_mesh(transform: Rc<RefCell<node::Node>>, center: Point3<f64>, radius: f64,
        mesh: &Rc<RefCell<collision_mesh::CollisionMesh>>) -> Object {
        Object {
            model: transform,
            local_radius: radius + center.x.max(center.y.max(center.z)),
            octree_cell: Weak::new(),
            mesh: Rc::downgrade(mesh),
        }
    }

    /// True if this object's bounding sphere overlaps with `other`'s bounding sphere
    pub fn bounding_sphere_collide(&self, other: &Object) -> bool {
        let dist2 = (self.center() - other.center()).dot(self.center() - other.center());
        (self.radius() + other.radius()).powi(2) >= dist2 
    }
}

impl std::fmt::Debug for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Object")
            .field("radius", &self.local_radius)
            .finish()
    }
}