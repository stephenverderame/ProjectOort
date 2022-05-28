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

/// Something that can apply a force on, or manipulate, one or more rigid bodies
/// 
/// Imperative interface and more general than a force
pub trait Manipulator<T> {
    /// Manipulates the rigid bodies in some way
    /// 
    /// `bodies` - mutable slice of all Rigid Bodies
    /// 
    /// `body_indices` - a map with keys of pointers for rigid body nodes
    /// and values of its index in the `bodies` slice
    fn affect_bodies(&self, bodies: &mut [&mut RigidBody<T>], 
        body_indices: &std::collections::HashMap<*const node::Node, u32>,
        dt: std::time::Duration);
}

/// Abstracts a force under a Manipulator interface
pub struct ForceManipulator<T> {
    forces: Vec<Box<dyn Forcer>>,
    _m: std::marker::PhantomData<T>,
}

impl<T> ForceManipulator<T> {
    /// Creates a new manipulator from multiple forces
    #[allow(unused)]
    pub fn new(forces: Vec<Box<dyn Forcer>>) -> Self {
        ForceManipulator { forces, _m: std::marker::PhantomData{} }
    }

    /// Creates a new manipulator from a single force
    #[allow(unused)]
    pub fn new_single(force: Box<dyn Forcer>) -> Self {
        ForceManipulator {forces: vec![force], _m: std::marker::PhantomData{} }
    }
}

/// Gets the difference in linear and angular velocity due to the force
fn delta_vels_from_force(pt: &Point3<f64>, force: &Vector3<f64>, 
    body: &BaseRigidBody, dt: f64) -> (Vector3<f64>, Vector3<f64>)
{
    let accl = force / body.mass;

    let r = pt - body.center();
    let torque = r.cross(*force);

    let angular_accl = body.moment_inertia().invert().unwrap() 
        * torque;

    (accl * dt, angular_accl * dt)
}

impl<T> Manipulator<T> for ForceManipulator<T> {


    fn affect_bodies(&self, bodies: &mut [&mut RigidBody<T>], 
        _body_indices: &std::collections::HashMap<*const node::Node, u32>,
        dt: std::time::Duration)
    {
        let dt = dt.as_secs_f64();
        for bod in bodies.iter_mut().filter(|bod| 
            bod.base.body_type != BodyType::Static) 
        {
            for f in &self.forces {
                if let Some((pt, force)) = f.get_force(&bod.base) {
                    let (delta_v, delta_a_v) = 
                        delta_vels_from_force(&pt, &force, &bod.base, dt);
                    bod.base.velocity += delta_v;
                    bod.base.rot_vel += delta_a_v;
                    
                }
            }
        }
    }
}

pub struct TetherData {
    pub a: Weak<RefCell<node::Node>>,
    /// Local space
    pub attach_a: Point3<f64>,
    pub b: Weak<RefCell<node::Node>>,
    /// Local space
    pub attach_b: Point3<f64>,
    pub length: f64,
}

pub struct Tether<T> {
    pub data: TetherData,
    _m: std::marker::PhantomData<T>,
}

impl<T> Tether<T> {
    pub fn new(data: TetherData) -> Self {
        Self { data, _m: std::marker::PhantomData{} }
    }

    /// `true` if the tether is taught (at or beyond its maximum length)
    #[allow(unused)]
    pub fn is_taught(&self) -> bool {
        if let (Some(a), Some(b)) = (self.data.a.upgrade(), self.data.b.upgrade()) {
            (a.borrow().transform_point(self.data.attach_a) 
                - b.borrow().transform_point(self.data.attach_b)).magnitude() 
                > self.data.length
        } else { false }
    }
}

/// Projection coefficient of `v` projected onto `u`
/// 
/// The vector would be this coefficient times `u`
fn project_onto(v: Vector3<f64>, u: Vector3<f64>) -> f64 {
    v.dot(u) / u.dot(u)
}

/// Gets the r vector, the projection of `body_a`'s velocity onto that vector,
/// the projection of `body_b`'s velocity onto that vector and the total mass
/// of both bodies
fn get_r_projections_mass<T>(body_a: &RigidBody<T>, body_b: &RigidBody<T>, 
    tether_length: f64) -> Option<(Vector3<f64>, f64, f64, f64)>
{
    /*let attach_a_world = body_a.base.transform.borrow()
        .transform_point(t.attach_a);
    let attach_b_world = body_b.base.transform.borrow()
        .transform_point(t.attach_b);*/
    let a_to_b = body_b.base.center() - body_a.base.center();
    if a_to_b.magnitude() < tether_length { None }
    else {
        let a_to_b = a_to_b.normalize();
        let t_a = project_onto(body_a.base.velocity, a_to_b);
        let t_b = project_onto(body_b.base.velocity, a_to_b);
        Some((a_to_b, t_a, t_b, body_a.base.mass + body_b.base.mass))
    }
}

impl<T> Manipulator<T> for Tether<T> {

    fn affect_bodies(&self, objs: &mut [&mut RigidBody<T>], 
        body_indices: &std::collections::HashMap<*const node::Node, u32>,
        _dt: std::time::Duration)
    {
        let t = &self.data;
        if let (Some(a), Some(b)) = (t.a.upgrade(), t.b.upgrade()) {
            let a_idx = body_indices[&(a.as_ptr() as *const _)] as usize;
            let b_idx = body_indices[&(b.as_ptr() as *const _)] as usize;
            if let Some((a_to_b, t_a, t_b, total_mass)) = 
                get_r_projections_mass(&objs[a_idx], &objs[b_idx], t.length) 
            {
                let mut total_parallel_p = vec3(0., 0., 0.);
                let calc_momentum = |body : &mut RigidBody<T>, t| {
                    let v = t * a_to_b;
                    body.base.velocity -= v;
                    v * body.base.mass
                };
                if t_a < 0. {
                    total_parallel_p += calc_momentum(&mut objs[a_idx], t_a);
                }
                if t_b > 0. {
                    total_parallel_p += calc_momentum(&mut objs[b_idx], t_b);
                }
                total_parallel_p /= total_mass;
                objs[a_idx].base.velocity += total_parallel_p;
                objs[b_idx].base.velocity += total_parallel_p;
            }

        }
    }
}
