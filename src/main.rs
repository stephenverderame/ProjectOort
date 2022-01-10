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
mod collisions;
extern crate gl;

use draw_traits::{Drawable, Viewer};
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
    let mut gen_sky_pass = render_pass::RenderPass::new(vec![&mut gen_sky], vec![&mut cp], render_pass::Pipeline::new(vec![0], vec![(0, (1, 0))]));
    let gen_sky_ptr = &mut gen_sky_pass as *mut render_pass::RenderPass;
    unsafe {
        let sky_cbo = gen_sky_scene.render_pass(&mut *gen_sky_ptr, &cam, shader_manager, 
        |fbo, scene_data, _, cache| {
            sky.render(fbo, scene_data, &cache, &shader_manager)
        });
        // safe bc we finish using the first borrow here
        let sky_hdr_cbo = gen_sky_scene.render_pass(&mut *gen_sky_ptr, &cam, shader_manager, 
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
    let mut cache = shader::PipelineCache::default();
    let res = rt.draw(&cam, None, &mut cache, &|fbo, viewer, _, cache, _| {
        let its = *iterations.borrow();
        let mip_level = its / 6;
        skybox.borrow_mut().set_mip_progress(Some(mip_level as f32 / (mip_levels - 1) as f32));
        let sd = draw_traits::default_scene_data(viewer);
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
        let transform = Rc::new(RefCell::new(transform));
        lasers.new_instance(entity::EntityInstanceData {
            collider: Some(collisions::CollisionObject::new(transform.clone(), "assets/laser2.obj", 
                collisions::TreeStopCriteria::AlwaysStop)),
            transform, 
            visible: true, velocity: user.forward() * 40f64,
        }, facade)
    }
}

fn gen_asteroid_field<F : glium::backend::Facade>(obj: &mut entity::EntityFlyweight, facade: &F,
    ct: &mut collisions::CollisionTree) {
    use rand::distributions::*;
    let scale_distrib = rand::distributions::Uniform::from(0.01 .. 0.3);
    let pos_distrib = rand::distributions::Uniform::from(-100.0 .. 100.0);
    let angle_distrib = rand::distributions::Uniform::from(0.0 .. 360.0);
    let mut rng = rand::thread_rng();
    for _ in 0 .. 100 {
        let scale = scale_distrib.sample(&mut rng);
        let axis = vec3(pos_distrib.sample(&mut rng), pos_distrib.sample(&mut rng), pos_distrib.sample(&mut rng)).normalize();
        let rot = Quaternion::<f64>::from_axis_angle(axis, Deg::<f64>(angle_distrib.sample(&mut rng)));
        let transform = Rc::new(RefCell::new(
            node::Node::new(Some(point3(pos_distrib.sample(&mut rng), pos_distrib.sample(&mut rng), pos_distrib.sample(&mut rng))),
            Some(rot), Some(vec3(scale, scale, scale)), None)));
        let collider = collisions::CollisionObject::new(transform.clone(), "assets/asteroid1/Asteroid.obj", 
            collisions::TreeStopCriteria::default());
        ct.insert(&collider, collisions::ObjectType::Static);
        obj.new_instance(entity::EntityInstanceData {
            collider: Some(collider),
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

    let ship_model = model::Model::new("assets/Ships/StarSparrow01.obj", &wnd_ctx);
    let user = Rc::new(RefCell::new(player::Player::new(ship_model, render_width as f32 / render_height as f32, 
        "assets/Ships/StarSparrow01.obj")));
    let mut asteroid = entity::EntityFlyweight::new(model::Model::new("assets/asteroid1/Asteroid.obj", &wnd_ctx));
    let asteroid_character = RefCell::new(entity::Entity::new(model::Model::new("assets/test/dancing_vampire.dae", &wnd_ctx)));
    asteroid_character.borrow_mut().data.transform.borrow_mut().scale = vec3(0.07, 0.07, 0.07);
    (*asteroid_character.borrow_mut()).get_animator().start("", true);
    
    let mut collision_tree = collisions::CollisionTree::new(point3(0., 0., 0.), 150.);

    let shader_manager = shader::ShaderManager::init(&wnd_ctx);

    let (sky_cbo, sky_hdr_cbo) = gen_skybox(1024, &shader_manager, &wnd_ctx);
    let main_skybox = Rc::new(RefCell::new(skybox::Skybox::new(skybox::SkyboxTex::Cube(sky_cbo), &wnd_ctx)));
    let (pre_filter, brdf_lut) = gen_prefilter_hdr_env(main_skybox.clone(), 128, &shader_manager, &wnd_ctx);
    let collision_compute = collisions::TriangleTriangleGPU::new(&shader_manager, &wnd_ctx);

    gen_asteroid_field(&mut asteroid, &wnd_ctx, &mut collision_tree);
    collision_tree.insert(&(*user.borrow()).collision_obj, collisions::ObjectType::Dynamic);

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
    let mut depth_render = render_target::DepthRenderTarget::new(render_width, render_height, None, None, shader::RenderPassType::Depth, &wnd_ctx);
    let dir_light = Rc::new(RefCell::new(camera::OrthoCamera::new(400., 400., 1., 300., point3(-120., 120., 0.), Some(point3(0., 0., 0.)), 
        Some(vec3(0., 1., 0.)))));
    main_scene.set_light_dir((*dir_light.borrow()).cam_pos().to_vec());
    let mut cull_lights = render_target::CullLightProcessor::new(render_width, render_height, 16);
    let mut to_cache = render_target::ToCacheProcessor::new(&wnd_ctx);
    //let uc = user.clone();
    let user_clone = user.clone();
    let mut render_cascade_1 = render_target::DepthRenderTarget::new(2048, 2048, None, 
    Some(Box::new(move |_| {user_clone.borrow().get_cam().get_cascade(vec3(-120., 120., 0.), 0.1, 30., 2048) })), shader::RenderPassType::Shadow, &wnd_ctx);
    //let uc = user.clone();
    let user_clone = user.clone();
    let mut render_cascade_2 = render_target::DepthRenderTarget::new(2048, 2048, None, 
    Some(Box::new(move |_| {user_clone.borrow().get_cam().get_cascade(vec3(-120., 120., 0.), 30., 80., 2048) })), shader::RenderPassType::Shadow, &wnd_ctx);
    //let uc = user.clone();
    let user_clone = user.clone();
    let mut render_cascade_3 = render_target::DepthRenderTarget::new(2048, 2048, None, 
    Some(Box::new(move |_| {user_clone.borrow().get_cam().get_cascade(vec3(-120., 120., 0.), 80., 400., 2048) })), shader::RenderPassType::Shadow, &wnd_ctx);
    let mut main_pass = render_pass::RenderPass::new(vec![&mut depth_render, &mut msaa, &mut render_cascade_1, &mut render_cascade_2, &mut render_cascade_3], 
        vec![&mut cull_lights, &mut eb, &mut blur, &mut compose, &mut to_cache], 
        render_pass::Pipeline::new(vec![0], vec![(0, (5, 0)), (5, (2, 0)), (5, (3, 0)), (5, (4, 0)), (2, (9, 0)), (3, (9, 1)), (4, (9, 2)), (9, (1, 0)),
            (1, (6, 0)), (6, (7, 0)), (7, (8, 1)), (1, (8, 0))]));

    let mut wnd_size : (u32, u32) = (render_width, render_height);
    let wnd = wnd_ctx.gl_window();
    let mut controller = controls::PlayerControls::new(wnd.window());
    let mut laser = entity::EntityFlyweight::new(model::Model::new("assets/laser2.obj", &wnd_ctx));
    let container = entity::Entity::from(model::Model::new("assets/BlackMarble/floor.obj", &wnd_ctx), 
        node::Node::new(Some(point3(0., -5., 0.)), None, Some(vec3(20., 1., 20.)), None));

    laser.new_instance(entity::EntityInstanceData {
        transform: Rc::new(RefCell::new(node::Node::new(
            Some(point3(0., 0., 0.)), None, Some(vec3(0.3, 0.3, 3.)), None))),
        velocity: vec3(0., 0., 0.), visible: true,
        collider: None,
    }, &wnd_ctx);
    laser.new_instance(entity::EntityInstanceData {
        transform: Rc::new(RefCell::new(
            node::Node::new(Some(point3(-120., 120., 0.)), None, None, None))),
        velocity: vec3(0., 0., 0.), visible: true,
        collider: None,
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
                        user.borrow_mut().aspect = new_size.width as f32 / new_size.height as f32;
                    },
                    _ => (),
                }
            },
            Event::DeviceEvent {event, ..} => controller.on_input(event),
            Event::MainEventsCleared => {
                let dt = Instant::now().duration_since(prev_time).as_secs_f64();
                prev_time = Instant::now();
                let old_user_pos = user.borrow().root.borrow().clone();
                user.borrow_mut().move_player(&controller, dt);
                {
                    let u = user.borrow();
                    handle_shots(&u, &controller, &mut laser, &wnd_ctx);
                }
                let user_colliders = collision_tree.get_colliders(&user.borrow().collision_obj);
                for collider in user_colliders {
                    if collider.is_collision(&(*user.borrow()).collision_obj, &collision_compute) {
                        *user.borrow_mut().root.borrow_mut() = old_user_pos;
                        break;
                    }
                }
                let light_data = {
                    let mut lights = Vec::new();
                    laser.iter_positions(|node| {
                        let mat : cgmath::Matrix4<f32> = From::from(node);
                        let start = mat.transform_point(point3(0., 0., 3.));
                        let end = mat.transform_point(point3(0., 0., -3.));
                        lights.push(shader::LightData {
                            light_start: [start.x, start.y, start.z, 0f32],
                            light_end: [end.x, end.y, end.z, 0f32],
                        });
                    });
                    lights
                };
                main_scene.set_lights(&light_data);
                let user_cam = {
                    let u = user.borrow();
                    u.get_cam()
                };
                main_scene.render_pass(&mut main_pass, &user_cam, &shader_manager, 
                    |fbo, scene_data, rt, cache| {
                    fbo.clear_color_and_depth((0., 0., 0., 1.), 1.);
                    if rt == shader::RenderPassType::Visual {
                        main_skybox.borrow().render(fbo, &scene_data, cache, &shader_manager);
                        laser.render(fbo, &scene_data, cache, &shader_manager);
                    }
                    user.borrow().render(fbo, &scene_data, cache, &shader_manager);
                    asteroid.render(fbo, &scene_data, cache, &shader_manager);
                    container.render(fbo, &scene_data, cache, &shader_manager);
                    asteroid_character.borrow().render(fbo, &scene_data, cache, &shader_manager);
                });
                controller.reset_toggles();
                let q : Quaternion<f64> = Euler::<Deg<f64>>::new(Deg::<f64>(0.), 
                    Deg::<f64>(45. * dt), Deg::<f64>(0.)).into();
                let orig_rot = laser.instances[0].transform.borrow().orientation;
                laser.instances[0].transform.borrow_mut().orientation = orig_rot * q;
                laser.instance_motion(dt);
                collision_tree.update();
            }
            _ => (),
        };
    });

}
