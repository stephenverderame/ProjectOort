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
    drawables: Vec<Box<dyn Drawable>>,
}

impl ParticleSystem {
    pub fn new() -> Self {
        Self {
            emitters: VecDeque::new(),
            drawables: Vec::new(),
        }
    }

    /// Adds a new emitter to this system, using the drawable at index `idx`
    /// Requires this system has `drawables.len() > idx`
    #[inline(always)]
    #[allow(dead_code)]
    pub fn with_emitter(mut self, emitter: Box<dyn Emitter>, drawable_idx: usize) -> Self {
        self.emitters.push_back((emitter, drawable_idx));
        self
    }

    /// Adds a new emitter to this system, using the drawable at index `idx`
    /// Requires this system has `drawables.len() > idx`
    #[inline(always)]
    pub fn new_emitter(&mut self, e: Box<dyn Emitter>, idx: usize) {
        self.emitters.push_back((e, idx));
    }

    /// Adds a drawable at the next available index
    #[allow(dead_code)]
    #[inline(always)]
    pub fn with_drawable(mut self, draw: Box<dyn Drawable>) -> Self {
        self.drawables.push(draw);
        self
    }

    /// Adds a billboard drawable at the next available index
    /// 
    /// `density` - participating medium density in spherical billboard
    pub fn with_billboard(mut self, path: &str, density: f32) -> Self {
        let ctx = super::super::get_active_ctx();
        let ctx = ctx.ctx.borrow();
        self.drawables.push(Box::new(super::super::billboard::Rect3D::new(
            super::super::textures::load_texture_srgb(path, &*ctx), density, &*ctx
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
            !death.iter().any(|d| *d == e as *const Box<dyn Emitter>));
    }

    pub fn lights(&self) -> Option<Vec<shader::LightData>> {
        let mut lights = Vec::new();
        for (e, _) in &self.emitters {
            if let Some(x) = e.lights().as_mut() { lights.append(x) }
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
    fn transformations(&self) -> Option<&[Rc<RefCell<dyn Transformation>>]> {
        None
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
