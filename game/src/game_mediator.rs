use std::rc::Rc;
use std::cell::{RefCell, Cell, Ref};
use super::object::*;
use crate::graphics_engine::entity::*;
use crate::graphics_engine::{primitives, cubes, model, entity, shader, 
    scene, particles};
use crate::cg_support::node::*;
use glium;
use super::object;
use cgmath::*;
use crate::physics::{self, RigidBody, };
use std::collections::HashMap;
use crate::collisions;
use shared_types::{id_list::IdList, game_controller::*};
use object::*;

pub trait GameMediator {
    fn get_entities(&self) -> Vec<Rc<RefCell<dyn AbstractEntity>>>;

    /// Gets the lights in this map
    fn get_lights(&self) -> Vec<shader::LightData>;

    /// Gets the rigid bodies in this map
    fn iter_bodies(&self, func: Box<dyn FnMut(&dyn Iterator<Item = &RigidBody<ObjectData>>)>);

    fn update_bodies(&mut self, func: Box<dyn FnMut(&mut dyn Iterator<Item = &mut RigidBody<ObjectData>>)>);

    fn get_lasers(&self) -> Ref<GameObject>;

    fn get_lines(&self) -> Ref<primitives::Lines>;

    fn add_line(&mut self, line_id: u32, line: primitives::LineData);

    fn remove_line(&mut self, line_id: u32);

    fn get_particles(&self) -> Ref<particles::ParticleSystem>;

    /// See `ParticleSystem::new_emitter`
    fn add_particle_emitter(&mut self, emitter: Box<dyn particles::Emitter>, emitter_id: usize);

    fn add_laser(&mut self, transform: Node, vel: Vector3<f64>);

    fn remove_lasers(&mut self, ids: &[ObjectId]);

    fn sync(&mut self);

    fn emit_particles(&self, dt: std::time::Duration);

}

struct GameMediatorBase {
    objs: HashMap<ObjectType, Rc<RefCell<GameObject>>>,
    entity: HashMap<ObjectType, Rc<RefCell<dyn AbstractEntity>>>,
    lines: Rc<RefCell<primitives::Lines>>,
    particles: Rc<RefCell<particles::ParticleSystem>>,
    ids: IdList,
}

fn init_objs<F : glium::backend::Facade>(sm: &shader::ShaderManager, ctx: &F)
    -> HashMap<ObjectType, Rc<RefCell<GameObject>>>
{
    let mut objs = HashMap::new();
    objs.insert(ObjectType::Asteroid, Rc::new(RefCell::new(
        object::GameObject::new(
            model::Model::new("assets/asteroid1/Asteroid.obj", ctx).with_instancing(), 
        object::ObjectType::Asteroid).with_depth()
        .with_collisions("assets/asteroid1/Asteroid.obj", 
            collisions::TreeStopCriteria::default()).density(2.71)
    )));
    objs.insert(ObjectType::Planet, Rc::new(RefCell::new(
        object::GameObject::new(
            model::Model::new("assets/planet/planet1.obj", ctx), 
                object::ObjectType::Planet).with_depth()
            .with_collisions("assets/planet/planet1.obj", Default::default())
            .immobile().density(10.)
    )));
    objs
}

fn init_entities<F : glium::backend::Facade>(sm: &shader::ShaderManager, ctx: &F)
    -> HashMap<ObjectType, Rc<RefCell<dyn AbstractEntity>>>
{
    let mut entities : HashMap<_, Rc<RefCell<dyn AbstractEntity>>> = HashMap::new();
    entities.insert(ObjectType::Any, Rc::new(RefCell::new(
        entity::EntityBuilder::new(cubes::Volumetric::cloud(128, ctx))
        .with_pass(shader::RenderPassType::Visual)
        .render_order(entity::RenderOrder::Last).build())));
    entities
}

/// Converts a remote object into a rigid body
fn remote_obj_to_body(obj: &shared_types::RemoteObject) -> Option<RigidBody<ObjectData>> {
    use crate::collisions::CollisionObject;
    let (node, vel, rot_vel, typ, id) 
        = shared_types::node::from_remote_object(obj);
    let node = Rc::new(RefCell::new(node));
    if let Some((path, bvh_options, density)) = col_data_of_obj_type(&typ) {
        let col_obj = CollisionObject::new(node.clone(), path, bvh_options);
        let mut body = 
            RigidBody::new(node, Some(col_obj), physics::BodyType::Dynamic, (typ, obj.id))
            .with_density(density);
        body.base.velocity = vel;
        body.base.rot_vel = rot_vel;
        Some(body)
    } else { None }
}

/// Converts a rigid body to a remote object
fn body_to_remote_obj(body: &RigidBody<ObjectData>) -> shared_types::RemoteObject {
    shared_types::node::to_remote_object(&body.base.transform.borrow(), &body.base.velocity, 
        &body.base.rot_vel, body.metadata.0, body.metadata.1)
}

impl GameMediatorBase {
    fn new<F : glium::backend::Facade>(sm: &shader::ShaderManager, ctx: &F) -> Self {
        let lines = Rc::new(RefCell::new(primitives::Lines::new(ctx)));
        let particles = Rc::new(RefCell::new(particles::ParticleSystem::new()
            .with_billboard("assets/particles/smoke_01.png", 0.4)
            .with_billboard("assets/particles/circle_05.png", 0.4)));
        GameMediatorBase {
            objs: init_objs(sm, ctx),
            entity: init_entities(sm, ctx),
            lines,
            particles,
            ids: IdList::new(),
        }
    }

