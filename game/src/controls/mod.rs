mod ai;
mod pathfinding;
mod user_input;
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum Movement {
    Stopped,
    Forward,
    Backwards,
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum PlayerActionState {
    Idle,
    Fire,
    FireRope,
    CutRope,
}

/// `MovementControl` is a trait that controls the movement of a character
pub trait MovementControl {
    /// Gets the current movement state of the character
    fn get_movement(&self) -> Movement;

    /// Gets the current roll (rotation around Z axis) of the character
    fn get_roll(&self) -> f64;

    /// Gets the current pitch (rotation around X axis) of the character
    fn get_pitch(&self) -> f64;

    /// Gets the current action state of the character
    fn get_action_state(&self) -> PlayerActionState;

    /// Get the player's transparency factor from `0.0` (opaque) to `1.0` (fully refractive)
    /// and updates it if it is transitioning
    fn get_transparency_fac(&mut self) -> f32;

    /// Registers that the player's action state has been recognized, and the next
    /// action state can be transitioned to
    fn transition_action_state(&mut self);

    /// Performs any necessary logic after every frame
    fn on_frame_update(&mut self, dt: std::time::Duration);
}

pub use user_input::PlayerControls;
