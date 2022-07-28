mod octree;
mod object;
mod obb;
mod bvh;
mod collision_mesh;
mod highp_col;
use octree::Octree;
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use crate::cg_support::node;
pub use bvh::TreeStopCriteria;
pub use highp_col::*;

static mut LOADED_MESHES: Option<HashMap<(String, TreeStopCriteria), Rc<RefCell<collision_mesh::CollisionMesh>>>> =
    None;

static mut MESH_MAP_INIT : bool = false;

struct MeshMap {
    loaded_meshes: Option<HashMap<(String, TreeStopCriteria), Rc<RefCell<collision_mesh::CollisionMesh>>>>,
}

fn get_loaded_meshes() -> MeshMap {

    let meshes = if unsafe { MESH_MAP_INIT } {
        unsafe { std::mem::replace(&mut LOADED_MESHES, None) }
    } else {
        unsafe { MESH_MAP_INIT = true };
        Some(HashMap::new())
    };
    MeshMap {
        loaded_meshes: Some(meshes.expect("Singleton already borrowed")),
    }
}

impl Drop for MeshMap {
    fn drop(&mut self) {
        unsafe { LOADED_MESHES = std::mem::replace(&mut self.loaded_meshes, None); }
    }
}

#[derive(Clone)]
pub struct CollisionObject {
    obj: Rc<RefCell<object::Object>>, //shared with octree, which holds weak pointers
    mesh: Rc<RefCell<collision_mesh::CollisionMesh>>, //shared between all objects with the same geometry
}

impl CollisionObject {
    /// Creates a new collision mesh for the collision mesh at `mesh_path` with the specified arguments if one does not exist
    /// or loads the cached one
    /// 
    /// `transform` - the transform specific for this particular collision object
    pub fn new(transform: Rc<RefCell<node::Node>>, mesh_path: &str, 
        bvh_stop: bvh::TreeStopCriteria) -> CollisionObject {
        let mut mmap = get_loaded_meshes();
        if let Some(mesh) = mmap.loaded_meshes.as_ref().unwrap().get(&(mesh_path.to_string(), bvh_stop)) {
            let (center, radius) = mesh.borrow().bounding_sphere();
            let obj = Rc::new(RefCell::new(object::Object::with_mesh(transform, center, radius, &mesh)));
            CollisionObject {
                obj,
                mesh: mesh.clone()
            }
        } else {
            let mesh = Rc::new(RefCell::new(collision_mesh::CollisionMesh::new(mesh_path, bvh_stop)));
            mmap.loaded_meshes.as_mut().unwrap().insert((mesh_path.to_owned(), bvh_stop), mesh.clone());
            let (center, radius) = mesh.borrow().bounding_sphere();
            let obj = Rc::new(RefCell::new(object::Object::with_mesh(transform, center, radius, &mesh)));
            CollisionObject {
                obj,
                mesh: mesh.clone()
            }
        }
    }

    /// Creates a new collision object that's meant to serve as a prototype to make
    /// new collision objects from
    pub fn prototype(mesh_path: &str, bvh_stop: TreeStopCriteria) -> Self {
        Self::new(Rc::new(RefCell::new(node::Node::default())), mesh_path, bvh_stop)
    }

    /// Creates a new collision object by copying an existing one
    #[allow(dead_code)]
    pub fn from(transform: Rc<RefCell<node::Node>>, prototype: &CollisionObject) -> CollisionObject {
        let obj = Rc::new(RefCell::new(object::Object {
            model: transform,
            local_radius: prototype.obj.borrow().local_radius,
            octree_cell: std::rc::Weak::new(),
            mesh: prototype.obj.borrow().mesh.clone(),
        }));
        CollisionObject {
            obj, mesh: prototype.mesh.clone(),
        }
    }

    /// Gets the hit point and normal for each collider
    /// The receiver's hit point and normal (`self`) is stored in `pt_norm_a` in the `HitData` if
    /// any data is returned
    pub fn collision(&self, other: &CollisionObject, highp_strategy: &dyn HighPCollision) -> Option<Hit> {
        self.mesh.borrow().collision(&self.obj.borrow().model.borrow().mat(), &other.mesh.borrow(),
            &other.obj.borrow().model.borrow().mat(), highp_strategy)
    }

