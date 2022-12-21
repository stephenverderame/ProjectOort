use super::ai::{ActionResult, BTNode, BehaviorTree, Blackboard};
use super::{ControllerAction, PlayerIterator};
use crate::collisions::CollisionTree;
use crate::physics::{self, BaseRigidBody};
use cgmath::*;
use priority_queue::PriorityQueue;
use std::collections::VecDeque;
use std::rc::Rc;

/// A linked list in order to backtrack the shortest path
#[derive(Debug, Clone, Eq)]
struct PathNode {
    index: Point3<i32>,
    parent: Option<Rc<Self>>,
    cost: u32,
}

impl PartialEq for PathNode {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl std::hash::Hash for PathNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
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

/// A computed path to a target with stores indices of tiles from the origin
/// to the target in units of `tile_dim`
///
/// The path is stored as a VecDeque of indices, where the first element is the
/// next tile that we haven't reached yet
pub(super) struct ComputedPath {
    path: VecDeque<Point3<i32>>,
    target: Point3<f64>,
    origin: Point3<f64>,
    tile_dim: f64,
}

impl ComputedPath {
    /// Constructs a computed path by backtracking from the terminal node
    ///
    /// The terminal node is the last node in the path
    fn new(
        terminal_node: Rc<PathNode>,
        target: Point3<f64>,
        origin: Point3<f64>,
        tile_dim: f64,
    ) -> Self {
        let mut path = VecDeque::new();
        let mut cur_node = terminal_node;
        while let Some(parent) = cur_node.parent.clone() {
            path.push_front(cur_node.index);
            println!(
                "{:?}",
                origin + cur_node.index.to_vec().cast().unwrap() * tile_dim
            );
            cur_node = parent;
        }
        Self {
            path,
            target,
            origin,
            tile_dim,
        }
    }

    /// Converts an index, which is relative to an origin and a tile dimension,
    /// to a point in world space
    #[inline]
    fn index_to_point(&self, index: &Point3<i32>) -> Point3<f64> {
        self.origin + index.to_vec().cast().unwrap() * self.tile_dim
    }
}

/// A Behavior Tree action node that moves the AI along a path in relatively straight lines
///
/// This is designed to be used to navigate on a precomputed path
/// relatively small spacing between points in the path
#[derive(Default)]
pub struct StraightLineNav {
    last_pos: Option<Point3<f64>>,
    last_velocity: Option<Vector3<f64>>,
}

impl StraightLineNav {
    /// Get's the next point we haven't passed yet in the path
    /// and mutates the path to remove the points we've already passed
    ///
    /// Returns None if the path is empty or we've passed the last point
    fn get_next_point_in_path(
        &self,
        path: &mut ComputedPath,
        cur_pos: &Point3<f64>,
    ) -> Option<Point3<f64>> {
        let mut next_point = path.index_to_point(path.path.front()?);
        let last_pos = self.last_pos.unwrap_or(*cur_pos);
        let v = cur_pos - last_pos;
        let get_projection_coef = |pt: Point3<f64>| {
            let v2 = pt - cur_pos;
            v2.dot(v) / (v.magnitude2() + f64::EPSILON)
        };
        while get_projection_coef(next_point) > 1.0 {
            path.path.pop_front();
            next_point = path.index_to_point(path.path.front()?);
        }
        Some(next_point)
    }

    /// Slightly adjusts the target location as little as possible to avoid
    /// any obstacles if the target location has become obstructed
    const fn adjust_point_to_avoid_obstacles(
        point: Point3<f64>,
    ) -> Point3<f64> {
        point
        // TODO: implement
    }

    /// Returns true if our current velocity has changed direction too much from
    /// the last velocity, which would indicate that we've hit an obstacle
    /// and should recalculate the path
    fn did_hit_obstacle(&self, npc: &BaseRigidBody) -> bool {
        self.last_velocity.map_or(false, |v| {
            v.normalize().dot(npc.velocity.normalize()) < 0.0
            // less than 0 means > 90 degrees
        })
    }

