mod forces;
mod rigid_body;
mod simulation;

use crate::node;
use cgmath::*;
pub use forces::*;
pub use rigid_body::*;
pub use simulation::Simulation;
use std::cell::RefCell;
use std::rc::Weak;

/// Data to resolve a collision
/// Sum of all collision resolving forces for a single object
/// Allows a single object to collide with multiple other objects
#[derive(Clone)]
pub struct CollisionResolution {
    vel: Vector3<f64>,
    rot: Vector3<f64>,
    is_collide: bool,
}

impl CollisionResolution {
    const fn identity() -> Self {
        Self {
            vel: vec3(0., 0., 0.),
            rot: vec3(0., 0., 0.),
            is_collide: false,
        }
    }

    /// Performs a collision between `body` and `colliding_body`
    /// where this instance is the resolution object for `body`
    fn do_collision<T>(
        &mut self,
        norm: Vector3<f64>,
        pt: Point3<f64>,
        body: &RigidBody<T>,
        colliding_body: &RigidBody<T>,
    ) {
        let body = &body.base;
        let colliding_body = &colliding_body.base;
        let body_inertia = body.moment_inertia();
        let colliding_inertia = colliding_body.moment_inertia();
        let body_lever = pt - body.center();
        let colliding_lever = pt - colliding_body.center();

        let m_eff = 1.0 / body.mass + 1.0 / colliding_body.mass;
        let impact_speed = norm.dot(body.velocity - colliding_body.velocity);
        let impact_angular_speed = body_lever.cross(norm).dot(body.rot_vel)
            - colliding_lever.cross(norm).dot(colliding_body.rot_vel);
        let body_angular_denom_term = body_lever
            .cross(norm)
            .dot(body_inertia.invert().unwrap() * body_lever.cross(norm));
        let colliding_angular_denom_term = colliding_lever.cross(norm).dot(
            colliding_inertia.invert().unwrap() * colliding_lever.cross(norm),
        );
        let impulse = 1.52 * (impact_speed + impact_angular_speed)
            / (m_eff + body_angular_denom_term + colliding_angular_denom_term);
        // (1 + coeff of resitution) * effective mass * impact speed
        // impulse = kg * m/s = Ns
        self.vel -= impulse / body.mass * norm;
        self.rot +=
            body_inertia.invert().unwrap() * (impulse * norm).cross(body_lever);
    }

    /// Performs a collision between `body` and `colliding_body`
    /// if the two objects are positioned in such a way to allow one
    ///
    /// Uses a different collision resolution method if objects are overlapping
    fn add_collision<T>(
        &mut self,
        norm: Vector3<f64>,
        pt: Point3<f64>,
        body: &RigidBody<T>,
        colliding_body: &RigidBody<T>,
    ) {
        // Assumes eleastic collisions
        let relative_vel = body.base.velocity - colliding_body.base.velocity;
        let v = relative_vel.dot(norm) * norm;
        if colliding_body.base.center().dot(v) / v.dot(v)
            > body.base.center().dot(v) / v.dot(v)
        {
            //self.vel -= v;
            self.do_collision(norm, pt, body, colliding_body);
        }
        if body.base.velocity.magnitude() < 0.00001
            && colliding_body.base.velocity.magnitude() < 0.00001
        {
            // two objects spawned in a collission
            self.vel -= norm * 5.;
        }
        if !self.is_collide {
            self.is_collide = true;
            //self.rot = body.rot_vel.invert();
        }
    }

    /// Adds a manual resolution to this collider by incrementing the velocities by the given values
    fn add_vel_change(&mut self, vel: Vector3<f64>, rot: Option<Vector3<f64>>) {
        self.vel += vel;
        self.is_collide = true;
        if let Some(rot) = rot {
            self.rot += rot;
        }
    }
}

/// Something that can apply a force on a rigid body
pub trait Forcer {
    /// If this forcer affects `body`, then gets the point of force application in world coordinates
    /// and the force vector
    ///
    /// Otherwise `None`
    fn get_force(
        &self,
        body: &BaseRigidBody,
    ) -> Option<(Point3<f64>, Vector3<f64>)>;
}

/// Something that can apply a force on, or manipulate, one or more rigid bodies
///
/// Imperative interface and more general than a force
pub trait Manipulator<T> {
    /// Manipulates the rigid bodies in some way
    ///
    /// `bodies` - slice of all Rigid Bodies
    ///
    /// `resolvers` - mutable slice of all Rigid Body Movement resolutions
    ///     Corresponding indices of resolvers and bodies should match
    ///
    /// `body_indices` - a map with keys of pointers for rigid body nodes
    /// and values of its index in the `bodies` slice
    fn affect_bodies(
        &self,
        bodies: &[&RigidBody<T>],
        resolvers: &mut [CollisionResolution],
        body_indices: &std::collections::HashMap<*const node::Node, u32>,
        dt: std::time::Duration,
    );
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
        Self {
            forces,
            _m: std::marker::PhantomData {},
        }
    }

    /// Creates a new manipulator from a single force
    #[allow(unused)]
    pub fn new_single(force: Box<dyn Forcer>) -> Self {
        Self {
            forces: vec![force],
            _m: std::marker::PhantomData {},
        }
    }
}

