use super::pathfinding::ComputedPath;
use crate::collisions::CollisionTree;
use crate::player;
use std::cell::RefCell;
use std::rc::Rc;
pub enum ActionResult {
    Success,
    Failure,
    Running,
}

pub trait BTNode {
    fn tick(
        &mut self,
        children: &mut [BehaviorTree],
        blackboard: &mut Blackboard,
        scene: &CollisionTree,
        dt: std::time::Duration,
    ) -> ActionResult;
}

pub struct Blackboard {
    pub(super) target_location: Option<cgmath::Point3<f64>>,
    pub(super) npc: Rc<RefCell<player::Player>>,
    pub(super) computed_path: Option<ComputedPath>,
    pub(super) target_id: Option<usize>,
}

pub struct BehaviorTree {
    root: Box<dyn BTNode>,
    children: Vec<BehaviorTree>,
}

impl BehaviorTree {
    pub fn new(root: Box<dyn BTNode>, children: Vec<Self>) -> Self {
        Self { root, children }
    }

    pub fn tick(
        &mut self,
        blackboard: &mut Blackboard,
        scene: &CollisionTree,
        dt: std::time::Duration,
    ) -> ActionResult {
        self.root.tick(&mut self.children, blackboard, scene, dt)
    }
}

/// A node that succeeds if all of its children succeed, processed left to right
/// If any child fails or is running, the sequence returns the status of
/// the first non-successful child
pub struct Sequence {}
impl BTNode for Sequence {
    fn tick(
        &mut self,
        children: &mut [BehaviorTree],
        blackboard: &mut Blackboard,
        scene: &CollisionTree,
        dt: std::time::Duration,
    ) -> ActionResult {
        for child in children {
            match child.tick(blackboard, scene, dt) {
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
    fn tick(
        &mut self,
        children: &mut [BehaviorTree],
        blackboard: &mut Blackboard,
        scene: &CollisionTree,
        dt: std::time::Duration,
    ) -> ActionResult {
        for child in children {
            match child.tick(blackboard, scene, dt) {
                ActionResult::Failure => continue,
                x => return x,
            }
        }
        ActionResult::Failure
    }
}
