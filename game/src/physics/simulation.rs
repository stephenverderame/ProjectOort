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

    /// Performs a collision between `body` and `colliding_body`
    /// where this instance is the resolution object for `body`
    fn do_collision<T>(&mut self, norm: Vector3<f64>, pt: Point3<f64>, 
        body: &RigidBody<T>, colliding_body: &RigidBody<T>)
    {
        let body = &body.base;
        let colliding_body = &colliding_body.base;
        let body_inertia = body.moment_inertia();
        let colliding_inertia = colliding_body.moment_inertia();
        let body_lever = pt - body.center();
        let colliding_lever = pt - colliding_body.center();

        let m_eff = 1.0 / body.mass + 1.0 / colliding_body.mass;
        let impact_speed = norm.dot(body.velocity - colliding_body.velocity);
        let impact_angular_speed = body_lever.cross(norm).dot(body.rot_vel) - 
            colliding_lever.cross(norm).dot(colliding_body.rot_vel);
        let body_angular_denom_term = body_lever.cross(norm)
            .dot(body_inertia.invert().unwrap() * body_lever.cross(norm));
        let colliding_angular_denom_term = colliding_lever.cross(norm)
            .dot(colliding_inertia.invert().unwrap() * colliding_lever.cross(norm));
        let impulse = 1.52 * (impact_speed + impact_angular_speed) / 
            (m_eff + body_angular_denom_term + colliding_angular_denom_term);
        // (1 + coeff of resitution) * effective mass * impact speed
        // impulse = kg * m/s = Ns
        self.vel -= impulse / body.mass * norm;
        self.rot += body_inertia.invert().unwrap() * (impulse * norm).cross(body_lever);
    }


    /// Performs a collision between `body` and `colliding_body`
    /// if the two objects are positioned in such a way to allow one
    /// 
    /// Uses a different collision resolution method if objects are overlapping
    fn add_collision<T>(&mut self, norm: Vector3<f64>, pt: Point3<f64>, 
        body: &RigidBody<T>, colliding_body: &RigidBody<T>) 
    {
        // Assumes eleastic collisions
        let relative_vel = body.base.velocity - colliding_body.base.velocity;
        let v = relative_vel.dot(norm) * norm;
        if colliding_body.base.center().dot(v) / v.dot(v) > 
            body.base.center().dot(v) / v.dot(v) {
            //self.vel -= v;
            self.do_collision(norm, pt, body, colliding_body)
            
        }
        if body.base.velocity.magnitude() < 0.00001 && 
            colliding_body.base.velocity.magnitude() < 0.00001 {
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
    collision_methods: HashMap<CollisionMethod, Box<dyn HighPCollision>>,
    on_hit: Cell<Option<Box<dyn FnMut(&RigidBody<T>, &RigidBody<T>, 
        &HitData, &BaseRigidBody) + 'a>>>,
    do_resolve: Cell<Option<Box<dyn Fn(&RigidBody<T>, 
        &RigidBody<T>, &HitData) -> bool + 'b>>>,
    scene_size: f64,
    scene_center: Point3<f64>,
}

/// Inserts any uninserted objects into the octree
/// 
/// Returns a hashmap of rigid bodies indices, with the key being the pointer to the node
/// transformation. This allows fast lookup of rigid bodies without iterating through
/// all of them again
fn insert_into_octree<T>(tree: &mut collisions::CollisionTree, 
    objs: &[&mut RigidBody<T>]) -> HashMap<*const node::Node, u32> 
{
    let mut m = HashMap::new();
    let mut idx = 0;
    for o in objs {
        m.insert(o.base.transform.as_ptr() as *const _, idx);
        idx += 1;
        if let Some(collider) = &o.base.collider {
            if !collider.is_in_collision_tree() {
                tree.insert(collider)
            }
        }
    }
    m
}

/// Converts an angular velocity to a rotation that can be applied to a body
/// orientation which will undergo a rotation of that angular velocity for `dt` seconds
/// 
/// `rot_vel` - direction is the axis of rotation and magnitude is the 
/// velocity in radians per second
fn rot_vel_to_quat(rot_vel: Vector3<f64>, dt: f64) -> Quaternion<f64> {
    let ha = rot_vel * 0.5 * 7000. * dt;
    let mag = rot_vel.magnitude();
    if mag > 1.0 {
        let ha = ha * f64::sin(mag) / mag;
        Quaternion::new(f64::cos(mag), ha.x, ha.y, ha.z)
    } else {
        Quaternion::new(1.0, ha.x, ha.y, ha.z)
    }.normalize()
    /*let mag = rot_vel.magnitude();
    let imag_part = rot_vel * f64::sin(mag / 2.) / mag;
    Quaternion::new(f64::cos(mag / 2.), imag_part.x, imag_part.y, imag_part.z).normalize()*/

}

/// Applies all the manipulators to the rigid bodies
fn apply_forces<T>(objs: &mut [&mut RigidBody<T>], 
    obj_indices: &HashMap<*const node::Node, u32>, forces: &[Box<dyn Manipulator<T>>],
    dt: std::time::Duration)
{
    for f in forces {
        f.affect_bodies(objs, obj_indices, dt);
    }
}

/// Updates position and orientation of each dynamic body
fn move_objects<T>(objs: &mut [&mut RigidBody<T>], dt: f64) {
    for obj in objs.iter_mut()
        .filter(|obj| obj.base.body_type != BodyType::Static) 
    {
        {
            let mut t = obj.base.transform.borrow_mut();
            (&mut *t).translate(obj.base.velocity * dt);
            (&mut *t).rotate_world(rot_vel_to_quat(obj.base.rot_vel, dt));
        }
        obj.base.collider.as_ref().map(|x| x.update_in_collision_tree());
    }
}

/// Uses `resolvers` to update position and rotation based on collisions
fn resolve_collisions<T>(objects: &mut [&mut RigidBody<T>], 
    resolvers: Vec<CollisionResolution>, _dt: f64) 
{
    for (resolver, body_idx) in resolvers.into_iter().zip(0 .. objects.len())
        .filter(|(resolver, _)| resolver.is_collide) 
    {
        let obj = &mut objects[body_idx];
        obj.base.velocity += resolver.vel;
        if obj.base.body_type == BodyType::Controlled {
            //obj.transform.borrow_mut().rotate_world(rot_vel_to_quat(resolver.rot / 2., dt));
            obj.base.rot_vel += resolver.rot / 100.;
        } else {
            obj.base.rot_vel += resolver.rot;
        }

    }
}

type HitCb<'a, T> = Option<Box<dyn FnMut(&RigidBody<T>, &RigidBody<T>, &HitData, &BaseRigidBody) + 'a>>;

/// Keeps the centers of all objects within the scene bounds specified by the center
/// and half width of the scene in each dimension
/// 
/// If a rigid body is moved to stay within the scene, calls `on_hit` with the rigid body
fn apply_bounds<'a, T>(objs: &mut [&mut RigidBody<T>], 
    scene_size: f64, scene_center: Point3<f64>, player_idx: usize, on_hit: &mut Cell<HitCb<'a, T>>)
{
    let objs_len = objs.len();
    // safe bc player_idx is an index in objs
    let player_ptr = unsafe { objs.as_ptr().add(player_idx) };
    for (obj, obj_idx) in objs.iter_mut().zip(0 .. objs_len) {
        let p = obj.base.center();
        let mut hit = false;
        let mut hit_data = HitData {
            pos_norm_a: (p, vec3(0., 0., 0.)),
            pos_norm_b: (p, vec3(0., 0., 0.))
        };
        if p.x < scene_center.x - scene_size && obj.base.velocity.x < 0. {
            hit = true;
            let diff = scene_center.x - scene_size - p.x;
            obj.base.velocity += vec3(diff, 0., 0.);
            hit_data.pos_norm_a.1.x += diff;
            hit_data.pos_norm_b.1.x += diff;
        } if p.x > scene_center.x + scene_size && obj.base.velocity.x > 0. {
            hit = true;
            let diff = scene_center.x + scene_size - p.x;
            obj.base.velocity += vec3(diff, 0., 0.);
            hit_data.pos_norm_a.1.x += diff;
            hit_data.pos_norm_b.1.x += diff;
        } if p.y < scene_center.y - scene_size && obj.base.velocity.y < 0. {
            hit = true;
            let diff = scene_center.y - scene_size - p.y;
            obj.base.velocity += vec3(0., diff, 0.);
            hit_data.pos_norm_a.1.y += diff;
            hit_data.pos_norm_b.1.y += diff;
        } if p.y > scene_center.y + scene_size && obj.base.velocity.y > 0. {
            hit = true;
            let diff = scene_center.y + scene_size - p.y;
            obj.base.velocity += vec3(0., diff, 0.);
            hit_data.pos_norm_a.1.y += diff;
            hit_data.pos_norm_b.1.y += diff;
        } if p.z < scene_center.z - scene_size && obj.base.velocity.z < 0. {
            hit = true;
            let diff = scene_center.z - scene_size - p.z;
            obj.base.velocity += vec3(0., 0., diff);
            hit_data.pos_norm_a.1.z += diff;
            hit_data.pos_norm_b.1.z += diff;
        } if p.z > scene_center.z + scene_size && obj.base.velocity.z > 0. {
            hit = true;
            let diff = scene_center.z + scene_size - p.z;
            obj.base.velocity += vec3(0., 0., diff);
            hit_data.pos_norm_a.1.z += diff;
            hit_data.pos_norm_b.1.z += diff;
        }

        let cb = on_hit.take();
        match (hit, cb) {
            (true, Some(mut cb)) => {
                hit_data.pos_norm_a.1 = hit_data.pos_norm_a.1.normalize();
                hit_data.pos_norm_b.1 = hit_data.pos_norm_b.1.normalize();
                if obj_idx == player_idx {
                    cb(obj, obj, &hit_data, &obj.base);
                } else {
                    // safe bc mutable borrowed obj is not the player
                    cb(obj, obj, &hit_data, unsafe { &(*player_ptr).base });
                }
                on_hit.set(Some(cb));
            },
            (false, Some(cb)) => {
                on_hit.set(Some(cb));
            },
            _ => (),
        }

    }
}

/// Updates every dynamic object in `objs` in the octree
fn update_octree<T>(objs: &[&mut RigidBody<T>]) {
    for o in objs {
        if let Some(collider) = &o.base.collider {
            if o.base.body_type == BodyType::Dynamic {
                collider.update_in_collision_tree();
            }
        }
    }
}

impl<'a, 'b, T> Simulation<'a, 'b, T> {
    pub fn new(scene_center: cgmath::Point3<f64>, scene_size: f64) 
        -> Simulation::<'static, 'static, T> 
    {
        let mut collision_methods : 
            HashMap<CollisionMethod, Box<dyn HighPCollision>> = HashMap::new();
        collision_methods.insert(CollisionMethod::Triangle, 
            Box::new(collisions::TriangleTriangleGPU::from_active_ctx()));
        Simulation {
            obj_tree: collisions::CollisionTree::new(scene_center, scene_size),
            collision_methods,
            on_hit: Cell::default(),
            do_resolve: Cell::default(),
            scene_size, scene_center,
        }
    }

    /// Adds a hit callback to this simulation
    /// 
    /// `f` - function which takes `rigid_body_a`, `rigid_body_b`, and Hit data 
    /// where `pos_norm_a` is the position and contact normal
    /// on `rigid_body_a` and returns `true` if the simulation should do 
    /// physical collision resolution
    pub fn with_on_hit<'c, F>(self, f: F) -> Simulation::<'c, 'b, T> 
        where F : FnMut(&RigidBody<T>, &RigidBody<T>, &HitData, &BaseRigidBody) + 'c
    {
        Simulation {
            obj_tree: self.obj_tree,
            collision_methods: self.collision_methods,
            on_hit: Cell::new(Some(Box::new(f))),
            do_resolve: self.do_resolve,
            scene_size: self.scene_size,
            scene_center: self.scene_center,
        }
    }

    /// Adds a hit callback that returns `true` if we should do collision resolution 
    /// or `false` if we shouldn't
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
                scene_size: self.scene_size,
                scene_center: self.scene_center,
            }
        }

    /// Adds a collision to `body` from `resolver`
    /// 
    /// `body` and `resolver` are the body and resolver for the rigid body 
    /// whose contact point and normal is stored in `pos_norm_a`
    fn add_collision(&self, resolver: &mut CollisionResolution, body: &RigidBody<T>, 
        other_body: &RigidBody<T>, data: HitData, player: &BaseRigidBody)
    {
        let mut func = self.on_hit.take();
        if let Some(cb) = func.as_mut() {
            cb(body, other_body, &data, player);
        } 
        self.on_hit.set(func);
        let test_func = self.do_resolve.take();
        if let Some(cb) = test_func.as_ref() {
            if cb(body, other_body, &data) {
                resolver.add_collision(data.pos_norm_b.1, data.pos_norm_b.0, 
                    body, other_body);
            }
        }
        else {
            resolver.add_collision(data.pos_norm_b.1, data.pos_norm_b.0, 
                body, other_body);
        }
        self.do_resolve.set(test_func);
    }

    /// Gets a vector equal in length to `objects` where corresponding 
    /// elements represent the change in position/rotation
    /// needed to resolve each object of collisions
    /// 
    /// Each CollisionResolution struct handles the collision for a single rigid body
    fn get_resolving_forces(&self, objects: &[&mut RigidBody<T>], player_idx: usize) 
        -> Vec<CollisionResolution> 
    {
        let mut resolvers = Vec::<CollisionResolution>::new();
        resolvers.resize(objects.len(), CollisionResolution::identity());
        let mut tested_collisions = HashMap::new();
        for (body, collider, body_idx) in objects.iter().zip(0 .. objects.len())
            .filter(|(body, _)| body.base.collider.is_some() 
                && body.base.body_type != BodyType::Static)
            .map(|(body, idx)| (body, body.base.collider.as_ref().unwrap(), idx)) 
        {
            let mut temp_map = HashMap::new();
            let method = &**self.collision_methods.get(&body.base.col_type).unwrap();
            for other in self.obj_tree.get_colliders(collider)
            {
                let other_body = objects.iter().find(|x| 
                    x.base.collider.as_ref().map(|x| x == &other).unwrap_or(false))
                    .unwrap();
                if let Some((pos, norm)) = 
                    tested_collisions.get(&(other.clone(), collider.clone())) 
                {
                    // if we already tested the collision, no need to retest it 
                    // or execute the collision callback again
                    // just do the collision resolution on this body now
                    resolvers[body_idx].add_collision(*norm, *pos, body, &other_body);
                }
                else {
                    match other.collision(&collider, method) {
                        Some(Hit::Hit(HitData {pos_norm_a, pos_norm_b })) => {
                            self.add_collision(&mut resolvers[body_idx], body, 
                            other_body, HitData {
                                pos_norm_a: pos_norm_b,
                                pos_norm_b: pos_norm_a,
                            }, &objects[player_idx].base);
                            temp_map.insert((collider.clone(), other.clone()), 
                                pos_norm_b);
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
    /// 
    /// `player_idx` - the index of the player's rigid body in `objects`
    pub fn step(&mut self, objects: &mut [&mut RigidBody<T>], 
        forces: &[Box<dyn Manipulator<T>>], player_idx: usize,
        dt: std::time::Duration) 
    {
        let dt_sec = dt.as_secs_f64();
        let body_map = insert_into_octree(&mut self.obj_tree, objects);
        apply_forces(objects, &body_map, forces, dt);
        move_objects(objects, dt_sec);
        resolve_collisions(objects, 
            self.get_resolving_forces(objects, player_idx), dt_sec);
        apply_bounds(objects, self.scene_size, self.scene_center, 
            player_idx, &mut self.on_hit);
        update_octree(objects);
    }
}