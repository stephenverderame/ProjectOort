use glutin::event::*;
pub enum Movement {
    Stopped,
    Forward,
}

/// PlayerControls converts device inputs to higher level
/// game controls
pub struct PlayerControls<'a> {
    pub movement: Movement,
    window: &'a glutin::window::Window,
    mouse_capture: bool,
}

impl<'a> PlayerControls<'a> {
    pub fn new(window: &'a glutin::window::Window) -> PlayerControls {
        PlayerControls {
            movement: Movement::Stopped, window, 
            mouse_capture: PlayerControls::change_mouse_mode(false, window),
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
            DeviceEvent::Key(KeyboardInput {scancode, state, virtual_keycode: Some(vk), modifiers}) => {
                match (vk, state) {
                    (VirtualKeyCode::W, ElementState::Pressed) => self.movement = Movement::Forward,
                    (VirtualKeyCode::W, ElementState::Released) => self.movement = Movement::Stopped,
                    (VirtualKeyCode::Escape, ElementState::Pressed) => 
                        self.mouse_capture = PlayerControls::change_mouse_mode(self.mouse_capture, self.window),
                    _ => (),
                }
            },
            _ => (),
        }
    }
}