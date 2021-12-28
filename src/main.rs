use glutin::window::{WindowBuilder};
use glutin::ContextBuilder;
use glutin::event::{Event, WindowEvent};
use glutin::event_loop::{ControlFlow, EventLoop};
use glium::{Surface, Display};
use std::time::Instant;

extern crate cgmath;
mod textures;
mod shader;
mod draw_traits;
mod skybox;
mod entity;
mod model;
mod node;
mod camera;
mod player;
mod scene;
mod render_target;
mod render_pass;
mod controls;

use draw_traits::Drawable;
use glutin::platform::run_return::*;

use cgmath::*;
use render_target::*;

use std::rc::Rc;
use std::cell::RefCell;

fn gen_skybox<F : glium::backend::Facade>(size: u32, shader_manager: &shader::ShaderManager, facade: &F) 
    -> (glium::texture::Cubemap, glium::texture::Cubemap) 
{
    let cam = camera::PerspectiveCamera::default(1.);
    let sky_hdr = skybox::Skybox::new(skybox::SkyboxTex::Sphere(
        textures::load_texture_hdr("assets/Milkyway/Milkyway_Light.hdr", facade)), facade);
    let sky = skybox::Skybox::new(skybox::SkyboxTex::Sphere(
        textures::load_texture_2d("assets/Milkyway/Milkyway_BG.jpg", facade)), facade);
    let gen_sky_scene = scene::Scene::new();
    let mut gen_sky = render_target::CubemapRenderTarget::new(size, 10., cgmath::point3(0., 0., 0.), facade);
    let mut cp = render_target::CopyTextureProcessor::new(size, size, None, None, facade);
    let mut gen_sky_pass = render_pass::RenderPass::new(&mut gen_sky, vec![&mut cp], render_pass::Pipeline::new(vec![0], vec![(0, 1)]));
    let gen_sky_ptr = &mut gen_sky_pass as *mut render_pass::RenderPass;
    unsafe {
        let sky_cbo = gen_sky_scene.render_pass(&mut *gen_sky_ptr, &cam, 1., shader_manager, |fbo, scene_data| {
            sky.render(fbo, scene_data, &shader_manager)
        });
        // safe bx we finish using the first borrow here
        let sky_hdr_cbo = gen_sky_scene.render_pass(&mut *gen_sky_ptr, &cam, 1., shader_manager, |fbo, scene_data| {
            sky_hdr.render(fbo, scene_data, shader_manager)
        });

        match (sky_cbo, sky_hdr_cbo) {
            (TextureType::TexCube(Ownership::Own(sky)), 
            TextureType::TexCube(Ownership::Own(sky_hdr))) => (sky, sky_hdr),
            _ => panic!("Unexpected return from sky generation"),
        }
    }
}

fn gen_prefilter_hdr_env<F : glium::backend::Facade>(skybox: Rc<RefCell<skybox::Skybox>>, size: u32, 
    shader_manager: &shader::ShaderManager, facade: &F) -> (glium::texture::Cubemap, glium::texture::Texture2d)
{
    let cam = camera::PerspectiveCamera::default(1.);
    let mip_levels = 5;
    let mut rt = render_target::MipCubemapRenderTarget::new(size, mip_levels, 10., cgmath::point3(0., 0., 0.), facade);
    let iterations = RefCell::new(0);
    let res = rt.draw(&cam, &|fbo, viewer| {
        let its = *iterations.borrow();
        let mip_level = its / 6;
        skybox.borrow_mut().set_mip_progress(Some(mip_level as f32 / (mip_levels - 1) as f32));
        let sd = draw_traits::default_scene_data(viewer, 1.);
        skybox.borrow().render(fbo, &sd, shader_manager);
        *iterations.borrow_mut() = its + 1;
    });
    skybox.borrow_mut().set_mip_progress(None);
    let mut tp = render_target::GenLutProcessor::new(facade, 512, 512);
    let brdf = tp.process(Vec::<&TextureType>::new(), shader_manager);
    match (res, brdf) {
        (TextureType::TexCube(Ownership::Own(x)), 
            TextureType::Tex2d(Ownership::Own(y))) => (x, y),
        _ => panic!("Unexpected return from read"),
    }
}


