use super::controls;
use super::game_mediator::*;
use super::player;
use crate::cg_support::node;
use crate::collisions::*;
use crate::entity::AbstractEntity;
use crate::graphics_engine::particles::*;
use crate::graphics_engine::scene;
use crate::object;
use crate::physics::*;
use crate::player::Player;
use cgmath::*;
use controls::PlayerActionState;
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Encapsulates the game map and handles the logic for base game mechanics
pub struct Game<M: GameMediator> {
    mediator: RefCell<M>,
    characters: Vec<Rc<RefCell<player::Player>>>,
    // to access some character data during hit callback when characters
    // are already borrowed
    health_deltas: RefCell<HashMap<object::ObjectData, f64>>,
    player_1_base: Cell<Option<BaseRigidBody>>,

    forces: RefCell<Vec<Box<dyn Manipulator<object::ObjectData>>>>,
    dead_lasers: RefCell<Vec<shared_types::ObjectId>>,
    new_forces: RefCell<Vec<Box<dyn Manipulator<object::ObjectData>>>>,
}

impl<M: GameMediator> Game<M> {
    /// Creates a new particle emitter from an emitter factory function
    fn create_emitter<Func>(
        &self,
        emitter_factory: Func,
        a: &RigidBody<object::ObjectData>,
        b: &RigidBody<object::ObjectData>,
        hit: &HitData,
        emitter_id: usize,
    ) where
        Func: Fn(
            Point3<f64>,
            Vector3<f64>,
            Vector3<f64>,
            &glium::Display,
        ) -> Box<dyn Emitter>,
    {
        use crate::graphics_engine;
        let relative_vel = a.base.velocity - b.base.velocity;
        if relative_vel.magnitude() > 1. {
            let ctx = graphics_engine::get_active_ctx();
            let facade = ctx.ctx.borrow();
            self.mediator.borrow_mut().add_particle_emitter(
                emitter_factory(
                    hit.pos_norm_b.0,
                    hit.pos_norm_b.1,
                    relative_vel,
                    &*facade,
                ),
                emitter_id,
            );
        }
    }

    /// Callback function for when a tether shot collides with
    /// an object
    fn on_hook(
        &self,
        a: &RigidBody<object::ObjectData>,
        b: &RigidBody<object::ObjectData>,
        hit: &HitData,
        player: &BaseRigidBody,
    ) {
        use crate::graphics_engine::primitives;
        use crate::physics;

        let target = if a.metadata.0 == object::ObjectType::Hook {
            b
        } else {
            a
        };
        let hit_point = if a.metadata.0 == object::ObjectType::Hook {
            hit.pos_norm_b.0
        } else {
            hit.pos_norm_a.0
        };
        let ship_front = player
            .transform
            .borrow()
            .transform_point(point3(0., 0., 8.));
        let hit_local = target
            .base
            .transform
            .borrow()
            .mat()
            .invert()
            .unwrap()
            .transform_point(hit_point);
        self.mediator.borrow_mut().add_line(
            0,
            primitives::LineData {
                color: [1., 0., 0., 1.],
                start: node::Node::default()
                    .parent(player.transform.clone())
                    .pos(point3(0., 0., 10.)),
                end: node::Node::default()
                    .parent(target.base.transform.clone())
                    .pos(hit_local),
            },
        );
        self.new_forces
            .borrow_mut()
            .push(Box::new(physics::Tether::new(physics::TetherData {
                attach_a: hit_local,
                a: Rc::downgrade(&target.base.transform),
                attach_b: ship_front,
                b: Rc::downgrade(&player.transform),
                length: (ship_front - hit_point).magnitude(),
            })));
    }

    /// Checks if a `check` is a character that was involved in a collision, and if so,
    /// reduces the player's health accordingly
    fn check_player_hit(
        &self,
        check: &RigidBody<object::ObjectData>,
        collider: &RigidBody<object::ObjectData>,
    ) {
        const SHIELD_DAMAGE_FAC: f64 = -0.01;
        if let Some((_, shield_delta)) = self
            .health_deltas
            .borrow_mut()
            .iter_mut()
            .find(|(key, _)| **key == collider.metadata)
        {
            if check.metadata.0 == object::ObjectType::Laser {
                *shield_delta -= 10.;
            } else if check.metadata.0 != object::ObjectType::Hook {
                *shield_delta -= SHIELD_DAMAGE_FAC
                    * (check.base.velocity - collider.base.velocity)
                        .magnitude();
            }
        }
    }

