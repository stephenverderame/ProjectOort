use glutin::window::{WindowBuilder};
use glutin::ContextBuilder;
use glutin::event::{Event, WindowEvent};
use glutin::event_loop::{ControlFlow, EventLoop};
use glutin::platform::run_return::EventLoopExtRunReturn;
use glium::{Display};
use std::time::{Instant, Duration};
use super::scene::{Scene, AbstractScene};
use std::rc::Rc;
use std::cell::{RefCell, RefMut};
use super::shader;

pub struct SceneManager {
    scenes: std::collections::HashMap<&'static str, Box<RefCell<dyn AbstractScene>>>,
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
    pub fn change_scene(&mut self, scene: &'static str) -> &mut Self {
        self.active_scene = Some(scene);
        self
    }

    /// Adds a new scene to be managed by this manager with the given name
    pub fn insert_scene(&mut self, name: &'static str, scene: Scene) 
        -> &mut Self 
    {
        self.scenes.insert(name, Box::new(RefCell::new(scene)));
        self
    }

    pub fn get_active_scene(&self) 
        -> Option<RefMut<dyn AbstractScene + 'static>> 
    {
        self.active_scene.map(|x| self.scenes[x].as_ref().borrow_mut())
    }
}

pub struct WindowCallbacks<'a> {
    input_cb: Option<&'a mut dyn 
        FnMut(glutin::event::DeviceEvent, RefMut<SceneManager>)>,
    resize_cb: Option<&'a mut dyn 
        FnMut(glutin::dpi::PhysicalSize<u32>)>,
    draw_cb: Option<&'a mut dyn FnMut(Duration, RefMut<dyn AbstractScene>)>,
}

impl<'a> WindowCallbacks<'a> {
    pub fn new() -> WindowCallbacks<'a> {
        WindowCallbacks {
            input_cb: None,
            resize_cb: None,
            draw_cb: None,
        }
    }

    pub fn with_input_handler(mut self, 
        on_input: &'a mut dyn FnMut(glutin::event::DeviceEvent, RefMut<SceneManager>)) -> Self 
    {
        self.input_cb = Some(on_input);
        self
    }

    pub fn with_resize_handler(mut self, 
        on_resize: &'a mut dyn FnMut(glutin::dpi::PhysicalSize<u32>)) -> Self
    {
        self.resize_cb = Some(on_resize);
        self
    }

    pub fn with_draw_handler(mut self,
        on_draw: &'a mut dyn FnMut(Duration, RefMut<dyn AbstractScene>)) -> Self
    {
        self.draw_cb = Some(on_draw);
        self
    }
}

pub struct Window {
    wnd_ctx: Rc<RefCell<Display>>,
    e_loop: RefCell<EventLoop<()>>,
    scenes: RefCell<SceneManager>,
    pub shaders: Rc<shader::ShaderManager>,
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
        let wnd_ctx = Rc::new(RefCell::new(Display::new(window_builder, wnd_ctx, &e_loop).unwrap()));
        let shaders = Rc::new(shader::ShaderManager::init(&*wnd_ctx.borrow()));
        gl::load_with(|s| wnd_ctx.borrow().gl_window().get_proc_address(s)); 
        super::set_active_ctx(wnd_ctx.clone(), shaders.clone());

        Window {
            e_loop: RefCell::new(e_loop),
            shaders,
            wnd_ctx,
            scenes: RefCell::new(SceneManager::new()),
        }
    }

    /// Runs the main loop and blocks until the window is closed
    pub fn main_loop(&self, mut callbacks: WindowCallbacks) {
        let shaders = self.shaders.clone();
        let mut last_time = Instant::now();
        self.e_loop.borrow_mut().run_return(|ev, _, control| {
            match ev {
                Event::LoopDestroyed => return,
                Event::WindowEvent {event, ..} => {
                    match event {
                        WindowEvent::CloseRequested => *control = ControlFlow::Exit,
                        WindowEvent::Resized(new_size) => {
                            if let Some(resize) = callbacks.resize_cb.as_mut() {
                                resize(new_size)
                            }
                        },
                        _ => (),
                    }
                },
                Event::DeviceEvent {event, ..} if callbacks.input_cb.is_some() => 
                    callbacks.input_cb.as_mut().unwrap()(event, self.scenes.borrow_mut()),
                Event::MainEventsCleared => {
                    let now = Instant::now();
                    let dt = now.duration_since(last_time);
                    last_time = now;

                    if let Some(mut active_scene) = self.scenes.borrow().get_active_scene() {
                        (&mut *active_scene).render(None, &*shaders);
                    }

                    if let (Some(cb), Some(scene)) = (&mut callbacks.draw_cb.as_mut(), 
                        self.scenes.borrow().get_active_scene()) {
                        cb(dt, scene);
                    }

                },
                _ => (),
            };
        });
    }

    pub fn scene_manager(&mut self) -> RefMut<SceneManager> {
        self.scenes.borrow_mut()
    }

    pub fn ctx(&self) -> std::cell::Ref<glium::Display> {
        self.wnd_ctx.borrow()
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        super::remove_ctx_if_active(self.wnd_ctx.clone());
    }
}

pub struct WindowMaker {
    width: u32,
    height: u32,
    title: Option<&'static str>,
    visible: bool,
    msaa: Option<u16>,
    depth_bits: Option<u8>,
    e_loop: Option<EventLoop<()>>,

}
#[allow(dead_code)]
impl WindowMaker {
    pub fn new(width: u32, height: u32) -> WindowMaker {
        WindowMaker {
            width, height, title: None, visible: true,
            msaa: None,
            depth_bits: None,
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

    pub fn msaa(mut self, samples: u16) -> Self {
        self.msaa = Some(samples);
        self
    }

    pub fn depth_buffer(mut self, bits: u8) -> Self {
        self.depth_bits = Some(bits);
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