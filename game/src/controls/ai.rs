use crate::cg_support::node;
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
        dt: std::time::Duration,
    ) -> ActionResult;
}

pub struct Blackboard {
    pub(super) target_location: Rc<RefCell<node::Node>>,
    pub(super) cur_location: Rc<RefCell<node::Node>>,
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
        dt: std::time::Duration,
    ) -> ActionResult {
        self.root.tick(&mut self.children, blackboard, dt)
    }
}

/// A node that succeeds if all of its children succeed, processed left to right
pub struct Sequence {}
impl BTNode for Sequence {
    fn tick(
        &mut self,
        children: &mut [BehaviorTree],
        blackboard: &mut Blackboard,
        dt: std::time::Duration,
    ) -> ActionResult {
        for child in children {
            match child.tick(blackboard, dt) {
                ActionResult::Success => continue,
                ActionResult::Failure => return ActionResult::Failure,
                ActionResult::Running => return ActionResult::Running,
            }
        }
        ActionResult::Success
    }
}

/// A node that succeeds if any of its children succeed, processed left to right
pub struct Fallback {}
impl BTNode for Fallback {
    fn tick(
        &mut self,
        children: &mut [BehaviorTree],
        blackboard: &mut Blackboard,
        dt: std::time::Duration,
    ) -> ActionResult {
        for child in children {
            match child.tick(blackboard, dt) {
                ActionResult::Success => return ActionResult::Success,
                ActionResult::Failure => continue,
                ActionResult::Running => return ActionResult::Running,
            }
        }
        ActionResult::Failure
    }
}