    /// Callback function for when two objects collide
    pub fn on_hit(
        &self,
        a: &RigidBody<object::ObjectData>,
        b: &RigidBody<object::ObjectData>,
        hit: &HitData,
    ) {
        use object::ObjectType::*;
        if a.metadata.0 == Laser || b.metadata.0 == Laser {
            self.create_emitter(
                laser_hit_emitter::<glium::Display>,
                a,
                b,
                hit,
                0,
            );
            let lt = if a.metadata.0 == Laser {
                a.metadata.1
            } else {
                b.metadata.1
            };
            self.dead_lasers.borrow_mut().push(lt);
        }
        if a.metadata.0 == Ship && b.metadata.0 == Asteroid
            || a.metadata.0 == Asteroid && b.metadata.0 == Ship
        {
            self.create_emitter(
                asteroid_hit_emitter::<glium::Display>,
                a,
                b,
                hit,
                1,
            );
        }
        if a.metadata.0 == Hook || b.metadata.0 == Hook {
            // TODO: allow multiple hooks
            let p1_base = self.player_1_base.take();
            self.on_hook(a, b, hit, p1_base.as_ref().unwrap());
            self.player_1_base.replace(p1_base);
            let lt = if a.metadata.0 == Hook {
                a.metadata.1
            } else {
                b.metadata.1
            };
            self.dead_lasers.borrow_mut().push(lt);
        }
        self.check_player_hit(a, b);
        self.check_player_hit(b, a);
    }

    /// Callback function for physics simulation
    ///
    /// Returns `true` if the simulation should do physical collision
    /// resolution
    pub const fn should_resolve(
        a: &RigidBody<object::ObjectData>,
        b: &RigidBody<object::ObjectData>,
        _: &HitData,
    ) -> bool {
        use object::ObjectType::*;
        #[allow(clippy::unnested_or_patterns)]
        !matches!(
            (a.metadata.0, b.metadata.0),
            (Hook, _) | (_, Hook) | (Laser, _) | (_, Laser)
        )
    }

    /// Steps the simulation `sim`, `dt` into the future
    fn step_sim<'a, 'b>(
        &self,
        sim: &mut Simulation<'a, 'b, object::ObjectData>,
        dt: std::time::Duration,
    ) {
        self.forces
            .borrow_mut()
            .append(&mut self.new_forces.borrow_mut());
        let mut characters: Vec<_> =
            self.characters.iter().map(|p| p.borrow_mut()).collect();

        // Reset health deltas
        self.health_deltas.borrow_mut().clear();
        for c in &characters {
            self.health_deltas
                .borrow_mut()
                .insert(c.get_rigid_body().metadata, 0.);
        }

        self.player_1_base
            .set(Some(characters[0].get_rigid_body().base.clone()));

        let forces = &self.forces.borrow();
        let objects: Vec<_> = self.mediator.borrow().game_objects().collect();
        let mut borrows: Vec<_> =
            objects.iter().map(|o| o.borrow_mut()).collect();
        let mut bodies: Vec<_> = borrows
            .iter_mut()
            .flat_map(|o| o.bodies_ref().into_iter())
            .collect();
        for c in &mut characters {
            bodies.push(c.update_rigid_body(dt));
        }
        let resolvers = {
            let v = unsafe {
                &*(bodies.as_slice()
                    as *const [&mut RigidBody<(
                        shared_types::ObjectType,
                        shared_types::ObjectId,
                    )>] as *const [&_])
            };
            sim.calc_resolvers(v, forces, dt)
        };
        Simulation::apply_resolvers(&mut bodies, &resolvers, dt);

        // Updates players' health
        for c in &mut characters {
            let index = c.get_rigid_body().metadata;
            c.change_shield(self.health_deltas.borrow()[&index]);
        }
    }

