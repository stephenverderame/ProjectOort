mod bvh;
mod collision_mesh;
mod highp_col;
mod obb;
mod object;
mod octree;
use crate::cg_support::node;
pub use bvh::TreeStopCriteria;
pub use highp_col::*;
pub use obb::{Aabb, BoundingVolume, Obb};
use octree::Octree;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

type MeshIdType = (String, TreeStopCriteria);
type LoadedMeshMap = HashMap<MeshIdType, Rc<collision_mesh::CollisionMesh>>;

static mut LOADED_MESHES: Option<LoadedMeshMap> = None;

static mut MESH_MAP_INIT: bool = false;

struct MeshMap {
    loaded_meshes: Option<LoadedMeshMap>,
}

fn get_loaded_meshes() -> MeshMap {
    let meshes = if unsafe { MESH_MAP_INIT } {
        unsafe { LOADED_MESHES.take() }
    } else {
        unsafe { MESH_MAP_INIT = true };
        Some(LoadedMeshMap::new())
    };
    MeshMap {
        loaded_meshes: Some(meshes.expect("Singleton already borrowed")),
    }
}

impl Drop for MeshMap {
    fn drop(&mut self) {
        unsafe {
            LOADED_MESHES = std::mem::replace(&mut self.loaded_meshes, None);
        }
    }
}

#[derive(Clone)]
pub struct CollisionObject {
    obj: Rc<RefCell<object::Object>>, //shared with octree, which holds weak pointers
    mesh: Rc<collision_mesh::CollisionMesh>, //shared between all objects with the same geometry
}

impl CollisionObject {
    /// Creates a new collision mesh for the collision mesh at `mesh_path` with the specified arguments if one does not exist
    /// or loads the cached one
    ///
    /// `transform` - the transform specific for this particular collision object
    pub fn new(
        transform: Rc<RefCell<node::Node>>,
        mesh_path: &str,
        bvh_stop: bvh::TreeStopCriteria,
    ) -> Self {
        let mut mmap = get_loaded_meshes();
        if let Some(mesh) = mmap
            .loaded_meshes
            .as_ref()
            .unwrap()
            .get(&(mesh_path.to_string(), bvh_stop))
        {
            let obj = Rc::new(RefCell::new(object::Object::with_mesh(
                transform, mesh,
            )));
            Self {
                obj,
                mesh: mesh.clone(),
            }
        } else {
            let mesh = Rc::new(collision_mesh::CollisionMesh::new(
                mesh_path, bvh_stop,
            ));
            mmap.loaded_meshes
                .as_mut()
                .unwrap()
                .insert((mesh_path.to_owned(), bvh_stop), mesh.clone());
            let obj = Rc::new(RefCell::new(object::Object::with_mesh(
                transform, &mesh,
            )));
            Self { obj, mesh }
        }
    }

    /// Creates a new collision object that's meant to serve as a prototype to make
    /// new collision objects from
    pub fn prototype(mesh_path: &str, bvh_stop: TreeStopCriteria) -> Self {
        Self::new(
            Rc::new(RefCell::new(node::Node::default())),
            mesh_path,
            bvh_stop,
        )
    }

    /// Creates a new collision object by copying an existing one
    pub fn from(transform: &Rc<RefCell<node::Node>>, prototype: &Self) -> Self {
        let obj = Rc::new(RefCell::new(object::Object::from_prototype(
            transform,
            &*prototype.obj.borrow(),
        )));
        Self {
            obj,
            mesh: prototype.mesh.clone(),
        }
    }

    /// Gets the hit point and normal for each collider
    /// The receiver's hit point and normal (`self`) is stored in `pt_norm_a` in the `HitData` if
    /// any data is returned
    pub fn collision(
        &self,
        other: &Self,
        highp_strategy: &dyn HighPCollision,
    ) -> Option<Hit> {
        self.mesh.collision(
            &self.obj.borrow().model.borrow().mat(),
            &other.mesh,
            &other.obj.borrow().model.borrow().mat(),
            highp_strategy,
        )
    }

    /// Gets transformation matrices transforming a -1 to 1 cube to each bounding box
    #[allow(dead_code)]
    pub fn get_main_and_leaf_cube_transformations(
        &self,
    ) -> (Vec<cgmath::Matrix4<f64>>, Vec<cgmath::Matrix4<f64>>) {
        use cgmath::*;
        let (main, leaf) = self.mesh.main_and_leaf_boxes();

        (
            main.into_iter()
                .map(|x| {
                    assert!(matches!(x, obb::BoundingVolume::Aabb(_)));
                    Matrix4::from_translation(x.center().to_vec())
                        * Matrix4::from_nonuniform_scale(
                            x.extents().x,
                            x.extents().y,
                            x.extents().z,
                        )
                })
                .collect(),
            leaf.into_iter()
                .map(|x| {
                    assert!(matches!(x, obb::BoundingVolume::Aabb(_)));
                    Matrix4::from_translation(x.center().to_vec())
                        * Matrix4::from_nonuniform_scale(
                            x.extents().x,
                            x.extents().y,
                            x.extents().z,
                        )
                })
                .collect(),
        )
    }

