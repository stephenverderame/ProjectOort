use glutin::window::{WindowBuilder};
use glutin::ContextBuilder;
use glutin::event::{Event, WindowEvent};
use glutin::event_loop::{ControlFlow, EventLoop};
use glium::{Surface, Display};
use std::time::Instant;

extern crate cgmath;
extern crate glium;
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
mod ssbo;
extern crate gl;

use draw_traits::Drawable;
use glutin::platform::run_return::*;

use cgmath::*;
use render_target::*;

use std::rc::Rc;
use std::cell::RefCell;

/// Generates the scene skybox and a cubemap version of the diffuse HDR
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
    let mut gen_sky_pass = render_pass::RenderPass::new(vec![&mut gen_sky], vec![&mut cp], render_pass::Pipeline::new(vec![0], vec![(0, 1)]));
    let gen_sky_ptr = &mut gen_sky_pass as *mut render_pass::RenderPass;
    unsafe {
        let sky_cbo = gen_sky_scene.render_pass(&mut *gen_sky_ptr, &cam, 1., shader_manager, 
        |fbo, scene_data, _, cache| {
            sky.render(fbo, scene_data, &cache, &shader_manager)
        });
        // safe bx we finish using the first borrow here
        let sky_hdr_cbo = gen_sky_scene.render_pass(&mut *gen_sky_ptr, &cam, 1., shader_manager, 
        |fbo, scene_data, _, cache| {
            sky_hdr.render(fbo, scene_data, &cache, shader_manager)
        });

        match (sky_cbo, sky_hdr_cbo) {
            (Some(TextureType::TexCube(Ownership::Own(sky))), 
            Some(TextureType::TexCube(Ownership::Own(sky_hdr)))) => (sky, sky_hdr),
            _ => panic!("Unexpected return from sky generation"),
        }
    }
}

/// Generates the specular IBL Cubemap and the BRDF LUT
fn gen_prefilter_hdr_env<F : glium::backend::Facade>(skybox: Rc<RefCell<skybox::Skybox>>, size: u32, 
    shader_manager: &shader::ShaderManager, facade: &F) -> (glium::texture::Cubemap, glium::texture::Texture2d)
{
    let cam = camera::PerspectiveCamera::default(1.);
    let mip_levels = 5;
    let mut rt = render_target::MipCubemapRenderTarget::new(size, mip_levels, 10., cgmath::point3(0., 0., 0.), facade);
    let iterations = RefCell::new(0);
    let mut cache = shader::PipelineCache::new();
    let res = rt.draw(&cam, None, &cache, &|fbo, viewer, _, cache, _| {
        let its = *iterations.borrow();
        let mip_level = its / 6;
        skybox.borrow_mut().set_mip_progress(Some(mip_level as f32 / (mip_levels - 1) as f32));
        let sd = draw_traits::default_scene_data(viewer, 1.);
        skybox.borrow().render(fbo, &sd, &cache, shader_manager);
        *iterations.borrow_mut() = its + 1;
    });
    skybox.borrow_mut().set_mip_progress(None);
    let mut tp = render_target::GenLutProcessor::new(facade, 512, 512);
    let brdf = tp.process(None, shader_manager, &mut cache, None);
    match (res.unwrap(), brdf.unwrap()) {
        (TextureType::TexCube(Ownership::Own(x)), 
            TextureType::Tex2d(Ownership::Own(y))) => (x, y),
        _ => panic!("Unexpected return from read"),
    }
}

fn handle_shots<F : glium::backend::Facade>(user: &player::Player, controller: &controls::PlayerControls, lasers: &mut entity::EntityFlyweight, facade: &F) {
    if controller.fire {
        let mut transform = user.root.borrow().clone();
        transform.scale = cgmath::vec3(0.3, 0.3, 1.);
        lasers.new_instance(entity::EntityInstanceData {
            transform, visible: true, velocity: user.forward() * 40f64,
        }, facade)
    }
}

