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
}

pub struct ParticleEmitter<I, S, D> where
    I : Fn(&Node) -> Particle,
    S : FnMut(&mut Particle, f64),
    D : Fn(&Particle) -> bool,
{
    pos: Node,
    /// Function that accepts the emitter transform and produces new particles
    gen_particle: I,
    /// Functions that moves the particles
    step_particle: S,
    /// Function that takes a particle and returns `true` if it can be deleted
    dead_particle: D,
    num_particles: u32,
    /// The instant that the emitter stops emitting particles.
    /// This is not necessarily the time all particles are no longer visible
    emitter_end: Instant,
    particles: VecDeque<Particle>,
    particle_model: Box<dyn Drawable>,
    instances: InstanceBuffer<InstanceAttributes>,
}

impl<I, S, D> ParticleEmitter<I, S, D> where
    I : Fn(&Node) -> Particle,
    S : FnMut(&mut Particle, f64),
    D : Fn(&Particle) -> bool,
{
    /// Creates a new particle emitter
    /// 
    /// `lifetime` - The time from now that the emitter will stop generating particles
    /// 
    /// `particle_generator` - Function that accepts the emitter transform and produces new particles
    /// 
    /// `particle_killer` - Function that takes a particle and returns `true` if it can be deleted
    /// 
    /// `particle_stepper` - Functions that moves the particles
    fn new<F : glium::backend::Facade>(pos: Node, lifetime: Duration, num: u32, particle: Box<dyn Drawable>, facade: &F,
        particle_generator: I, particle_killer: D, particle_stepper: S) -> Self
    {
        Self {
            pos, emitter_end: Instant::now() + lifetime,
            num_particles: num,
            instances: InstanceBuffer::new_sized(num as usize, facade),
            gen_particle: particle_generator,
            dead_particle: particle_killer,
            step_particle: particle_stepper,
            particle_model: particle,
            particles: VecDeque::new(),
        }
    }
}


impl<I, S, D> Drawable for ParticleEmitter<I, S, D> where
    I : Fn(&Node) -> Particle,
    S : FnMut(&mut Particle, f64),
    D : Fn(&Particle) -> bool,
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

impl<I, S, D> Emitter for ParticleEmitter<I, S, D> where
    I : Fn(&Node) -> Particle,
    S : FnMut(&mut Particle, f64),
    D : Fn(&Particle) -> bool,
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

        if Instant::now() < self.emitter_end {
            for _ in 0 .. self.num_particles as usize - self.particles.len() {
                self.particles.push_back((self.gen_particle)(&self.pos));
            }
        }
    }

    fn expired(&self) -> bool {
        self.particles.is_empty() && Instant::now() > self.emitter_end
    }
    
    fn lights(&self) -> Option<Vec<shader::LightData>> {
        None,
    }
}