fn main() {
    let render_width = 1920;
    let render_height = 1080;

    let mut e_loop = EventLoop::new();
    let window_builder = WindowBuilder::new().with_title("Rust Graphics")
        .with_decorations(true).with_inner_size(glium::glutin::dpi::PhysicalSize::<u32>{
            width: render_width, height: render_height,
        });
    let wnd_ctx = ContextBuilder::new().with_multisampling(4).with_depth_buffer(24)
        .with_srgb(true);
    let wnd_ctx = Display::new(window_builder, wnd_ctx, &e_loop).unwrap();

    let ship_model = model::Model::load("assets/Ships/StarSparrow01.obj", &wnd_ctx);
    //let asteroid_model = model::Model::load("assets/asteroid1/Asteroid.obj", &wnd_ctx);
    let mut user = player::Player::new(ship_model);
    let mut asteroid1 = entity::Entity::new(model::Model::load("assets/asteroid1/Asteroid.obj", &wnd_ctx));
    asteroid1.transform.scale = cgmath::vec3(0.08, 0.08, 0.08);
    asteroid1.transform.pos = cgmath::point3(5., 2., 5.);

    let shader_manager = shader::ShaderManager::init(&wnd_ctx);

    let (sky_cbo, sky_hdr_cbo) = gen_skybox(1024, &shader_manager, &wnd_ctx);
    let main_skybox = Rc::new(RefCell::new(skybox::Skybox::new(skybox::SkyboxTex::Cube(sky_cbo), &wnd_ctx)));
    let (pre_filter, brdf_lut) = gen_prefilter_hdr_env(main_skybox.clone(), 128, &shader_manager, &wnd_ctx);

    //let main_skybox = RefCell::new(skybox::Skybox::new(skybox::SkyboxTex::Cube(pre_filter), &wnd_ctx));
    let mut main_scene = scene::Scene::new();
    main_scene.set_ibl_maps(shader::PbrMaps {
        diffuse_ibl: sky_hdr_cbo, spec_ibl: pre_filter, brdf_lut
    });

    let mut msaa = render_target::MsaaRenderTarget::new(8, render_width, render_height, &wnd_ctx);
    let mut eb = render_target::ExtractBrightProcessor::new(&wnd_ctx, render_width, render_height);
    let mut blur = render_target::SepConvProcessor::new(render_width, render_height, 10, &wnd_ctx);
    let mut compose = render_target::UiCompositeProcessor::new(&wnd_ctx, || { 
        let mut surface = wnd_ctx.draw();
        surface.clear_color_and_depth((0., 0., 0., 1.), 1.);
        surface
    }, |disp| disp.finish().unwrap());
    let mut main_pass = render_pass::RenderPass::new(&mut msaa, vec![&mut eb, &mut blur, &mut compose], 
        render_pass::Pipeline::new(vec![0], vec![(0, 1), (1, 2), (2, 3), (0, 3)]));

    let mut prev_time = Instant::now();
    let mut wnd_size : (u32, u32) = (render_width, render_height);
    let wnd = wnd_ctx.gl_window();
    let mut controller = controls::PlayerControls::new(wnd.window());
    e_loop.run_return(|ev, _, control| {
        let dt = Instant::now().duration_since(prev_time).as_secs_f64();
        prev_time = Instant::now();
        match ev {
            Event::LoopDestroyed => return,
            Event::WindowEvent {event, ..} => {
                match event {
                    WindowEvent::CloseRequested => *control = ControlFlow::Exit,
                    WindowEvent::Resized(new_size) => {
                        wnd_size = (new_size.width, new_size.height);
                    },
                    _ => (),
                }
            },
            Event::DeviceEvent {event, ..} => controller.on_input(event),
            _ => (),
        };
        let aspect = (wnd_size.0 as f32) / (wnd_size.1 as f32);
        user.move_player(&controller, dt);
        main_scene.render_pass(&mut main_pass, &user, aspect, &shader_manager, |fbo, scene_data| {
            fbo.clear_color_and_depth((0., 0., 0., 1.), 1.);
            main_skybox.borrow().render(fbo, &scene_data, &shader_manager);
            user.render(fbo, &scene_data, &shader_manager);
            asteroid1.render(fbo, &scene_data, &shader_manager);
        });
        controller.reset_toggles();
    });

}
