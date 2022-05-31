use crate::graphics_engine::map;
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
use crate::collisions;
use crate::physics;


pub trait GameMap : map::Map<object::ObjectType> {
    fn get_lasers(&self) -> &Rc<RefCell<GameObject>>;

    fn get_lines(&self) -> &Rc<RefCell<primitives::Lines>>;

    fn get_particles(&self) -> &Rc<RefCell<particles::ParticleSystem>>;
}


pub struct AsteroidMap {
    lasers: Rc<RefCell<GameObject>>,
    entities: Vec<Rc<RefCell<dyn AbstractEntity>>>,
    objects: RefCell<Vec<GameObject>>,
    lines: Rc<RefCell<primitives::Lines>>,
    particles: Rc<RefCell<particles::ParticleSystem>>,
    ibl_maps: Cell<Option<shader::PbrMaps>>,
}

impl AsteroidMap {
    fn gen_asteroid_field(asteroid_obj : &mut GameObject) {
        use rand::distributions::*;
        let scale_distrib = rand::distributions::Uniform::from(0.01 .. 0.3);
        let pos_distrib = rand::distributions::Uniform::from(-100.0 .. 100.0);
        let angle_distrib = rand::distributions::Uniform::from(0.0 .. 360.0);
        let mut rng = rand::thread_rng();
        for _ in 0 .. 100 {
            let scale = scale_distrib.sample(&mut rng);
            let axis = vec3(pos_distrib.sample(&mut rng), 
                pos_distrib.sample(&mut rng), pos_distrib.sample(&mut rng))
                .normalize();
            let rot = Quaternion::<f64>::from_axis_angle(axis, 
                Deg::<f64>(angle_distrib.sample(&mut rng)));
            let transform = Node::new(Some(
                point3(pos_distrib.sample(&mut rng), 
                pos_distrib.sample(&mut rng), 
                pos_distrib.sample(&mut rng))),
                Some(rot), Some(vec3(scale, scale, scale)), None);
            asteroid_obj.new_instance(transform, None);
        }
    }


    fn make_objects<F : glium::backend::Facade>(_ : &shader::ShaderManager, ctx: &F) 
        -> Vec<GameObject> 
    {
        let mut asteroid = object::GameObject::new(
            model::Model::new("assets/asteroid1/Asteroid.obj", ctx)
            .with_instancing(), 
        object::ObjectType::Asteroid).with_depth()
        .with_collisions("assets/asteroid1/Asteroid.obj", 
            collisions::TreeStopCriteria::default()).density(2.71);

        Self::gen_asteroid_field(&mut asteroid);

        let container = object::GameObject::new(
            model::Model::new("assets/BlackMarble/floor.obj", ctx), 
                object::ObjectType::Any) 
            .at_pos(Node::new(Some(point3(0., -5., 0.)), None, 
                Some(vec3(20., 1., 20.)), None)).with_depth();
        vec![asteroid, container]

    }

    fn make_entities<F : glium::backend::Facade>(sm : &shader::ShaderManager, ctx: &F)
        -> (Vec<Rc<RefCell<dyn AbstractEntity>>>, shader::PbrMaps)
    {
        let mut asteroid_character = object::AnimGameObject::new(
            model::Model::new("assets/animTest/dancing_vampire.dae", ctx))
            .with_depth();
        asteroid_character.transform().borrow_mut()
            .set_scale(vec3(0.07, 0.07, 0.07));
        asteroid_character.start_anim("", true);
        let mut skybox = cubes::Skybox::cvt_from_sphere(
            "assets/Milkyway/Milkyway_BG.jpg", 2048, sm, ctx);
        let ibl = scene::gen_ibl_from_hdr("assets/Milkyway/Milkyway_Light.hdr", 
            &mut skybox, sm, ctx);
        let sky_entity = Rc::new(RefCell::new(skybox.to_entity()));
        let cloud = entity::EntityBuilder::new(cubes::Volumetric::cloud(128, ctx))
            .at(Node::default().pos(point3(15., 5., 5.)).u_scale(15.))
            .with_pass(shader::RenderPassType::Visual)
            .render_order(entity::RenderOrder::Last).build();
        let cloud = Rc::new(RefCell::new(cloud));
        (vec![sky_entity, asteroid_character.to_entity(), cloud], ibl)
    }

    pub fn new<F : glium::backend::Facade>(sm: &shader::ShaderManager, ctx: &F) 
        -> Self 
    {
        let lasers = Rc::new(RefCell::new(object::GameObject::new(
            model::Model::new("assets/laser2.obj", ctx).with_instancing(), 
            object::ObjectType::Laser).with_collisions("assets/laser2.obj", 
                collisions::TreeStopCriteria::default())));
        let lines = Rc::new(RefCell::new(primitives::Lines::new(ctx)));
        let particles = Rc::new(RefCell::new(particles::ParticleSystem::new()
            .with_billboard("assets/particles/smoke_01.png", 0.4)
            .with_billboard("assets/particles/circle_05.png", 0.4)));
        let (entities, ibl) = Self::make_entities(sm, ctx);

        Self {
            lasers, lines, particles, entities,
            objects: RefCell::new(Self::make_objects(sm, ctx)),
            ibl_maps: Cell::new(Some(ibl))
        }
    }
}

impl map::Map<object::ObjectType> for AsteroidMap {
        fn entities(&self) -> Vec<Rc<RefCell<dyn AbstractEntity>>> {
            let obj = self.objects.borrow();
            let it = obj.iter().map(|b| 
                (b.as_entity().clone() as Rc<RefCell<dyn AbstractEntity>>));
            let mut v : Vec<_> = self.entities.iter().map(|e| e.clone())
                .chain(it).collect();
            v.push(self.lasers.borrow().as_entity());
            v.push(self.lines.clone());
            v.push(self.particles.clone());
            v
        }

        fn lights(&self) -> Vec<shader::LightData> {
            let mut lights = Vec::new();
            self.lasers.borrow().iter_positions(|node| {
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

        /// Gets the rigid bodies in this map
        fn iter_bodies<'a>(&self, 
            mut func: Box<dyn FnMut(&mut dyn Iterator<Item = 
                &mut physics::RigidBody<object::ObjectType>>) + 'a>)
        {
            let mut lzs = self.lasers.borrow_mut();
            func(&mut self.objects.borrow_mut().iter_mut()
                .flat_map(|o| o.bodies_ref().into_iter())
                .chain(lzs.bodies_ref()))
        }
    
        fn lighting_info(&self) -> (shader::PbrMaps, cgmath::Vector3<f32>) {
            let ibl = self.ibl_maps.take().unwrap();
            (ibl, vec3(-1., -1., -1.))
        }
}

impl GameMap for AsteroidMap {

    fn get_lasers(&self) -> &Rc<RefCell<GameObject>> { &self.lasers }

    fn get_lines(&self) -> &Rc<RefCell<primitives::Lines>> { &self.lines }

    fn get_particles(&self) -> &Rc<RefCell<particles::ParticleSystem>> 
        { &self.particles}
}