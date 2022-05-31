use crate::physics::*;
use crate::object;
use super::game_map::*;
use std::rc::Rc;
use std::cell::RefCell;
use super::player;
use crate::cg_support::node;
use cgmath::*;
use crate::collisions::*;
use super::controls;
use crate::graphics_engine::scene;
use crate::graphics_engine::particles::*;

pub struct Game<'c> {
    map: Box<dyn GameMap + 'c>,
    pub player: Rc<RefCell<player::Player>>,
    forces: RefCell<Vec<Box<dyn Manipulator<object::ObjectType>>>>,
    dead_lasers: RefCell<Vec<Rc<RefCell<node::Node>>>>,
    new_forces: RefCell<Vec<Box<dyn Manipulator<object::ObjectType>>>>,
}

impl<'c> Game<'c> {
    pub fn new<M : GameMap + 'c>(map: M, player: player::Player) -> Self {
        Self {
            map: Box::new(map),
            player: Rc::new(RefCell::new(player)),
            forces: RefCell::default(),
            dead_lasers: RefCell::new(Vec::new()),
            new_forces: RefCell::new(Vec::new()),
        }
    }

    fn create_emitter<Func>(&self, emitter_factory: Func, 
        a: &RigidBody<object::ObjectType>, b: &RigidBody<object::ObjectType>, 
        hit: &HitData, emitter_id: usize)
        where Func : Fn(Point3<f64>,Vector3<f64>,
                Vector3<f64>, &glium::Display) -> Box<dyn Emitter>
    {
        use crate::graphics_engine;
        let relative_vel = a.base.velocity - b.base.velocity;
        if relative_vel.magnitude() > 1. {
            let ctx = graphics_engine::get_active_ctx();
            let facade = ctx.ctx.borrow();
            self.map.as_ref().get_particles().borrow_mut().new_emitter(
                emitter_factory(hit.pos_norm_b.0, 
                    hit.pos_norm_b.1, relative_vel, &*facade), emitter_id
            );
        }
    }

    fn on_hook(&self, a: &RigidBody<object::ObjectType>, 
        b: &RigidBody<object::ObjectType>, hit: &HitData, 
        player: &BaseRigidBody)
    {
        use crate::graphics_engine::primitives;
        use crate::physics;

        let target = if a.metadata == object::ObjectType::Hook 
            { b } else { a };
        let hit_point = if a.metadata == object::ObjectType::Hook 
            { hit.pos_norm_b.0 } else { hit.pos_norm_a.0 };
        let ship_front = player.transform.borrow().transform_point(point3(0., 0., 8.));
        let hit_local = target.base.transform.borrow().mat()
            .invert().unwrap().transform_point(hit_point);
        self.map.as_ref().get_lines().borrow_mut().add_line(0, primitives::LineData {
            color: [1., 0., 0., 1.],
            start: node::Node::default().parent(player.transform.clone())
                .pos(point3(0., 0., 10.)),
            end: node::Node::default().parent(target.base.transform.clone())
                .pos(hit_local),
        });
        self.new_forces.borrow_mut().push(Box::new(physics::Tether::new( 
        physics::TetherData {
            attach_a: hit_local,
            a: Rc::downgrade(&target.base.transform),
            attach_b: ship_front,
            b: Rc::downgrade(&player.transform),
            length: (ship_front - hit_point).magnitude()
        })));
    }

    pub fn on_hit(&self, a: &RigidBody<object::ObjectType>, 
        b: &RigidBody<object::ObjectType>, hit: &HitData, 
        player: &BaseRigidBody)
    {
        use object::ObjectType::*;
        if a.metadata == Laser || b.metadata == Laser {
            self.create_emitter(laser_hit_emitter::<glium::Display>, 
                a, b, hit, 0);
            let lt = if a.metadata == Laser { &a.base.transform }
            else { &b.base.transform }.clone();
            self.dead_lasers.borrow_mut().push(lt);
        }
        if a.metadata == Asteroid || b.metadata == Asteroid {
            self.create_emitter(asteroid_hit_emitter::<glium::Display>, 
                a, b, hit, 1);
        }
        if a.metadata == Hook || b.metadata == Hook {
            self.on_hook(a, b, hit, player);
            let lt = if a.metadata == Hook { &a.base.transform }
            else { &b.base.transform }.clone();
            self.dead_lasers.borrow_mut().push(lt);
        }

    }

    pub fn should_resolve(a: &RigidBody<object::ObjectType>, 
        b: &RigidBody<object::ObjectType>, _: &HitData) -> bool 
    {
        use object::ObjectType::*;
        match (a.metadata, b.metadata) {
            (Hook, _) | (_, Hook) | (Laser, _) | (_, Laser) => false,
            _ => true,
        }
    }

    fn step_sim<'a, 'b>(&self, 
        sim: &mut Simulation<'a, 'b, object::ObjectType>,
        controls: &controls::PlayerControls, dt: std::time::Duration) 
    {
        self.forces.borrow_mut().append(&mut self.new_forces.borrow_mut());
        let mut u = self.player.borrow_mut();
        let forces = &self.forces.borrow();
        self.map.as_ref().iter_bodies(Box::new(|it| {
            let mut v : Vec<_> = it.collect();
            v.push(u.as_rigid_body(controls));
            let p_idx = v.len() - 1;
            sim.step(&mut v, forces, p_idx, dt);
        }));
    }

    fn handle_shots(user: &player::Player, controller: &controls::PlayerControls, 
        lasers: &mut object::GameObject) 
    {
        if controller.fire {
            let mut transform = user.root().borrow().clone()
                .scale(cgmath::vec3(0.3, 0.3, 1.));
            transform.translate(user.forward() * 10.);
            let (typ, speed) = if controller.fire_rope 
                { (object::ObjectType::Hook, 200.) }
            else 
                { (object::ObjectType::Laser, 120.) };
            lasers.new_instance(transform, Some(user.forward() * speed))
                .metadata = typ;
        }
    }

    pub fn on_draw<'a, 'b>(&self,
        sim: &mut Simulation<'a, 'b, object::ObjectType>,
        dt : std::time::Duration, scene : &mut scene::Scene,
        controller: &mut controls::PlayerControls)
    {
        *self.player.borrow().trans_fac() = controller.compute_transparency_fac();
        self.dead_lasers.borrow_mut().clear();
        {
            if controller.cut_rope {
                self.forces.borrow_mut().clear();
                self.map.as_ref().get_lines().borrow_mut().remove_line(0);
            }
            let mut lz = self.map.as_ref().get_lasers().borrow_mut();
            let u = self.player.borrow();
            Self::handle_shots(&*u, controller, &mut *lz);
        }
        self.step_sim(sim, controller, dt);
        self.map.as_ref().get_lasers().borrow_mut().retain(|laser_ptr|
            !self.dead_lasers.borrow().iter().any(
                |dead| dead.as_ptr() as *const () == laser_ptr));
        self.map.as_ref().get_particles().borrow_mut().emit(dt);
        scene.set_lights(&self.map.as_ref().lights());
    }

    pub fn get_map(&self) -> &dyn GameMap
    { self.map.as_ref() }
}