    /// For testing purposes, gets the colliding leaf bounding volumes as transformation matrices to
    /// a -1 to 1 cube
    #[allow(dead_code)]
    pub fn get_colliding_volume_transformations(
        &self,
        other: &Self,
    ) -> Vec<cgmath::Matrix4<f64>> {
        use cgmath::*;
        let our_mat = self.obj.borrow().model.borrow().mat();
        let other_mat = other.obj.borrow().model.borrow().mat();
        let (our, other) =
            self.mesh
                .get_colliding_volumes(&our_mat, &other.mesh, &other_mat);
        our.into_iter()
            .map(|x| {
                assert!(matches!(x, obb::BoundingVolume::Aabb(_)));
                our_mat
                    * Matrix4::from_translation(x.center().to_vec())
                    * Matrix4::from_nonuniform_scale(
                        x.extents().x,
                        x.extents().y,
                        x.extents().z,
                    )
            })
            .chain(other.into_iter().map(|x| {
                assert!(matches!(x, obb::BoundingVolume::Aabb(_)));
                other_mat
                    * Matrix4::from_translation(x.center().to_vec())
                    * Matrix4::from_nonuniform_scale(
                        x.extents().x,
                        x.extents().y,
                        x.extents().z,
                    )
            }))
            .collect()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn get_transformation(&self) -> Rc<RefCell<node::Node>> {
        self.obj.borrow().model.clone()
    }

    #[inline]
    pub fn update_in_collision_tree(&self) {
        Octree::update(&self.obj);
    }

    #[inline]
    pub fn is_in_collision_tree(&self) -> bool {
        self.obj.borrow().octree_cell.upgrade().is_some()
    }

    /// Gets the center and radius of a bounding sphere for this mesh in world space
    ///
    /// The bounding sphere is scaled based on the current transformation matrix,
    /// the center is not
    #[inline]
    pub fn bounding_sphere(&self) -> (cgmath::Point3<f64>, f64) {
        let transform = self.obj.borrow().model.clone();
        let transform = transform.borrow();
        let (pt, radius) = self.mesh.bounding_sphere(&transform.get_scale());
        (pt, radius)
    }

    /// Gets the estimated volume of this collision object
    #[inline]
    pub fn aabb_volume(&self) -> f64 {
        self.mesh.aabb_volume()
    }

    /// Calls `func` on all vertices of this mesh
    #[inline]
    pub fn forall_verts<F: FnMut(&bvh::CollisionVertex<f32>)>(
        &self,
        mut func: F,
    ) {
        self.mesh.forall_verts(&mut func);
    }

    /// Gets an id that uniquely identifies this collision objects's shared geometry
    pub fn geometry_id(&self) -> usize {
        assert_eq_size!(usize, *mut collision_mesh::CollisionMesh);
        self.mesh.as_ref() as *const _ as usize
    }

    /// Gets an id that uniquely identifies this collision object by its transformation
    /// node
    pub fn node_id(&self) -> usize {
        self.obj.borrow().model.as_ptr() as usize
    }

    /// Performs collision detection with just another bounding volume
    /// performs no high precision collision detection
    pub fn collision_simple(
        &self,
        volume: obb::BoundingVolume,
        volume_transform: &cgmath::Matrix4<f64>,
    ) -> bool {
        self.mesh.bounding_volume_collision(
            &self.obj.borrow().model.borrow().mat(),
            volume,
            volume_transform,
        )
    }
}

impl std::hash::Hash for CollisionObject {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
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
    pub fn new(center: cgmath::Point3<f64>, half_width: f64) -> Self {
        Self {
            tree: Octree::new(center, half_width),
        }
    }

    /// Inserts the given object into the collision tree if it is not already in
    /// the tree, otherwise updates its position
    #[inline]
    pub fn insert(&mut self, obj: &CollisionObject) {
        if Self::update(obj) {
            return;
        }
        self.tree.insert(&obj.obj);
    }

    /// Updates the position of the given object in the collision tree
    /// Does nothing if the object is not in the tree
    ///
    /// Returns true if the object was in the tree and updated, false otherwise
    #[inline]
    pub fn update(obj: &CollisionObject) -> bool {
        Octree::update(&obj.obj)
    }

    #[inline]
    #[allow(dead_code)]
    pub fn remove(obj: &CollisionObject) {
        Octree::remove(&obj.obj);
    }

