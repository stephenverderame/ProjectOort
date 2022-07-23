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
use std::ops::Range;

/// A Map contains the entities and lighting information of a scene
pub trait Map<T> {
    /// Gets the entities in this map
    fn entities(&self) -> Vec<Rc<RefCell<dyn AbstractEntity>>>;

    /// Gets the lights in this map
    fn lights(&self) -> Vec<shader::LightData>;

    /// Gets the rigid bodies in this map
    fn iter_bodies<'a>(&self, 
        func: Box<dyn FnMut(&mut dyn Iterator<Item = &mut physics::RigidBody<T>>) + 'a>);

    /// Gets the map's IBL maps and directional light direction
    fn lighting_info(&self) -> (shader::PbrMaps, cgmath::Vector3<f32>);
}


pub trait GameMap : Map<object::ObjectType> {
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
    /// Generates `count` number of transforms all arranged around `center` in
    /// a sphere. Calls `func`, passing in each transform
    /// 
    /// `radius` - the range of radii to place objects
    /// 
    /// `theta` - the range of radians to place objects around (horizontally)
    /// 
    /// `phi` - the range of radians to place objects from the zenith
    /// 
    /// `scale` - the range of uniform scale factors
    fn randomize_spherical<F>(center: Point3<f64>, radius: Range<f64>, 
        theta: Range<f64>, phi: Range<f64>, scale: Range<f64>, 
        count: usize, mut func : F)
        where F : FnMut(Node)
    {
        use rand::distributions::*;
        let radius_distrib = Uniform::from(radius);
        let theta_distrib = Uniform::from(theta);
        let phi_distrib = Uniform::from(phi);
        let scale_distrib = Uniform::from(scale);
        let axis_distrib = Uniform::from(-1.0 .. 1.0);
        let angle_distrib = Uniform::from(0.0 .. 360.0);
        let (mut rng_r, mut rng_t, mut rng_p, mut rng_s, mut rng_a) = 
            (rand::thread_rng(), rand::thread_rng(), 
            rand::thread_rng(), rand::thread_rng(), rand::thread_rng());
        for (((radius, theta), phi), scale) in radius_distrib.sample_iter(&mut rng_r)
            .zip(theta_distrib.sample_iter(&mut rng_t))
            .zip(phi_distrib.sample_iter(&mut rng_p))
            .zip(scale_distrib.sample_iter(&mut rng_s))
            .take(count)
        {
            let x = vec3(phi.sin() * theta.cos(), phi.sin() * theta.sin(),
                phi.cos()) * radius;
            let pos : [f64; 3] = (center.to_vec() + x).into();
            let axis = vec3(axis_distrib.sample(&mut rng_a), 
                axis_distrib.sample(&mut rng_a), 
                axis_distrib.sample(&mut rng_a)).normalize();
            let rot = Quaternion::<f64>::from_axis_angle(axis, 
                Deg::<f64>(angle_distrib.sample(&mut rng_a))).normalize();
            let n = Node::default().pos(pos.into()).u_scale(scale)
                .rot(rot);
            func(n);
        }
    }


    /// Constructs the GameObjects in this map
    fn make_objects<F : glium::backend::Facade>(_ : &shader::ShaderManager, ctx: &F) 
        -> Vec<GameObject> 
    {
        use std::f64::consts::PI;
        let mut asteroid = object::GameObject::new(
            model::Model::new("assets/asteroid1/Asteroid.obj", ctx)
            .with_instancing(), 
        object::ObjectType::Asteroid).with_depth()
        .with_collisions("assets/asteroid1/Asteroid.obj", 
            collisions::TreeStopCriteria::default()).density(2.71);

        Self::randomize_spherical(point3(0., 0., 0.), 120. .. 600., 0. .. 2. * PI, 
            0. .. PI, 0.002 .. 0.8, 100, 
            |t| { asteroid.new_instance(t, None); });

        let planet = object::GameObject::new(
            model::Model::new("assets/planet/planet1.obj", ctx), 
                object::ObjectType::Any) 
            .at_pos(Node::default().u_scale(5.)).with_depth()
            .with_collisions("assets/planet/planet1.obj", Default::default())
            .immobile().density(10.);
        /*let test_floor = object::GameObject::new(
            model::Model::new("assets/BlackMarble/floor.obj", ctx), object::ObjectType::Any) 
            .at_pos(Node::new(Some(point3(0., -5., 0.)), None, Some(vec3(20., 1., 20.)), None)).with_depth();*/
        vec![asteroid, planet/* , test_floor*/]

    }

    /// Constructs entities of this map
    /// 
    /// Returns a tuple of entities in this map, and the IBL of the map
    fn make_entities<F : glium::backend::Facade>(sm : &shader::ShaderManager, ctx: &F)
        -> (Vec<Rc<RefCell<dyn AbstractEntity>>>, shader::PbrMaps)
    {
        use std::f64::consts::PI;
        let mut asteroid_character = object::AnimGameObject::new(
            model::Model::new("assets/animTest/dancing_vampire.dae", ctx))
            .with_depth();
        asteroid_character.transform().borrow_mut()
            .set_scale(vec3(0.07, 0.07, 0.07));
        asteroid_character.transform().borrow_mut()
            .set_pos(point3(100., 100., -100.));
        asteroid_character.start_anim("", true);
        let mut skybox = cubes::Skybox::cvt_from_sphere(
            "assets/Milkyway/Milkyway_BG.jpg", 2048, sm, ctx);
        let ibl = scene::gen_ibl_from_hdr("assets/Milkyway/Milkyway_Light.hdr", 
            &mut skybox, sm, ctx);
        let sky_entity = Rc::new(RefCell::new(skybox.to_entity()));
        let mut cloud = entity::EntityBuilder::new(cubes::Volumetric::cloud(128, ctx))
            //.at(Node::default().u_scale(80.))
            .with_pass(shader::RenderPassType::Visual)
            .render_order(entity::RenderOrder::Last).build();

        Self::randomize_spherical(point3(0., 0., 0.), 80. .. 100., 0. .. 2. * PI, 
            0. .. PI, 5.0 .. 70., 30, 
            |t| cloud.locations.push(Rc::new(RefCell::new(t))));

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

impl Map<object::ObjectType> for AsteroidMap {
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
            (ibl, vec3(-2396.8399272563433, -1668.5529287640434, 
                3637.5010772434753).normalize())
        }
}

impl GameMap for AsteroidMap {

    fn get_lasers(&self) -> &Rc<RefCell<GameObject>> { &self.lasers }

    fn get_lines(&self) -> &Rc<RefCell<primitives::Lines>> { &self.lines }

    fn get_particles(&self) -> &Rc<RefCell<particles::ParticleSystem>> 
        { &self.particles}
}