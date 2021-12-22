//use std::collections::BTreeMap;
use std::rc::Rc;
use cgmath::*;
use std::cell::RefCell;

#[derive(Clone)]
pub enum Rot {
    Quat(Quaternion<f32>),
    Mat(Matrix4<f32>),
}

fn rot_to_mat(rot: &Rot) -> Matrix4<f32> {
    match rot {
        Rot::Quat(q) => Matrix4::<f32>::from(q.clone()),
        Rot::Mat(m) => m.clone(),
    }
}

#[derive(Clone)]
pub struct Node {
    pub pos: cgmath::Point3<f32>,
    pub scale: f32,
    pub orientation: Rot,
    pub anchor: Point3<f32>,
    parent: Option<Rc<RefCell<Node>>>,
}

impl Node {
    pub fn new(trans: Option<Point3<f32>>, rot: Option<Quaternion<f32>>, 
        scale: Option<f32>, anchor: Option<Point3<f32>>) -> Node 
    {
        Node {
            pos: match trans {
                Some(p) => p,
                None => point3(0., 0., 0.),
            },
            scale: match scale {
                Some(f) => f,
                None => 1f32,
            },
            orientation: match rot {
                Some(quat) => Rot::Quat(quat),
                None => Rot::Quat(Quaternion::<f32>::new(1., 0., 0., 0.)),
            },
            anchor: match anchor {
                Some(pt) => pt,
                None => point3(0., 0., 0.),
            },
            parent: None,
        }
    }

    /*pub fn add_child(&mut self, node: Node, name: &str) {
        self.children.insert(String::from(name), Box::new(node));
    }*/
    pub fn set_parent(&mut self, parent: Rc<RefCell<Node>>) {
        self.parent = Some(parent);
    }
}

impl From<&'_ Node> for Matrix4<f32> {
    fn from(node: &'_ Node) -> Matrix4<f32> {
        let mat = Matrix4::from_translation(node.pos.to_vec()) * 
            rot_to_mat(&node.orientation) * 
            Matrix4::from_scale(node.scale) * 
            Matrix4::from_translation(node.anchor.to_vec() * -1f32);
        match &node.parent {
            Some(node) => {
                let parent : Matrix4<f32> = (&*node.borrow()).into();
                parent * mat
            },
            None => mat,
        }
    }
}

impl Into<Matrix4<f32>> for Node {
    fn into(self) -> Matrix4<f32> {
        Matrix4::<f32>::from(&self)
    }
}