    fn follow_path(
        &mut self,
        path: &mut ComputedPath,
        npc: &physics::BaseRigidBody,
        _dt: std::time::Duration,
    ) -> ActionResult {
        if self.did_hit_obstacle(npc) {
            // recalculate path
            return ActionResult::Failure;
        }
        self.get_next_point_in_path(path, &npc.transform.borrow().get_pos())
            .map_or(ActionResult::Success, |point| {
                let point = Self::adjust_point_to_avoid_obstacles(point);
                let dir = point - npc.transform.borrow().get_pos();
                // let rot_axis = dir.cross(vec3(0.0, 1.0, 0.0));
                // TODO: rotate the npc to face the direction of the next point
                let velocity = dir.normalize() * 10.0;
                ActionResult::Running(Some(ControllerAction { velocity }))
            })
    }
}

impl BTNode for StraightLineNav {
    fn tick(
        &mut self,
        _children: &mut [BehaviorTree],
        blackboard: &mut Blackboard,
        _scene: &CollisionTree,
        player: &physics::BaseRigidBody,
        dt: std::time::Duration,
        _other_players: PlayerIterator,
    ) -> ActionResult {
        blackboard
            .computed_path
            .as_mut()
            .map_or(ActionResult::Failure, |path| {
                self.follow_path(path, player, dt)
            })
    }
}

const NEIGHBORS: [Vector3<i32>; 6] = [
    vec3(1, 0, 0),
    vec3(-1, 0, 0),
    vec3(0, 1, 0),
    vec3(0, -1, 0),
    vec3(0, 0, 1),
    vec3(0, 0, -1),
];

/// Gets the cost of a tile centered around `tile_center` with a dimension of `tile_dim`
fn tile_cost(
    tile_center: Point3<f64>,
    tile_dim: f64,
    scene: &CollisionTree,
) -> u32 {
    use crate::collisions;
    let r = f64::sqrt((tile_dim / 2.0) * (tile_dim / 2.0));
    let colliders = scene.test_for_collisions(tile_center, r);
    let obb = collisions::BoundingVolume::Obb(collisions::Obb {
        center: tile_center,
        extents: vec3(tile_dim, tile_dim, tile_dim),
        x: vec3(1.0f64, 0.0, 0.0),
        y: vec3(0.0f64, 1.0, 0.0),
        z: vec3(0.0f64, 0.0, 1.0),
    });
    let id = Matrix4::identity();
    for c in colliders {
        if c.collision_simple(obb.clone(), &id) {
            return u32::MAX;
        }
    }
    0
}

/// A Behavior Tree action node that computes a path to a target using A*
/// Implements A* by tiling the space around the current position in intervals of `tile_dim`
///
/// The action will succeed if a path was computed or fail if there is no target or no path
pub struct ComputePath {
    tile_dim: f64,
}

impl ComputePath {
    /// Creates a new `ComputePath` node with the given tile dimension
    /// The tile dimension is the length, width, and height of the tiles
    /// that the space is divided into for the A* algorithm
    pub fn new(tile_dim: f64) -> Self {
        Self { tile_dim }
    }

