use cgmath::*;
use super::obb::AOBB;
use std::rc::Rc;
use std::cell::RefCell;
use crate::node::Node;

#[derive(Clone)]
pub struct Triangle<'a, T : BaseFloat> {
    indices: [u32; 3],
    vertices: &'a [Point3<T>],
}

impl<'a, T : BaseFloat> Triangle<'a, T> {
    pub fn centroid(&self) -> Point3<f64> {
        let v = ((self.vertices[self.indices[0] as usize].to_vec() +
        self.vertices[self.indices[1] as usize].to_vec() +
        self.vertices[self.indices[2] as usize].to_vec()) / 
        T::from(3).unwrap()).cast().unwrap();
        point3(v.x, v.y, v.z)
    }

    pub fn verts(&self) -> Vec<Point3<T>> {
        vec![self.vertices[self.indices[0] as usize],
        self.vertices[self.indices[1] as usize],
        self.vertices[self.indices[2] as usize]]
    }
}

const MAX_PRIMITIVES_PER_LEAF : usize = 12;


struct BVHNode<'a, T : BaseFloat> {
    left: Option<Box<BVHNode<'a, T>>>,
    right: Option<Box<BVHNode<'a, T>>>,
    volume: AOBB,
    triangles: Option<Vec<Triangle<'a, T>>>,
}

impl<'a, T : BaseFloat> BVHNode<'a, T> {
    fn new(world_transform: Rc<RefCell<Node>>, triangles: Vec<Triangle<'a, T>>) -> BVHNode<'a, T> {
        let volume = {
            let v : Vec<Point3<T>> = triangles.iter().flat_map(|s| s.verts().into_iter()).collect();
            AOBB::from_aabb(world_transform.clone(), &v)
        };
        let split = {
            let mut idx = 0;
            let mut max_extents = f64::MIN;
            for i in 0 .. 3 {
                if volume.extents[i] > max_extents {
                    max_extents = volume.extents[i];
                    idx = i;
                }
            }
            idx
        };
        if triangles.len() > MAX_PRIMITIVES_PER_LEAF {
            let mut left = Vec::<Triangle<'a, T>>::new();
            let mut right = Vec::<Triangle<'a, T>>::new();
            for tri in triangles {
                if tri.centroid()[split] < volume.center[split] {
                    left.push(tri)
                } else {
                    right.push(tri)
                }
            }
            BVHNode {
                left: Some(Box::new(BVHNode::new(world_transform.clone(), left))),
                right: Some(Box::new(BVHNode::new(world_transform, right))),
                volume, 
                triangles: None,
            }
        } else {
            BVHNode {
                left: None, right: None,
                volume,
                triangles: Some(triangles),
            }
        }
        
    }
    /// If there is a collision, gets a vector of triangles to check from each object as a tuple
    /// If no bounding volume collision occurs, `None` is returned
    fn triangles_to_check(&self, other: &BVHNode<'a, T>) -> Option<(Vec<Triangle<'a, T>>, Vec<Triangle<'a, T>>)> {
        if self.volume.collide(&other.volume) {
            let mut added_none_none = false;
            let mut check = |a : &Option<Box<BVHNode<'a, T>>>, b : &Option<Box<BVHNode<'a, T>>>| { 
                match (&*a, &*b) {
                    (Some(a), Some(b)) => a.triangles_to_check(&b),
                    (None, Some(b)) => self.triangles_to_check(b),
                    (Some(a), None) => a.triangles_to_check(other),
                    (None, None) if !added_none_none && 
                        self.triangles.is_some() && other.triangles.is_some() => 
                    {
                        added_none_none = true;
                        Some((self.triangles.as_ref().unwrap().clone(), 
                        other.triangles.as_ref().unwrap().clone()))
                    },
                    (None, None) => None,
                }
            };
            let mut our_triangles = Vec::<Triangle<'a, T>>::new();
            let mut other_triangles = our_triangles.clone();
            check(&self.left, &other.left).map(
                |mut x| { our_triangles.append(&mut x.0); other_triangles.append(&mut x.1) });
            check(&self.left, &other.right).map(
                |mut x| { our_triangles.append(&mut x.0); other_triangles.append(&mut x.1) });
            check(&self.right, &other.left).map(
                |mut x| { our_triangles.append(&mut x.0); other_triangles.append(&mut x.1) });
            check(&self.right, &other.right).map(
                |mut x| { our_triangles.append(&mut x.0); other_triangles.append(&mut x.1) });
            if our_triangles.is_empty() { None }
            else { Some((our_triangles, other_triangles)) }
        } else { None }
    }
}