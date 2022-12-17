use super::*;
use crate::collisions;
use cgmath::*;
use collisions::*;
use std::cell::Cell;
use std::collections::HashMap;

type HitCallback<'a, T> =
    Box<dyn FnMut(&RigidBody<T>, &RigidBody<T>, &HitData, &BaseRigidBody) + 'a>;
type ResolveCallback<'a, T> =
    Box<dyn Fn(&RigidBody<T>, &RigidBody<T>, &HitData) -> bool + 'a>;

/// A simulation handles the collision detection, resolution, and movement of all objects
pub struct Simulation<'a, 'b, T> {
    obj_tree: collisions::CollisionTree,
    collision_methods: HashMap<CollisionMethod, Box<dyn HighPCollision>>,
    on_hit: Cell<Option<HitCallback<'a, T>>>,
    do_resolve: Cell<Option<ResolveCallback<'b, T>>>,
    scene_size: f64,
    scene_center: Point3<f64>,
}

/// Inserts any uninserted objects into the octree
///
/// Returns a hashmap of rigid bodies indices, with the key being the pointer to the node
/// transformation. This allows fast lookup of rigid bodies without iterating through
/// all of them again
fn insert_into_octree<T>(
    tree: &mut collisions::CollisionTree,
    objs: &[&RigidBody<T>],
) -> HashMap<*const node::Node, u32> {
    let mut m = HashMap::new();
    for (idx, o) in objs.iter().enumerate() {
        m.insert(o.base.transform.as_ptr() as *const _, idx as u32);
        if let Some(collider) = &o.base.collider {
            if !collider.is_in_collision_tree() {
                tree.insert(collider);
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
    }
    .normalize()
    /*let mag = rot_vel.magnitude();
    let imag_part = rot_vel * f64::sin(mag / 2.) / mag;
    Quaternion::new(f64::cos(mag / 2.), imag_part.x, imag_part.y, imag_part.z).normalize()*/
}

/// Applies all the manipulators to the rigid bodies
fn apply_forces<T>(
    objs: &[&RigidBody<T>],
    resolvers: &mut [CollisionResolution],
    obj_indices: &HashMap<*const node::Node, u32>,
    forces: &[Box<dyn Manipulator<T>>],
    dt: std::time::Duration,
) {
    for f in forces {
        f.affect_bodies(objs, resolvers, obj_indices, dt);
    }
}

/// Updates position and orientation of each dynamic body
fn move_objects<T>(objs: &mut [&mut RigidBody<T>], dt: f64) {
    for obj in objs
        .iter_mut()
        .filter(|obj| obj.base.body_type != BodyType::Static)
    {
        {
            let mut t = obj.base.transform.borrow_mut();
            (&mut *t).translate(obj.base.velocity * dt);
            (&mut *t).rotate_world(rot_vel_to_quat(obj.base.rot_vel, dt));
        }
        if let Some(collider) = &obj.base.collider {
            collider.update_in_collision_tree();
        }
    }
}

/// Uses `resolvers` to update position and rotation based on collisions
#[allow(clippy::mut_mut)]
fn resolve_collisions<T>(
    objects: &mut [&mut RigidBody<T>],
    resolvers: &[CollisionResolution],
    _dt: f64,
) {
    for (resolver, body_idx) in resolvers
        .iter()
        .zip(0..objects.len())
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

type HitCb<'a, T> = Option<
    Box<dyn FnMut(&RigidBody<T>, &RigidBody<T>, &HitData, &BaseRigidBody) + 'a>,
>;

/// Keeps the centers of all objects within the scene bounds specified by the center
/// and half width of the scene in each dimension
///
/// If a rigid body is moved to stay within the scene, calls `on_hit` with the rigid body
#[allow(clippy::too_many_lines)]
fn apply_bounds<'a, T>(
    objs: &[&RigidBody<T>],
    resolvers: &mut [CollisionResolution],
    scene_size: f64,
    scene_center: Point3<f64>,
    player_idx: usize,
    on_hit: &mut Cell<HitCb<'a, T>>,
) {
    let objs_len = objs.len();
    // safe bc player_idx is an index in objs
    let player_ptr = unsafe { objs.as_ptr().add(player_idx) };
    for (obj, obj_idx) in objs.iter().zip(0..objs_len) {
        let p = obj.base.center();
        let mut hit = false;
        let mut hit_data = HitData {
            pos_norm_a: (p, vec3(0., 0., 0.)),
            pos_norm_b: (p, vec3(0., 0., 0.)),
        };
        if p.x < scene_center.x - scene_size && obj.base.velocity.x < 0. {
            hit = true;
            let diff = scene_center.x - scene_size - p.x;
            resolvers[obj_idx].add_vel_change(vec3(diff, 0., 0.), None);
            hit_data.pos_norm_a.1.x += diff;
            hit_data.pos_norm_b.1.x += diff;
        }
        if p.x > scene_center.x + scene_size && obj.base.velocity.x > 0. {
            hit = true;
            let diff = scene_center.x + scene_size - p.x;
            resolvers[obj_idx].add_vel_change(vec3(diff, 0., 0.), None);
            hit_data.pos_norm_a.1.x += diff;
            hit_data.pos_norm_b.1.x += diff;
        }
        if p.y < scene_center.y - scene_size && obj.base.velocity.y < 0. {
            hit = true;
            let diff = scene_center.y - scene_size - p.y;
            resolvers[obj_idx].add_vel_change(vec3(0., diff, 0.), None);
            hit_data.pos_norm_a.1.y += diff;
            hit_data.pos_norm_b.1.y += diff;
        }
        if p.y > scene_center.y + scene_size && obj.base.velocity.y > 0. {
            hit = true;
            let diff = scene_center.y + scene_size - p.y;
            resolvers[obj_idx].add_vel_change(vec3(0., diff, 0.), None);
            hit_data.pos_norm_a.1.y += diff;
            hit_data.pos_norm_b.1.y += diff;
        }
        if p.z < scene_center.z - scene_size && obj.base.velocity.z < 0. {
            hit = true;
            let diff = scene_center.z - scene_size - p.z;
            resolvers[obj_idx].add_vel_change(vec3(0., 0., diff), None);
            hit_data.pos_norm_a.1.z += diff;
            hit_data.pos_norm_b.1.z += diff;
        }
        if p.z > scene_center.z + scene_size && obj.base.velocity.z > 0. {
            hit = true;
            let diff = scene_center.z + scene_size - p.z;
            resolvers[obj_idx].add_vel_change(vec3(0., 0., diff), None);
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
            }
            (false, Some(cb)) => {
                on_hit.set(Some(cb));
            }
            _ => (),
        }
    }
}

/// Updates every dynamic object in `objs` in the octree
fn update_octree<T>(objs: &[&RigidBody<T>]) {
    for o in objs {
        if let Some(collider) = &o.base.collider {
            if o.base.body_type == BodyType::Dynamic {
                collider.update_in_collision_tree();
            }
        }
    }
}

impl<'a, 'b, T> Simulation<'a, 'b, T> {
    pub fn new(
        scene_center: cgmath::Point3<f64>,
        scene_size: f64,
    ) -> Simulation<'static, 'static, T> {
        let mut collision_methods: HashMap<
            CollisionMethod,
            Box<dyn HighPCollision>,
        > = HashMap::new();
        collision_methods.insert(
            CollisionMethod::Triangle,
            Box::new(collisions::TriangleTriangleGPU::from_active_ctx()),
        );
        Simulation {
            obj_tree: collisions::CollisionTree::new(scene_center, scene_size),
            collision_methods,
            on_hit: Cell::default(),
            do_resolve: Cell::default(),
            scene_size,
            scene_center,
        }
    }

    /// Adds a hit callback to this simulation
    ///
    /// `f` - function which takes `rigid_body_a`, `rigid_body_b`, and Hit data
    /// where `pos_norm_a` is the position and contact normal
    /// on `rigid_body_a` and returns `true` if the simulation should do
    /// physical collision resolution
    pub fn with_on_hit<'c, F>(self, f: F) -> Simulation<'c, 'b, T>
    where
        F: FnMut(&RigidBody<T>, &RigidBody<T>, &HitData, &BaseRigidBody) + 'c,
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
    pub fn with_do_resolve<
        'c,
        F: Fn(&RigidBody<T>, &RigidBody<T>, &HitData) -> bool + 'c,
    >(
        self,
        f: F,
    ) -> Simulation<'a, 'c, T> {
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
    fn add_collision(
        &self,
        resolver: &mut CollisionResolution,
        body: &RigidBody<T>,
        other_body: &RigidBody<T>,
        data: &HitData,
        player: &BaseRigidBody,
    ) {
        let mut func = self.on_hit.take();
        if let Some(cb) = func.as_mut() {
            cb(body, other_body, data, player);
        }
        self.on_hit.set(func);
        let test_func = self.do_resolve.take();
        if let Some(cb) = test_func.as_ref() {
            if cb(body, other_body, data) {
                resolver.add_collision(
                    data.pos_norm_b.1,
                    data.pos_norm_b.0,
                    body,
                    other_body,
                );
            }
        } else {
            resolver.add_collision(
                data.pos_norm_b.1,
                data.pos_norm_b.0,
                body,
                other_body,
            );
        }
        self.do_resolve.set(test_func);
    }

    /// Gets a vector equal in length to `objects` where corresponding
    /// elements represent the change in position/rotation
    /// needed to resolve each object of collisions
    ///
    /// Each `CollisionResolution` struct handles the collision for a single rigid body
    #[allow(clippy::too_many_lines)]
    fn get_resolving_forces(
        &self,
        objects: &[&RigidBody<T>],
        player_idx: usize,
    ) -> Vec<CollisionResolution> {
        let mut resolvers = Vec::<CollisionResolution>::new();
        resolvers.resize(objects.len(), CollisionResolution::identity());
        let mut tested_collisions = HashMap::new();
        for (body, collider, body_idx) in objects
            .iter()
            .enumerate()
            .filter(|(_, body)| {
                body.base.collider.is_some()
                    && body.base.body_type != BodyType::Static
            })
            .map(|(idx, body)| {
                (body, body.base.collider.as_ref().unwrap(), idx)
            })
        {
            let mut temp_map = HashMap::new();
            let method =
                &**self.collision_methods.get(&body.base.col_meth()).unwrap();
            for other in CollisionTree::get_colliders(collider) {
                let other_body = objects
                    .iter()
                    .find(|x| {
                        x.base.collider.as_ref().map_or(false, |x| x == &other)
                    })
                    .unwrap();
                if let Some((pos, norm)) =
                    tested_collisions.get(&(other.clone(), collider.clone()))
                {
                    // if we already tested the collision, no need to retest it
                    // or execute the collision callback again
                    // just do the collision resolution on this body now
                    resolvers[body_idx]
                        .add_collision(*norm, *pos, body, other_body);
                } else {
                    match other.collision(collider, method) {
                        Some(Hit::Hit(HitData {
                            pos_norm_a,
                            pos_norm_b,
                        })) => {
                            self.add_collision(
                                &mut resolvers[body_idx],
                                body,
                                other_body,
                                &HitData {
                                    pos_norm_a: pos_norm_b,
                                    pos_norm_b: pos_norm_a,
                                },
                                &objects[player_idx].base,
                            );
                            temp_map.insert(
                                (collider.clone(), other.clone()),
                                pos_norm_b,
                            );
                        }
                        Some(Hit::NoData) => {
                            panic!("Complete undo not implemented");
                        }
                        None => (),
                    }
                }
            }
            for e in temp_map {
                tested_collisions.insert(e.0, e.1);
            }
        }
        resolvers
    }

    /// Steps the simulation `dt` into the future
    ///
    /// `player_idx` - the index of the player's rigid body in `objects`
    ///
    /// Returns a vector of the change in position/rotation needed to resolve any forces
    ///  and collisions. The resolution at index `idx` is the change for the body at index
    /// `idx` in `objects`
    pub fn calc_resolvers(
        &mut self,
        objects: &[&RigidBody<T>],
        forces: &[Box<dyn Manipulator<T>>],
        player_idx: usize,
        dt: std::time::Duration,
    ) -> Vec<CollisionResolution> {
        let body_map = insert_into_octree(&mut self.obj_tree, objects);
        update_octree(objects);
        let mut resolvers = self.get_resolving_forces(objects, player_idx);
        apply_forces(objects, &mut resolvers, &body_map, forces, dt);
        /*move_objects(objects, dt_sec);
        resolve_collisions(objects,
            self.get_resolving_forces(objects, player_idx), dt_sec);*/
        apply_bounds(
            objects,
            &mut resolvers,
            self.scene_size,
            self.scene_center,
            player_idx,
            &mut self.on_hit,
        );
        resolvers
    }

    /// Steps the simulation `dt` into the future by applying the resolving forces to each object
    ///
    /// Requires resolvers and objects of the corresponding indices to match
    pub fn apply_resolvers(
        objects: &mut [&mut RigidBody<T>],
        resolvers: &[CollisionResolution],
        dt: std::time::Duration,
    ) {
        let dt_sec = dt.as_secs_f64();
        move_objects(objects, dt_sec);
        resolve_collisions(objects, resolvers, dt_sec);
    }
}