    /// Function thet should be called every frame to handle shooting lasers
    fn handle_shots(user: &mut player::Player, mediator: &mut M) {
        const ENERGY_PER_SHOT: f64 = 1.;
        if matches!(
            user.get_action_state(),
            PlayerActionState::Fire | PlayerActionState::FireRope
        ) && user.energy() > ENERGY_PER_SHOT
        {
            let mut transform = user
                .root()
                .borrow()
                .clone()
                .scale(cgmath::vec3(0.3, 0.3, 1.));
            transform.translate(user.forward() * 10.);
            let (typ, speed) =
                if user.get_action_state() == PlayerActionState::FireRope {
                    (object::ObjectType::Hook, 200.)
                } else {
                    (object::ObjectType::Laser, 120.)
                };
            mediator.add_laser(transform, user.forward() * speed, typ);
            user.change_energy(-ENERGY_PER_SHOT);
        }
        user.transition_action_state();
    }

    /// Callback function for when a frame is drawn
    pub fn on_draw<'a, 'b>(
        &self,
        sim: &mut Simulation<'a, 'b, object::ObjectData>,
        dt: std::time::Duration,
        scene: &mut dyn scene::AbstractScene,
    ) {
        use controls::PlayerIteratorHolder;
        self.mediator.borrow_mut().sync();
        self.dead_lasers.borrow_mut().clear();
        for player in &self.characters {
            let mut u = player.borrow_mut();
            if u.get_action_state() == PlayerActionState::CutRope {
                self.forces.borrow_mut().clear();
                self.mediator.borrow_mut().remove_line(0);
            }
            Self::handle_shots(&mut *u, &mut self.mediator.borrow_mut());
        }
        self.step_sim(sim, dt);

        self.mediator
            .borrow_mut()
            .remove_lasers(&self.dead_lasers.borrow());
        self.mediator.borrow_mut().emit_particles(dt);
        scene.set_lights(&self.mediator.borrow().get_lights());

        let it = self.characters.iter();
        let mut actions = HashMap::new();
        for (player, idx) in self.characters.iter().zip(0..) {
            if let Some(action) = player.borrow_mut().on_controller_tick(
                sim.get_collision_tree(),
                dt,
                &PlayerIteratorHolder(Box::new(
                    it.clone()
                        .filter(|p| !Rc::ptr_eq(p, player))
                        .map(|p| p.borrow().get_node()),
                )),
            ) {
                actions.insert(idx, action);
            }
        }
        for (idx, action) in actions {
            self.characters[idx]
                .borrow_mut()
                .get_rigid_body_mut()
                .base
                .velocity = action.velocity;
        }
    }

    pub fn get_mediator(&self) -> std::cell::Ref<M> {
        self.mediator.borrow()
    }

    // #[inline]
    // pub fn get_entities(&self) -> Vec<Rc<RefCell<dyn AbstractEntity>>> {
    //     self.mediator.borrow().get_entities()
    // }

    /// Returns the player who view we are currently rendering
    pub fn player_1(&self) -> Rc<RefCell<Player>> {
        self.characters[0].clone()
    }

    /// Gets all of the players' entity representations
    pub fn get_player_entities(&self) -> Vec<Rc<RefCell<dyn AbstractEntity>>> {
        self.characters
            .iter()
            .map(|p| p.borrow().as_entity() as Rc<RefCell<dyn AbstractEntity>>)
            .collect()
    }
}

impl<M: GameMediatorLightingAvailable> Game<M> {
    pub fn new(mediator: M, player: player::Player) -> Self {
        Self {
            mediator: RefCell::new(mediator),
            characters: vec![Rc::new(RefCell::new(player))],
            forces: RefCell::default(),
            dead_lasers: RefCell::new(Vec::new()),
            new_forces: RefCell::new(Vec::new()),
            health_deltas: RefCell::new(HashMap::new()),
            player_1_base: Cell::default(),
        }
    }

    pub fn add_character(&mut self, player: Rc<RefCell<player::Player>>) {
        self.characters.push(player);
    }

    pub fn get_lighting(
        self,
    ) -> (super::shader::PbrMaps, Vector3<f32>, Game<M::ReturnType>)
    where
        <M as GameMediatorLightingAvailable>::ReturnType: GameMediator,
    {
        let (maps, vec, mediator) = self.mediator.into_inner().lighting_info();
        (
            maps,
            vec,
            Game {
                mediator: RefCell::new(mediator),
                characters: self.characters,
                forces: self.forces,
                dead_lasers: self.dead_lasers,
                new_forces: self.new_forces,
                health_deltas: self.health_deltas,
                player_1_base: self.player_1_base,
            },
        )
    }
}
