use crate::collisions;
use super::*;
use cgmath::*;
use std::collections::HashMap;
use collisions::{Hit, HitData};

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
    for obj in objs {
        if obj.body_type == BodyType::Dynamic {
            {
                let t = obj.transform.borrow_mut();
                (*t).pos += obj.velocity * dt;
                let rot = (*t).orientation;
                (*t).orientation = rot * obj.rot_vel * dt;
            }
            obj.collider.as_ref().map(|x| x.update_in_collision_tree());

        }
    }
}

/// Uses `resolvers` to update position and rotation based on collisions
fn resolve_forces(objects: &[&mut RigidBody], resolvers: Vec<CollisionResolution>) {
    for (resolver, body_idx) in resolvers.into_iter().zip(0 .. objects.len()) {
        if resolver.is_collide {
            let obj = objects[body_idx];
            obj.transform.borrow_mut().pos += resolver.vel;
            let rot = obj.transform.borrow().orientation;
            obj.transform.borrow_mut().orientation = rot * resolver.rot;
        }

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
        for (body, body_idx) in objects.iter().zip(0 .. objects.len()) { // fix: checking every collision twice
            if let Some(collider) = &body.collider {
                if body.body_type == BodyType::Dynamic {
                    let method = &**self.collision_methods.get(&body.col_type).unwrap();
                    for other in self.obj_tree.get_colliders(collider) {
                        if let Some(Hit::Hit(HitData { pos_norm_a: (pos, norm), .. })) 
                            = collider.collision(&other, method) 
                        {
                            resolvers[body_idx].vel -= body.velocity.dot(norm) * norm;
                            resolvers[body_idx].rot = 1. / body.rot_vel;
                            resolvers[body_idx].is_collide = true;
                        }
                    }
                }
            }
        }
        resolvers
    }

    /// Steps the simulation `dt` into the future
    pub fn step(&mut self, objects: &[&mut RigidBody], dt: std::time::Duration) {
        insert_into_octree(&mut self.obj_tree, objects);
        apply_forces(objects, dt.as_secs_f64());
        let resolvers = self.get_resolving_forces(objects);
        resolve_forces(objects, resolvers);
        update_octree(objects);
    }
}