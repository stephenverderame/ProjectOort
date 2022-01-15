pub mod model;
pub mod camera;
pub mod render_pass;
pub mod render_target;
pub mod scene;
pub mod shader;
pub mod skybox;
pub mod textures;
pub mod drawable;
pub mod window;
pub mod entity;
mod instancing;
use std::rc::Rc;
use std::cell::Cell;

std::thread_local! {
    static ACTIVE_CTX: Cell<Option<Rc<glium::Display>>> = Cell::new(None);
}

fn set_active_ctx(ctx: Rc<glium::Display>) {
    ACTIVE_CTX.with(|v| v.set(Some(ctx)))
}

struct ActiveCtx {
    ctx: Rc<glium::Display>,
    shader: shader::ShaderManager,
}

impl Drop for ActiveCtx {
    fn drop(&mut self) {
        ACTIVE_CTX.with(|v| v.set(Some(self.ctx.clone())))
    }
}

fn get_active_ctx() -> ActiveCtx {
    ActiveCtx {
        ctx: ACTIVE_CTX.with(|v| 
            v.take().expect("Active context not set or already in use"))
    }
}