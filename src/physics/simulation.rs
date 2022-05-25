use crate::collisions;
use super::*;
use cgmath::*;
use std::collections::{HashMap};
use collisions::*;
use std::cell::Cell;

/// Data to resolve a collision
/// Sum of all collision resolving forces for a single object
/// Allows a single object to collide with multiple other objects
#[derive(Clone)]
struct CollisionResolution {
    vel: Vector3<f64>,
    rot: Vector3<f64>,
    is_collide: bool,
}

impl CollisionResolution {
    fn identity() -> Self {
        Self {
            vel: vec3(0., 0., 0.),
            rot: vec3(0., 0., 0.),
            is_collide: false,
        }
    }

    fn add_collision<T>(&mut self, norm: Vector3<f64>, pt: Point3<f64>, 
        body: &RigidBody<T>, colliding_body: &RigidBody<T>, dt_sec: f64) 
    {
        // Assumes eleastic collisions
        let relative_vel = body.velocity - colliding_body.velocity;
        let v = relative_vel.dot(norm) * norm;
        if colliding_body.center().dot(v) / v.dot(v) > body.center().dot(v) / v.dot(v) {
            //self.vel -= v;
            let m_eff = 1.0 / (1.0 / body.mass + 1.0 / colliding_body.mass);
            let impact_speed = norm.dot(body.velocity - colliding_body.velocity);
            let impulse = 1.52 * m_eff * impact_speed;
            // (1 + coeff of resitution) * effective mass * impact speed
            self.vel -= impulse / body.mass * norm;

            let force = impulse / dt_sec * norm;
            let lever = pt - body.center();
            let torque = force.cross(lever);
            self.rot += torque / body.mass;
        }
        if body.velocity.magnitude() < 0.001 && colliding_body.velocity.magnitude() < 0.001 {
            // two objects spawned in a collission
            self.vel -= norm * 5.;
        }
        if !self.is_collide {
            self.is_collide = true;
            //self.rot = body.rot_vel.invert();
        }
    }
}

/// A simulation handles the collision detection, resolution, and movement of all objects
pub struct Simulation<'a, 'b, T> {
    obj_tree: collisions::CollisionTree,
    collision_methods: HashMap<CollisionMethod, Box<dyn collisions::HighPCollision>>,
    on_hit: Cell<Option<Box<dyn FnMut(&RigidBody<T>, &RigidBody<T>, &HitData) + 'a>>>,
    do_resolve: Cell<Option<Box<dyn Fn(&RigidBody<T>, &RigidBody<T>, &HitData) -> bool + 'b>>>
}

/// Inserts any uninserted objects into the octree
fn insert_into_octree<T>(tree: &mut collisions::CollisionTree, objs: &[&mut RigidBody<T>]) {
    for o in objs {
        if let Some(collider) = &o.collider {
            if !collider.is_in_collision_tree() {
                tree.insert(collider)
            }
        }
    }
}

fn rot_vel_to_quat(rot_vel: Vector3<f64>, dt: f64) -> Quaternion<f64> {
    let ha = rot_vel * dt * 0.5;
    let mag = ha.magnitude();
    if mag > 1.0 {
        let ha = ha * f64::sin(mag) / mag;
        Quaternion::new(f64::cos(mag), ha.x, ha.y, ha.z)
    } else {
        Quaternion::new(1.0, ha.x, ha.y, ha.z)
    }.normalize()
}

/// Calculates velocity and updates position and orientation of each dynamic body
fn apply_forces<T>(objs: &[&mut RigidBody<T>], dt: f64) {
    //println!("{:?}", objs[100].rot_vel);
    for obj in objs {
        if obj.body_type != BodyType::Static {
            {
                let mut t = obj.transform.borrow_mut();
                (&mut *t).pos += obj.velocity * dt;
                let rot = (*t).orientation;
                (&mut *t).orientation = rot * rot_vel_to_quat(obj.rot_vel, dt);
            }
            obj.collider.as_ref().map(|x| x.update_in_collision_tree());

        }
    }
}

/// Uses `resolvers` to update position and rotation based on collisions
fn resolve_forces<T>(objects: &mut [&mut RigidBody<T>], resolvers: Vec<CollisionResolution>, dt: f64) {
    for (resolver, body_idx) in resolvers.into_iter().zip(0 .. objects.len())
        .filter(|(resolver, _)| resolver.is_collide) 
    {
        let obj = &mut objects[body_idx];
        if obj.body_type == BodyType::Controlled {
            obj.transform.borrow_mut().pos += resolver.vel * dt;
            let rot = obj.transform.borrow().orientation;
            obj.transform.borrow_mut().orientation = rot * rot_vel_to_quat(resolver.rot / obj.mass * -0.05, dt);
        } else {
            obj.velocity += resolver.vel;
            obj.rot_vel += resolver.rot / obj.mass;
        }

    }
}

/// Updates every dynamic object in `objs` in the octree
fn update_octree<T>(objs: &[&mut RigidBody<T>]) {
    for o in objs {
        if let Some(collider) = &o.collider {
            if o.body_type == BodyType::Dynamic {
                collider.update_in_collision_tree();
            }
        }
    }
}

