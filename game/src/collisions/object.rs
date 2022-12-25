use super::collision_mesh;
use super::octree::ONode;
use crate::cg_support::node;
use cgmath::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

/// Internal use to collisions module
pub(super) struct Object {
    pub(super) model: Rc<RefCell<node::Node>>,
    last_radius: Cell<f64>,
    local_center: Point3<f64>,
    last_scale: Cell<Option<Vector3<f64>>>,
    pub(super) octree_cell: Weak<RefCell<ONode>>,
    pub(super) mesh: Weak<collision_mesh::CollisionMesh>,
}

impl Object {
    pub fn center(&self) -> Point3<f64> {
        //let p = self.model.borrow().pos.to_vec() + self.local_center.to_vec();
        //point3(p.x, p.y, p.z)
        self.model.borrow().transform_point(self.local_center)
        // center in local coordinates always 0, 0, 0 to be rotationally invariant
    }

    pub fn radius(&self) -> f64 {
        let model = self.model.borrow();
        match self.last_scale.get() {
            Some(scale)
                if scale.abs_diff_eq(&model.get_scale(), f64::EPSILON) =>
            {
                self.last_radius.get()
            }
            _ => self.mesh.upgrade().map_or_else(
                || self.last_radius.get(),
                |mesh| {
                    let (_, radius) = mesh.bounding_sphere(&model.get_scale());
                    self.last_radius.set(radius);
                    self.last_scale.set(Some(model.get_scale()));
                    radius
                },
            ),
        }
    }

    #[cfg(test)]
    pub fn radius_mut(&mut self) -> &mut f64 {
        self.last_radius.get_mut()
    }

    /// Testing helper function to make a new object
    #[allow(dead_code)]
    pub fn new(transform: Rc<RefCell<node::Node>>, radius: f64) -> Self {
        Self {
            model: transform,
            last_radius: Cell::new(radius),
            last_scale: Cell::new(None),
            local_center: Point3::new(0.0, 0.0, 0.0),
            octree_cell: Weak::new(),
            mesh: Weak::new(),
        }
    }

    pub fn from_prototype(
        model: &Rc<RefCell<node::Node>>,
        prototype: &Self,
    ) -> Self {
        Self {
            model: model.clone(),
            last_radius: Cell::new(prototype.last_radius.get()),
            last_scale: Cell::new(prototype.last_scale.get()),
            local_center: prototype.local_center,
            octree_cell: Weak::new(),
            mesh: prototype.mesh.clone(),
        }
    }

    /// Constructs a new object with the given transformation and mesh
    ///
    /// `radius` - the maximum extents of the mesh based around `center`
    pub fn with_mesh(
        transform: Rc<RefCell<node::Node>>,
        mesh: &Rc<collision_mesh::CollisionMesh>,
    ) -> Self {
        let scale = transform.borrow().get_scale();
        let (local_center, last_radius) = mesh.bounding_sphere(&scale);
        Self {
            model: transform,
            last_radius: Cell::new(last_radius),
            local_center,
            last_scale: Cell::new(Some(scale)),
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
        let dist = self.center().distance(other.center());
        self.radius() + other.radius() >= dist
    }
}

impl std::fmt::Debug for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Object")
            .field("radius", &self.radius())
            .finish()
    }
}
