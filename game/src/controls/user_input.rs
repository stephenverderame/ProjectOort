use super::{Movement, MovementControl, PlayerActionState, PlayerIterator};
use crate::{collisions, physics};
use glutin::event::*;
// `PlayerControls` converts device inputs to higher level
/// game controls
pub struct PlayerControls {
    movement: Movement,
    pitch: f64,
    roll: f64,
    state: PlayerActionState,
    mouse_capture: bool,
    visible: bool,
    vis_transition_start: std::time::Instant,
    inv_fac: f32,
    inv_trans_fac_start: f32,
}

impl PlayerControls {
    pub fn new() -> Self {
        Self {
            movement: Movement::Stopped,
            mouse_capture: false,
            pitch: 0.,
            roll: 0.,
            state: PlayerActionState::Idle,
            visible: true,
            vis_transition_start: std::time::Instant::now(),
            inv_fac: 0.,
            inv_trans_fac_start: 0.,
        }
    }
    /// Changes the mouse capture mode and returns the new value
    fn change_mouse_mode(
        mouse_capture: bool,
        wnd: &glutin::window::Window,
    ) -> bool {
        let rev = !mouse_capture;
        wnd.set_cursor_grab(rev).unwrap();
        wnd.set_cursor_visible(mouse_capture);
        rev
    }

    /// Callback to handle input events from the window to control the player
    #[allow(clippy::too_many_lines)]
    pub fn on_input(&mut self, ev: &DeviceEvent) {
        let ctx = crate::graphics_engine::get_active_ctx();
        match ev {
            #[allow(deprecated)]
            DeviceEvent::Key(KeyboardInput {
                scancode: _,
                state,
                virtual_keycode: Some(vk),
                modifiers: _,
            }) => match (vk, state) {
                (VirtualKeyCode::W, ElementState::Pressed) => {
                    self.movement = Movement::Forward;
                }
                (
                    VirtualKeyCode::W | VirtualKeyCode::S,
                    ElementState::Released,
                ) => {
                    self.movement = Movement::Stopped;
                }
                (VirtualKeyCode::S, ElementState::Pressed) => {
                    self.movement = Movement::Backwards;
                }
                (VirtualKeyCode::T, ElementState::Pressed) => {
                    self.inv_trans_fac_start = self.inv_fac;
                    self.vis_transition_start = std::time::Instant::now();
                    self.visible = !self.visible;
                }
                (VirtualKeyCode::Escape, ElementState::Pressed) => {
                    self.mouse_capture = Self::change_mouse_mode(
                        self.mouse_capture,
                        &*ctx.ctx.borrow().gl_window().window(),
                    );
                }
                _ => (),
            },
            DeviceEvent::MouseMotion { delta: (dx, dy) }
                if self.mouse_capture =>
            {
                self.pitch = *dy;
                self.roll = *dx;
            }
            DeviceEvent::Button { button, state } if self.mouse_capture => {
                // button 1 is lmouse, 3 is rmouse, 2 is middle mouse
                if *button == 1 && *state == ElementState::Pressed {
                    self.state = PlayerActionState::Fire;
                } else if *button == 3 && *state == ElementState::Pressed {
                    self.state = PlayerActionState::FireRope;
                } else if *button == 3 && *state == ElementState::Released {
                    self.state = PlayerActionState::CutRope;
                }
            }
            _ => (),
        }
    }
}

impl MovementControl for PlayerControls {
    fn get_movement(&self) -> Movement {
        self.movement
    }

    fn get_roll(&self) -> f64 {
        self.roll
    }

    fn get_pitch(&self) -> f64 {
        self.pitch
    }

    fn get_action_state(&self) -> PlayerActionState {
        self.state
    }

    fn get_transparency_fac(&mut self) -> f32 {
        let goal_fac = if self.visible { 0.0 } else { 1.0 };
        if (self.inv_fac - goal_fac).abs() > f32::EPSILON {
            let dt = (std::time::Instant::now()
                .duration_since(self.vis_transition_start)
                .as_secs_f32()
                / 3.)
                .min(1.);
            self.inv_fac = dt.mul_add(
                goal_fac - self.inv_trans_fac_start,
                self.inv_trans_fac_start,
            );
        }
        self.inv_fac
    }

    fn transition_action_state(&mut self) {
        self.state = PlayerActionState::Idle;
    }

    fn on_frame_update(
        &mut self,
        _scene: &collisions::CollisionTree,
        _player: &physics::BaseRigidBody,
        _dt: std::time::Duration,
        _other_players: PlayerIterator,
    ) -> Option<super::ControllerAction> {
        self.pitch = 0.;
        self.roll = 0.;
        None
    }

    fn on_death(&mut self) {
        self.state = PlayerActionState::Idle;
        self.visible = true;
        self.inv_fac = 0.0;
    }
}
