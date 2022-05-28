extern crate cgmath;
extern crate glium;
mod cg_support;
mod graphics_engine;
mod collisions;
mod object;
mod player;
mod controls;
mod physics;
extern crate gl;
use graphics_engine::window::*;

use cgmath::*;
use graphics_engine::pipeline::*;
use graphics_engine::*;

use std::rc::Rc;
use std::cell::{RefCell};
use cg_support::node;
use graphics_engine::particles;

fn handle_shots(user: &player::Player, controller: &controls::PlayerControls, lasers: &mut object::GameObject) {
    if controller.fire {
        let mut transform = user.root().borrow().clone()
            .scale(cgmath::vec3(0.3, 0.3, 1.));
        transform.translate(user.forward() * 10.);
        let (typ, speed) = if controller.fire_rope 
            { (object::ObjectType::Hook, 200.) }
        else 
            { (object::ObjectType::Laser, 120.) };
        lasers.new_instance(transform, Some(user.forward() * speed))
            .metadata = typ;
    }
}

fn gen_asteroid_field(obj: &mut object::GameObject) {
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
        obj.new_instance(transform, None);
    }
}

fn get_main_render_pass(render_width: u32, render_height: u32, user: Rc<RefCell<player::Player>>, wnd_ctx: &glium::Display) -> RenderPass {
    use glium::{Surface};
    use pipeline::*;
    use graphics_engine::drawable::Viewer;
    let msaa = Box::new(render_target::MsaaRenderTarget::new(8, render_width, render_height, wnd_ctx));
    let eb = Box::new(texture_processor::ExtractBrightProcessor::new(wnd_ctx, render_width, render_height));
    let blur = Box::new(texture_processor::SepConvProcessor::new(render_width, render_height, 10, wnd_ctx));
    let compose = Box::new(texture_processor::UiCompositeProcessor::new(wnd_ctx, || { 
        let mut surface = graphics_engine::get_active_ctx().as_surface();
        surface.clear_color_and_depth((1., 0., 0., 1.), 1.);
        surface
    }, |disp| disp.finish()));
    let depth_render = Box::new(render_target::DepthRenderTarget::new(render_width, render_height, 
        None, None, false));
    let cull_lights = Box::new(texture_processor::CullLightProcessor::new(render_width, render_height, 16));
    let to_cache = Box::new(texture_processor::ToCacheProcessor::new());

    let user_clone = user.clone();
    let translucency = Box::new(render_target::CubemapRenderTarget::new(1024, user.borrow().view_dist().1, 
        Box::new(move || user_clone.borrow().root()
            .borrow().mat().transform_point(point3(0., 0., 0.)).cast().unwrap()), wnd_ctx)
        .with_trans_getter(Box::new(|| 0))
        .with_pass(shader::RenderPassType::Transparent(user.borrow().as_entity().as_ptr() as *const entity::Entity)));
    let trans_to_cache = Box::new(texture_processor::ToCacheProcessor::new());
    let cam_depth_to_cache = Box::new(texture_processor::ToCacheProcessor::new());

    let user_clone = user.clone();
    let render_cascade_1 = Box::new(render_target::DepthRenderTarget::new(2048, 2048, None, 
    Some(Box::new(move |_| {user_clone.borrow().get_cam().get_cascade(vec3(-120., 120., 0.), 0.1, 30., 2048) })), true));

    let user_clone = user.clone();
    let render_cascade_2 = Box::new(render_target::DepthRenderTarget::new(2048, 2048, None, 
    Some(Box::new(move |_| {user_clone.borrow().get_cam().get_cascade(vec3(-120., 120., 0.), 30., 80., 2048) })), true));

    let user_clone = user.clone();
    let render_cascade_3 = Box::new(render_target::DepthRenderTarget::new(2048, 2048, None, 
    Some(Box::new(move |_| {user_clone.borrow().get_cam().get_cascade(vec3(-120., 120., 0.), 80., 400., 2048) })), true));

    let user_clone = user.clone();
    pipeline::RenderPass::new(vec![depth_render, msaa, render_cascade_1, render_cascade_2, render_cascade_3, translucency], 
        vec![cull_lights, eb, blur, compose, to_cache, trans_to_cache, cam_depth_to_cache], 
        pipeline::Pipeline::new(vec![0], vec![
            (0, (6, 0)), (0, (12, 0)), (12, (1, 0)), // depth map to light culling and main render
            (6, (2, 0)), (6, (3, 0)), (6, (4, 0)), // cull -> render cascades
            (2, (10, 0)), (3, (10, 1)), (4, (10, 2)), // cascade to cache
            (10, (1, 0)), (10, (5, 0)), (5, (11, 0)), (11, (1, 1)), // translucency, and store in cache
            (1, (7, 0)), (7, (8, 0)), (8, (9, 1)), (1, (9, 0)) // main pass w/ bloom
        ])
    ).with_active_pred(Box::new(move |stage| {
        match stage {
            5 | 11 if *user_clone.borrow().trans_fac() > f32::EPSILON => true,
            5 | 11 => false,
            _ => true,
        }
    }))
}


