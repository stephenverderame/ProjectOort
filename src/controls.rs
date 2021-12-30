use glutin::event::*;
pub enum Movement {
    Stopped,
    Forward,
    Backwards,
}

/// PlayerControls converts device inputs to higher level
/// game controls
pub struct PlayerControls<'a> {
    pub movement: Movement,
    pub pitch: f64,
    pub roll: f64,
    pub fire: bool,
    window: &'a glutin::window::Window,
    mouse_capture: bool,
}

impl<'a> PlayerControls<'a> {
    pub fn new(window: &'a glutin::window::Window) -> PlayerControls {
        PlayerControls {
            movement: Movement::Stopped, window, 
            mouse_capture: false,
            pitch: 0., roll: 0., fire: false,
        }
    }
    /// Changes the mouse capture mode and returns the new value
    fn change_mouse_mode(mouse_capture: bool, wnd: &glutin::window::Window) -> bool {
        let rev = !mouse_capture;
        wnd.set_cursor_grab(rev).unwrap();
        wnd.set_cursor_visible(mouse_capture);
        rev
    }

    pub fn on_input(&mut self, ev: DeviceEvent) {
        match ev {
            #[allow(deprecated)]
            DeviceEvent::Key(KeyboardInput {scancode: _, state, virtual_keycode: Some(vk), modifiers: _}) => {
                match (vk, state) {
                    (VirtualKeyCode::W, ElementState::Pressed) => self.movement = Movement::Forward,
                    (VirtualKeyCode::W, ElementState::Released) => self.movement = Movement::Stopped,
                    (VirtualKeyCode::S, ElementState::Pressed) => self.movement = Movement::Backwards,
                    (VirtualKeyCode::S, ElementState::Released) => self.movement = Movement::Stopped,
                    (VirtualKeyCode::Escape, ElementState::Pressed) => 
                        self.mouse_capture = PlayerControls::change_mouse_mode(self.mouse_capture, self.window),
                    _ => (),
                }
            },
            DeviceEvent::MouseMotion {delta: (dx, dy)} if self.mouse_capture => {
                self.pitch = dy;
                self.roll = dx;
            },
            DeviceEvent::Button {button, state} if self.mouse_capture => {
                // button 1 is lmouse, 3 is rmouse, 2 is middle mouse
                if button == 1 && state == ElementState::Pressed {
                    self.fire = true;
                }
            },
            _ => (),
        }
    }

    /// Resets all toggle controls.
    /// Should be called at the end of every iteration of the game loop
    pub fn reset_toggles(&mut self) {
        self.pitch = 0.;
        self.roll = 0.;
        self.fire = false;
    }
}