extern crate cgmath;
extern crate glium;
mod cg_support;
mod graphics_engine;
mod collisions;
mod object;
mod player;
mod controls;
mod physics;
mod game_map;
mod game;
extern crate gl;
use graphics_engine::window::*;

use cgmath::*;
use graphics_engine::pipeline::*;
use graphics_engine::*;

use std::rc::Rc;
use std::cell::{RefCell};
use cg_support::node;

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

    let map = game_map::AsteroidMap::new(&*wnd.shaders, &*wnd.ctx());
    let player = player::Player::new(
        model::Model::new("assets/Ships/StarSparrow01.obj", &*wnd.ctx()),
        render_width as f32 / render_height as f32, "assets/Ships/StarSparrow01.obj");
    let game = RefCell::new(game::Game::new(map, player));

    let mut main_scene = scene::Scene::new(
        get_main_render_pass(render_width, render_height, 
            game.borrow().player.clone(), &*wnd.ctx()),
        game.borrow().player.clone());
    let (ibl, ldir) = game.borrow().get_map().lighting_info();
    main_scene.set_ibl_maps(ibl);
    main_scene.set_light_dir(ldir);
    
    // skybox must be rendered first, particles must be rendered last
    let mut entities = game.borrow().get_map().entities();
    entities.push(game.borrow().player.borrow().as_entity());
    main_scene.set_entities(entities);
    wnd.scene_manager().insert_scene("main", main_scene).change_scene("main");

    let sim = RefCell::new(physics::Simulation::<object::ObjectType>::new(point3(0., 0., 0.), 500.)
        .with_do_resolve(game::Game::should_resolve)
        .with_on_hit(|a, b, hit, player| {
            game.borrow().on_hit(a, b, hit, player)
        }));

    let mut draw_cb = |dt, mut scene : std::cell::RefMut<scene::Scene>| {
        game.borrow().on_draw(&mut sim.borrow_mut(), dt, &mut *scene, 
            &mut controller.borrow_mut());
        // will call on_hit
        controller.borrow_mut().reset_toggles();
    };
    let mut controller_cb = |ev, _: std::cell::RefMut<SceneManager>| 
        (&mut *controller.borrow_mut()).on_input(ev);
    let mut resize_cb = |new_size : glutin::dpi::PhysicalSize<u32>| {
        if new_size.height != 0 {
            game.borrow().player.borrow_mut().aspect = 
                new_size.width as f32 / new_size.height as f32;
        }
    };
    let cbs = WindowCallbacks::new().with_draw_handler(&mut draw_cb)
        .with_input_handler(&mut controller_cb)
        .with_resize_handler(&mut resize_cb);
    println!("Start game loop");
    wnd.main_loop(cbs);

}