fn main() {
    let render_width = 1920;
    let render_height = 1080;

    let controller = RefCell::new(controls::PlayerControls::new());
    let mut wnd = WindowMaker::new(render_width, render_height).title("Space Fight")
        .depth_buffer(24).msaa(4).build();

    let ship_model = model::Model::new("assets/Ships/StarSparrow01.obj", &*wnd.ctx());
    let user = Rc::new(RefCell::new(player::Player::new(ship_model, render_width as f32 / render_height as f32, 
        "assets/Ships/StarSparrow01.obj")));
    let mut asteroid = object::GameObject::new(model::Model::new("assets/asteroid1/Asteroid.obj", &*wnd.ctx()).with_instancing(), 
        object::ObjectType::Asteroid).with_depth()
        .with_collisions("assets/asteroid1/Asteroid.obj", collisions::TreeStopCriteria::default()).density(2.71);//.immobile();
    let asteroid_character = RefCell::new(object::AnimGameObject::new(model::Model::new("assets/animTest/dancing_vampire.dae", &*wnd.ctx())).with_depth());
    asteroid_character.borrow().transform().borrow_mut().set_scale(vec3(0.07, 0.07, 0.07));
    (*asteroid_character.borrow_mut()).start_anim("", true);
    let mut skybox = cubes::Skybox::cvt_from_sphere("assets/Milkyway/Milkyway_BG.jpg", 2048, &*wnd.shaders, &*wnd.ctx());
    let ibl = scene::gen_ibl_from_hdr("assets/Milkyway/Milkyway_Light.hdr", &mut skybox, &*wnd.shaders, &*wnd.ctx());
    let sky_entity = Rc::new(RefCell::new(skybox.to_entity()));
    let cloud = entity::EntityBuilder::new(cubes::Volumetric::cloud(128, &*wnd.ctx()))
        .at(node::Node::default().pos(point3(15., 5., 5.)).u_scale(15.))
        .with_pass(shader::RenderPassType::Visual)
        .render_order(entity::RenderOrder::Last).build();
    let cloud = Rc::new(RefCell::new(cloud));

    gen_asteroid_field(&mut asteroid);

    let mut main_scene = scene::Scene::new(get_main_render_pass(render_width, render_height, user.clone(), &*wnd.ctx()),
        user.clone());
    main_scene.set_ibl_maps(ibl);
    main_scene.set_light_dir(vec3(-120., 120., 0.));

    let laser = Rc::new(RefCell::new(object::GameObject::new(model::Model::new("assets/laser2.obj", &*wnd.ctx()).with_instancing(), 
        object::ObjectType::Laser).with_collisions("assets/laser2.obj", collisions::TreeStopCriteria::default())));
    let lines = Rc::new(RefCell::new(primitives::Lines::new(&*wnd.ctx())));
    let container = object::GameObject::new(model::Model::new("assets/BlackMarble/floor.obj", &*wnd.ctx()), object::ObjectType::Any) 
        .at_pos(node::Node::new(Some(point3(0., -5., 0.)), None, Some(vec3(20., 1., 20.)), None)).with_depth();
    let particles = Rc::new(RefCell::new(
        particles::ParticleSystem::new()//.with_emitter(particles::dust_emitter(&*wnd.ctx(), point3(0., 0., 0.)), 0)
        .with_billboard("assets/particles/smoke_01.png", 0.4).with_billboard("assets/particles/circle_05.png", 0.4)));
    
    // skybox must be rendered first, particles must be rendered last
    main_scene.set_entities(vec![sky_entity, user.borrow().as_entity(), laser.borrow().as_entity(), container.as_entity(), asteroid.as_entity(),
        asteroid_character.borrow().as_entity(), lines.clone(), particles.clone(), cloud]);
    wnd.scene_manager().insert_scene("main", main_scene).change_scene("main");

    laser.borrow_mut().new_instance(node::Node::default().scale(vec3(0.3, 0.3, 3.)).pos(point3(10., 0., 10.)), None);
    laser.borrow_mut().new_instance(node::Node::default().pos(point3(-120., 120., 0.)), None);
    laser.borrow_mut().body(0).base.rot_vel = vec3(0., 10., 0.);

    let dead_lasers = RefCell::new(Vec::new());
    let mut forces = Vec::new();
    let new_forces : RefCell<Vec<Box<dyn physics::Manipulator<object::ObjectType>>>> 
        = RefCell::new(Vec::new());
    let mut sim = physics::Simulation::<object::ObjectType>::new(point3(0., 0., 0.), 200.)
    .with_on_hit(|a, b, hit, player| {
        if a.metadata == object::ObjectType::Laser ||
            b.metadata == object::ObjectType::Laser {
            let ctx = graphics_engine::get_active_ctx();
            let facade = ctx.ctx.borrow();
            particles.borrow_mut().new_emitter(
                particles::laser_hit_emitter(hit.pos_norm_b.0, 
                    hit.pos_norm_b.1, a.base.velocity - b.base.velocity, &*facade), 1
            );
            let laser_transform = if a.metadata == object::ObjectType::Laser 
            { a.base.transform.clone() } 
            else { b.base.transform.clone() };
            dead_lasers.borrow_mut().push(laser_transform);
        } 
        if b.metadata == object::ObjectType::Asteroid ||
            a.metadata == object::ObjectType::Asteroid {
            let relative_vel = a.base.velocity - b.base.velocity;
            if relative_vel.magnitude() > 1. {
                let ctx = graphics_engine::get_active_ctx();
                let facade = ctx.ctx.borrow();
                particles.borrow_mut().new_emitter(
                    particles::asteroid_hit_emitter(hit.pos_norm_b.0, 
                        hit.pos_norm_b.1, relative_vel, &*facade), 0
                );
            }
        }
        if a.metadata == object::ObjectType::Hook ||
            b.metadata == object::ObjectType::Hook {
                let target = if a.metadata == object::ObjectType::Hook 
                    { b } else { a };
                let hit_point = if a.metadata == object::ObjectType::Hook 
                    { hit.pos_norm_b.0 } else { hit.pos_norm_a.0 };
                let ship_front = player.transform.borrow().transform_point(point3(0., 0., 8.));
                let hit_local = target.base.transform.borrow().mat()
                    .invert().unwrap().transform_point(hit_point);
                lines.borrow_mut().add_line(0, primitives::LineData {
                    color: [1., 0., 0., 1.],
                    start: node::Node::default().parent(player.transform.clone())
                        .pos(point3(0., 0., 10.)),
                    end: node::Node::default().parent(target.base.transform.clone())
                        .pos(hit_local),
                });
                new_forces.borrow_mut().push(Box::new(physics::Tether::new( 
                physics::TetherData {
                    attach_a: hit_local,
                    a: Rc::downgrade(&target.base.transform),
                    attach_b: ship_front,
                    b: Rc::downgrade(&player.transform),
                    length: (ship_front - hit_point).magnitude()
                })));
            let laser_transform = if a.metadata == object::ObjectType::Hook
            { a.base.transform.clone() } 
            else { b.base.transform.clone() };
            dead_lasers.borrow_mut().push(laser_transform);
        }
    }).with_do_resolve(|a, b, _| {
        // no collision resolution for lasers
        a.metadata != object::ObjectType::Laser && b.metadata != object::ObjectType::Laser &&
        a.metadata != object::ObjectType::Hook && b.metadata != object::ObjectType::Hook
    });

    let mut draw_cb = |dt : std::time::Duration, mut scene : std::cell::RefMut<scene::Scene>| {

        *user.borrow().trans_fac() = controller.borrow_mut().compute_transparency_fac();
        dead_lasers.borrow_mut().clear();
        {
            let mut bodies = asteroid.bodies_ref();
            let mut u = user.borrow_mut();
            if controller.borrow().cut_rope {
                forces.clear();
                lines.borrow_mut().remove_line(0);
            }
            let c = controller.borrow();
            let mut lz = laser.borrow_mut();
            handle_shots(&*u, &*c, &mut *lz);
            bodies.append(&mut lz.bodies_ref());
            bodies.push(u.as_rigid_body(&*c));
            let player_idx = bodies.len() - 1;
            forces.append(&mut new_forces.borrow_mut());
            sim.step(&mut bodies, &forces, player_idx, dt);
        }
        laser.borrow_mut().retain(|laser_ptr|
            !dead_lasers.borrow().iter().any(|dead| dead.as_ptr() as *const () == laser_ptr));
        particles.borrow_mut().emit(dt);
        let light_data = {
            let mut lights = Vec::new();
            laser.borrow().iter_positions(|node| {
                let mat : cgmath::Matrix4<f32> = From::from(node);
                let start = mat.transform_point(point3(0., 0., 3.));
                let end = mat.transform_point(point3(0., 0., -3.));
                let radius = 1.5;
                let luminance = 80.;
                lights.push(shader::LightData::tube_light(start, end, radius, luminance, 
                    vec3(0.5451, 0., 0.5451)));
            });
            lights.append(&mut particles.borrow().lights().unwrap_or_else(|| Vec::new()));
            lights
        };
        (&mut *scene).set_lights(&light_data);
        controller.borrow_mut().reset_toggles();
    };
    let mut controller_cb = |ev, _: std::cell::RefMut<SceneManager>| 
        (&mut *controller.borrow_mut()).on_input(ev);
    let mut resize_cb = |new_size : glutin::dpi::PhysicalSize<u32>| {
        if new_size.height != 0 {
            user.borrow_mut().aspect = new_size.width as f32 / new_size.height as f32;
        }
    };
    let cbs = WindowCallbacks::new().with_draw_handler(&mut draw_cb)
        .with_input_handler(&mut controller_cb)
        .with_resize_handler(&mut resize_cb);
    println!("Start game loop");
    wnd.main_loop(cbs);

}
