use super::super::instancing::*;
use super::super::shader;
use super::Emitter;
use crate::cg_support::node::Node;
use cgmath::*;
use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

pub struct Particle {
    /// Time the particle was created
    pub birth: std::time::Instant,
    pub transform: Node,
    /// Location of emitter
    pub origin: cgmath::Point3<f64>,
    pub color: cgmath::Vector4<f32>,
    pub vel: cgmath::Vector3<f64>,
    pub rot_vel: Quaternion<f64>,
    pub lifetime: std::time::Duration,
}

impl Particle {
    pub fn new(emitter_location: Point3<f64>, particle_transform: Node) -> Self {
        Self {
            birth: std::time::Instant::now(),
            transform: particle_transform,
            origin: emitter_location,
            color: vec4(0.5, 0.5, 0.5, 1.0),
            vel: vec3(0., 0., 0.),
            rot_vel: Quaternion::new(1.0, 0., 0., 0.),
            lifetime: std::time::Duration::from_secs(1),
        }
    }

    #[inline(always)]
    pub fn vel(mut self, vel: Vector3<f64>) -> Self {
        self.vel = vel;
        self
    }

    #[allow(dead_code)]
    #[inline(always)]
    pub fn rot_vel(mut self, rot_vel: Quaternion<f64>) -> Self {
        self.rot_vel = rot_vel;
        self
    }

    #[inline(always)]
    pub fn lifetime(mut self, life: std::time::Duration) -> Self {
        self.lifetime = life;
        self
    }

    #[inline(always)]
    pub fn color(mut self, color: Vector4<f32>) -> Self {
        self.color = color;
        self
    }
}

pub struct ParticleEmitter<I, S, D, Ia, G, Lg>
where
    I: Fn(&Node) -> Particle,
    S: FnMut(&mut Particle, f64),
    D: Fn(&Particle) -> bool,
    Ia: glium::Vertex + Copy,
    G: Fn(&Particle) -> Ia,
    Lg: Fn(&Particle) -> Option<shader::LightData>,
{
    pos: Node,
    /// Function that accepts the emitter transform and produces new particles
    gen_particle: I,
    /// Functions that moves the particles
    step_particle: S,
    /// Function that takes a particle and returns `true` if it can be deleted
    dead_particle: D,
    /// Gets the shader buffer data from a particle
    particle_data: G,
    num_particles: u32,
    /// The instant that the emitter stops emitting particles.
    /// This is not necessarily the time all particles are no longer visible
    emitter_end: Option<Instant>,
    particles: VecDeque<Particle>,
    instances: InstanceBuffer<Ia>,
    /// Function that turns a particle into a light source
    light_getter: Lg,
}

impl<I, S, D, Ia, G, Lg> ParticleEmitter<I, S, D, Ia, G, Lg>
where
    I: Fn(&Node) -> Particle,
    S: FnMut(&mut Particle, f64),
    D: Fn(&Particle) -> bool,
    Ia: glium::Vertex + Copy,
    G: Fn(&Particle) -> Ia,
    Lg: Fn(&Particle) -> Option<shader::LightData>,
{
    /// Creates a new particle emitter
    ///
    /// `lifetime` - The time from now that the emitter will stop generating particles, or `None` to last forever
    ///
    /// `particle_generator` - Function that accepts the emitter transform and produces new particles
    ///
    /// `particle_killer` - Function that takes a particle and returns `true` if it can be deleted
    ///
    /// `particle_stepper` - Function that moves the particles
    ///
    /// `particle_getter` - Function that gets the instance data to be sent to the shader from a particle
    ///
    /// `light_getter` - Function that gets a light source from a particle
    #[allow(clippy::too_many_arguments)]
    pub fn new<F: glium::backend::Facade>(
        pos: Node,
        lifetime: Option<Duration>,
        num: u32,
        facade: &F,
        particle_generator: I,
        particle_killer: D,
        particle_stepper: S,
        particle_getter: G,
        light_getter: Lg,
    ) -> Self {
        Self {
            pos,
            emitter_end: lifetime.map(|duration| Instant::now() + duration),
            num_particles: num,
            instances: InstanceBuffer::new_sized(num as usize, facade),
            gen_particle: particle_generator,
            dead_particle: particle_killer,
            step_particle: particle_stepper,
            particles: VecDeque::new(),
            particle_data: particle_getter,
            light_getter,
        }
    }

    fn update_instance_buffer(&mut self) {
        let v: Vec<Ia> = self
            .particles
            .iter()
            .map(|x| (self.particle_data)(x))
            .collect();
        self.instances
            .update_no_grow(&v, unsafe { std::mem::zeroed() });
    }

    /// If `particles.len() < num_partices`, generates new particles
    fn replenish_particles(&mut self) {
        if self.emitter_end.is_none() || Instant::now() < self.emitter_end.unwrap() {
            for _ in 0..self.num_particles as usize - self.particles.len() {
                self.particles.push_back((self.gen_particle)(&self.pos));
            }
        }
    }
}

impl<I, S, D, Ia, G, Lg> Emitter for ParticleEmitter<I, S, D, Ia, G, Lg>
where
    I: Fn(&Node) -> Particle,
    S: FnMut(&mut Particle, f64),
    D: Fn(&Particle) -> bool,
    Ia: glium::Vertex + Copy,
    G: Fn(&Particle) -> Ia,
    Lg: Fn(&Particle) -> Option<shader::LightData>,
{
    fn emit(&mut self, dt: Duration) {
        let mut death = HashSet::new();
        for p in self.particles.iter_mut() {
            if (self.dead_particle)(p) {
                death.insert(p as *const Particle);
            } else {
                (self.step_particle)(p, dt.as_secs_f64());
            }
        }
        self.particles
            .retain(|x| death.get(&(x as *const Particle)).is_none());
        self.replenish_particles();
        self.update_instance_buffer();
    }

    fn expired(&self) -> bool {
        self.particles.is_empty()
            && self.emitter_end.is_some()
            && Instant::now() > self.emitter_end.unwrap()
    }

    fn lights(&self) -> Option<Vec<shader::LightData>> {
        let mut v = Vec::new();
        for p in &self.particles {
            if let Some(data) = (self.light_getter)(p) {
                v.push(data)
            }
        }
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    }

    /// Gets the instance data for all particles
    fn instance_data(&self) -> glium::vertex::VerticesSource<'_> {
        From::from(
            self.instances
                .get_stored_buffer()
                .unwrap()
                .per_instance()
                .unwrap(),
        )
    }
}