/// Gets the difference in linear and angular velocity due to the force
fn delta_vels_from_force(
    pt: &Point3<f64>,
    force: &Vector3<f64>,
    body: &BaseRigidBody,
    dt: f64,
) -> (Vector3<f64>, Vector3<f64>) {
    let accl = force / body.mass;

    let r = pt - body.center();
    let torque = r.cross(*force);

    let angular_accl = body.moment_inertia().invert().unwrap() * torque;

    (accl * dt, angular_accl * dt)
}

impl<T> Manipulator<T> for ForceManipulator<T> {
    fn affect_bodies(
        &self,
        bodies: &[&RigidBody<T>],
        resolvers: &mut [CollisionResolution],
        _body_indices: &std::collections::HashMap<*const node::Node, u32>,
        dt: std::time::Duration,
    ) {
        let dt = dt.as_secs_f64();
        let len = bodies.len();
        for (bod, idx) in bodies
            .iter()
            .zip(0..len)
            .filter(|(bod, _)| bod.base.body_type != BodyType::Static)
        {
            for f in &self.forces {
                if let Some((pt, force)) = f.get_force(&bod.base) {
                    let (delta_v, delta_a_v) =
                        delta_vels_from_force(&pt, &force, &bod.base, dt);
                    resolvers[idx].add_vel_change(delta_v, Some(delta_a_v));
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
    pub const fn new(data: TetherData) -> Self {
        Self {
            data,
            _m: std::marker::PhantomData {},
        }
    }

    /// `true` if the tether is taught (at or beyond its maximum length)
    #[allow(unused)]
    pub fn is_taught(&self) -> bool {
        if let (Some(a), Some(b)) =
            (self.data.a.upgrade(), self.data.b.upgrade())
        {
            (a.borrow().transform_point(self.data.attach_a)
                - b.borrow().transform_point(self.data.attach_b))
            .magnitude()
                > self.data.length
        } else {
            false
        }
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
fn get_r_projections_mass<T>(
    body_a: &RigidBody<T>,
    body_b: &RigidBody<T>,
    tether_length: f64,
) -> Option<(Vector3<f64>, f64, f64, f64)> {
    /*let attach_a_world = body_a.base.transform.borrow()
        .transform_point(t.attach_a);
    let attach_b_world = body_b.base.transform.borrow()
        .transform_point(t.attach_b);*/
    let a_to_b = body_b.base.center() - body_a.base.center();
    if a_to_b.magnitude() < tether_length {
        None
    } else {
        let a_to_b = a_to_b.normalize();
        let t_a = project_onto(body_a.base.velocity, a_to_b);
        let t_b = project_onto(body_b.base.velocity, a_to_b);
        Some((a_to_b, t_a, t_b, body_a.base.mass + body_b.base.mass))
    }
}

impl<T> Manipulator<T> for Tether<T> {
    fn affect_bodies(
        &self,
        objs: &[&RigidBody<T>],
        resolvers: &mut [CollisionResolution],
        body_indices: &std::collections::HashMap<*const node::Node, u32>,
        _dt: std::time::Duration,
    ) {
        let t = &self.data;
        if let (Some(a), Some(b)) = (t.a.upgrade(), t.b.upgrade()) {
            let a_idx = body_indices.get(&(a.as_ptr() as *const _));
            let b_idx = body_indices.get(&(b.as_ptr() as *const _));
            if let (Some(a_idx), Some(b_idx)) = (a_idx, b_idx) {
                let a_idx = *a_idx as usize;
                let b_idx = *b_idx as usize;
                if let Some((a_to_b, t_a, t_b, total_mass)) =
                    get_r_projections_mass(objs[a_idx], objs[b_idx], t.length)
                {
                    let mut total_parallel_p = vec3(0., 0., 0.);
                    let calc_momentum =
                    |body: &RigidBody<T>,
                     t,
                     resolver: &mut CollisionResolution| {
                        let v = t * a_to_b;
                        resolver.add_vel_change(v * -1., None);
                        v * body.base.mass
                    };
                    if t_a < 0. {
                        total_parallel_p += calc_momentum(
                            objs[a_idx],
                            t_a,
                            &mut resolvers[a_idx],
                        );
                    }
                    if t_b > 0. {
                        total_parallel_p += calc_momentum(
                            objs[b_idx],
                            t_b,
                            &mut resolvers[b_idx],
                        );
                    }
                    total_parallel_p /= total_mass;
                    resolvers[a_idx].add_vel_change(total_parallel_p, None);
                    resolvers[b_idx].add_vel_change(total_parallel_p, None);
                }
            }
        }
    }
}
