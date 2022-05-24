use cgmath::*;
use super::obb::AABB;
use std::collections::{VecDeque, HashSet};
use std::hash::{Hash, Hasher};
use std::pin::Pin;
use std::marker::PhantomPinned;

/// The criteria for stopping the growth of a BVH tree
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub enum TreeStopCriteria {
    MaxPrimitivesPerLeaf(usize),
    #[allow(dead_code)]
    MaxDepth(u32),
    #[allow(dead_code)]
    AlwaysStop,
}

impl Default for TreeStopCriteria {
    fn default() -> TreeStopCriteria {
        TreeStopCriteria::MaxPrimitivesPerLeaf(32)
    }
}

impl TreeStopCriteria {
    /// Returns `true` if the tree should stop growing
    fn should_stop(&self, primitive_count: usize, cur_depth: u32) -> bool {
        use TreeStopCriteria::*;
        match self {
            AlwaysStop => true,
            MaxPrimitivesPerLeaf(x) => primitive_count <= *x,
            MaxDepth(x) => cur_depth >= *x,
        }
    }
}

#[derive(Clone)]
pub struct CollisionVertex<T : BaseFloat> {
    pub pos: Point3<T>,
    pub norm: Vector3<T>,
}

#[derive(Clone)]
pub struct Triangle<T : BaseFloat> {
    indices: [u32; 3],
    vertices: *const Vec<CollisionVertex<T>>,
}

impl<T : BaseFloat> std::fmt::Debug for Triangle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Triangle {:?}", self.verts())
    }
}

impl<T : BaseFloat> Triangle<T> {
    pub fn centroid(&self) -> Point3<f64> {
        let vertices = unsafe { &*self.vertices };
        let v = ((vertices[self.indices[0] as usize].pos.to_vec() +
        vertices[self.indices[1] as usize].pos.to_vec() +
        vertices[self.indices[2] as usize].pos.to_vec()) / 
        T::from(3).unwrap()).cast().unwrap();
        point3(v.x, v.y, v.z)
    }

    pub fn verts(&self) -> Vec<Point3<T>> {
        let vertices = unsafe { &*self.vertices };
        vec![vertices[self.indices[0] as usize].pos,
        vertices[self.indices[1] as usize].pos,
        vertices[self.indices[2] as usize].pos]
    }

    pub fn norms(&self) -> Vec<Vector3<T>> {
        let vertices = unsafe { &*self.vertices };
        vec![vertices[self.indices[0] as usize].norm,
        vertices[self.indices[1] as usize].norm,
        vertices[self.indices[2] as usize].norm]
    }

    pub fn array_from(indices: Vec<u32>, vertices: *const Vec<CollisionVertex<T>>) -> Vec<Triangle<T>> {
        use itertools::Itertools;
        assert_eq!(indices.len() % 3, 0);
        let mut res = Vec::new();
        for (a, b, c) in indices.into_iter().tuples() {
            res.push(Triangle {
                indices: [a, b, c],
                vertices,
            });
        }
        res
    }
}

impl<T : BaseFloat> PartialEq for Triangle<T> {
    fn eq(&self, other: &Triangle<T>) -> bool {
        std::ptr::eq(self.vertices, other.vertices) && 
        self.indices == other.indices
    }
}

impl<T : BaseFloat> Eq for Triangle<T> {}

impl<T : BaseFloat> Hash for Triangle<T> {
    fn hash<H : Hasher>(&self, state: &mut H) {
        self.indices.hash(state);
    }
}

fn aobb_from_triangles<T : BaseFloat>(triangles: &[Triangle<T>]) -> AABB {
    let v : Vec<Point3<T>> = triangles.iter().flat_map(|s| s.verts().into_iter()).collect();
    AABB::from(&v)
}

fn largest_extent_index(aobb: &AABB) -> usize {
    let mut idx = 0;
    let mut max_extents = f64::MIN;
    for i in 0 .. 3 {
        if aobb.extents[i] > max_extents {
            max_extents = aobb.extents[i];
            idx = i;
        }
    }
    idx
}


struct BVHNode<T : BaseFloat> {
    left: Option<Box<BVHNode<T>>>,
    right: Option<Box<BVHNode<T>>>,
    volume: AABB,
    triangles: Option<Vec<Triangle<T>>>,
}

