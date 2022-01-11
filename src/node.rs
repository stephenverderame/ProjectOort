//use std::collections::BTreeMap;
use std::rc::Rc;
use cgmath::*;
use std::cell::RefCell;

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

    pub fn remove_parent(&mut self) {
        self.parent = None;
    }

    pub fn mat(&self) -> Matrix4<f64> {
        From::from(self)
    }
}

impl From<&'_ Node> for Matrix4<f64> {
    fn from(node: &'_ Node) -> Matrix4<f64> {
        let mat = Matrix4::from_translation(node.anchor.to_vec()) *
            Matrix4::from_translation(node.pos.to_vec()) * 
            Matrix4::from(node.orientation) * 
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

impl std::default::Default for Node {
    fn default() -> Node {
        Node::new(None, None, None, None)
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

#[cfg(test)]
mod test {
    use cgmath::*;
    use super::*;
    use assert_approx_eq::assert_approx_eq;

    #[test]
    fn anchor_rotation() {
        let a = Euler::new(Deg(0.), Deg(0.), Deg(90f64));
        let t = Node::new(None, Some(Quaternion::from(a)), None, Some(point3(8., 10., 10.)));
        let p = t.mat().transform_point(point3(10., 10., 10.));
        assert_eq!(p, point3(8f64, 12., 10.));
        let mut t = Node::default();
        t.pos = point3(10., 0., 0.);
        t.orientation = From::from(Euler::new(Deg(0.), Deg(0f64), Deg(-60.)));
        let  p = t.mat().transform_point(point3(0., 2., 0.));
        let q = point3(10. + f64::cos(30. * std::f64::consts::PI / 180.0) * 2.0, 1.0, 0.);
        assert_approx_eq!(p.x, q.x);
        assert_approx_eq!(p.y, q.y);
        assert_approx_eq!(p.z, q.z);
    }

    #[test]
    fn parent_transform() {
        let parent = Rc::new(RefCell::new(Node::new(Some(point3(10., 0., 0.)), None, None, None)));
        let mut child = Node::new(None, None, None, Some(point3(2., 2., 2.)));
        child.set_parent(parent.clone());
        let p = child.mat().transform_point(point3(10., 2., 2.));
        assert_eq!(p, point3(20., 2., 2.));
        parent.borrow_mut().scale = vec3(2., 1., 1.);
        let p = child.mat().transform_point(point3(2., 0., 0.));
        assert_eq!(p, point3(14., 0., 0.));
        child.pos = point3(0., 3., 0.);
        let p = child.mat().transform_point(point3(2., 0., 0.));
        assert_eq!(p, point3(14., 3., 0.));
    }

    #[test]
    fn scale_test() {
        let n = Node::new(None, None, Some(vec3(2., 2., 1.)), None);
        assert_eq!(n.mat().transform_point(point3(10., 1., 0.)),
            point3(20., 2., 0.));
        let p = Rc::new(RefCell::new(n));
        let mut c = Node::new(Some(point3(3., 0., 3.)), None, None, None);
        c.set_parent(p.clone());
        assert_eq!(c.mat().transform_point(point3(1., 0., 1.)),
            point3(8., 0., 4.));

        p.borrow_mut().remove_parent();
        p.borrow_mut().anchor = point3(0f64, 0., 0.);
        p.borrow_mut().pos = point3(0.058007f64, 0.452938, 0.037287);
        p.borrow_mut().orientation = Quaternion::new(0.991916f64, 0.051606, 0.099089, -0.060177);
        p.borrow_mut().scale = vec3(1.2f64, 0.8, 3.);
        let o = p.borrow().mat().transform_point(point3(-0.737862f64, 1.01066, 0.478124));
        cgmath::assert_relative_eq!(o, point3(0.638277f64, 0.26307, 1.37283));
    }
}