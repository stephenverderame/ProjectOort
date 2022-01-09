mod octree;
mod object;
mod obb;
mod bvh;
mod collision_mesh;
pub use octree::Octree;
use std::rc::Rc;
use std::cell::RefCell;

pub struct CollisionObject {
    obj: object::Object,
    bvh: Rc<RefCell<collision_mesh::CollisionMesh>>,
}