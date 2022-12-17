#![warn(clippy::pedantic, clippy::nursery)]
#![deny(clippy::all)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::wildcard_imports,
    clippy::enum_glob_use,
    clippy::similar_names,
    clippy::module_name_repetitions
)]
extern crate cgmath;
extern crate glium;
#[macro_use]
extern crate static_assertions;
#[macro_use]
extern crate lazy_static;
mod cg_support;
#[macro_use]
mod graphics_engine;
mod collisions;
mod controls;
mod game;
mod game_mediator;
mod minimap;
mod object;
mod physics;
mod player;
extern crate gl;
use graphics_engine::window::*;

use cgmath::*;
use game_mediator::*;
use graphics_engine::pipeline::*;
use graphics_engine::*;
use shared_types::game_controller::{GameController, LocalGameController};

use cg_support::node;
use std::cell::RefCell;
use std::rc::Rc;

fn get_cascade_target(
    width: u32,
    height: u32,
    user: Rc<RefCell<player::Player>>,
    near: f32,
    far: f32,
) -> Box<dyn RenderTarget> {
    Box::new(render_target::CustomViewRenderTargetDecorator::new(
        render_target::DepthRenderTarget::new_cascade(width, height, true),
        move |_| {
            user.borrow().get_cam().get_cascade(
                vec3(-120., 120., 0.),
                near,
                far,
                2048,
            )
        },
    ))
}
#[allow(clippy::too_many_lines)]
fn get_main_render_pass(
    render_width: u32,
    render_height: u32,
    user: Rc<RefCell<player::Player>>,
    wnd_ctx: &glium::Display,
) -> RenderPass {
    use graphics_engine::drawable::Viewer;
    use pipeline::*;
    let msaa = Box::new(render_target::MsaaRenderTarget::new(
        8,
        render_width,
        render_height,
        wnd_ctx,
    ));
    let eb = Box::new(texture_processor::ExtractBrightProcessor::new(
        wnd_ctx,
        render_width,
        render_height,
    ));
    let blur = Box::new(texture_processor::SepConvProcessor::new(
        render_width,
        render_height,
        10,
        wnd_ctx,
    ));
    let compose = Box::new(texture_processor::CompositorProcessor::new(
        render_width,
        render_height,
        shader::BlendFn::Add,
        wnd_ctx,
    ));
    let depth_render = Box::new(render_target::DepthRenderTarget::new(
        render_width,
        render_height,
        false,
    ));
    let cull_lights = Box::new(texture_processor::CullLightProcessor::new(
        render_width,
        render_height,
        16,
    ));
    let to_cache = Box::new(texture_processor::ToCacheProcessor::new());

    let user_clone = user.clone();
    let translucency = Box::new(
        render_target::CubemapRenderTarget::new(
            1024,
            user.borrow().view_dist().1,
            Box::new(move || {
                user_clone
                    .borrow()
                    .root()
                    .borrow()
                    .mat()
                    .transform_point(point3(0., 0., 0.))
                    .cast()
                    .unwrap()
            }),
            wnd_ctx,
        )
        .with_trans_getter(Box::new(|| 0))
        .with_pass(shader::RenderPassType::Transparent(
            user.borrow().get_entity_id(),
        )),
    );
    let trans_to_cache = Box::new(texture_processor::ToCacheProcessor::new());
    let cam_depth_to_cache =
        Box::new(texture_processor::ToCacheProcessor::new());

    let render_cascade_1 =
        get_cascade_target(2048, 2048, user.clone(), 0.1, 40.);
    let render_cascade_2 =
        get_cascade_target(2048, 2048, user.clone(), 40., 200.);
    let render_cascade_3 =
        get_cascade_target(2048, 2048, user.clone(), 200., 600.);

    pipeline! ([depth_render, msaa, render_cascade_1, render_cascade_2, render_cascade_3, translucency],
        [cull_lights, eb, blur, compose, to_cache, trans_to_cache, cam_depth_to_cache],

        depth_render -> cull_lights.0,
        depth_render -> cam_depth_to_cache.0,
        cam_depth_to_cache -> msaa.0,

        cull_lights -> render_cascade_1.0,
        cull_lights -> render_cascade_2.0,
        cull_lights -> render_cascade_3.0,

        render_cascade_1 -> to_cache.0,
        render_cascade_2 -> to_cache.1,
        render_cascade_3 -> to_cache.2,

        to_cache -> msaa.0,
        to_cache -> translucency.0,
        translucency -> trans_to_cache.0,
        trans_to_cache -> msaa.1,

        msaa -> eb.0,
        eb -> blur.0,
        blur -> compose.1,
        msaa -> compose.0,

        { trans_to_cache | translucency if *user.borrow().trans_fac() > f32::EPSILON }
    )
}