impl<'a, 'b, T> Simulation<'a, 'b, T> {
    pub fn new(scene_center: cgmath::Point3<f64>, scene_size: f64) -> Simulation::<'static, 'static, T> {
        let mut collision_methods : HashMap<CollisionMethod, Box<dyn collisions::HighPCollision>>
            = HashMap::new();
        collision_methods.insert(CollisionMethod::Triangle, Box::new(collisions::TriangleTriangleGPU::from_active_ctx()));
        Simulation {
            obj_tree: collisions::CollisionTree::new(scene_center, scene_size),
            collision_methods,
            on_hit: Cell::default(),
            do_resolve: Cell::default(),
        }
    }

    /// Adds a hit callback to this simulation
    /// 
    /// `f` - function which takes `rigid_body_a`, `rigid_body_b`, and Hit data where `pos_norm_a` is the position and contact normal
    /// on `rigid_body_a` and returns `true` if the simulation should do physical collision resolution
    pub fn with_on_hit<'c, F : FnMut(&RigidBody<T>, &RigidBody<T>, &HitData) + 'c>(self, f: F) -> Simulation::<'c, 'b, T> {
        Simulation {
            obj_tree: self.obj_tree,
            collision_methods: self.collision_methods,
            on_hit: Cell::new(Some(Box::new(f))),
            do_resolve: self.do_resolve,
        }
    }

    /// Adds a hit callback that returns `true` if we should do collision resolution or `false` if we shouldn't
    /// @see `with_on_hit`
    pub fn with_do_resolve<'c, 
        F : Fn(&RigidBody<T>, &RigidBody<T>, &HitData) -> bool + 'c>
        (self, f : F) -> Simulation::<'a, 'c, T> 
        {
            Simulation {
                obj_tree: self.obj_tree,
                collision_methods: self.collision_methods,
                on_hit: self.on_hit,
                do_resolve: Cell::new(Some(Box::new(f))),
            }
        }

    /// Adds a collision to `body` from `resolver`
    /// 
    /// `body` and `resolver` are the body and resolver for the rigid body whose contact point and normal
    /// is stored in `pos_norm_a`
    fn add_collision(&self, resolver: &mut CollisionResolution, body: &RigidBody<T>, 
        other_body: &RigidBody<T>, data: HitData, dt_sec: f64)
    {
        let mut func = self.on_hit.take();
        if let Some(cb) = func.as_mut() {
            cb(body, other_body, &data);
        } 
        self.on_hit.set(func);
        let test_func = self.do_resolve.take();
        if let Some(cb) = test_func.as_ref() {
            if cb(body, other_body, &data) {
                resolver.add_collision(data.pos_norm_b.1, data.pos_norm_b.0, 
                    body, other_body, dt_sec);
            }
        }
        else {
            resolver.add_collision(data.pos_norm_b.1, data.pos_norm_b.0, 
                body, other_body, dt_sec);
        }
        self.do_resolve.set(test_func);
    }

    /// Gets a vector equal in length to `objects` where corresponding elements represent the change in position/rotation
    /// needed to resolve each object of collisions
    fn get_resolving_forces(&self, objects: &[&mut RigidBody<T>], dt_sec: f64) -> Vec<CollisionResolution> {
        let mut resolvers = Vec::<CollisionResolution>::new();
        resolvers.resize(objects.len(), CollisionResolution::identity());
        let mut tested_collisions = HashMap::new();
        for (body, collider, body_idx) in objects.iter().zip(0 .. objects.len())
            .filter(|(body, _)| body.collider.is_some() && body.body_type != BodyType::Static)
            .map(|(body, idx)| (body, body.collider.as_ref().unwrap(), idx)) 
        {
            let mut temp_map = HashMap::new();
            let method = &**self.collision_methods.get(&body.col_type).unwrap();
            for other in self.obj_tree.get_colliders(collider)
            {
                let other_body = objects.iter().find(|x| x.collider.as_ref().map(|x| x == &other).unwrap_or(false)).unwrap();
                if let Some((pos, norm)) = tested_collisions.get(&(other.clone(), collider.clone())) 
                {
                    // if we already tested the collision, no need to retest it or execute the collision callback again
                    // just do the collision resolution on this body now
                    resolvers[body_idx].add_collision(*norm, *pos, body, &other_body, dt_sec);
                }
                else {
                    match other.collision(&collider, method) {
                        Some(Hit::Hit(HitData {pos_norm_a, pos_norm_b })) => {
                            self.add_collision(&mut resolvers[body_idx], body, other_body, HitData {
                                pos_norm_a: pos_norm_b,
                                pos_norm_b: pos_norm_a,
                            }, dt_sec);
                            temp_map.insert((collider.clone(), other.clone()), pos_norm_b);
                        },
                        Some(Hit::NoData) => {
                            panic!("Complete undo not implemented")
                        },
                        None => (),
                    }
                }
            }
            for e in temp_map { tested_collisions.insert(e.0, e.1); }
        }
        resolvers
    }

    /// Steps the simulation `dt` into the future
    pub fn step(&mut self, objects: &mut [&mut RigidBody<T>], dt: std::time::Duration) {
        let dt_sec = dt.as_secs_f64();
        insert_into_octree(&mut self.obj_tree, objects);
        apply_forces(objects, dt_sec);
        let resolvers = self.get_resolving_forces(objects, dt_sec);
        resolve_forces(objects, resolvers, dt_sec);
        update_octree(objects);
    }
}