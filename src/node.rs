//use std::collections::BTreeMap;
use std::rc::Rc;
use cgmath::*;
use std::cell::RefCell;

/// Rotation matrix or quaternion
#[allow(dead_code)]
#[derive(Clone)]
pub enum Rot {
    Quat(Quaternion<f32>),
    Mat(Matrix4<f32>),
}

/// Converts `rot` to matrix form
#[allow(dead_code)]
fn rot_to_mat(rot: &Rot) -> Matrix4<f32> {
    match rot {
        Rot::Quat(q) => Matrix4::<f32>::from(q.clone()),
        Rot::Mat(m) => m.clone(),
    }
}

/// A node in a transformation heirarchy with a position, rotation, scale, and anchor point
/// that can have a parent. The node represents the transformation from the local coordinate space to 
/// the parent's coordinate space. A node without an explicit parent is implicitly the child of the 
/// root scene node.
/// 
/// Conversion of a Node into a `Matrix4` returns the transformation matrix from
/// this node's local space to world space. 
#[derive(Clone)]
pub struct Node {
    pub pos: cgmath::Point3<f64>,
    pub scale: cgmath::Vector3<f64>,
    pub orientation: Quaternion<f64>,
    pub anchor: Point3<f64>,
    parent: Option<Rc<RefCell<Node>>>,
}

impl Node {
    /// Creates a new scene node
    /// # Arguments
    /// `trans` - The position of the node or `None` for `(0, 0, 0)`
    /// 
    /// `rot` - rotation quaternion or `None` for identity
    /// 
    /// `scale` - node scale or `None` for uniform scale of `1`
    /// 
    /// `anchor` - the center of rotation/scaling or `None` for `(0, 0, 0)`
    pub fn new(trans: Option<Point3<f64>>, rot: Option<Quaternion<f64>>, 
        scale: Option<cgmath::Vector3<f64>>, anchor: Option<Point3<f64>>) -> Node 
    {
        Node {
            pos: match trans {
                Some(p) => p,
                None => point3(0., 0., 0.),
            },
            scale: match scale {
                Some(f) => f,
                None => vec3(1., 1., 1.),
            },
            orientation: match rot {
                Some(quat) => quat,
                None => Quaternion::<f64>::new(1., 0., 0., 0.),
            },
            anchor: match anchor {
                Some(pt) => pt,
                None => point3(0., 0., 0.),
            },
            parent: None,
        }
    }

    pub fn set_parent(&mut self, parent: Rc<RefCell<Node>>) {
        self.parent = Some(parent);
    }
}

impl From<&'_ Node> for Matrix4<f64> {
    fn from(node: &'_ Node) -> Matrix4<f64> {
        let mat = Matrix4::from_translation(node.pos.to_vec()) * 
            Matrix4::<f64>::from(node.orientation) * 
            Matrix4::from_nonuniform_scale(node.scale.x, node.scale.y, node.scale.z) * 
            Matrix4::from_translation(node.anchor.to_vec() * -1f64);
        match &node.parent {
            Some(node) => {
                let parent : Matrix4<f64> = (&*node.borrow()).into();
                parent * mat
            },
            None => mat,
        }
    }
}

impl Into<Matrix4<f64>> for Node {
    fn into(self) -> Matrix4<f64> {
        Matrix4::<f64>::from(&self)
    }
}

impl From<&'_ Node> for Matrix4<f32> {
    fn from(node: &'_ Node) -> Matrix4<f32> {
        Matrix4::<f64>::from(node).cast::<f32>().unwrap()
    }
}

impl Into<Matrix4<f32>> for Node {
    fn into(self) -> Matrix4<f32> {
        Matrix4::<f32>::from(&self)
    }
}