fn get_ui_render_pass(
    render_width: u32,
    render_height: u32,
    wnd_ctx: &glium::Display,
) -> RenderPass {
    use pipeline::*;
    let msaa = Box::new(render_target::MsaaRenderTarget::new(
        8,
        render_width,
        render_height,
        wnd_ctx,
    ));

    pipeline::RenderPass::new(
        vec![msaa],
        Vec::new(),
        pipeline::Pipeline::new(vec![0], Vec::new()),
    )
}

#[allow(clippy::too_many_lines)]
fn main() {
    let render_width = 1920;
    let render_height = 1080;

    let player_controls = RefCell::new(controls::PlayerControls::new());
    let mut wnd = WindowMaker::new(render_width, render_height)
        .title("Space Fight")
        .depth_buffer(24)
        .build();

    let controller = LocalGameController::new(
        &shared_types::game_controller::AsteroidMap {},
    );
    let player = player::Player::new(
        model::Model::new("assets/Ships/StarSparrow01.obj", &*wnd.ctx()),
        render_width as f32 / render_height as f32,
        "assets/Ships/StarSparrow01.obj",
        controller.get_player_stats().pid,
    );
    let mediator = LocalGameMediator::<HasLightingAvailable>::new(
        &wnd.shaders,
        &*wnd.ctx(),
        controller,
    );
    let game = game::Game::new(mediator, player);

    let mut main_scene = scene::Scene::new(
        get_main_render_pass(
            render_width,
            render_height,
            game.player.clone(),
            &*wnd.ctx(),
        ),
        game.player.clone(),
    );
    let (ibl, ldir, game) = game.get_lighting();
    main_scene.set_ibl_maps(ibl);
    main_scene.set_light_dir(ldir);

    let mut ui_scene = scene::Scene::new_no_lights(
        get_ui_render_pass(render_width, render_height, &*wnd.ctx()),
        Rc::new(RefCell::new(camera::Camera2D::new(
            render_width,
            render_height,
        ))),
    );

    let mut map_scene = scene::Scene::new_no_lights(
        get_ui_render_pass(render_width, render_height, &*wnd.ctx()),
        Rc::new(RefCell::new(camera::Camera2D::new(
            render_width,
            render_height,
        ))),
    )
    .bg((0., 0., 0., 0.6));
    let map = minimap::Minimap::new(
        game.player.borrow().root().clone(),
        3000.,
        &*wnd.ctx(),
    );
    let minimap = Rc::new(RefCell::new(map));
    map_scene.set_entities(vec![minimap.clone()]);

    let stat_text = Rc::new(RefCell::new(graphics_engine::text::Text::new(
        Rc::new(text::Font::new(
            "assets/fonts/SignedDistanceArial.fnt",
            &*wnd.ctx(),
        )),
        &*wnd.ctx(),
    )));

    let power_icon = entity::EntityBuilder::new(text::Icon::new(
        "assets/icons/electric.png",
        &*wnd.ctx(),
    ))
    .at(node::Node::default()
        .u_scale(0.05)
        .pos(point3(-0.9, 0.8, 0.0)))
    .with_pass(shader::RenderPassType::Visual)
    .build();
    let shield_icon = entity::EntityBuilder::new(text::Icon::new(
        "assets/icons/bubble-shield.png",
        &*wnd.ctx(),
    ))
    .at(node::Node::default()
        .u_scale(0.05)
        .pos(point3(-0.9, 0.9, 0.0)))
    .with_pass(shader::RenderPassType::Visual)
    .build();
    ui_scene.set_entities(vec![
        stat_text.clone(),
        Rc::new(RefCell::new(power_icon)),
        Rc::new(RefCell::new(shield_icon)),
    ]);

    // skybox must be rendered first, particles must be rendered last
    let mut entities = game.get_mediator().get_entities();
    entities.push(game.player.borrow().as_entity());
    main_scene.set_entities(entities);

    let map_screen_location = Matrix3::from_translation(vec2(-2.0f32, 0.0))
        * Matrix3::from_scale(3.0f32);
    let screen_width = Rc::new(RefCell::new(render_width));
    let screen_height = Rc::new(RefCell::new(render_height));
    let compositor_scene = scene::compositor_scene_new(
        screen_width.clone(),
        screen_height.clone(),
        Rc::new(RefCell::new(camera::Camera2D::new(
            render_width,
            render_height,
        ))),
        vec![
            (Box::new(main_scene), None),
            (Box::new(ui_scene), None),
            (Box::new(map_scene), Some(map_screen_location)),
        ],
        &*wnd.ctx(),
    );
    wnd.scene_manager()
        .insert_scene("main", Box::new(RefCell::new(compositor_scene)))
        .change_scene("main");

    let game = RefCell::new(game);

    let sim = RefCell::new(
        physics::Simulation::<object::ObjectData>::new(point3(0., 0., 0.), 1500.)
            .with_do_resolve(game::Game::<LocalGameMediator<NoLightingAvailable>>::should_resolve)
            .with_on_hit(|a, b, hit, player| game.borrow().on_hit(a, b, hit, player)),
    );

    let mut draw_cb =
        |dt, mut scene: std::cell::RefMut<dyn scene::AbstractScene>| {
            minimap.borrow_mut().clear_items();
            game.borrow().get_mediator().iter_bodies(|bods| {
                for bod in bods {
                    minimap.borrow_mut().add_item(bod);
                }
            });
            stat_text.borrow_mut().clear_text();
            stat_text.borrow_mut().add_text(
                &format!(
                    "{}",
                    game.borrow().player.borrow().shield().round() as u64
                ),
                &Rc::new(RefCell::new(
                    node::Node::default()
                        .u_scale(0.07)
                        .pos(point3(-0.78, 0.85, 0.1)),
                )),
                [0., 0., 1., 1.],
            );
            stat_text.borrow_mut().add_text(
                &format!(
                    "{}",
                    game.borrow().player.borrow().energy().round() as u64
                ),
                &Rc::new(RefCell::new(
                    node::Node::default()
                        .u_scale(0.07)
                        .pos(point3(-0.78, 0.75, 0.1)),
                )),
                [1., 1., 0., 1.],
            );
            game.borrow().on_draw(
                &mut sim.borrow_mut(),
                dt,
                &mut *scene,
                &mut player_controls.borrow_mut(),
            );
            // will call on_hit, so cannot mutably borrow game
            player_controls.borrow_mut().reset_toggles();
        };
    let mut controller_cb = |ev, _: std::cell::RefMut<SceneManager>| {
        (&mut *player_controls.borrow_mut()).on_input(&ev);
    };
    let mut resize_cb = |new_size: glutin::dpi::PhysicalSize<u32>| {
        if new_size.height != 0 {
            game.borrow().player.borrow_mut().aspect =
                new_size.width as f32 / new_size.height as f32;
            *screen_width.borrow_mut() = new_size.width;
            *screen_height.borrow_mut() = new_size.height;
        }
    };
    let cbs = WindowCallbacks::new()
        .with_draw_handler(&mut draw_cb)
        .with_input_handler(&mut controller_cb)
        .with_resize_handler(&mut resize_cb);
    println!("Start game loop");
    wnd.main_loop(cbs);
}
