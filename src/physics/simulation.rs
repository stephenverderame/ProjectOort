use crate::collisions;
use super::*;
use cgmath::*;
use std::collections::{HashMap};
use collisions::*;

/// Data to resolve a collision
#[derive(Clone)]
struct CollisionResolution {
    vel: Vector3<f64>,
    rot: Quaternion<f64>,
    is_collide: bool,
}

impl CollisionResolution {
    fn identity() -> Self {
        Self {
            vel: vec3(0., 0., 0.),
            rot: Quaternion::new(1., 0., 0., 0.),
            is_collide: false,
        }
    }

    fn add_collision(&mut self, norm: Vector3<f64>, body: &RigidBody, colliding_body: &RigidBody) {
        let v = body.velocity.dot(norm) * norm;
        if colliding_body.center().dot(v) / v.dot(v) > body.center().dot(v) / v.dot(v) {
            self.vel -= v;
        }
        if !self.is_collide {
            self.is_collide = true;
            //self.rot = body.rot_vel.invert();
        }
    }
}

/// A simulation handles the collision detection, resolution, and movement of all objects
pub struct Simulation {
    obj_tree: collisions::CollisionTree,
    collision_methods: HashMap<CollisionMethod, Box<dyn collisions::HighPCollision>>,
}

/// Inserts any uninserted objects into the octree
fn insert_into_octree(tree: &mut collisions::CollisionTree, objs: &[&mut RigidBody]) {
    for o in objs {
        if let Some(collider) = &o.collider {
            if !collider.is_in_collision_tree() {
                tree.insert(collider)
            }
        }
    }
}

/// Calculates velocity and updates position and orientation of each dynamic body
fn apply_forces(objs: &[&mut RigidBody], dt: f64) {
    //println!("{:?}", objs[100].rot_vel);
    for obj in objs {
        if obj.body_type == BodyType::Dynamic {
            {
                let mut t = obj.transform.borrow_mut();
                (&mut *t).pos += obj.velocity * dt;
                let rot = (*t).orientation;
                (&mut *t).orientation = rot * obj.rot_vel;// * dt;
            }
            obj.collider.as_ref().map(|x| x.update_in_collision_tree());

        }
    }
}

/// Uses `resolvers` to update position and rotation based on collisions
fn resolve_forces(objects: &[&mut RigidBody], resolvers: Vec<CollisionResolution>, dt: f64) {
    for (resolver, body_idx) in resolvers.into_iter().zip(0 .. objects.len())
        .filter(|(resolver, _)| resolver.is_collide) 
    {
        let obj = &objects[body_idx];
        obj.transform.borrow_mut().pos += resolver.vel * dt;
        let rot = obj.transform.borrow().orientation;
        obj.transform.borrow_mut().orientation = rot * resolver.rot;

    }
}

/// Updates every dynamic object in `objs` in the octree
fn update_octree(objs: &[&mut RigidBody]) {
    for o in objs {
        if let Some(collider) = &o.collider {
            if o.body_type == BodyType::Dynamic {
                collider.update_in_collision_tree();
            }
        }
    }
}

impl Simulation {
    pub fn new(scene_center: cgmath::Point3<f64>, scene_size: f64) -> Self {
        let mut collision_methods : HashMap<CollisionMethod, Box<dyn collisions::HighPCollision>>
            = HashMap::new();
        collision_methods.insert(CollisionMethod::Triangle, Box::new(collisions::TriangleTriangleGPU::from_active_ctx()));
        Simulation {
            obj_tree: collisions::CollisionTree::new(scene_center, scene_size),
            collision_methods
        }
    }

    /// Gets a vector equal in length to `objects` where corresponding elements represent the change in position/rotation
    /// needed to resolve each object of collisions
    fn get_resolving_forces(&self, objects: &[&mut RigidBody]) -> Vec<CollisionResolution> {
        let mut resolvers = Vec::<CollisionResolution>::new();
        resolvers.resize(objects.len(), CollisionResolution::identity());
        let mut tested_collisions = HashMap::new();
        for (body, collider, body_idx) in objects.iter().zip(0 .. objects.len())
            .filter(|(body, _)| body.collider.is_some() && body.body_type == BodyType::Dynamic)
            .map(|(body, idx)| (body, body.collider.as_ref().unwrap(), idx)) 
        {
            let mut temp_map = HashMap::new();
            let method = &**self.collision_methods.get(&body.col_type).unwrap();
            for other in self.obj_tree.get_colliders(collider)
            {
                let other_body = objects.iter().find(|x| x.collider.as_ref().map(|x| x == &other).unwrap_or(false)).unwrap();
                if let Some((_pos, norm)) = tested_collisions.get(&(other.clone(), collider.clone())) 
                {
                    resolvers[body_idx].add_collision(*norm, body, &other_body);
                }
                else {
                    match other.collision(&collider, method) {
                        Some(Hit::Hit(HitData {pos_norm_a: (pos_a, norm_a), pos_norm_b: (_pos_b, _norm_b)})) => {
                            resolvers[body_idx].add_collision(norm_a, body, &other_body);
                            temp_map.insert((collider.clone(), other.clone()), (pos_a, norm_a));
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
    pub fn step(&mut self, objects: &[&mut RigidBody], dt: std::time::Duration) {
        let dt_sec = dt.as_secs_f64();
        insert_into_octree(&mut self.obj_tree, objects);
        apply_forces(objects, dt_sec);
        let resolvers = self.get_resolving_forces(objects);
        resolve_forces(objects, resolvers, dt_sec);
        update_octree(objects);
    }
}