    /// Gets transformation matrices transforming a -1 to 1 cube to each bounding box
    #[allow(dead_code)]
    pub fn get_main_and_leaf_cube_transformations(&self) -> (Vec<cgmath::Matrix4<f64>>, Vec<cgmath::Matrix4<f64>>) {
        use cgmath::*;
        let (main, leaf) = self.mesh.borrow().main_and_leaf_boxes();

        (main.into_iter().map(|x| {
            Matrix4::from_translation(x.center.to_vec()) * 
            Matrix4::from_nonuniform_scale(x.extents.x, x.extents.y, x.extents.z)
        }).collect(),
        leaf.into_iter().map(|x| {
            Matrix4::from_translation(x.center.to_vec()) * 
            Matrix4::from_nonuniform_scale(x.extents.x, x.extents.y, x.extents.z)
        }).collect())
    }

    /// For testing purposes, gets the colliding leaf bounding volumes as transformation matrices to
    /// a -1 to 1 cube
    #[allow(dead_code)]
    pub fn get_colliding_volume_transformations(&self, other: &CollisionObject) 
        -> Vec<cgmath::Matrix4<f64>>
    {
        use cgmath::*;
        let our_mat = self.obj.borrow().model.borrow().mat();
        let other_mat = other.obj.borrow().model.borrow().mat();
        let (our, other) = self.mesh.borrow().get_colliding_volumes(&our_mat,
            &other.mesh.borrow(), &other_mat);
        our.into_iter().map(|x| {
            our_mat * Matrix4::from_translation(x.center.to_vec()) * 
            Matrix4::from_nonuniform_scale(x.extents.x, x.extents.y, x.extents.z)
        }).chain(other.into_iter().map(|x| {
            other_mat * Matrix4::from_translation(x.center.to_vec()) * 
            Matrix4::from_nonuniform_scale(x.extents.x, x.extents.y, x.extents.z)
        })).collect()
    }

    #[allow(dead_code)]
    #[inline(always)]
    pub fn get_transformation(&self) -> Rc<RefCell<node::Node>> {
        self.obj.borrow().model.clone()
    }

    #[inline(always)]
    pub fn update_in_collision_tree(&self) {
        Octree::update(&self.obj)
    }

    #[inline(always)]
    pub fn is_in_collision_tree(&self) -> bool {
        self.obj.borrow().octree_cell.upgrade().is_some()
    }

    /// Gets the center and radius of a bounding sphere for this mesh in world space
    #[inline(always)]
    pub fn bounding_sphere(&self) -> (cgmath::Point3<f64>, f64) {
        use cgmath::*;
        let transform = self.obj.borrow().model.clone();
        let transform = transform.borrow();
        let (pt, radius) = self.mesh.borrow().bounding_sphere();
        let scale = transform.local_scale();
        let max_scale = scale.x.abs().max(scale.y.abs().max(scale.z.abs()));
        (transform.mat().transform_point(pt), radius * max_scale)
    }

    /// Gets the estimated volume of this collision object
    #[inline(always)]
    pub fn aabb_volume(&self) -> f64 {
        self.mesh.borrow().aabb_volume()
    }

    /// Calls `func` on all vertices of this mesh
    #[inline(always)]
    pub fn forall_verts<F : FnMut(&bvh::CollisionVertex<f32>)>(&self, mut func: F) {
        self.mesh.borrow().forall_verts(&mut func)
    }

    /// Gets an id that uniquely identifies this collision objects's shared geometry
    pub fn geometry_id(&self) -> usize {
        assert_eq_size!(usize, *mut collision_mesh::CollisionMesh);
        self.mesh.as_ptr() as usize
    }
}

impl std::hash::Hash for CollisionObject {
    fn hash<H : std::hash::Hasher>(&self, state: &mut H) {
        self.obj.as_ptr().hash(state);
    }
}

impl PartialEq for CollisionObject {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.obj, &other.obj)
    }
}

impl Eq for CollisionObject {}

impl PartialEq<Rc<RefCell<object::Object>>> for CollisionObject {
    fn eq(&self, other: &Rc<RefCell<object::Object>>) -> bool {
        Rc::ptr_eq(&self.obj, other)
    }
}

pub struct CollisionTree {
    tree: Octree,
}

impl CollisionTree {
    #[inline]
    pub fn new(center: cgmath::Point3<f64>, half_width: f64) -> CollisionTree {
        CollisionTree {
            tree: Octree::new(center, half_width),
        }
    }

    #[inline]
    pub fn insert(&mut self, obj: &CollisionObject) {
        self.tree.insert(obj.obj.clone());
    }

    #[inline]
    #[allow(dead_code)]
    pub fn remove(&mut self, obj: &CollisionObject) {
        self.tree.remove(&obj.obj)
    }

    pub fn get_colliders(&self, obj: &CollisionObject) -> Vec<CollisionObject> {
        self.tree.get_colliders(&obj.obj).into_iter().map(|x| {
            CollisionObject {
                mesh: x.borrow().mesh.upgrade().unwrap(),
                obj: x.clone(),
            }
        }).collect()
    }


}