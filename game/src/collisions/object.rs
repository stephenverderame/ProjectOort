use super::collision_mesh;
use super::octree::ONode;
use crate::cg_support::node;
use cgmath::*;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

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
        self.model
            .borrow()
            .mat()
            .transform_point(point3(0., 0., 0.))
        // center in local coordinates always 0, 0, 0 to be rotationally invariant
    }

    pub fn radius(&self) -> f64 {
        let model = self.model.borrow();
        let scale = model.local_scale();
        let max_extents = scale.x.max(scale.y.max(scale.z));
        self.local_radius * max_extents
    }

    /// Testing helper function to make a new object
    #[allow(dead_code)]
    pub fn new(transform: Rc<RefCell<node::Node>>, radius: f64) -> Self {
        Self {
            model: transform,
            local_radius: radius,
            octree_cell: Weak::new(),
            mesh: Weak::new(),
        }
    }

    /// Constructs a new object with the given transformation and mesh
    ///
    /// `radius` - the maximum extents of the mesh based around `center`
    pub fn with_mesh(
        transform: Rc<RefCell<node::Node>>,
        mesh: &Rc<RefCell<collision_mesh::CollisionMesh>>,
    ) -> Self {
        let (_, local_radius) = mesh.borrow().bounding_sphere();
        Self {
            model: transform,
            local_radius,
            octree_cell: Weak::new(),
            mesh: Rc::downgrade(mesh),
        }
        // local_radius was previously set as radius + center.x.max(center.y.max(center.z)),
        // when `radius` was incorrectly the maximum of the extents

        // I have no idea why I then added the max of the center. Since I didn't
        // leave a comment, I'm guessing it was a mistake but maybe I'm thinking
        // too highly of myself
    }

    /// True if this object's bounding sphere overlaps with `other`'s bounding sphere
    pub fn bounding_sphere_collide(&self, other: &Self) -> bool {
        let dist2 = (self.center() - other.center())
            .dot(self.center() - other.center());
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
