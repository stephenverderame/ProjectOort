use super::Emitter;
use std::collections::VecDeque;
use super::super::drawable::{Drawable, VertexHolder};
use super::super::entity::AbstractEntity;
use super::super::shader;
use std::rc::Rc;
use std::cell::RefCell;
use crate::cg_support::Transformation;

/// A collection of particle emitters
pub struct ParticleSystem {
    emitters: VecDeque<Box<dyn Emitter>>,
    tmp_transform: [Rc<RefCell<dyn Transformation>>; 1],
}

impl ParticleSystem {
    pub fn new() -> Self {
        Self {
            emitters: VecDeque::new(),
            tmp_transform: [Rc::new(RefCell::new(cgmath::Matrix4::from_scale(1.)))],
        }
    }

    pub fn with_emitter(mut self, emitter: Box<dyn Emitter>) -> Self {
        self.emitters.push_back(emitter);
        self
    }

    /// Adds a new emitter to this system
    pub fn new_emitter(&mut self, e: Box<dyn Emitter>) {
        self.emitters.push_back(e);
    }

}

impl Drawable for ParticleSystem {
    fn render_args<'a>(&'a mut self, p: &[[[f32; 4]; 4]]) -> Vec<(shader::UniformInfo<'a>, VertexHolder<'a>, glium::index::IndicesSource<'a>)>
    {
        let mut v = Vec::new();
        for e in self.emitters.iter_mut() {
            v.append(&mut e.render_args(p));
        }
        v
    }
}

impl AbstractEntity for ParticleSystem {
    fn transformations(&self) -> &[Rc<RefCell<dyn Transformation>>] {
        &self.tmp_transform[..]
    }
    fn drawable(&mut self) -> &mut dyn Drawable {
        self
    }
    fn should_render(&self, pass: shader::RenderPassType) -> bool {
        pass == shader::RenderPassType::Visual
    }
}

impl Emitter for ParticleSystem {
    fn emit(&mut self, dt: std::time::Duration) {
        let mut death = Vec::new();
        for e in self.emitters.iter_mut() {
            if e.expired() {
                death.push(e as *const Box<dyn Emitter>);
            } else {
                e.emit(dt);
            }
        }
        self.emitters.retain(|e|
            death.iter().find(|d| **d == e as *const Box<dyn Emitter>).is_none());
    }

    fn expired(&self) -> bool {
        self.emitters.is_empty()
    }

    fn lights(&self) -> Option<Vec<shader::LightData>> {
        let mut lights = Vec::new();
        for e in &self.emitters {
            e.lights().as_mut().map(|x| lights.append(x));
        }
        if lights.is_empty() { None }
        else { Some(lights) }
    }
}