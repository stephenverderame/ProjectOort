use cgmath::*;
use std::rc::{Weak, Rc};
use std::cell::RefCell;
use crate::node;

#[derive(PartialEq, Eq)]
pub enum RestoringMode {
    #[allow(unused)]
    Spring,
    /// Imparts no restoring force if compressed
    String
}

pub struct Spring {
    pub k: f64,
    /// attach points are in local coordinates
    pub attach_pt_a: Point3<f64>,
    /// local coordinates
    pub attach_pt_b: Point3<f64>,
    pub obj_a_ptr: Weak<RefCell<node::Node>>,
    pub obj_b_ptr: Weak<RefCell<node::Node>>,
    pub mode: RestoringMode,
    pub natural_length: f64,
}

impl Spring {
    /// Gets the force vector acting on object a, and the force vector acting on object b
    /// 
    /// Forces can be 0. `None` if one of the attached objects are destroyed
    pub fn force(&self) -> Option<(Vector3<f64>, Vector3<f64>)> {
        self.obj_a_ptr.upgrade().and_then(|obj_a_ptr| {
            self.obj_b_ptr.upgrade().map(|obj_b_ptr| {
                let a_to_b = obj_b_ptr.borrow().transform_point(self.attach_pt_b) -
                    obj_a_ptr.borrow().transform_point(self.attach_pt_a);
                let x = a_to_b.magnitude();
                if self.mode == RestoringMode::String && x <= self.natural_length {
                    (vec3(0., 0., 0.), vec3(0., 0., 0.))
                } else {
                    let kx = self.k * (x - self.natural_length);
                    (a_to_b.normalize() * kx, a_to_b.normalize() * -kx)
                }
            })
        })
    }
}

impl super::Forcer for Spring {

    fn get_force(&self, body: &super::BaseRigidBody) 
        -> Option<(Point3<f64>, Vector3<f64>)> 
    {
        let a_ptr = self.obj_a_ptr.upgrade();
        let b_ptr = self.obj_b_ptr.upgrade();
        let (force_a, force_b) = self.force().unwrap_or((vec3(0., 0., 0.), vec3(0., 0., 0.)));
        if a_ptr.is_some() && Rc::ptr_eq(&body.transform, &a_ptr.unwrap()) {
            Some((body.transform.borrow().transform_point(self.attach_pt_a), force_a))
        } else if b_ptr.is_some() && Rc::ptr_eq(&body.transform, &b_ptr.unwrap()) {
            Some((body.transform.borrow().transform_point(self.attach_pt_b), force_b))
        } else {
            None
        }
    }
}

pub struct Centripetal {
    /// attach points are in local coordinates
    pub attach_pt_a: Point3<f64>,
    /// local coordinates
    pub attach_pt_b: Point3<f64>,
    pub obj_a_ptr: Weak<RefCell<node::Node>>,
    pub obj_b_ptr: Weak<RefCell<node::Node>>,
    pub natural_length: f64,
}

impl super::Forcer for Centripetal {
    fn get_force(&self, body: &super::BaseRigidBody) 
        -> Option<(Point3<f64>, Vector3<f64>)> 
    {
        let a_ptr = self.obj_a_ptr.upgrade();
        let b_ptr = self.obj_b_ptr.upgrade();
        if a_ptr.is_none() || b_ptr.is_none() { return None }
        let a = a_ptr.unwrap();
        let b = b_ptr.unwrap();
        if Rc::ptr_eq(&body.transform, &a) {
            let r = b.borrow().transform_point(self.attach_pt_b) - 
                a.borrow().transform_point(self.attach_pt_a);
            let force = body.mass * body.velocity.magnitude2() / 
                r.magnitude() * r.normalize();
            Some((body.transform.borrow().transform_point(self.attach_pt_a), force))
        } else if Rc::ptr_eq(&body.transform, &b) {
            let r = a.borrow().transform_point(self.attach_pt_a) - 
                b.borrow().transform_point(self.attach_pt_b);
            let force = body.mass * body.velocity.magnitude2() / 
                r.magnitude() * r.normalize();
            Some((body.transform.borrow().transform_point(self.attach_pt_b), force))
        } else {
            None
        }
    }
}