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
use std::cell::RefCell;
use cg_support::node;
use graphics_engine::particles;

fn handle_shots(user: &player::Player, controller: &controls::PlayerControls, lasers: &mut object::GameObject) {
    if controller.fire {
        let mut transform = user.root().borrow().clone();
        transform.scale = cgmath::vec3(0.3, 0.3, 1.);
        transform.pos += user.forward() * 10.;
        lasers.new_instance(transform, Some(user.forward() * 40f64));
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
    let msaa = Box::new(render_target::MsaaRenderTarget::new(8, render_width, render_height, wnd_ctx));
    let eb = Box::new(texture_processor::ExtractBrightProcessor::new(wnd_ctx, render_width, render_height));
    let blur = Box::new(texture_processor::SepConvProcessor::new(render_width, render_height, 10, wnd_ctx));
    let compose = Box::new(texture_processor::UiCompositeProcessor::new(wnd_ctx, || { 
        let mut surface = graphics_engine::get_active_ctx().as_surface();
        surface.clear_color_and_depth((1., 0., 0., 1.), 1.);
        surface
    }, |disp| disp.finish()));
    let depth_render = Box::new(render_target::DepthRenderTarget::new(render_width, render_height, 
        None, None));
    let cull_lights = Box::new(texture_processor::CullLightProcessor::new(render_width, render_height, 16));
    let to_cache = Box::new(texture_processor::ToCacheProcessor::new());

    let user_clone = user.clone();
    let render_cascade_1 = Box::new(render_target::DepthRenderTarget::new(2048, 2048, None, 
    Some(Box::new(move |_| {user_clone.borrow().get_cam().get_cascade(vec3(-120., 120., 0.), 0.1, 30., 2048) }))));

    let user_clone = user.clone();
    let render_cascade_2 = Box::new(render_target::DepthRenderTarget::new(2048, 2048, None, 
    Some(Box::new(move |_| {user_clone.borrow().get_cam().get_cascade(vec3(-120., 120., 0.), 30., 80., 2048) }))));

    let user_clone = user.clone();
    let render_cascade_3 = Box::new(render_target::DepthRenderTarget::new(2048, 2048, None, 
    Some(Box::new(move |_| {user_clone.borrow().get_cam().get_cascade(vec3(-120., 120., 0.), 80., 400., 2048) }))));
    pipeline::RenderPass::new(vec![depth_render, msaa, render_cascade_1, render_cascade_2, render_cascade_3], 
        vec![cull_lights, eb, blur, compose, to_cache], 
        pipeline::Pipeline::new(vec![0], vec![(0, (5, 0)), (5, (2, 0)), (5, (3, 0)), (5, (4, 0)), (2, (9, 0)), (3, (9, 1)), (4, (9, 2)), (9, (1, 0)),
            (1, (6, 0)), (6, (7, 0)), (7, (8, 1)), (1, (8, 0))]))
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
        .with_collisions("assets/asteroid1/Asteroid.obj", collisions::TreeStopCriteria::default()).immobile();
    let asteroid_character = RefCell::new(object::AnimGameObject::new(model::Model::new("assets/animTest/dancing_vampire.dae", &*wnd.ctx())).with_depth());
    asteroid_character.borrow().transform().borrow_mut().scale = vec3(0.07, 0.07, 0.07);
    (*asteroid_character.borrow_mut()).start_anim("", true);
    let mut skybox = cubes::Skybox::cvt_from_sphere("assets/Milkyway/Milkyway_BG.jpg", 2048, &*wnd.shaders, &*wnd.ctx());
    let ibl = scene::gen_ibl_from_hdr("assets/Milkyway/Milkyway_Light.hdr", &mut skybox, &*wnd.shaders, &*wnd.ctx());
    let sky_entity = Rc::new(RefCell::new(skybox.to_entity()));

    gen_asteroid_field(&mut asteroid);

    let mut main_scene = scene::Scene::new(get_main_render_pass(render_width, render_height, user.clone(), &*wnd.ctx()),
        user.clone());
    main_scene.set_ibl_maps(ibl);
    main_scene.set_light_dir(vec3(-120., 120., 0.));

    let laser = Rc::new(RefCell::new(object::GameObject::new(model::Model::new("assets/laser2.obj", &*wnd.ctx()).with_instancing(), 
        object::ObjectType::Laser).with_collisions("assets/laser2.obj", collisions::TreeStopCriteria::default())));
    let container = object::GameObject::new(model::Model::new("assets/BlackMarble/floor.obj", &*wnd.ctx()), object::ObjectType::Any) 
        .at_pos(node::Node::new(Some(point3(0., -5., 0.)), None, Some(vec3(20., 1., 20.)), None)).with_depth();
    let particles = Rc::new(RefCell::new(
        particles::ParticleSystem::new().with_emitter(particles::dust_emitter(&*wnd.ctx(), point3(0., 0., 0.)), 0)
        .with_billboard("assets/particles/smoke_07.png").with_billboard("assets/particles/circle_05.png")));
    
    // skybox must be rendered first, particles must be rendered last
    main_scene.set_entities(vec![sky_entity, user.borrow().as_entity(), laser.borrow().as_entity(), container.as_entity(), asteroid.as_entity(),
        asteroid_character.borrow().as_entity(), particles.clone()]);
    wnd.scene_manager().insert_scene("main", main_scene).change_scene("main");

    laser.borrow_mut().new_instance(node::Node::default().scale(vec3(0.3, 0.3, 3.)).pos(point3(10., 0., 10.)), None);
    laser.borrow_mut().new_instance(node::Node::default().pos(point3(-120., 120., 0.)), None);
    laser.borrow_mut().body(0).rot_vel = Euler::<Deg<f64>>::new(Deg::<f64>(0.), Deg::<f64>(45. * 0.05), Deg::<f64>(0.)).into();

    let dead_lasers = RefCell::new(Vec::new());
    let mut sim = physics::Simulation::<object::ObjectType>::new(point3(0., 0., 0.), 200.)
    .with_on_hit(|a, _, hit| {
        if a.metadata == object::ObjectType::Laser {
            let ctx = graphics_engine::get_active_ctx();
            let facade = ctx.ctx.borrow();
            particles.borrow_mut().new_emitter(
                particles::laser_hit_emitter(hit.pos_norm_b.0, hit.pos_norm_b.1, a.velocity, &*facade), 1
            );
            dead_lasers.borrow_mut().push(a.transform.clone());
            false
        } else { true }
    });

    let mut draw_cb = |dt : std::time::Duration, mut scene : std::cell::RefMut<scene::Scene>| {

        dead_lasers.borrow_mut().clear();
        {
            let mut u = user.borrow_mut();
            let c = controller.borrow();
            let mut lz = laser.borrow_mut();
            handle_shots(&*u, &*c, &mut *lz);
            let mut bodies = asteroid.bodies_ref();
            bodies.append(&mut lz.bodies_ref());
            bodies.push(u.as_rigid_body(&*c));
            sim.step(&bodies, dt);
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
                lights.push(shader::LightData::tube_light(start, end, 1.5, 10., 
                    vec3(0.5451, 0., 0.5451)));
            });
            lights.append(&mut particles.borrow().lights().unwrap_or_else(|| Vec::new()));
            lights
        };
        (&mut *scene).set_lights(&light_data);
        controller.borrow_mut().reset_toggles();
    };
    let mut controller_cb = |ev, _: std::cell::RefMut<SceneManager>| (&mut *controller.borrow_mut()).on_input(ev);
    let mut resize_cb = |new_size : glutin::dpi::PhysicalSize<u32>| {
        if new_size.height != 0 {
            user.borrow_mut().aspect = new_size.width as f32 / new_size.height as f32;
        }
    };
    let cbs = WindowCallbacks::new().with_draw_handler(&mut draw_cb).with_input_handler(&mut controller_cb)
        .with_resize_handler(&mut resize_cb);
    println!("Start game loop");
    wnd.main_loop(cbs);

}
