use glutin::window::WindowBuilder;
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

use draw_traits::Drawable;
use glutin::platform::run_return::*;

use cgmath::*;


fn main() {
    let mut e_loop = EventLoop::new();
    let window_builder = WindowBuilder::new().with_title("Rust Graphics")
        .with_decorations(true).with_inner_size(glium::glutin::dpi::PhysicalSize::<u32>{
            width: 1920, height: 1080,
        });
    let wnd_ctx = ContextBuilder::new().with_multisampling(4).with_depth_buffer(24);
    let wnd_ctx = Display::new(window_builder, wnd_ctx, &e_loop).unwrap();

    let ship_model = model::Model::load("assets/Ships/StarSparrow01.obj", &wnd_ctx);
    let mut user = player::Player::new(ship_model);

    //let sky = model::Model::load("assets/skybox/sky.obj", &wnd_ctx);
    //let sky = model::Model::load("assets/Milkyway/sky.obj", &wnd_ctx);
    let sky_hdr = skybox::Skybox::new(skybox::SkyboxTex::Sphere(
        textures::load_texture_hdr("assets/Milkyway/Milkyway_Light.hdr", &wnd_ctx)), &wnd_ctx);
    let sky = skybox::Skybox::new(skybox::SkyboxTex::Sphere(
        textures::load_texture_2d("assets/Milkyway/Milkyway_BG.jpg", &wnd_ctx)), &wnd_ctx);

    let shader_manager = shader::ShaderManager::init(&wnd_ctx);

    let mut theta = 0.;
    let mut prev_time = Instant::now();

    let gen_sky_scene = scene::Scene::new();
    let sky_cbo = gen_sky_scene.render_to_cubemap(cgmath::point3(0., 0., 0.), &wnd_ctx, |fbo, mats| {
        sky.render(fbo, mats, &shader_manager)
    });


    let sky_hdr_cbo = gen_sky_scene.render_to_cubemap(cgmath::point3(0., 0., 0.), &wnd_ctx, |fbo, mats| {
        sky_hdr.render(fbo, mats, &shader_manager)
    });

    let main_skybox = skybox::Skybox::new(skybox::SkyboxTex::Cube(sky_cbo), &wnd_ctx);

    let mut main_scene = scene::Scene::new();
    main_scene.set_ibl_map(sky_hdr_cbo);

    let mut hdr = render_target::RenderTarget::new(8, 1920, 1080, &wnd_ctx);

    
    e_loop.run_return(|ev, _, control| {
        let dt = Instant::now() - prev_time;
        let mut wnd_size : (u32, u32) = (1920, 1080);
        match ev {
            Event::LoopDestroyed => return,
            Event::WindowEvent {event, ..} => {
                match event {
                    WindowEvent::CloseRequested => *control = ControlFlow::Exit,
                    WindowEvent::Resized(new_size) => {
                        wnd_size = (new_size.width, new_size.height);
                        //hdr.resize(new_size.width, new_size.height, 8, &wnd_ctx);
                    },
                    _ => (),
                }
            },
            _ => (),
        };
        let aspect = (wnd_size.0 as f32) / (wnd_size.1 as f32);
        let rot = Quaternion::<f32>::from_angle_y(Deg::<f32>(theta));
        theta += dt.as_secs_f32() * 450.;
        user.set_rot(rot);
        let surface = hdr.draw(&wnd_ctx);
        surface.clear_color_and_depth((0., 0., 0., 1.), 1.);
        main_scene.render(surface, &user, aspect, |surface, mats| {
            main_skybox.render(surface, mats, &shader_manager);
            user.render(surface, mats, &shader_manager)
        });
        let mut display = wnd_ctx.draw();
        display.clear_color_and_depth((0.5, 0., 0.5, 1.0), 1.);
        main_scene.render(&mut display, &user, aspect, |surface, mats| {
            hdr.render(surface, mats, &shader_manager);
        });
        display.finish().unwrap();
        prev_time = Instant::now();
    });

}
