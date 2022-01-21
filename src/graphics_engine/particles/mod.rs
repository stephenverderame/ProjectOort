mod particle;
mod system;
use std::time::Duration;
use super::drawable::Drawable;
use super::shader;
use super::billboard::Rect3D;
use crate::cg_support::node;
use super::instancing;

pub use particle::{Particle, ParticleEmitter};
pub use system::ParticleSystem;

pub trait Emitter : Drawable {
    /// Emits particles and moves them according to the change in time since
    /// last frame
    fn emit(&mut self, dt: Duration);
    
    /// Returns `true` if the emitter has finished emitting, and can be deleted
    /// (so all particles died)
    fn expired(&self) -> bool;

    /// Gets the lights emitted by this emitter or `None` if this emitter
    /// does not emit lights
    fn lights(&self) -> Option<Vec<shader::LightData>>;
}

pub fn dust_emitter<F : glium::backend::Facade>(facade: &F, pos: cgmath::Point3<f64>) -> Box<dyn Emitter> {
    use super::textures;
    use std::time::Instant;
    use cgmath::*;
    use rand::Rng;
    let tex = textures::load_texture_srgb("assets/particles/smoke_04.png", facade);
    Box::new(ParticleEmitter::new(node::Node::default().pos(pos), None, 100, 
    Box::new(Rect3D::new(tex, facade)), facade, 
    |origin| {
        let mut rnd = rand::thread_rng();
        Particle {
            birth: Instant::now(),
            origin: origin.pos,
            color: vec4(0.5, 0.5, 0.5, 0.8),
            transform: node::Node::default().pos(origin.pos).u_scale(rnd.gen_range(0. .. 1f64)),
            rot_vel: Quaternion::new(1., 0., 0., 0.),
            vel: vec3(rnd.gen_range(0. .. 30f64), rnd.gen_range(0. .. 30f64), rnd.gen_range(0. .. 30f64)),
            lifetime: Duration::from_millis(rnd.gen_range(10 .. 1500)),
        }
    },
    |particle| {
        Instant::now().duration_since(particle.birth) > particle.lifetime
    },
    |particle, dt| {
        particle.transform.pos += particle.vel * dt;
        particle.transform.orientation.s += 0.2;
    },
    |particle| {
        let pos = particle.transform.pos;
        let rot = particle.transform.orientation.s;
        instancing::BillboardAttributes {
            instance_color: particle.color.into(),
            instance_pos_rot: vec4(pos.x, pos.y, pos.z, rot).cast().unwrap().into(),
            instance_scale: [particle.transform.scale.x as f32, particle.transform.scale.y as f32],
        }
    }))
}