impl<T : BaseFloat> BVHNode<T> {
    /// Creates a new internal BVHNode with bounding volume `volume`
    /// 
    /// It's children will be given triangles divided based on being less than or greater than `volume`'s center
    /// along the `split` axis. `split` of `0` indicates the `x` coordinates are being divided, whereas `split` of `2`
    /// are the `z` coordinates.
    #[inline(always)]
    fn with_split(triangles: Vec<Triangle<T>>, split: usize, volume: AABB, 
        rec_depth: u32, stop: TreeStopCriteria) -> BVHNode<T> 
    {
        let mut left = Vec::<Triangle<T>>::new();
        let mut right = Vec::<Triangle<T>>::new();
        for tri in triangles {
            if tri.centroid()[split] < volume.center[split] {
                left.push(tri)
            } else {
                right.push(tri)
            }
        }
        if left.is_empty() || right.is_empty() {
            left.append(&mut right);
            BVHNode {
                left: None, right: None,
                volume, triangles: Some(left),
            }
        } else {
            println!("Splitting {} and {}", left.len(), right.len());
            BVHNode {
                left: Some(Box::new(BVHNode::new(left, rec_depth + 1, stop))),
                right: Some(Box::new(BVHNode::new(right, rec_depth + 1, stop))),
                volume, 
                triangles: None,
            }
        }
    }

    fn new(triangles: Vec<Triangle<T>>, recursion_depth: u32, stop: TreeStopCriteria) -> BVHNode<T> {
        let volume = aobb_from_triangles(&triangles);
        if !stop.should_stop(triangles.len(), recursion_depth) {
            let split = largest_extent_index(&volume);
            BVHNode::with_split(triangles, split, volume, recursion_depth, stop)
        } else {
            BVHNode {
                left: None, right: None,
                volume,
                triangles: Some(triangles),
            }
        }
        
    }

    #[inline(always)]
    fn is_leaf(&self) -> bool {
        self.triangles.is_some()
    }

    /// Returns `true` if we should descend this BVH heirarchy, otherwise `false` to indicate
    /// we should descend `other` during a collision query
    #[inline(always)]
    fn should_descend<F : BaseFloat>(&self, other: &BVHNode<F>) -> bool {
        !self.is_leaf() && (self.volume.vol() > other.volume.vol() || other.is_leaf())
    }

    /// Descends the collision heirarchy, descending into the largest
    /// nodes first.
    /// 
    /// `on_both_leaf` - function called when two colliding nodes are leaf nodes
    /// The first parameter is always a descendant of `self` and the second parameter
    /// is the colliding descendant of `other`
    fn descend_heirarchy<F>(&self, self_transform: &Matrix4<f64>, 
        other: &BVHNode<T>, other_transform: &Matrix4<f64>,
        mut on_both_leaf: F) where F : FnMut(&BVHNode<T>, &BVHNode<T>)
    {
        let mut stack = VecDeque::<(&BVHNode<T>, &BVHNode<T>)>::new();
        stack.push_front((self, other));
        while !stack.is_empty() {
            let (a, b) = stack.pop_front().unwrap();
            if !a.volume.collide(self_transform, &b.volume, other_transform) { continue; }
            if a.is_leaf() && b.is_leaf() {
                on_both_leaf(a, b);
            } else if a.should_descend(b) {
                a.right.as_ref().map(|x| stack.push_front((&*x, b))).unwrap();
                a.left.as_ref().map(|x| stack.push_front((&*x, b))).unwrap();
            } else {
                b.right.as_ref().map(|x| stack.push_front((a, &*x))).unwrap();
                b.left.as_ref().map(|x| stack.push_front((a, &*x))).unwrap();
            }
        }
    }

