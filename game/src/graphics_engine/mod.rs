pub mod model;
pub mod camera;
#[macro_use]
pub mod pipeline;
pub mod scene;
pub mod shader;
pub mod cubes;
pub mod textures;
pub mod drawable;
pub mod window;
pub mod entity;
pub mod instancing;
mod billboard;
pub mod particles;
pub mod primitives;
pub mod text;
use std::rc::Rc;
use std::cell::{Cell, RefCell};
use std::mem::MaybeUninit;

std::thread_local! {
    static ACTIVE_CTX: Cell<Option<Rc<RefCell<glium::Display>>>> = Cell::new(None);
    static ACTIVE_MANAGER: Cell<Option<Rc<shader::ShaderManager>>> = Cell::new(None);
}

/// Sets the thread local active window context and shader
fn set_active_ctx(ctx: Rc<RefCell<glium::Display>>, shader: Rc<shader::ShaderManager>) {
    ACTIVE_CTX.with(|v| v.set(Some(ctx)));
    ACTIVE_MANAGER.with(|v| v.set(Some(shader)))
}

/// If `ctx` is the active context, removes it and the active shader manager
/// to allow them to be destroyed
/// Requires the active context is not in use
fn remove_ctx_if_active(ctx: Rc<RefCell<glium::Display>>) {
    ACTIVE_CTX.with(|v| {
        let active_ctx = v.take().expect("Active ctx in use");
        if Rc::ptr_eq(&active_ctx, &ctx) {
            ACTIVE_MANAGER.with(|m| m.set(None));
        } else {
            v.set(Some(active_ctx))
        }
    });
}
/// RAII for the active glium context and shader manager
/// Returns the context to the shared static variable when it is dropped
pub struct ActiveCtx {
    pub ctx: Rc<RefCell<glium::Display>>,
    pub shader: Rc<shader::ShaderManager>,
}

impl ActiveCtx {
    /// Gets the mutable surface of the active context
    pub fn as_surface(self) -> MutCtx {
        use std::ptr::addr_of_mut;
        let mut ctx : MaybeUninit<MutCtx> = MaybeUninit::uninit();
        let ptr = ctx.as_mut_ptr();

        unsafe { 
            addr_of_mut!((*ptr).ctx).write(self); 
            addr_of_mut!((*ptr).display).write((*ptr).ctx.ctx.borrow());
            addr_of_mut!((*ptr).frame).write((*ptr).display.draw());
            ctx.assume_init()
        }

    }
}

impl Drop for ActiveCtx {
    fn drop(&mut self) {
        ACTIVE_CTX.with(|v| v.set(Some(self.ctx.clone())));
        ACTIVE_MANAGER.with(|v| v.set(Some(self.shader.clone())));
    }
}
/// Gets the thread_local active context
/// Panics if the active context has already been borrowed
pub fn get_active_ctx() -> ActiveCtx {
    ActiveCtx {
        ctx: ACTIVE_CTX.with(|v| 
            v.take().expect("Active context not set or already in use")),
        shader: ACTIVE_MANAGER.with(|v|
            v.take().expect("Active manager not set or already in use")),
    }
}
/// RAII for the draw frame of the shared active context
/// Dereferences into a `glium::Frame`
pub struct MutCtx {
    ctx: ActiveCtx,
    display: std::cell::Ref<'static, glium::Display>,
    frame: glium::Frame,
}

impl MutCtx {
    pub fn finish(self) {
        self.frame.finish().unwrap()
    }
}

impl std::ops::Deref for MutCtx {
    type Target = glium::Frame;

    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}

impl std::ops::DerefMut for MutCtx {

    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.frame
    }
}
