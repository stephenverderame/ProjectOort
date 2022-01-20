mod particle;
mod system;
use std::time::Duration;
use super::drawable::Drawable;
use super::shader;

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