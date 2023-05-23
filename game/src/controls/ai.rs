use std::cell::RefCell;
use std::rc::Rc;

use cgmath::{vec3, InnerSpace};

use super::pathfinding::ComputedPath;
use super::{Movement, MovementControl, PlayerActionState, PlayerIterator};
use crate::cg_support::node;
use crate::collisions::CollisionTree;
use crate::physics;

pub enum ActionResult {
    Success(Option<super::ControllerAction>),
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
    pub(super) path_target_location: Option<cgmath::Point3<f64>>,
}

impl Blackboard {
    /// Creates a new blackboard for the player
    pub const fn new() -> Self {
        Self {
            target_location: None,
            computed_path: None,
            target_id: None,
            path_target_location: None,
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
        let mut last_action = ActionResult::Failure;
        for child in children {
            match child.tick(blackboard, scene, player, dt, other_players) {
                x @ ActionResult::Success(_) => last_action = x,
                x => return x,
            }
        }
        last_action
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
                ActionResult::Failure => (),
                x => return x,
            }
        }
        ActionResult::Failure
    }
}

pub struct Shoot {
    last_time: std::time::Instant,
}

impl Shoot {
    pub fn new() -> Self {
        Self {
            last_time: std::time::Instant::now(),
        }
    }

    fn is_aimed_at(
        controlled: &physics::BaseRigidBody,
        target: &Rc<RefCell<node::Node>>,
    ) -> bool {
        let controlled_pos = controlled.transform.borrow().get_pos();
        let target_pos = target.borrow().get_pos();
        let controlled_dir = controlled
            .transform
            .borrow()
            .transform_vec(vec3(0., 0., 1.));
        let controlled_to_target = target_pos - controlled_pos;
        let controlled_to_target_dir = controlled_to_target.normalize();
        let controlled_to_target_angle =
            controlled_dir.angle(controlled_to_target_dir);
        controlled_to_target_angle < cgmath::Rad(0.1)
    }
}

impl BTNode for Shoot {
    fn tick<'a>(
        &mut self,
        _children: &mut [BehaviorTree],
        _blackboard: &mut Blackboard,
        _scene: &CollisionTree,
        player: &physics::BaseRigidBody,
        _dt: std::time::Duration,
        other_players: PlayerIterator<'a>,
    ) -> ActionResult {
        let is_aiming = other_players
            .copy()
            .any(|other| Self::is_aimed_at(player, &other));
        if !is_aiming {
            return ActionResult::Failure;
        }
        if self.last_time.elapsed().as_secs_f32() > 0.5 {
            self.last_time = std::time::Instant::now();
            ActionResult::Success(Some(super::ControllerAction {
                fire: true,
                velocity: vec3(0., 0., 0.),
            }))
        } else {
            ActionResult::Running(None)
        }
    }
}

/// A node that fails if the target location is too far
/// from the target location used to compute a path
/// or if a path has never been computed
///
/// So succeeds when a path exists, fails when a path needs to be computed
pub struct ShouldRecomputePath {}

impl BTNode for ShouldRecomputePath {
    fn tick<'a>(
        &mut self,
        _children: &mut [BehaviorTree],
        blackboard: &mut Blackboard,
        _scene: &CollisionTree,
        _player: &physics::BaseRigidBody,
        _dt: std::time::Duration,
        _other_players: PlayerIterator<'a>,
    ) -> ActionResult {
        if let (Some(target_loc), Some(actual_loc)) =
            (blackboard.path_target_location, blackboard.target_location)
        {
            if (target_loc - actual_loc).magnitude() > 10. {
                return ActionResult::Failure;
            }
            return ActionResult::Success(None);
        }
        ActionResult::Failure
    }
}

/// Forces a path to be recomputed
/// Always succeeds
pub struct TriggerRecomputePath {}

impl BTNode for TriggerRecomputePath {
    fn tick<'a>(
        &mut self,
        _children: &mut [BehaviorTree],
        blackboard: &mut Blackboard,
        _scene: &CollisionTree,
        _player: &physics::BaseRigidBody,
        _dt: std::time::Duration,
        _other_players: PlayerIterator<'a>,
    ) -> ActionResult {
        blackboard.path_target_location = None;
        ActionResult::Success(None)
    }
}

pub struct AIController {
    pub(super) behavior_tree: BehaviorTree,
    pub(super) blackboard: Blackboard,
    last_action_state: PlayerActionState,
}

impl AIController {
    /// Creates a new AI controller for the player
    pub const fn new(behavior_tree: BehaviorTree) -> Self {
        Self {
            behavior_tree,
            blackboard: Blackboard::new(),
            last_action_state: PlayerActionState::Idle,
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
        self.last_action_state
    }

    fn get_transparency_fac(&mut self) -> f32 {
        0.
    }

    fn transition_action_state(&mut self) {
        // TODO
    }

    fn on_death(&mut self) {
        self.blackboard = Blackboard::new();
    }

    fn on_frame_update<'a>(
        &mut self,
        scene: &CollisionTree,
        player: &physics::BaseRigidBody,
        dt: std::time::Duration,
        other_players: PlayerIterator<'a>,
    ) -> Option<super::ControllerAction> {
        match self.behavior_tree.tick(
            &mut self.blackboard,
            scene,
            player,
            dt,
            other_players,
        ) {
            ActionResult::Running(action) | ActionResult::Success(action) => {
                if action.as_ref().map_or(false, |x| x.fire) {
                    self.last_action_state = PlayerActionState::Fire;
                } else {
                    self.last_action_state = PlayerActionState::Idle;
                }
                action
            }
            ActionResult::Failure => {
                self.last_action_state = PlayerActionState::Idle;
                None
            }
        }
    }
}