fn gen_asteroid_field<F : glium::backend::Facade>(obj: &mut entity::EntityFlyweight, facade: &F) {
    use rand::distributions::*;
    let scale_distrib = rand::distributions::Uniform::from(0.01 .. 0.3);
    let pos_distrib = rand::distributions::Uniform::from(-100.0 .. 100.0);
    let angle_distrib = rand::distributions::Uniform::from(0.0 .. 360.0);
    let mut rng = rand::thread_rng();
    for _ in 0 .. 100 {
        let scale = scale_distrib.sample(&mut rng);
        let axis = vec3(pos_distrib.sample(&mut rng), pos_distrib.sample(&mut rng), pos_distrib.sample(&mut rng)).normalize();
        let rot = Quaternion::<f64>::from_axis_angle(axis, Deg::<f64>(angle_distrib.sample(&mut rng)));
        let transform = node::Node::new(Some(point3(pos_distrib.sample(&mut rng), pos_distrib.sample(&mut rng), pos_distrib.sample(&mut rng))),
            Some(rot), Some(vec3(scale, scale, scale)), None);
        obj.new_instance(entity::EntityInstanceData {
            transform, velocity: vec3(0., 0., 0.), visible: true,
        }, facade)
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
    gl::load_with(|s| wnd_ctx.gl_window().get_proc_address(s)); // for things I can't figure out how to do in glium

    let ship_model = model::Model::load("assets/Ships/StarSparrow01.obj", &wnd_ctx);
    //let asteroid_model = model::Model::load("assets/asteroid1/Asteroid.obj", &wnd_ctx);
    let mut user = player::Player::new(ship_model);
    let mut asteroid = entity::EntityFlyweight::new(model::Model::load("assets/asteroid1/Asteroid.obj", &wnd_ctx));
    gen_asteroid_field(&mut asteroid, &wnd_ctx);

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
    let mut depth_render = render_target::DepthRenderTarget::new(render_width, render_height, None, &wnd_ctx);
    let dir_light = camera::OrthoCamera::new(400., 400., 1., 300., point3(-120., 120., 0.), Some(point3(0., 0., 0.)), 
        Some(vec3(0., 1., 0.)));
    let mut dir_light_depth_render = render_target::DepthRenderTarget::new(2048, 2048, Some(Box::new(dir_light.clone())), &wnd_ctx);
    main_scene.set_dir_light(Box::new(dir_light), 1.);
    let mut cull_lights = render_target::CullLightProcessor::new(render_width, render_height, 16);
    let mut to_cache = render_target::ToCacheProcessor{};
    let mut main_pass = render_pass::RenderPass::new(vec![&mut depth_render, &mut msaa, &mut dir_light_depth_render], 
        vec![&mut cull_lights, &mut eb, &mut blur, &mut compose, &mut to_cache], 
        render_pass::Pipeline::new(vec![0], vec![(0, 3), (3, 2), (2, 7), (7, 1), (1, 4), (4, 5), (5, 6), (1, 6)]));

    let mut wnd_size : (u32, u32) = (render_width, render_height);
    let wnd = wnd_ctx.gl_window();
    let mut controller = controls::PlayerControls::new(wnd.window());
    let mut laser = entity::EntityFlyweight::new(model::Model::load("assets/laser2.obj", &wnd_ctx));
    let container = entity::Entity::from(model::Model::load("assets/BlackMarble/floor.obj", &wnd_ctx), 
        node::Node::new(Some(point3(0., -5., 0.)), None, Some(vec3(20., 1., 20.)), None));

    laser.new_instance(entity::EntityInstanceData {
        transform: node::Node::new(Some(point3(0., 0., 0.)), None, Some(vec3(0.3, 0.3, 3.)), None),
        velocity: vec3(0., 0., 0.), visible: true,
    }, &wnd_ctx);
    laser.new_instance(entity::EntityInstanceData {
        transform: node::Node::new(Some(point3(-120., 120., 0.)), None, None, None),
        velocity: vec3(0., 0., 0.), visible: true,
    }, &wnd_ctx);
    let mut prev_time = Instant::now();
    e_loop.run_return(|ev, _, control| {
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
            Event::MainEventsCleared => {
                let dt = Instant::now().duration_since(prev_time).as_secs_f64();
                prev_time = Instant::now();
                let aspect = (wnd_size.0 as f32) / (wnd_size.1 as f32);
                user.move_player(&controller, dt);
                handle_shots(&user, &controller, &mut laser, &wnd_ctx);
                let light_data : Vec<shader::LightData> = laser.positions().iter().map(|node| {
                    let mat : cgmath::Matrix4<f32> = From::from(*node);
                    let start = mat.transform_point(point3(0., 0., 3.));
                    let end = mat.transform_point(point3(0., 0., -3.));
                    shader::LightData {
                        light_start: [start.x, start.y, start.z, 0f32],
                        light_end: [end.x, end.y, end.z, 0f32],
                    }
                }).collect();
                main_scene.set_lights(&light_data);
                main_scene.render_pass(&mut main_pass, &user, aspect, &shader_manager, 
                    |fbo, scene_data, rt, cache| {
                    fbo.clear_color_and_depth((0., 0., 0., 1.), 1.);
                    if rt == shader::RenderPassType::Visual {
                        main_skybox.borrow().render(fbo, &scene_data, cache, &shader_manager);
                        laser.render(fbo, &scene_data, cache, &shader_manager);
                    }
                    user.render(fbo, &scene_data, cache, &shader_manager);
                    asteroid.render(fbo, &scene_data, cache, &shader_manager);
                    container.render(fbo, &scene_data, cache, &shader_manager);
                });
                controller.reset_toggles();
                let q : Quaternion<f64> = Euler::<Deg<f64>>::new(Deg::<f64>(0.), 
                    Deg::<f64>(45. * dt), Deg::<f64>(0.)).into();
                laser.instances[0].transform.orientation = laser.instances[0].transform.orientation *
                    q;
                laser.instance_motion(dt);
            }
            _ => (),
        };
    });

}
