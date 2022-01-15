use glutin::window::{WindowBuilder};
use glutin::ContextBuilder;
use glutin::event::{Event, WindowEvent};
use glutin::event_loop::{ControlFlow, EventLoop};
use glutin::platform::run_return::EventLoopExtRunReturn;
use glium::{Display};
use std::time::{Instant, Duration};
use super::scene::Scene;
use std::rc::Rc;
use super::shader;

pub trait OnInputCallback : FnMut(glutin::event::DeviceEvent, &mut SceneManager) {}
pub trait OnResizeCallback : FnMut(glutin::dpi::PhysicalSize<u32>) {}
pub trait OnDrawCallback : FnMut(Duration) {}

pub struct SceneManager {
    scenes: std::collections::HashMap<&'static str, Scene>,
    active_scene: Option<&'static str>,
}

impl SceneManager {
    pub fn new() -> Self {
        SceneManager {
            scenes: std::collections::HashMap::new(),
            active_scene: None,
        }
    }

    /// Sets the active scene to `scene`
    /// Requires that `scene` is a name of a scene managed by this manager
    pub fn change_scene(&mut self, scene: &'static str) {
        self.active_scene = Some(scene);
    }

    /// Adds a new scene to be managed by this manager with the given name
    pub fn insert_scene(&mut self, name: &'static str, scene: Scene) {
        self.scenes.insert(name, scene);
    }
}

struct Window {
    wnd_ctx: Rc<Display>,
    e_loop: EventLoop<()>,
    scenes: SceneManager,
    input_cb: Option<Box<dyn OnInputCallback>>,
    resize_cb: Option<Box<dyn OnResizeCallback>>,
    draw_cb: Option<Box<dyn OnDrawCallback>>,
    pub shaders: shader::ShaderManager,
}

impl Window {

    fn from_builder(builder: WindowMaker) -> Window {
        let e_loop = builder.e_loop.unwrap_or_else(|| EventLoop::new());
        let mut window_builder = WindowBuilder::new()
            .with_decorations(true).with_inner_size(glium::glutin::dpi::PhysicalSize::<u32>{
                width: builder.width, height: builder.height,
            }).with_visible(builder.visible);
        if let Some(title) = builder.title {
            window_builder = window_builder.with_title(title);
        }
        let mut wnd_ctx = ContextBuilder::new().with_srgb(true);
        if let Some(depth) = builder.depth_bits {
            wnd_ctx = wnd_ctx.with_depth_buffer(depth);
        }
        if let Some(msaa) = builder.msaa {
            wnd_ctx = wnd_ctx.with_multisampling(msaa);
        }
        let wnd_ctx = Rc::new(Display::new(window_builder, wnd_ctx, &e_loop).unwrap());
        gl::load_with(|s| wnd_ctx.gl_window().get_proc_address(s)); 
        super::set_active_ctx(wnd_ctx.clone());

        Window {
            e_loop,
            shaders: shader::ShaderManager::init(&*wnd_ctx),
            wnd_ctx,
            scenes: builder.scenes,
            input_cb: builder.input_cb,
            resize_cb: builder.resize_cb,
            draw_cb: builder.draw_cb,
        }
    }

    pub fn main_loop(mut self) {
        let mut resize = self.resize_cb;
        let mut draw = self.draw_cb;
        let mut input = self.input_cb;
        let mut scenes = self.scenes;
        let shaders = self.shaders.clone();
        let mut last_time = Instant::now();
        self.e_loop.run_return(|ev, _, control| {
            match ev {
                Event::LoopDestroyed => return,
                Event::WindowEvent {event, ..} => {
                    match event {
                        WindowEvent::CloseRequested => *control = ControlFlow::Exit,
                        WindowEvent::Resized(new_size) => {
                            if let Some(resize) = &mut resize {
                                resize(new_size)
                            }
                        },
                        _ => (),
                    }
                },
                Event::DeviceEvent {event, ..} if input.is_some() => 
                    input.as_mut().unwrap()(event, &mut scenes),
                Event::MainEventsCleared => {
                    let now = Instant::now();
                    let dt = now.duration_since(last_time);
                    last_time = now;

                    if let Some(active_scene) = scenes.active_scene {
                        scenes.scenes[active_scene].render(&*shaders);
                    }

                    if let Some(cb) = &mut draw {
                        cb(dt);
                    }

                },
                _ => (),
            };
        });
    }

    pub fn scene_manager(&mut self) -> &mut SceneManager {
        &mut self.scenes
    }

    pub fn ctx(&self) -> &glium::Display {
        &*self.wnd_ctx
    }
}

pub struct WindowMaker {
    width: u32,
    height: u32,
    title: Option<&'static str>,
    visible: bool,
    scenes: SceneManager,
    msaa: Option<u16>,
    depth_bits: Option<u8>,
    input_cb: Option<Box<dyn OnInputCallback>>,
    resize_cb: Option<Box<dyn OnResizeCallback>>,
    draw_cb: Option<Box<dyn OnDrawCallback>>,
    e_loop: Option<EventLoop<()>>,

}

impl WindowMaker {
    pub fn new(width: u32, height: u32) -> WindowMaker {
        WindowMaker {
            width, height, title: None, visible: false,
            scenes: SceneManager::new(),
            msaa: None,
            depth_bits: None,
            input_cb: None,
            resize_cb: None,
            draw_cb: None,
            e_loop: None,
        }
    }

    pub fn title(mut self, title: &'static str) -> Self {
        self.title = Some(title);
        self
    }

    pub fn invisible(mut self) -> Self {
        self.visible = false;
        self
    }

    pub fn with_scene(mut self, scene_name: &'static str, scene: Scene) -> Self {
        self.scenes.scenes.insert(scene_name, scene);
        self
    }

    pub fn with_active_scene(mut self, scene_name: &'static str, scene: Scene) -> Self {
        self.scenes.active_scene = Some(scene_name);
        self.with_scene(scene_name, scene)
    }

    pub fn msaa(mut self, samples: u16) -> Self {
        self.msaa = Some(samples);
        self
    }

    pub fn depth_buffer(mut self, bits: u8) -> Self {
        self.depth_bits = Some(bits);
        self
    }

    pub fn with_input_handler(mut self, cb: Box<dyn OnInputCallback>) -> Self {
        self.input_cb = Some(cb);
        self
    }

    pub fn with_resize_handler(mut self, cb: Box<dyn OnResizeCallback>) -> Self {
        self.resize_cb = Some(cb);
        self
    }

    pub fn with_draw_func(mut self, cb: Box<dyn OnDrawCallback>) -> Self {
        self.draw_cb = Some(cb);
        self
    }

    pub fn build(self) -> Window {
        Window::from_builder(self)
    }

    #[cfg(test)]
    pub fn any_thread(mut self) -> Self {
        use glutin::platform::windows::EventLoopExtWindows;
        self.e_loop = Some(glutin::event_loop::EventLoop::new_any_thread());
        self
    }
}