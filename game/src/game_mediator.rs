use std::rc::Rc;
use std::cell::{RefCell, Cell};
use super::object::*;
use crate::graphics_engine::entity::*;
use crate::graphics_engine::{primitives, cubes, model, entity, shader, 
    scene, particles};
use crate::cg_support::node::*;
use glium;
use super::object;
use cgmath::*;
use crate::physics;
use std::collections::HashMap;
use crate::collisions;

pub trait GameMediator {
    fn get_entities(&self) -> Vec<Rc<RefCell<dyn AbstractEntity>>>;

    /// Gets the lights in this map
    fn get_lights(&self) -> Vec<shader::LightData>;

    /// Gets the rigid bodies in this map
    fn get_bodies(&self) -> Vec<physics::RigidBody<object::ObjectType>>;

    fn get_lasers(&self) -> &Rc<RefCell<GameObject>>;

    fn get_lines(&self) -> &Rc<RefCell<primitives::Lines>>;

    fn get_particles(&self) -> &Rc<RefCell<particles::ParticleSystem>>;

}

struct GameMediatorBase {
    objs: HashMap<ObjectType, Rc<RefCell<GameObject>>>,
    entity: HashMap<ObjectType, Rc<RefCell<dyn AbstractEntity>>>,
    lines: Rc<RefCell<primitives::Lines>>,
    particles: Rc<RefCell<particles::ParticleSystem>>,
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
        }
    }
}