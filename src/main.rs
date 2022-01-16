extern crate cgmath;
extern crate glium;
mod cg_support;
mod graphics_engine;
mod collisions;
mod object;
mod player;
mod controls;
extern crate gl;
use graphics_engine::window::*;

use cgmath::*;
use graphics_engine::pipeline::*;
use graphics_engine::*;

use std::rc::Rc;
use std::cell::RefCell;
use cg_support::node;

fn handle_shots(user: &player::Player, controller: &controls::PlayerControls, lasers: &mut object::GameObjects) {
    if controller.fire {
        let mut transform = user.root.borrow().clone();
        transform.scale = cgmath::vec3(0.3, 0.3, 1.);
        let transform = Rc::new(RefCell::new(transform));
        lasers.new_instance(object::ObjectInstanceData {
            collider: Some(collisions::CollisionObject::new(transform.clone(), "assets/laser2.obj", 
                collisions::TreeStopCriteria::AlwaysStop)),
            transform, 
            velocity: user.forward() * 40f64,
        });
    }
}

fn gen_asteroid_field(obj: &mut object::GameObjects, ct: &mut collisions::CollisionTree) {
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
        obj.new_instance(object::ObjectInstanceData {
            collider: Some(collider),
            transform, velocity: vec3(0., 0., 0.),
        })
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
        surface.clear_color_and_depth((0., 0., 0., 1.), 1.);
        surface
    }, |disp| disp.finish()));
    let depth_render = Box::new(render_target::DepthRenderTarget::new(render_width, render_height, 
        None, None));
    let cull_lights = Box::new(texture_processor::CullLightProcessor::new(render_width, render_height, 16));
    let to_cache = Box::new(texture_processor::ToCacheProcessor::new());
    //let uc = user.clone();
    let user_clone = user.clone();
    let render_cascade_1 = Box::new(render_target::DepthRenderTarget::new(2048, 2048, None, 
    Some(Box::new(move |_| {user_clone.borrow().get_cam().get_cascade(vec3(-120., 120., 0.), 0.1, 30., 2048) }))));
    //let uc = user.clone();
    let user_clone = user.clone();
    let render_cascade_2 = Box::new(render_target::DepthRenderTarget::new(2048, 2048, None, 
    Some(Box::new(move |_| {user_clone.borrow().get_cam().get_cascade(vec3(-120., 120., 0.), 30., 80., 2048) }))));
    //let uc = user.clone();
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
    let mut wnd = WindowMaker::new(render_width, render_height).title("Space Fight").build();

    let ship_model = model::Model::new("assets/Ships/StarSparrow01.obj", &*wnd.ctx());
    let user = Rc::new(RefCell::new(player::Player::new(ship_model, render_width as f32 / render_height as f32, 
        "assets/Ships/StarSparrow01.obj")));
    let mut asteroid = object::GameObjects::new(model::Model::new("assets/asteroid1/Asteroid.obj", &*wnd.ctx())).with_depth();
    let asteroid_character = RefCell::new(object::GameObject::new(model::Model::new("assets/test/dancing_vampire.dae", &*wnd.ctx())).with_depth());
    asteroid_character.borrow_mut().data.transform.borrow_mut().scale = vec3(0.07, 0.07, 0.07);
    (*asteroid_character.borrow_mut()).start_anim("", true);
    let skybox = cubes::Skybox::new(cubes::SkyboxTex::Cube(cubes::gen_cubemap_from_sphere("assets/Milkyway/Milkyway_BG.jpg", 1024, 
        &*wnd.shaders, &*wnd.ctx())), &*wnd.ctx());
    let sky_entity = Rc::new(RefCell::new(
        entity::Entity {
            geometry: Box::new(skybox),
            locations: vec![Rc::new(RefCell::new(Matrix4::from_scale(1f64)))],
            render_passes: vec![shader::RenderPassType::Visual],
        }
    ));
    
    let mut collision_tree = collisions::CollisionTree::new(point3(0., 0., 0.), 150.);
    let collision_compute = collisions::TriangleTriangleGPU::from_active_ctx();

    gen_asteroid_field(&mut asteroid, &mut collision_tree);
    collision_tree.insert(&(*user.borrow()).collision_obj, collisions::ObjectType::Dynamic);

    let mut main_scene = scene::Scene::new(get_main_render_pass(render_width, render_height, user.clone(), &*wnd.ctx()),
        user.clone());
    main_scene.set_ibl_maps(scene::gen_ibl_from_hdr("assets/Milkyway/Milkyway_Light.hdr", &*wnd.shaders, &*wnd.ctx()));
    main_scene.set_light_dir(vec3(-120., 120., 0.));

    let mut laser = object::GameObjects::new(model::Model::new("assets/laser2.obj", &*wnd.ctx()));
    let container = object::GameObject::from(model::Model::new("assets/BlackMarble/floor.obj", &*wnd.ctx()), 
        node::Node::new(Some(point3(0., -5., 0.)), None, Some(vec3(20., 1., 20.)), None));
    
    main_scene.set_entities(vec![user.borrow().as_entity(), laser.as_entity(), container.as_entity(), asteroid.as_entity(),
        asteroid_character.borrow().as_entity(), sky_entity]);
    wnd.scene_manager().insert_scene("main", main_scene).change_scene("main");

    laser.new_instance(object::ObjectInstanceData {
        transform: Rc::new(RefCell::new(node::Node::new(
            Some(point3(0., 0., 0.)), None, Some(vec3(0.3, 0.3, 3.)), None))),
        velocity: vec3(0., 0., 0.),
        collider: None,
    });
    laser.new_instance(object::ObjectInstanceData {
        transform: Rc::new(RefCell::new(
            node::Node::new(Some(point3(-120., 120., 0.)), None, None, None))),
        velocity: vec3(0., 0., 0.),
        collider: None,
    });

    let mut draw_cb = |dt : std::time::Duration, mut scene : std::cell::RefMut<scene::Scene>| {
        let dt = dt.as_secs_f64();
        let old_user_pos = user.borrow().root.borrow().clone();
        user.borrow_mut().move_player(&*controller.borrow(), dt);
        {
            let u = user.borrow();
            handle_shots(&u, &*controller.borrow(), &mut laser);
        }
        let user_colliders = collision_tree.get_colliders(&user.borrow().collision_obj);
        for collider in user_colliders {
            if collider.is_collision(&(*user.borrow()).collision_obj, &collision_compute) {
                *user.borrow_mut().root.borrow_mut() = old_user_pos;
                break;
            }
        }

        controller.borrow_mut().reset_toggles();
        let q : Quaternion<f64> = Euler::<Deg<f64>>::new(Deg::<f64>(0.), 
            Deg::<f64>(45. * dt), Deg::<f64>(0.)).into();
        let orig_rot = laser.instances[0].transform.borrow().orientation;
        laser.instances[0].transform.borrow_mut().orientation = orig_rot * q;
        laser.instance_motion(dt);
        asteroid.instance_motion(dt);
        collision_tree.update();

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
        (&mut *scene).set_lights(&light_data);
    };
    let mut controller_cb = |ev, _: std::cell::RefMut<SceneManager>| (&mut *controller.borrow_mut()).on_input(ev);
    let mut resize_cb = |new_size : glutin::dpi::PhysicalSize<u32>| 
        user.borrow_mut().aspect = new_size.width as f32 / new_size.height as f32;
    let cbs = WindowCallbacks::new().with_draw_handler(&mut draw_cb).with_input_handler(&mut controller_cb)
        .with_resize_handler(&mut resize_cb);
    println!("Start game loop");
    wnd.main_loop(cbs);

}
