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
use crate::node;
pub use bvh::TreeStopCriteria;
pub use highp_col::*;

static mut LOADED_MESHES: Option<HashMap<String, Rc<RefCell<collision_mesh::CollisionMesh>>>> =
    None;

static mut MESH_MAP_INIT : bool = false;

struct MeshMap {
    loaded_meshes: Option<HashMap<String, Rc<RefCell<collision_mesh::CollisionMesh>>>>,
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

pub struct CollisionObject {
    obj: Rc<RefCell<object::Object>>,
    bvh: Rc<RefCell<collision_mesh::CollisionMesh>>,
}

impl CollisionObject {
    pub fn new(transform: Rc<RefCell<node::Node>>, mesh_path: &str, 
        bvh_stop: bvh::TreeStopCriteria) -> CollisionObject {
        let mut mmap = get_loaded_meshes();
        if let Some(mesh) = mmap.loaded_meshes.as_ref().unwrap().get(mesh_path) {
            let (center, radius) = mesh.borrow().bounding_sphere();
            let o = object::Object::with_mesh(transform, center, radius, &mesh);
            CollisionObject {
                obj: Rc::new(RefCell::new(o)),
                bvh: mesh.clone()
            }
        } else {
            let mesh = Rc::new(RefCell::new(collision_mesh::CollisionMesh::new(mesh_path, bvh_stop)));
            mmap.loaded_meshes.as_mut().unwrap().insert(mesh_path.to_owned(), mesh.clone());
            let (center, radius) = mesh.borrow().bounding_sphere();
            let o = object::Object::with_mesh(transform, center, radius, &mesh);
            CollisionObject {
                obj: Rc::new(RefCell::new(o)),
                bvh: mesh.clone()
            }
        }
    }

    pub fn from(transform: Rc<RefCell<node::Node>>, prototype: &CollisionObject) -> CollisionObject {
        let obj = Rc::new(RefCell::new(object::Object {
            model: transform,
            local_radius: prototype.obj.borrow().local_radius,
            octree_cell: std::rc::Weak::new(),
            mesh: prototype.obj.borrow().mesh.clone(),
        }));
        CollisionObject {
            obj, bvh: prototype.bvh.clone(),
        }
    }

    pub fn is_collision(&self, other: &CollisionObject, highp_strategy: &dyn HighPCollision) -> bool {
        self.bvh.borrow().collision(&self.obj.borrow().model.borrow().mat(), &other.bvh.borrow(),
            &other.obj.borrow().model.borrow().mat(), highp_strategy)
    }

    /// Gets transformation matrices transforming a -1 to 1 cube to each bounding box
    pub fn get_main_and_leaf_cube_transformations(&self) -> (Vec<cgmath::Matrix4<f64>>, Vec<cgmath::Matrix4<f64>>) {
        use cgmath::*;
        let (main, leaf) = self.bvh.borrow().main_and_leaf_boxes();

        (main.into_iter().map(|x| {
            Matrix4::from_translation(x.center.to_vec()) * 
            Matrix4::from_nonuniform_scale(x.extents.x, x.extents.y, x.extents.z)
        }).collect(),
        leaf.into_iter().map(|x| {
            Matrix4::from_translation(x.center.to_vec()) * 
            Matrix4::from_nonuniform_scale(x.extents.x, x.extents.y, x.extents.z)
        }).collect())
    }
}
#[derive(PartialEq, Eq)]
pub enum ObjectType {
    Static, Dynamic
}

pub struct CollisionTree {
    tree: Octree,
    dynamic_objects: Vec<Rc<RefCell<object::Object>>>,
}

impl CollisionTree {
    #[inline]
    pub fn new(center: cgmath::Point3<f64>, half_width: f64) -> CollisionTree {
        CollisionTree {
            tree: Octree::new(center, half_width),
            dynamic_objects: Vec::new(),
        }
    }

    #[inline]
    pub fn insert(&mut self, obj: &CollisionObject, typ: ObjectType) {
        self.tree.insert(obj.obj.clone());
        if typ == ObjectType::Dynamic {
            self.dynamic_objects.push(obj.obj.clone());
        }
    }

    /// Updates all dynamic objects in the tree
    #[inline]
    pub fn update(&self) {
        for obj in &self.dynamic_objects {
            Octree::update(obj)
        }
    }

    #[inline]
    pub fn remove(&mut self, obj: &CollisionObject) {
        self.dynamic_objects.retain(|e| !Rc::ptr_eq(e, &obj.obj));
        self.tree.remove(&obj.obj)
    }

    pub fn get_colliders(&self, obj: &CollisionObject) -> Vec<CollisionObject> {
        self.tree.get_colliders(&obj.obj).into_iter().map(|x| {
            CollisionObject {
                bvh: x.borrow().mesh.upgrade().unwrap(),
                obj: x.clone(),
            }
        }).collect()
    }


}