    fn tile_cost(
        cur_pos: &Point3<f64>,
        tile_dim: f64,
        index: Point3<i32>,
        scene: &CollisionTree,
    ) -> u32 {
        let tile_center = cur_pos
            + vec3(
                f64::from(index.x) * tile_dim,
                f64::from(index.y) * tile_dim,
                f64::from(index.z) * tile_dim,
            );
        tile_cost(tile_center, tile_dim, scene)
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
    ///
    /// Returns None if no path could be found
    fn get_path(
        cur_pos: &Point3<f64>,
        tile_dim: f64,
        target: &Point3<f64>,
        scene: &CollisionTree,
    ) -> Option<Rc<PathNode>> {
        let target_tile = Self::get_tile(cur_pos, tile_dim, target);
        let mut cur_node = Rc::new(PathNode {
            index: point3(0, 0, 0),
            parent: None,
            cost: 0,
        });
        let mut frontier = PriorityQueue::new();
        frontier.push(cur_node.clone(), Priority::from_cost(0));
        while let Some((n, _)) = frontier.pop() {
            cur_node = n;
            let cur_tile = cur_node.index;
            if cur_tile == target_tile {
                break;
            }
            let cur_cost = cur_node.cost;
            for neighbor in NEIGHBORS.map(|n| cur_tile + n) {
                let neighbor_cost = cur_cost
                    .saturating_add(Self::tile_cost(
                        cur_pos, tile_dim, neighbor, scene,
                    ))
                    .saturating_add(1);
                let n = Rc::new(PathNode {
                    index: neighbor,
                    parent: Some(cur_node.clone()),
                    cost: neighbor_cost,
                });
                frontier.push(
                    n,
                    Priority::from_cost(neighbor_cost.saturating_add(
                        Self::heuristic(&neighbor, &target_tile),
                    )),
                );
            }
        }
        if cur_node.index == target_tile && cur_node.cost < u32::MAX {
            Some(cur_node)
        } else {
            None
        }
    }
}

impl BTNode for ComputePath {
    fn tick(
        &mut self,
        _children: &mut [BehaviorTree],
        blackboard: &mut Blackboard,
        scene: &CollisionTree,
        player: &physics::BaseRigidBody,
        _dt: std::time::Duration,
        _other_players: PlayerIterator,
    ) -> ActionResult {
        if let Some(target_location) = &blackboard.target_location {
            let cur_pos = player.transform.borrow().get_pos();
            blackboard.computed_path =
                Self::get_path(&cur_pos, self.tile_dim, target_location, scene)
                    .map(|path| {
                        ComputedPath::new(
                            path,
                            *target_location,
                            cur_pos,
                            self.tile_dim,
                        )
                    });
            blackboard
                .computed_path
                .as_ref()
                .map_or(ActionResult::Failure, |_| ActionResult::Success)
        } else {
            ActionResult::Failure
        }
    }
}

/// A Behavior Tree action node that identifies a target to follow
/// This will succeed if there is a target already identified or if a new target is identified
/// otherwise it will be running
pub struct IdentifyTarget {
    //pub(super) fov: cgmath::Rad<f64>,
    //pub(super) view_distance: f64,
}

impl BTNode for IdentifyTarget {
    fn tick(
        &mut self,
        _children: &mut [BehaviorTree],
        blackboard: &mut Blackboard,
        _scene: &CollisionTree,
        _player: &physics::BaseRigidBody,
        _dt: std::time::Duration,
        other_players: PlayerIterator,
    ) -> ActionResult {
        // TODO
        if blackboard.target_id.is_none() {
            if let Some(player) = other_players.copy().next() {
                blackboard.target_id = Some(player.as_ptr() as usize);
            }
            println!("Identified target");
        }
        ActionResult::Success
    }
}

/// A Behavior Tree action node that gets the position of an IDed target
///
/// If the target is too far away or out of sight too long, this will mark the target
/// as lost so a new one can be identified
///
/// This will succeed if the target is in sight and close enough, otherwise it
/// will fail and the target will be marked as lost
pub struct SearchForIDedTarget {
    //fov: cgmath::Rad<f64>,
    //view_distance: f64,
    // max_lost_time: std::time::Duration,
}

impl BTNode for SearchForIDedTarget {
    fn tick(
        &mut self,
        _children: &mut [BehaviorTree],
        blackboard: &mut Blackboard,
        scene: &CollisionTree,
        _player: &physics::BaseRigidBody,
        _dt: std::time::Duration,
        _other_players: PlayerIterator,
    ) -> ActionResult {
        // TODO:
        if let (Some(id), None) =
            (blackboard.target_id, blackboard.target_location)
        {
            for obj in scene.get_all_objects() {
                if obj.node_id() == id {
                    println!(
                        "Found target {} at {:?}",
                        id,
                        obj.get_transformation().borrow().get_pos()
                    );
                    blackboard.target_location =
                        Some(obj.get_transformation().borrow().get_pos());
                    return ActionResult::Success;
                }
            }
            blackboard.target_id = None;
            ActionResult::Failure
        } else {
            ActionResult::Failure
        }
    }
}
