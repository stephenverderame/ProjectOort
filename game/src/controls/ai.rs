use super::pathfinding::ComputedPath;
use super::{Movement, MovementControl, PlayerActionState, PlayerIterator};
use crate::collisions::CollisionTree;
use crate::physics;

pub enum ActionResult {
    Success,
    Failure,
    Running(Option<super::ControllerAction>),
}

pub trait BTNode {
    #[allow(clippy::too_many_arguments)]
    // TODO: fix
    fn tick<'a>(
        &mut self,
        children: &mut [BehaviorTree],
        blackboard: &mut Blackboard,
        scene: &CollisionTree,
        player: &physics::BaseRigidBody,
        dt: std::time::Duration,
        other_players: PlayerIterator<'a>,
    ) -> ActionResult;
}

pub struct Blackboard {
    pub(super) target_location: Option<cgmath::Point3<f64>>,
    pub(super) computed_path: Option<ComputedPath>,
    pub(super) target_id: Option<usize>,
}

impl Blackboard {
    /// Creates a new blackboard for the player
    pub fn new() -> Self {
        Self {
            target_location: None,
            computed_path: None,
            target_id: None,
        }
    }
}

pub struct BehaviorTree {
    root: Box<dyn BTNode>,
    children: Vec<BehaviorTree>,
}

impl BehaviorTree {
    pub fn new(root: Box<dyn BTNode>, children: Vec<Self>) -> Self {
        Self { root, children }
    }

    pub fn tick<'a>(
        &mut self,
        blackboard: &mut Blackboard,
        scene: &CollisionTree,
        player: &physics::BaseRigidBody,
        dt: std::time::Duration,
        other_players: PlayerIterator<'a>,
    ) -> ActionResult {
        self.root.tick(
            &mut self.children,
            blackboard,
            scene,
            player,
            dt,
            other_players,
        )
    }
}

/// A node that succeeds if all of its children succeed, processed left to right
/// If any child fails or is running, the sequence returns the status of
/// the first non-successful child
///
/// Later non-none action results overwrite earlier ones
pub struct Sequence {}
impl BTNode for Sequence {
    fn tick<'a>(
        &mut self,
        children: &mut [BehaviorTree],
        blackboard: &mut Blackboard,
        scene: &CollisionTree,
        player: &physics::BaseRigidBody,
        dt: std::time::Duration,
        other_players: PlayerIterator<'a>,
    ) -> ActionResult {
        for child in children {
            match child.tick(blackboard, scene, player, dt, other_players) {
                ActionResult::Success => continue,
                x => return x,
            }
        }
        ActionResult::Success
    }
}

/// A node that succeeds if any of its children succeed, processed left to right
/// The fallback returns the status of the first non-failure child
pub struct Fallback {}
impl BTNode for Fallback {
    fn tick<'a>(
        &mut self,
        children: &mut [BehaviorTree],
        blackboard: &mut Blackboard,
        scene: &CollisionTree,
        player: &physics::BaseRigidBody,
        dt: std::time::Duration,
        other_players: PlayerIterator<'a>,
    ) -> ActionResult {
        for child in children {
            match child.tick(blackboard, scene, player, dt, other_players) {
                ActionResult::Failure => continue,
                x => return x,
            }
        }
        ActionResult::Failure
    }
}

pub struct AIController {
    pub(super) behavior_tree: BehaviorTree,
    pub(super) blackboard: Blackboard,
}

impl AIController {
    /// Creates a new AI controller for the player
    pub fn new(behavior_tree: BehaviorTree) -> Self {
        Self {
            behavior_tree,
            blackboard: Blackboard::new(),
        }
    }
}

impl MovementControl for AIController {
    fn get_movement(&self) -> Movement {
        Movement::Stopped
    }

    fn get_roll(&self) -> f64 {
        0.
    }

    fn get_pitch(&self) -> f64 {
        0.
    }

    fn get_action_state(&self) -> PlayerActionState {
        PlayerActionState::Idle
    }

    fn get_transparency_fac(&mut self) -> f32 {
        0.
    }

    fn transition_action_state(&mut self) {
        // TODO
    }

    fn on_frame_update<'a>(
        &mut self,
        scene: &CollisionTree,
        player: &physics::BaseRigidBody,
        dt: std::time::Duration,
        other_players: PlayerIterator<'a>,
    ) -> Option<super::ControllerAction> {
        if let ActionResult::Running(Some(action)) = self.behavior_tree.tick(
            &mut self.blackboard,
            scene,
            player,
            dt,
            other_players,
        ) {
            Some(action)
        } else {
            None
        }
    }
}
