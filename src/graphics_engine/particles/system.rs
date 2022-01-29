use super::Emitter;
use std::collections::VecDeque;
use super::super::drawable::{Drawable, VertexHolder};
use super::super::entity::{AbstractEntity, RenderOrder};
use super::super::shader;
use std::rc::Rc;
use std::cell::RefCell;
use crate::cg_support::Transformation;

/// A collection of particle emitters
pub struct ParticleSystem {
    emitters: VecDeque<(Box<dyn Emitter>, usize)>,
    tmp_transform: [Rc<RefCell<dyn Transformation>>; 1],
    drawables: Vec<Box<dyn Drawable>>,
}

impl ParticleSystem {
    pub fn new() -> Self {
        Self {
            emitters: VecDeque::new(),
            tmp_transform: [Rc::new(RefCell::new(cgmath::Matrix4::from_scale(1.)))],
            drawables: Vec::new(),
        }
    }

    /// Adds a new emitter to this system, using the drawable at index `idx`
    /// Requires this system has `drawables.len() > idx`
    pub fn with_emitter(mut self, emitter: Box<dyn Emitter>, drawable_idx: usize) -> Self {
        self.emitters.push_back((emitter, drawable_idx));
        self
    }

    /// Adds a new emitter to this system, using the drawable at index `idx`
    /// Requires this system has `drawables.len() > idx`
    pub fn new_emitter(&mut self, e: Box<dyn Emitter>, idx: usize) {
        self.emitters.push_back((e, idx));
    }

    /// Adds a drawable at the next available index
    pub fn with_drawable(mut self, draw: Box<dyn Drawable>) -> Self {
        self.drawables.push(draw);
        self
    }

    /// Adds a billboard drawable at the next available index
    pub fn with_billboard(mut self, path: &str) -> Self {
        let ctx = super::super::get_active_ctx();
        let ctx = ctx.ctx.borrow();
        self.drawables.push(Box::new(super::super::billboard::Rect3D::new(
            super::super::textures::load_texture_srgb(path, &*ctx), &*ctx
        )));
        self
    }

    pub fn emit(&mut self, dt: std::time::Duration) {
        let mut death = Vec::new();
        for (e, _) in self.emitters.iter_mut() {
            if e.expired() {
                death.push(e as *const Box<dyn Emitter>);
            } else {
                e.emit(dt);
            }
        }
        self.emitters.retain(|(e, _)|
            death.iter().find(|d| **d == e as *const Box<dyn Emitter>).is_none());
    }

    pub fn lights(&self) -> Option<Vec<shader::LightData>> {
        let mut lights = Vec::new();
        for (e, _) in &self.emitters {
            e.lights().as_mut().map(|x| lights.append(x));
        }
        if lights.is_empty() { None }
        else { Some(lights) }
    }

}

impl Drawable for ParticleSystem {
    fn render_args<'a>(&'a mut self, p: &[[[f32; 4]; 4]]) -> Vec<(shader::UniformInfo<'a>, VertexHolder<'a>, glium::index::IndicesSource<'a>)>
    {
        let mut v = Vec::new();
        let drawables = self.drawables.as_mut_ptr();
        for (e, draw_idx) in self.emitters.iter_mut() {
            let (u, vh, i) = unsafe{ &mut *drawables.add(*draw_idx) }.render_args(p).swap_remove(0);
            v.push((u, vh.append(e.instance_data()), i));
        }
        v
    }

    fn transparency(&self) -> Option<f32> { None }
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
    fn render_order(&self) -> RenderOrder {
        RenderOrder::Last
    }
}