    /// If there is a collision, gets a vector of triangles to check from each object as a tuple
    /// If no bounding volume collision occurs, `None` is returned
    fn triangles_to_check(&self, self_transform: &Matrix4<f64>, 
        other: &BVHNode<T>, other_transform: &Matrix4<f64>) -> Option<(Vec<Triangle<T>>, Vec<Triangle<T>>)> 
    {
        let mut our_tris = Vec::<Triangle<T>>::new();
        let mut other_tris = Vec::<Triangle<T>>::new();
        let mut added_triangles = HashSet::<*const BVHNode<T>>::new();
        let mut add = |r: &BVHNode<T>, vec : &mut Vec<Triangle<T>>| {
            let ptr = r as *const BVHNode<T>;
            if !added_triangles.contains(&ptr) {
                added_triangles.insert(ptr);
                vec.append(&mut r.triangles.as_ref().unwrap().clone());
            }
        };
        self.descend_heirarchy(self_transform, other, other_transform, 
        |a, b| {
            add(a, &mut our_tris);
            add(b, &mut other_tris);
        });
        if our_tris.is_empty() || other_tris.is_empty() { None }
        else {
            Some((our_tris, other_tris))
        }

    }


    /// get's all bounding boxes of leaves
    /// Testing purposes
    #[allow(dead_code)]
    fn get_leaf_boxes(&self, boxes: &mut Vec<AABB>) {
        if self.is_leaf() {
            boxes.push(self.volume.clone())
        };
        if let Some(l) = &self.left {
            l.get_leaf_boxes(boxes);
        }
        if let Some(r) = &self.right {
            r.get_leaf_boxes(boxes);
        }
    }

    /// Gets all bounding boxes colliding from `self` and `other`
    /// testing purposes
    #[allow(dead_code)]
    fn get_colliding_boxes(&self, self_transform: &Matrix4<f64>, 
        other: &BVHNode<T>, other_transform: &Matrix4<f64>) -> (Vec<AABB>, Vec<AABB>)
    {
        let mut our_v = Vec::new();
        let mut other_v = Vec::new();
        let mut added_triangles = HashSet::<*const BVHNode<T>>::new();
        let mut add = |r: &BVHNode<T>, vec : &mut Vec<AABB>| {
            let ptr = r as *const BVHNode<T>;
            if !added_triangles.contains(&ptr) {
                added_triangles.insert(ptr);
                vec.push(r.volume.clone());
            }
        };
        self.descend_heirarchy(self_transform, other, other_transform, 
        |a, b| {
            add(a, &mut our_v);
            add(b, &mut other_v);
        });
        (our_v, other_v)
    }
}

struct SelfRef<T : BaseFloat> {
    vertices: Vec<CollisionVertex<T>>,
    _m: PhantomPinned,
}

pub struct OBBTree<T : BaseFloat> {
    _vertices: Pin<Box<SelfRef<T>>>,
    root: BVHNode<T>,
}

impl<T : BaseFloat> OBBTree<T> {
    pub fn from(indices: Vec<u32>, vertices: Vec<CollisionVertex<T>>, stop: TreeStopCriteria) -> OBBTree<T> {
        let vertices = Box::pin(SelfRef { vertices, _m: PhantomPinned });
        let ptr = &vertices.as_ref().vertices as *const Vec<CollisionVertex<T>>;
        let triangles = unsafe { Triangle::array_from(indices, &*ptr) };
        OBBTree {
            _vertices: vertices,
            root: BVHNode::new(triangles, 0, stop),
        }
    }


    /// If there is a collision, gets a vector of triangles to check from each object as a tuple
    /// If no bounding volume collision occurs, `None` is returned
    pub fn collision(&self, self_transform: &Matrix4<f64>, 
        other: &OBBTree<T>, other_transform: &Matrix4<f64>) -> Option<(Vec<Triangle<T>>, Vec<Triangle<T>>)>
    {
        self.root.triangles_to_check(self_transform, &other.root, other_transform)
    }

    /// Gets the largest local space AABB that encloses the entire bvh
    pub fn bounding_box(&self) -> AABB {
        self.root.volume.clone()
    }

    /// Gets the main bounding box at index 0, followed by all leaf bounding boxes
    /// Testing method
    #[allow(dead_code)]
    pub fn main_and_leaf_bounding_boxes(&self) -> Vec<AABB> {
        let mut v = vec![self.root.volume.clone()];
        self.root.get_leaf_boxes(&mut v);
        v
    }

    /// Testing method to get all colliding AABB's
    #[allow(dead_code)]
    pub fn get_colliding_volumes(&self, self_transform: &Matrix4<f64>, 
        other: &OBBTree<T>, other_transform: &Matrix4<f64>) -> (Vec<AABB>, Vec<AABB>)
    {
        self.root.get_colliding_boxes(self_transform, &other.root, other_transform)
    }

}