    /// Gets all objects that collide with the given object which is part of this
    /// collision tree
    pub fn get_colliders(obj: &CollisionObject) -> Vec<CollisionObject> {
        Octree::get_colliders(&obj.obj)
            .into_iter()
            .map(|x| CollisionObject {
                mesh: x.borrow().mesh.upgrade().unwrap(),
                obj: x.clone(),
            })
            .collect()
    }

    /// Gets all objects that collide with the given sphere in this collision tree
    pub fn test_for_collisions(
        &self,
        center: cgmath::Point3<f64>,
        radius: f64,
    ) -> Vec<CollisionObject> {
        self.tree
            .test_for_collisions(center, radius)
            .into_iter()
            .map(|x| CollisionObject {
                mesh: x.borrow().mesh.upgrade().unwrap(),
                obj: x.clone(),
            })
            .collect()
    }

    /// Gets all objects in the tree
    pub fn get_all_objects(&self) -> Vec<CollisionObject> {
        self.tree
            .get_all_objects()
            .into_iter()
            .map(|x| CollisionObject {
                mesh: x.borrow().mesh.upgrade().unwrap(),
                obj: x.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::unreadable_literal)]
    use super::*;
    use assertables::*;
    use cgmath::*;
    use serial_test::serial;

    #[serial]
    #[test]
    fn collision_tree_test() {
        let mut tree =
            CollisionTree::new(cgmath::Point3::new(0.0, 0.0, 0.0), 15.0);
        let mut n = node::Node::default();

        n.set_pos(point3(
            -3.0430575401731623,
            -2.2713675814903596,
            -3.811697260699404,
        ));
        n.set_scale(vec3(
            0.7901060645496794,
            0.5186714524834486,
            1.2765283791294075,
        ));
        n.set_rot(Quaternion::new(
            0.1383083848590439,
            -0.9795480588051816,
            -0.5675755819185906,
            0.9629267337964779,
        ));
        let n = Rc::new(RefCell::new(n));
        let cube = CollisionObject::new(
            n.clone(),
            "assets/default_cube.obj",
            TreeStopCriteria::default(),
        );
        tree.insert(&cube);
        // should have no collision in tree
        assert_eq!(
            // diagonal is sqrt(3) * 2, so radius is sqrt(3)
            tree.test_for_collisions(point3(-5., -5., -6.), f64::sqrt(3.))
                .len(),
            1
        );
        // distance is greater than the sum of the radii
        assert_lt!(
            point3(-5., -5., -6.).distance(cube.bounding_sphere().0),
            f64::sqrt(3.) + cube.bounding_sphere().1
        );
        // bounding sphere radius at least as large as actual radius
        assert_ge!(
            cube.bounding_sphere().1,
            (n.borrow().local_scale() * 2.0).magnitude() / 2.0
        );

        // should be collision
        assert!(cube.collision_simple(
            BoundingVolume::Obb(Obb {
                center: point3(-5., -5., -6.),
                extents: vec3(0.5, 0.5, 0.5),
                x: vec3(1., 0., 0.),
                y: vec3(0., 1., 0.),
                z: vec3(0., 0., 1.),
            }),
            &Matrix4::identity()
        ));

        // should have no collision
        assert!(cube.collision_simple(
            BoundingVolume::Aabb(Aabb {
                center: point3(0., 0., 0.),
                extents: vec3(0.5, 0.5, 0.5),
            }),
            &node::Node::default().pos(point3(-5., -5., -6.)).mat()
        ));

        n.borrow_mut().set_pos(point3(
            -4.039229615816211,
            -4.776894924136137,
            0.09186428210173148,
        ));
        n.borrow_mut().set_scale(vec3(
            1.487217370923572,
            1.3709115200925492,
            1.4952020627318765,
        ));
        n.borrow_mut().set_rot(Quaternion::new(
            0.447239114144479,
            0.8552816679147671,
            -0.7830254264138842,
            -0.7984538885011099,
        ));
        CollisionTree::update(&cube);
        let tile = BoundingVolume::Aabb(Aabb {
            center: point3(1., 0., 0.),
            extents: vec3(0.5, 0.5, 0.5),
        });
        // distance is greater than total radius
        assert_gt!(
            tile.center().distance(n.borrow().get_pos()),
            (n.borrow().local_scale() * 2.0).magnitude() / 2.0 + f64::sqrt(3.)
        );
        // should have collision iff detected collision in tree

        assert_eq!(
            tree.test_for_collisions(point3(1., 0., 0.), f64::sqrt(3.))
                .len(),
            if cube.collision_simple(tile, &Matrix4::identity()) {
                1
            } else {
                0
            }
        );
    }
}
