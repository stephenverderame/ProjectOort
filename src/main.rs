use glutin::window::WindowBuilder;
use glutin::ContextBuilder;
use glutin::event::{Event, WindowEvent};
use glutin::event_loop::{ControlFlow, EventLoop};
use glium::{Surface, Display};
use std::time::Instant;
use std::ops::Mul;

extern crate cgmath;
mod textures;
mod model;
mod shader;
mod node;
mod player;

use cgmath::*;


fn draw(ctx : &mut glium::Frame) {
    ctx.clear_color_and_depth((0.5, 0., 0.5, 1.0), 1.);
}

fn main() {
    let e_loop = EventLoop::new();
    let window_builder = WindowBuilder::new().with_title("Rust Graphics")
        .with_decorations(true).with_inner_size(glium::glutin::dpi::PhysicalSize::<u32>{
            width: 800, height: 800,
        });
    let wnd_ctx = ContextBuilder::new().with_depth_buffer(24);
    let wnd_ctx = Display::new(window_builder, wnd_ctx, &e_loop).unwrap();

    let view = cgmath::Matrix4::look_at_rh(point3(0., 0., -5.), point3(0., 0.5, 0.), vec3(0., 1., 0.));

    let ship_model = model::Model::load("assets/Ships/StarSparrow01.obj", &wnd_ctx);
    let mut user = player::Player::new(ship_model);

    let sky = model::Model::load("assets/skybox/sky.obj", &wnd_ctx);

    let shader_manager = shader::ShaderManager::init(&wnd_ctx);

    let mut theta = 0.;
    let mut prev_time = Instant::now();

    e_loop.run(move |ev, _, control| {
        let dt = Instant::now() - prev_time;
        let mut wnd_size : (u32, u32) = (1, 1);
        match ev {
            Event::LoopDestroyed => return,
            Event::WindowEvent {event, ..} => {
                match event {
                    WindowEvent::CloseRequested => *control = ControlFlow::Exit,
                    WindowEvent::Resized(new_size) => wnd_size = (new_size.width, new_size.height),
                    _ => (),
                }
            },
            _ => (),
        };
        let aspect = (wnd_size.0 as f32) / (wnd_size.1 as f32);
        let rot = Quaternion::<f32>::from_angle_y(Deg::<f32>(theta));
        theta += dt.as_secs_f32() * 450.;
        let mut display = wnd_ctx.draw();
        draw(&mut display);
        user.set_rot(rot);
        let proj = cgmath::perspective(cgmath::Deg::<f32>(60f32), aspect, 0.1, 100.);
        let view = user.view_mat();
        let viewproj : [[f32; 4]; 4] = proj.mul(view).into();
        let mats = shader::Matrices { viewproj: viewproj, proj: proj.into(), view: view.into()};
        let sky_scale = cgmath::Matrix4::from_scale(1000f32);
        sky.render(&mut display, &mats, sky_scale.into(), &shader_manager);
        user.render(&mut display, &mats, &shader_manager);
        display.finish().unwrap();
        prev_time = Instant::now();
    });

}
