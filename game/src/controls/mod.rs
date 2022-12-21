mod ai;
mod pathfinding;
mod user_input;
use crate::node;
use crate::{collisions, physics};
use ai::*;
use pathfinding::*;
use std::{cell::RefCell, rc::Rc};

/// A cloneable iterator type for players
pub trait PlayerIteratorTrait:
    Iterator<Item = Rc<RefCell<node::Node>>>
{
    fn copy(&self) -> Box<dyn PlayerIteratorTrait + '_>;
}

/// Provides type erasure to convert any cloneable player iterator into a
/// `PlayerIteratorTrait`
#[derive(Clone)]
pub struct PlayerIteratorHolder<
    T: Iterator<Item = Rc<RefCell<node::Node>>> + Clone,
>(pub T);

impl<T: Iterator<Item = Rc<RefCell<node::Node>>> + Clone> PlayerIteratorTrait
    for PlayerIteratorHolder<T>
{
    fn copy(&self) -> Box<dyn PlayerIteratorTrait + '_> {
        Box::new(Self(self.0.clone()))
    }
}

impl<T: Iterator<Item = Rc<RefCell<node::Node>>> + Clone> Iterator
    for PlayerIteratorHolder<T>
{
    type Item = Rc<RefCell<node::Node>>;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

type PlayerIterator<'a> = &'a dyn PlayerIteratorTrait;
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

/// The action that the controller will take as a result of performing an
/// update
pub struct ControllerAction {
    // TODO: control forces instead of velocity directly
    pub velocity: cgmath::Vector3<f64>,
    // TODO: add rotational control, firing control, etc.
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
    /// returning any state updates to perform
    fn on_frame_update<'a>(
        &mut self,
        scene: &collisions::CollisionTree,
        player_dynamics: &physics::BaseRigidBody,
        dt: std::time::Duration,
        other_players: PlayerIterator<'a>,
    ) -> Option<ControllerAction>;
}

pub use user_input::PlayerControls;

/// Returns a standard behavior tree
pub fn get_std_behavior_tree() -> BehaviorTree {
    let root = Box::new(Sequence {});
    let children = vec![
        BehaviorTree::new(
            Box::new(Fallback {}),
            vec![
                BehaviorTree::new(Box::new(SearchForIDedTarget {}), vec![]),
                BehaviorTree::new(Box::new(IdentifyTarget {}), vec![]),
            ],
        ),
        BehaviorTree::new(
            Box::new(Fallback {}),
            vec![
                BehaviorTree::new(Box::new(StraightLineNav::default()), vec![]),
                BehaviorTree::new(Box::new(ComputePath::new(50.)), vec![]),
            ],
        ),
    ];
    BehaviorTree::new(root, children)
}

/// Returns a standard AI controller
pub fn get_std_ai_controller() -> Rc<RefCell<dyn MovementControl>> {
    let behavior_tree = get_std_behavior_tree();
    Rc::new(RefCell::new(AIController::new(behavior_tree)))
}
