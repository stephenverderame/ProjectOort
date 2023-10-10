use super::ai::{ActionResult, BTNode, BehaviorTree, Blackboard};
use super::{ControllerAction, PlayerIterator};
use crate::cg_support;
use crate::collisions::CollisionTree;
use crate::physics::{self, BaseRigidBody};
use cgmath::*;
use priority_queue::PriorityQueue;
use std::collections::{HashSet, VecDeque};
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
/// The path is stored as a `VecDeque` of indices, where the first element is the
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
            // println!(
            //     "{:?}",
            //     origin + cur_node.index.to_vec().cast().unwrap() * tile_dim
            // );
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

/// Velocity of updates made by `StraightLineNav`
const SLN_FOLLOW_VELOCITY: f64 = 15.0;

impl StraightLineNav {
    /// Returns true if the next point in the path is not the target
    /// or the target is within 1 unit of the current position
    ///
    /// This prevents us from skipping the last point in the path if we are way
    /// off
    #[inline]
    fn can_remove_next_point(
        path: &ComputedPath,
        next_point: &Point3<f64>,
        cur_pos: &Point3<f64>,
    ) -> bool {
        next_point.abs_diff_ne(&path.target, f64::EPSILON)
            || (cur_pos - path.target).magnitude() < 1.0
    }
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
        let u = cur_pos - last_pos;
        let get_projection_coef = |goal: Point3<f64>| {
            let v = goal - last_pos;
            let mag = v.magnitude2();
            if mag <= f64::EPSILON {
                0.0
            } else {
                v.dot(u) / mag
            }
        };
        while (get_projection_coef(next_point) >= 1.0 - f64::EPSILON
            || (next_point - cur_pos).magnitude() < 0.01)
            && Self::can_remove_next_point(path, &next_point, cur_pos)
        {
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
        // move the point in the direction of the collision normal
        // or in a random direction if no normal is defined
    }

    /// Returns true if our current velocity has changed direction too much from
    /// the last velocity, which would indicate that we've hit an obstacle
    /// and should recalculate the path
    fn did_hit_obstacle(&self, npc: &BaseRigidBody) -> bool {
        self.last_velocity.map_or(false, |v| {
            let hit = v.normalize().dot(npc.velocity.normalize()) < 0.0;
            if hit {
                println!("Hit an obstacle during path following");
            }
            hit
            // less than 0 means > 90 degrees
        })
    }

    fn reset(&mut self) {
        self.last_pos = None;
        self.last_velocity = None;
    }

