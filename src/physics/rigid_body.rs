use std::rc::Rc;
use std::cell::RefCell;
use crate::collisions;
use crate::cg_support::*;
use cgmath::*;

#[derive(PartialEq, Eq, Copy, Clone, Hash)]
pub enum BodyType {
    Static, Dynamic
}

#[derive(PartialEq, Eq, Hash, Copy, Clone)]
pub enum CollisionMethod {
    Triangle
}

pub struct RigidBody<T> {
    pub transform: Rc<RefCell<node::Node>>,
    pub velocity: cgmath::Vector3<f64>,
    pub rot_vel: Quaternion<f64>,
    pub collider: Option<collisions::CollisionObject>,
    pub body_type: BodyType,
    pub col_type: CollisionMethod,
    pub metadata: T,
}

impl<T> RigidBody<T> {
    pub fn new(transform: Rc<RefCell<node::Node>>, collider: Option<collisions::CollisionObject>,
        body_type: BodyType, metadata: T) -> Self
    {
        Self {
            transform,
            collider,
            velocity: vec3(0., 0., 0.),
            rot_vel: Quaternion::new(1., 0., 0., 0.),
            body_type,
            col_type: CollisionMethod::Triangle,
            metadata
        }
    }

    /// Get's the world space center of this rigid body
    pub fn center(&self) -> Point3<f64> {
        if let Some(collider) = &self.collider {
            collider.bounding_sphere().0
        } else {
            self.transform.borrow().mat().transform_point(point3(0., 0., 0.))
        }
    }
}