    fn get_lights(&self) -> Vec<shader::LightData> {
        let mut lights = Vec::new();
        self.objs[&ObjectType::Laser].borrow().iter_positions(|node| {
            let mat : Matrix4<f32> = From::from(node);
            let start = mat.transform_point(point3(0., 0., 3.));
            let end = mat.transform_point(point3(0., 0., -3.));
            let radius = 1.5;
            let luminance = 80.;
            lights.push(shader::LightData::tube_light(start, end, radius, 
                luminance, vec3(0.5451, 0., 0.5451)));
        });
        lights.append(&mut self.particles.borrow().lights()
            .unwrap_or_else(|| Vec::new()));
        lights
    }

    #[inline]
    fn get_entities(&self) -> Vec<Rc<RefCell<dyn AbstractEntity>>> {
        self.objs.iter().map(|(_, obj)| 
            (obj.borrow().as_entity().clone() as Rc<RefCell<dyn AbstractEntity>>)).collect()
    }

    /// Iterates through all the rigid bodies, can be mutated
    fn update_bodies(&mut self, func: Box<dyn FnMut(&mut dyn Iterator<Item = &mut RigidBody<ObjectData>>)>) {
        let objs : Vec<_> = self.objs.iter().map(|(_, obj)| {
            obj.borrow_mut()
        }).collect();
        func(&mut objs.iter_mut().flat_map(|obj| {
            obj.bodies_ref().into_iter()
        }));
    }

    /// Iterates through all the rigid bodies, non-mutably
    fn iter_bodies(&self, func: Box<dyn FnMut(&dyn Iterator<Item = &RigidBody<ObjectData>>)>) {
        let objs : Vec<_> = self.objs.iter().map(|(_, obj)| {
            obj.borrow()
        }).collect();
        func(&objs.iter().flat_map(|obj| {
            obj.bodies_slice().iter()
        }));
    }

    /// Adds a new laser to lasers
    #[inline]
    fn add_laser(&mut self, transform: Node, vel: Vector3<f64>) {
        if let Some(id) = self.ids.next() {
            self.objs[&ObjectType::Laser].borrow_mut()
                .new_instance(transform, Some(vel), id);
        } else {
            println!("No more IDs!");
        }
    }

    fn remove_lasers(&mut self, ids: &[ObjectId]) {
        let bad_ptrs = self.objs[&ObjectType::Laser].borrow().bodies_slice()
            .iter().filter_map(|body| {
                if ids.contains(&body.metadata.1) {
                    Some(body.base.transform.as_ptr() as * const ())
                } else {
                    None
                }
            }).collect::<Vec<_>>();
        self.objs[&ObjectType::Laser].borrow_mut().retain(|ptr| !bad_ptrs.contains(&ptr));
    }

    #[inline]
    fn add_line(&mut self, line_id: u32, line: primitives::LineData) {
        self.lines.borrow_mut().add_line(line_id, line);
    }

    #[inline]
    fn remove_line(&mut self, line_id: u32) {
        self.lines.borrow_mut().remove_line(line_id);
    }

    #[inline]
    fn emit_particles(&self, dt: std::time::Duration) {
        self.particles.borrow_mut().emit(dt);
    }
}

struct LocalGameMediator {
    base: GameMediatorBase,
    controller: Box<dyn GameController>,
}

impl LocalGameMediator {
    pub fn new<F, C>(sm: &shader::ShaderManager, ctx: &F, controller: C) 
        -> Self where C : GameController + 'static, F : glium::backend::Facade
    {
        Self {
            base: GameMediatorBase::new(sm, ctx),
            controller: Box::new(controller),
        }
    }
}

impl GameMediator for LocalGameMediator {
    fn sync(&mut self) {
        self.controller.sync();
        if self.base.ids.remaining() < 64 {
            self.controller.request_n_ids(1024);
        }
        if let Some(ids) = self.controller.get_requested_ids() {
            self.base.ids.add_ids(ids);
        }
    }

    fn get_entities(&self) -> Vec<Rc<RefCell<dyn AbstractEntity>>> {
        self.base.get_entities()
    }

    fn get_lights(&self) -> Vec<shader::LightData> {
        self.base.get_lights()
    }

    fn iter_bodies(&self, func: Box<dyn FnMut(&dyn Iterator<Item = &RigidBody<ObjectData>>)>) {
        self.base.iter_bodies(func);
    }

    fn get_lasers(&self) -> Ref<GameObject> {
        self.base.objs[&ObjectType::Laser].borrow()
    }

    fn get_lines(&self) -> Ref<primitives::Lines> {
        self.base.lines.borrow()
    }

    fn get_particles(&self) -> Ref<particles::ParticleSystem> {
        self.base.particles.borrow()
    }

    fn add_laser(&mut self, transform: Node, vel: Vector3<f64>) {
        self.base.add_laser(transform, vel);
    }

    fn update_bodies(&mut self, 
        func: Box<dyn FnMut(&mut dyn Iterator<Item = &mut RigidBody<ObjectData>>)>) 
    {
        self.base.update_bodies(func);
    }

    fn add_particle_emitter(&mut self, emitter: Box<dyn particles::Emitter>, emitter_id: usize) {
        self.base.particles.borrow_mut().new_emitter(emitter, emitter_id);
    }

    fn remove_lasers(&mut self, ids: &[ObjectId]) {
        self.base.remove_lasers(ids);
    }

    fn add_line(&mut self, line_id: u32, line: primitives::LineData) {
        self.base.add_line(line_id, line)
    }

    fn remove_line(&mut self, line_id: u32) {
        self.base.remove_line(line_id)
    }

    fn emit_particles(&self, dt: std::time::Duration) {
        self.base.emit_particles(dt);
    }
}