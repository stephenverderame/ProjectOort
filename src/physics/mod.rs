mod rigid_body;
mod simulation;
mod forces;

pub use rigid_body::*;
pub use simulation::Simulation;
pub use forces::*;
use cgmath::*;
use std::rc::Weak;
use std::cell::RefCell;
use crate::node;

/// Something that can apply a force on a rigid body
pub trait Forcer {
    /// If this forcer affects `body`, then gets the point of force application in world coordinates
    /// and the force vector
    /// 
    /// Otherwise `None`
    fn get_force(&self, body: &BaseRigidBody) -> Option<(Point3<f64>, Vector3<f64>)>;
}

pub struct Tether {
    pub a: Weak<RefCell<node::Node>>,
    /// Local space
    pub attach_a: Point3<f64>,
    pub b: Weak<RefCell<node::Node>>,
    /// Local space
    pub attach_b: Point3<f64>,
    pub length: f64,
}

impl Tether {
    /// `true` if the tether is taught (at or beyond its maximum length)
    #[allow(unused)]
    pub fn is_taught(&self) -> bool {
        if let (Some(a), Some(b)) = (self.a.upgrade(), self.b.upgrade()) {
            (a.borrow().transform_point(self.attach_a) 
                - b.borrow().transform_point(self.attach_b)).magnitude() 
                > self.length
        } else { false }
    }
}
