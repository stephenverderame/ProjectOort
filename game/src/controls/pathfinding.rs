use super::ai::{ActionResult, BTNode, BehaviorTree, Blackboard};
use cgmath::*;
use priority_queue::PriorityQueue;
use std::rc::Rc;

/// A linked list in order to backtrack the shortest path
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PathNode {
    index: Point3<i32>,
    parent: Option<Rc<Self>>,
}

/// Priority Newtype to wrap the priority of a node
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Priority(u32);

impl Priority {
    /// Converts a cost (lower has more precedence) to a priority
    /// (higher has more precedence)
    const fn from_cost(p: u32) -> Self {
        Self(u32::MAX - p)
    }
}

/// A Behavior Tree action node that moves the AI to a target
/// Implements A* by tiling the space around the current position in intervals of `tile_dim`
pub struct GoTo {
    tile_dim: f64,
    // the computed path and its target
    computed_path: Option<(Rc<PathNode>, Point3<f64>)>,
}

/// A Behavior Tree action node that moves the AI to a target in a straight line
/// If the target is obstructed, we search for the nearest unobstructed tile to
/// the original target
///
/// This is designed to be used to navigate over small distances
pub struct StraightLineNav {}

const NEIGHBORS: [Vector3<i32>; 6] = [
    vec3(1, 0, 0),
    vec3(-1, 0, 0),
    vec3(0, 1, 0),
    vec3(0, -1, 0),
    vec3(0, 0, 1),
    vec3(0, 0, -1),
];

impl GoTo {
    /// Gets the cost of the tile
    fn tile_cost(
        _cur_pos: &Point3<f64>,
        _tile_dim: f64,
        _tile_idx: Point3<i32>,
    ) -> u32 {
        todo!()
        // TODO: implement this
    }

    /// Gets the heuristic cost of the tile
    fn heuristic(pos: &Point3<i32>, target: &Point3<i32>) -> u32 {
        let diff = target - pos;
        // diff.x.abs() + diff.y.abs() + diff.z.abs()
        diff.magnitude2() as u32
        // magnitude squared for efficiency and to favor a straight line
        // to the target
    }

    /// Gets the tile index of the target
    fn get_tile(
        cur_pos: &Point3<f64>,
        tile_dim: f64,
        target: &Point3<f64>,
    ) -> Point3<i32> {
        let diff = target - cur_pos;
        let tile_diff = diff / tile_dim;
        Point3::new(
            tile_diff.x.round() as i32,
            tile_diff.y.round() as i32,
            tile_diff.z.round() as i32,
        )
    }

    /// Gets a path to the target, avoiding obstacles at a resolution of `tile_dim`
    /// using A*
    fn get_path(
        cur_pos: &Point3<f64>,
        tile_dim: f64,
        target: &Point3<f64>,
    ) -> Option<Rc<PathNode>> {
        let target_tile = Self::get_tile(cur_pos, tile_dim, target);
        let mut cur_node = Rc::new(PathNode {
            index: point3(0, 0, 0),
            parent: None,
        });
        let mut frontier = PriorityQueue::new();
        frontier.push(
            cur_node.clone(),
            Priority::from_cost(Self::heuristic(
                &point3(0, 0, 0),
                &target_tile,
            )),
        );
        while let Some((n, cur_cost)) = frontier.pop() {
            cur_node = n;
            let cur_tile = cur_node.index;
            if cur_tile == target_tile {
                break;
            }
            for neighbor in NEIGHBORS.map(|n| cur_tile + n) {
                let neighbor_cost = cur_cost.0
                    + Self::tile_cost(cur_pos, tile_dim, neighbor)
                    + 1;
                let n = Rc::new(PathNode {
                    index: neighbor,
                    parent: Some(cur_node.clone()),
                });
                frontier.push(n, Priority::from_cost(neighbor_cost));
            }
        }
        if cur_node.index == target_tile {
            Some(cur_node)
        } else {
            None
        }
    }
}

impl BTNode for GoTo {
    fn tick(
        &mut self,
        _children: &mut [BehaviorTree],
        blackboard: &mut Blackboard,
        dt: std::time::Duration,
    ) -> ActionResult {
        // TODO: Review this
        if self.computed_path.is_none() {
            self.computed_path = Self::get_path(
                &blackboard.cur_location.borrow().get_pos(),
                self.tile_dim,
                &blackboard.target_location.borrow().get_pos(),
            )
            .map(|path| (path, blackboard.target_location.borrow().get_pos()));
        }
        ActionResult::Running
        // straight line navigate from point to point?
        // how to deal if we overshoot a few points and end up closer to a later point?
        // TODO
    }
}