    fn follow_path(
        &mut self,
        path: &mut ComputedPath,
        npc: &physics::BaseRigidBody,
        _dt: std::time::Duration,
    ) -> ActionResult {
        if self.did_hit_obstacle(npc) {
            self.reset();
            // recalculate path
            return ActionResult::Failure;
        }
        if let Some(point) =
            self.get_next_point_in_path(path, &npc.transform.borrow().get_pos())
        {
            let point = Self::adjust_point_to_avoid_obstacles(point);
            let dir = point - npc.transform.borrow().get_pos();
            let velocity = if dir.is_zero() {
                vec3(0., 0., 0.)
            } else {
                dir.normalize() * SLN_FOLLOW_VELOCITY
            };
            self.last_velocity = Some(velocity);
            self.last_pos = Some(npc.transform.borrow().get_pos());
            // println!("Following path");
            ActionResult::Running(Some(ControllerAction {
                velocity,
                fire: false,
            }))
        } else {
            self.reset();
            println!("No next point");
            ActionResult::Success(Some(ControllerAction {
                velocity: vec3(0., 0., 0.),
                fire: false,
            }))
        }
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

const COLLISION_COST: u32 = 1000;

/// Gets the cost of a tile centered around `tile_center` with a dimension of `tile_dim`
///
/// * `tile_center` - The center of the tile
/// * `tile_dim` - The dimension of the tile
/// * `scene` - The scene to test for collisions
/// * `players` - The players to ignore in the scene
fn tile_cost(
    tile_center: Point3<f64>,
    tile_dim: f64,
    scene: &CollisionTree,
    players: &HashSet<usize>,
) -> u32 {
    // Need to ignore controlled
    use crate::collisions;
    const DIAGONAL_LEN_FAC: f64 = 1.732_050_807_57; //sqrt(3)
    let r = DIAGONAL_LEN_FAC * tile_dim;
    let colliders = scene.test_for_collisions(tile_center, r);
    let obb = collisions::BoundingVolume::Obb(collisions::Obb {
        center: tile_center,
        extents: vec3(tile_dim, tile_dim, tile_dim) / 2.0,
        x: vec3(1.0f64, 0.0, 0.0),
        y: vec3(0.0f64, 1.0, 0.0),
        z: vec3(0.0f64, 0.0, 1.0),
    });
    let id = Matrix4::identity();
    // println!("Players: {:?}", players);
    for c in colliders
        .into_iter()
        .filter(|c| !players.contains(&c.node_id()))
    {
        if c.collision_simple(obb.clone(), &id) {
            // println!("Collision with {} at {:?}", c.geometry_id(), tile_center);
            return COLLISION_COST;
        }
    }
    0
}

/// General setup information for A*
struct AStarInformation {
    tile_dim: f64,
    cur_pos: Point3<f64>,
    target: Point3<i32>,
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
    pub const fn new(tile_dim: f64) -> Self {
        Self { tile_dim }
    }

    /// Wrapper around `tile_cost` that takes a tile index instead of a tile center
    fn tile_cost(
        cur_pos: &Point3<f64>,
        tile_dim: f64,
        index: Point3<i32>,
        scene: &CollisionTree,
        players: &HashSet<usize>,
    ) -> u32 {
        let tile_center =
            cur_pos + index.to_vec().cast::<f64>().unwrap() * tile_dim;
        tile_cost(tile_center, tile_dim, scene, players)
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

    /// Runs the A* algorithm, helper for `get_path`
    ///
    /// See `get_path`
    fn run_a_star(
        frontier: &mut PriorityQueue<Rc<PathNode>, Priority>,
        cur_node: &mut Rc<PathNode>,
        info: &AStarInformation,
        scene: &CollisionTree,
        players: &HashSet<usize>,
    ) {
        let start_time = std::time::Instant::now();
        while let Some((n, _)) = frontier.pop() {
            if std::time::Instant::now() - start_time
                >= std::time::Duration::from_millis(80)
            {
                break;
            }
            *cur_node = n;
            let cur_tile = cur_node.index;
            if cur_tile == info.target {
                break;
            }
            let cur_cost = cur_node.cost;
            for neighbor in NEIGHBORS.map(|n| cur_tile + n) {
                let neighbor_cost = cur_cost
                    .saturating_add(Self::tile_cost(
                        &info.cur_pos,
                        info.tile_dim,
                        neighbor,
                        scene,
                        players,
                    ))
                    .saturating_add(1);
                let n = Rc::new(PathNode {
                    index: neighbor,
                    parent: Some(cur_node.clone()),
                    cost: neighbor_cost,
                });
                let new_priority =
                    Priority::from_cost(neighbor_cost.saturating_add(
                        Self::heuristic(&neighbor, &info.target),
                    ));
                let mut exchange = true;
                if let Some(old_priority) = frontier.get_priority(&n) {
                    if new_priority < *old_priority {
                        // higher priority means lower cost
                        exchange = false;
                    }
                }
                if exchange {
                    frontier.remove(&n);
                    // remove `n` bc I'm not sure if pushing an existing element will cause
                    // the element to be updated since we need to update `parent` and `cost`
                    frontier.push(n, new_priority);
                }
            }
        }
    }

    /// Gets a path to the target, avoiding obstacles at a resolution of `tile_dim`
    /// using A*
    ///
    /// Returns None if no path could be found
    ///
    /// # Arguments
    /// * `cur_pos` - The current position of the entity
    /// * `tile_dim` - The dimension of the tiles used for A*
    /// * `target` - The target position
    /// * `scene` - The collision tree of the scene
    /// * `players` - The set of player node pointers to identify objects to ignore
    /// in the collision test
    fn get_path(
        cur_pos: &Point3<f64>,
        tile_dim: f64,
        target: &Point3<f64>,
        scene: &CollisionTree,
        players: &HashSet<usize>,
    ) -> Option<Rc<PathNode>> {
        let target_tile = Self::get_tile(cur_pos, tile_dim, target);
        let mut cur_node = Rc::new(PathNode {
            index: point3(0, 0, 0),
            parent: None,
            cost: 0,
        });
        // println!("Searching for path");
        if Self::tile_cost(cur_pos, tile_dim, target_tile, scene, players) > 0 {
            // println!("Target obstructed");
            return None;
        }
        if Self::tile_cost(cur_pos, tile_dim, point3(0, 0, 0), scene, players)
            > 0
        {
            // println!("Self obstructed");
            return None;
        }
        let mut frontier = PriorityQueue::new();
        frontier.push(cur_node.clone(), Priority::from_cost(0));
        let info = AStarInformation {
            cur_pos: *cur_pos,
            tile_dim,
            target: target_tile,
        };
        Self::run_a_star(&mut frontier, &mut cur_node, &info, scene, players);
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
        other_players: PlayerIterator,
    ) -> ActionResult {
        // println!("Ticking ComputePath");
        if let Some(target_location) = &blackboard.target_location {
            let mut player_map: HashSet<usize> =
                other_players.copy().map(|p| p.as_ptr() as usize).collect();
            player_map.insert(player.transform.as_ptr() as usize);
            let cur_pos = player.transform.borrow().get_pos();
            blackboard.computed_path = Self::get_path(
                &cur_pos,
                self.tile_dim,
                target_location,
                scene,
                &player_map,
            )
            .map(|path| {
                // println!("Got path to {:?}", target_location);
                ComputedPath::new(
                    path,
                    *target_location,
                    cur_pos,
                    self.tile_dim,
                )
            });
            blackboard.path_target_location = blackboard.target_location;
            blackboard
                .computed_path
                .as_ref()
                .map_or(ActionResult::Failure, |_| ActionResult::Success(None))
        } else {
            println!("No target location to compute path to");
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
        ActionResult::Success(None)
    }
}

#[allow(clippy::doc_markdown)]
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
        player: &physics::BaseRigidBody,
        _dt: std::time::Duration,
        _other_players: PlayerIterator,
    ) -> ActionResult {
        // TODO:
        if let (Some(id), _) =
            (blackboard.target_id, blackboard.target_location)
        {
            for obj in scene.get_all_objects() {
                if obj.node_id() == id {
                    // println!(
                    //     "Found target {} at {:?}",
                    //     id,
                    //     obj.get_transformation().borrow().get_pos()
                    // );
                    let dir = obj.get_transformation().borrow().get_pos()
                        - player.transform.borrow().get_pos();
                    blackboard.target_location =
                        Some(obj.get_transformation().borrow().get_pos());
                    blackboard.rot =
                        cg_support::look_at(dir.normalize(), &vec3(0., 1., 0.));
                    return ActionResult::Success(None);
                }
            }
            blackboard.target_id = None;
            ActionResult::Failure
        } else {
            ActionResult::Failure
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::collisions::*;
    use crate::node;
    use assertables::*;
    use rand::Rng;
    use serial_test::serial;
    use std::cell::RefCell;

    #[test]
    fn sln_get_next_point() {
        let mut sln = StraightLineNav {
            last_pos: None,
            last_velocity: None,
        };
        let mut path1 = VecDeque::new();
        path1.push_back(point3(1, 0, 0));
        path1.push_back(point3(2, 0, 0));
        path1.push_back(point3(2, 2, 0));
        path1.push_back(point3(2, 2, 2));
        path1.push_back(point3(2, 2, 3));
        let p_len = path1.len();
        let mut path1 = ComputedPath {
            path: path1,
            target: point3(2., 2., 3.),
            origin: point3(0.0, 0.0, 0.0),
            tile_dim: 1.0,
        };
        let mut cur_pos = point3(0.0, 0.0, 0.0);
        assert_relative_eq!(
            sln.get_next_point_in_path(&mut path1, &cur_pos).unwrap(),
            point3(1f64, 0., 0.)
        );

        sln.last_pos = Some(point3(0f64, 0., 0.));
        cur_pos = point3(0.5, 0.0, 0.0);
        assert_relative_eq!(
            sln.get_next_point_in_path(&mut path1, &cur_pos).unwrap(),
            point3(1f64, 0., 0.)
        );

        sln.last_pos = Some(point3(0.5f64, 0., 0.));
        cur_pos = point3(1.2, 0.0, 0.0);
        assert_relative_eq!(
            sln.get_next_point_in_path(&mut path1, &cur_pos).unwrap(),
            point3(2f64, 0., 0.)
        );
        assert_eq!(path1.path.len(), p_len - 1);

        sln.last_pos = Some(point3(1.2f64, 0., 0.));
        cur_pos = point3(2.2, -1.0, 0.0);
        assert_relative_eq!(
            sln.get_next_point_in_path(&mut path1, &cur_pos).unwrap(),
            point3(2f64, 2., 0.)
        );
        assert_eq!(path1.path.len(), p_len - 2);

        sln.last_pos = Some(point3(2.2f64, -1., 0.));
        cur_pos = point3(2.2, 2.4, 2.2);
        assert_relative_eq!(
            sln.get_next_point_in_path(&mut path1, &cur_pos).unwrap(),
            point3(2f64, 2., 3.)
        );
    }

    /// Simulates straight line navigation of following `path` and asserts
    /// if we can't get to the target
    fn test_follows_path(
        mut sln: StraightLineNav,
        mut path: ComputedPath,
        start_pos: Point3<f64>,
    ) {
        let body = physics::BaseRigidBody::new(Rc::new(RefCell::new(
            node::Node::default().pos(start_pos),
        )));
        loop {
            match sln.follow_path(
                &mut path,
                &body,
                std::time::Duration::from_secs(1),
            ) {
                ActionResult::Failure => unreachable!("Shouldn't fail"),
                ActionResult::Success(maybe_action) => {
                    assert_lt!(
                        body.transform
                            .borrow()
                            .local_pos()
                            .distance(path.target),
                        1.0
                    );
                    if let Some(action) = maybe_action {
                        body.transform
                            .borrow_mut()
                            .translate(action.velocity / SLN_FOLLOW_VELOCITY);
                    }
                    break;
                }
                ActionResult::Running(Some(action)) => {
                    body.transform
                        .borrow_mut()
                        .translate(action.velocity / SLN_FOLLOW_VELOCITY);
                    // normalize to always have velocity of unit 1

                    assert!(path.path.iter().any(|p| {
                        (path.index_to_point(p)
                            - body.transform.borrow().local_pos())
                        .magnitude()
                            < 2.0
                    }));
                }
                ActionResult::Running(None) => {
                    unreachable!("Shouldn't be none")
                }
            }
        }
    }

    #[test]
    fn sln_follow_path() {
        let sln = StraightLineNav {
            last_pos: None,
            last_velocity: None,
        };
        let mut path1 = VecDeque::new();
        path1.push_back(point3(1, 0, 0));
        path1.push_back(point3(2, 0, 0));
        path1.push_back(point3(2, -1, 0));
        path1.push_back(point3(2, -3, 0));
        path1.push_back(point3(2, -5, 0));
        path1.push_back(point3(3, -5, 0));
        path1.push_back(point3(5, -5, 0));
        path1.push_back(point3(5, -3, 0));
        path1.push_back(point3(5, -1, 0));
        path1.push_back(point3(5, 0, 0));
        path1.push_back(point3(7, 0, 0));
        path1.push_back(point3(7, 0, 2));
        path1.push_back(point3(7, 0, 3));
        path1.push_back(point3(7, 0, 5));
        path1.push_back(point3(8, 0, 5));
        path1.push_back(point3(10, 0, 5));
        path1.push_back(point3(12, 0, 5));
        path1.push_back(point3(12, 0, 3));
        path1.push_back(point3(12, 0, 1));
        path1.push_back(point3(12, 0, 0));
        let path1 = ComputedPath {
            path: path1,
            target: point3(12., 0., 0.),
            origin: point3(0.0, 0.0, 0.0),
            tile_dim: 1.0,
        };
        test_follows_path(sln, path1, point3(0.0, 0.0, 0.0));
    }

    #[test]
    fn sln_random_follow_path() {
        use rand::Rng;
        for _ in 0..10 {
            let sln = StraightLineNav {
                last_pos: None,
                last_velocity: None,
            };
            let mut rng = rand::thread_rng();
            let mut path = VecDeque::new();
            let mut last_pt = point3(0, 0, 0);
            let len = rng.gen_range(10..100);
            for _ in 0..len {
                path.push_back(
                    last_pt
                        + vec3(
                            rng.gen_range(-1i32..1),
                            rng.gen_range(-1i32..1),
                            rng.gen_range(-1i32..1),
                        ),
                );
                last_pt = *path.back().unwrap();
            }
            let path1 = ComputedPath {
                target: path.back().unwrap().cast().unwrap(),
                path,
                origin: point3(0.0f64, 0.0, 0.0),
                tile_dim: 1.0,
            };
            test_follows_path(sln, path1, point3(0.0, 0.0, 0.0));
        }
    }

    #[serial]
    #[test]
    fn cp_simple_obstacle() {
        let mut tree = CollisionTree::new(point3(0., 0., 0.), 10.0);
        let obstacle = CollisionObject::new(
            Rc::new(RefCell::new(node::Node::default())),
            "assets/default_cube.obj",
            TreeStopCriteria::default(),
        );
        assert_abs_diff_eq!(obstacle.aabb_volume(), 8.0);
        tree.insert(&obstacle);
        let tile_dim = 1.0;
        let start = point3(-5f64, 0., 0.);
        let target = point3(5f64, 0., 0.);
        let path = ComputePath::get_path(
            &start,
            tile_dim,
            &target,
            &tree,
            &HashSet::new(),
        )
        .unwrap();
        let path = ComputedPath::new(path, target, start, tile_dim);
        for p in path.path {
            let p = start + p.to_vec().cast().unwrap() * tile_dim;
            assert!(
                !(p.x.abs() <= 1. && p.y.abs() <= 1. && p.z.abs() <= 1.),
                "Path goes through obstacle at tile {:?}",
                p
            );
        }
    }

    /// asserts that a tile does not collide with any of the objects
    fn assert_tile_not_collides(
        tile_center: &Point3<f64>,
        tile_dim: f64,
        objects: &[CollisionObject],
    ) {
        let tile_vol = Obb {
            center: *tile_center,
            extents: vec3(tile_dim, tile_dim, tile_dim) / 2.0,
            x: vec3(1., 0., 0.),
            y: vec3(0., 1., 0.),
            z: vec3(0., 0., 1.),
        };
        for obj in objects {
            assert!(
                !obj.collision_simple(
                    BoundingVolume::Obb(tile_vol.clone()),
                    &Matrix4::identity()
                ),
                "Tile {:?} collides with object \n\n{:?}\n\n",
                tile_center,
                *obj.get_transformation().borrow(),
            );
        }
    }

    #[serial]
    #[test]
    fn cp_mine_field() {
        let mut tree = CollisionTree::new(point3(0., 0., 0.), 15.0);
        let mut obstacles = Vec::new();
        let mut rand = rand::thread_rng();
        let obstacle_count = rand.gen_range(25..100);
        for _ in 0..obstacle_count {
            let mut node = node::Node::default();
            node.translate(vec3(
                rand.gen_range(-8f64..8.),
                rand.gen_range(-8f64..8.),
                rand.gen_range(-8f64..8.),
            ));
            node.rotate_local(Quaternion::from_axis_angle(
                vec3(
                    rand.gen_range(-1f64..1.),
                    rand.gen_range(-1f64..1.),
                    rand.gen_range(-1f64..1f64),
                ),
                Rad(rand.gen_range(-std::f64::consts::PI..std::f64::consts::PI)),
            ));
            node.set_scale(vec3(
                rand.gen_range(0.5f64..1.5),
                rand.gen_range(0.5f64..1.5),
                rand.gen_range(0.5f64..1.5),
            ));
            let obstacle = CollisionObject::new(
                Rc::new(RefCell::new(node)),
                "assets/default_cube.obj",
                TreeStopCriteria::default(),
            );
            assert_abs_diff_eq!(obstacle.aabb_volume(), 8.0);
            tree.insert(&obstacle);
            obstacles.push(obstacle);
        }
        let tile_dim = 1.0;
        let start = point3(-11f64, -11., -11.);
        let target = point3(11f64, 11., 11.);
        let path = ComputePath::get_path(
            &start,
            tile_dim,
            &target,
            &tree,
            &HashSet::new(),
        )
        .unwrap();
        println!("Path cost: {}", path.cost);
        let path = ComputedPath::new(path, target, start, tile_dim);
        for p in path.path {
            let p = start + p.to_vec().cast().unwrap() * tile_dim;
            assert_tile_not_collides(&p, tile_dim, &obstacles);
        }
    }

    #[test]
    fn cp_mine_field_repeated() {
        for _ in 0..30 {
            cp_mine_field();
        }
    }
}
