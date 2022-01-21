use super::super::drawable::*;
use crate::cg_support::node::Node;
use cgmath::*;
use super::super::shader;
use super::super::instancing::*;
use std::collections::{VecDeque, HashSet};
use std::time::{Instant, Duration};
use super::Emitter;

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

pub struct ParticleEmitter<I, S, D, Ia, G> where
    I : Fn(&Node) -> Particle,
    S : FnMut(&mut Particle, f64),
    D : Fn(&Particle) -> bool,
    Ia : glium::Vertex + Copy,
    G : Fn(&Particle) -> Ia,
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
    particle_model: Box<dyn Drawable>,
    instances: InstanceBuffer<Ia>,
}

impl<I, S, D, Ia, G> ParticleEmitter<I, S, D, Ia, G> where
    I : Fn(&Node) -> Particle,
    S : FnMut(&mut Particle, f64),
    D : Fn(&Particle) -> bool,
    Ia : glium::Vertex + Copy,
    G : Fn(&Particle) -> Ia,
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
    pub fn new<F : glium::backend::Facade>(pos: Node, lifetime: Option<Duration>, num: u32, particle: Box<dyn Drawable>, facade: &F,
        particle_generator: I, particle_killer: D, particle_stepper: S, particle_getter: G) -> Self
    {
        Self {
            pos, emitter_end: lifetime.map(|duration| Instant::now() + duration),
            num_particles: num,
            instances: InstanceBuffer::new_sized(num as usize, facade),
            gen_particle: particle_generator,
            dead_particle: particle_killer,
            step_particle: particle_stepper,
            particle_model: particle,
            particles: VecDeque::new(),
            particle_data: particle_getter,
        }
    }

    fn update_instance_buffer(&mut self) {
        let v : Vec<Ia> = self.particles.iter().map(|x| (self.particle_data)(x)).collect();
        self.instances.update_no_grow(&v, unsafe { std::mem::zeroed() });
    }

    /// If `particles.len() < num_partices`, generates new particles
    fn replenish_particles(&mut self) {
        if self.emitter_end.is_none() || Instant::now() < self.emitter_end.unwrap() {
            for _ in 0 .. self.num_particles as usize - self.particles.len() {
                self.particles.push_back((self.gen_particle)(&self.pos));
            }
        }
    }
}


impl<I, S, D, Ia, G> Drawable for ParticleEmitter<I, S, D, Ia, G> where
    I : Fn(&Node) -> Particle,
    S : FnMut(&mut Particle, f64),
    D : Fn(&Particle) -> bool,
    Ia : glium::Vertex + Copy,
    G : Fn(&Particle) -> Ia,
{
    fn render_args<'a>(&'a mut self, p: &[[[f32; 4]; 4]]) -> Vec<(shader::UniformInfo<'a>, VertexHolder<'a>, glium::index::IndicesSource<'a>)>
    {
        let mut v = Vec::new();
        for (u, holder, i) in self.particle_model.render_args(p) {
            v.push((u, 
                holder.append(From::from(self.instances.get_stored_buffer().unwrap().per_instance().unwrap())), 
                i));
        }
        v
    }
}

impl<I, S, D, Ia, G> Emitter for ParticleEmitter<I, S, D, Ia, G> where
    I : Fn(&Node) -> Particle,
    S : FnMut(&mut Particle, f64),
    D : Fn(&Particle) -> bool,
    Ia : glium::Vertex + Copy,
    G : Fn(&Particle) -> Ia,
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
        self.particles.retain(|x| 
            death.get(&(x as *const Particle)).is_none()
        );
        self.replenish_particles();
        self.update_instance_buffer();
    }

    fn expired(&self) -> bool {
        self.particles.is_empty() && self.emitter_end.is_some() && 
            Instant::now() > self.emitter_end.unwrap()
    }
    
    fn lights(&self) -> Option<Vec<shader::LightData>> {
        None
    }
}