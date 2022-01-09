use crate::node;
use cgmath::*;
use std::rc::{Rc, Weak};
use super::octree::ONode;
use std::cell::{RefCell};
use super::obb::*;

/// Internal use to collisions module
pub struct Object {
    pub model: Rc<RefCell<node::Node>>,
    pub local_center: Point3<f64>,
    pub local_radius: f64,
    pub octree_cell: Weak<RefCell<ONode>>,
    pub local_obb: AABB
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

    pub fn from<T : BaseNum>(transform: Rc<RefCell<node::Node>>, points: &[Point3<T>]) -> Object {
        let obb = AABB::from(points);
        let radius = obb.extents.x.max(obb.extents.y.max(obb.extents.z));
        Object {
            model: transform,
            local_center: obb.center,
            local_radius: radius,
            octree_cell: Weak::new(),
            local_obb: obb
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