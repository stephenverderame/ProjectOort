//use std::collections::BTreeMap;
use std::rc::Rc;
use cgmath::*;
use std::cell::{RefCell, Cell};

/// A node in a transformation heirarchy with a position, rotation, scale, and anchor point
/// that can have a parent. The node represents the transformation from the local coordinate space to 
/// the parent's coordinate space. A node without an explicit parent is implicitly the child of the 
/// root scene node.
/// 
/// Conversion of a Node into a `Matrix4` returns the transformation matrix from
/// this node's local space to world space. 
/// 
/// Invariant: `transform` is the matrix which combines `pos`, `scale`, `orientation`, `anchor`, and `parent`
/// 
/// 
/// 
/// Node encapsulates its internals to avoid accidentally using one of its members directly, such as `pos`, without
/// taking the parent or anchor transformation into account.
/// 
/// This also allows us to more effectively cache the transformation matrix instead of needing to allways recompute it
#[derive(Clone, Debug)]
pub struct Node {
    pos: cgmath::Point3<f64>,
    scale: cgmath::Vector3<f64>,
    orientation: Quaternion<f64>,
    anchor: Point3<f64>,
    parent: Option<Rc<RefCell<Node>>>,
    /// None iff invalidated
    transform: Cell<Option<Matrix4<f64>>>,
    /// The version number of `parent`'s `transform` that was used to compute `transform`
    last_parent_ver: Cell<u64>,
    /// The version number of `transform`
    trans_ver: Cell<u64>,
}

fn transform_matrix(anchor: &Point3<f64>, pos: &Point3<f64>,
    orientation: &Quaternion<f64>, scale: &Vector3<f64>) -> Matrix4<f64> 
{
    Matrix4::from_translation(anchor.to_vec()) *
    Matrix4::from_translation(pos.to_vec()) * 
    Matrix4::from(*orientation) * 
    Matrix4::from_nonuniform_scale(scale.x, scale.y, scale.z) * 
    Matrix4::from_translation(anchor.to_vec() * -1f64)
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
        let pos = match trans {
            Some(p) => p,
            None => point3(0., 0., 0.),
        };
        let scale = match scale {
            Some(f) => f,
            None => vec3(1., 1., 1.),
        };
        let orientation = match rot {
            Some(quat) => quat,
            None => Quaternion::<f64>::new(1., 0., 0., 0.),
        };
        let anchor = match anchor {
            Some(pt) => pt,
            None => point3(0., 0., 0.),
        };
        Node {
            transform: Cell::new(Some(transform_matrix(&anchor, &pos, &orientation, &scale))),
            pos, scale, orientation, anchor,
            parent: None,
            last_parent_ver: Cell::new(0),
            trans_ver: Cell::new(0),
        }
    }

    /// Sets the translation
    #[inline]
    pub fn pos(mut self, pos: Point3<f64>) -> Self {
        self.set_pos(pos);
        self
    }

    /// Sets the xyz scale factors
    #[inline]
    pub fn scale(mut self, scale: Vector3<f64>) -> Self {
        self.set_scale(scale);
        self
    }

    /// Sets the uniform scale factor
    #[inline]
    pub fn u_scale(mut self, scale: f64) -> Self {
        self.set_u_scale(scale);
        self
    }

    /// Sets the orientation
    #[inline]
    pub fn rot(mut self, rot: Quaternion<f64>) -> Self {
        self.set_rot(rot);
        self
    }

    /// Sets the anchor shift
    #[inline]
    pub fn anchor(mut self, anchor: Point3<f64>) -> Self {
        self.set_anchor(anchor);
        self
    }

    #[inline]
    pub fn set_parent(&mut self, parent: Rc<RefCell<Node>>) {
        self.parent = Some(parent);
        self.transform.set(None);
    }

    #[inline]
    pub fn remove_parent(&mut self) {
        self.parent = None;
        self.transform.set(None);
    }

    /// Sets the parent
    #[inline]
    pub fn parent(mut self, parent: Rc<RefCell<Node>>) -> Self {
        self.set_parent(parent);
        self
    }

    /// Gets the transformation matrix
    #[inline]
    pub fn mat(&self) -> Matrix4<f64> {
        if self.needs_to_recompute() {
            self.update_matrix()
        } else {
            let t = self.transform.take();
            let m = t.unwrap();
            self.transform.set(t);
            m
        }
    }

    /// Transforms a point from the node's local space to the "world" space
    /// 
    /// Different operation from `transform_vec`
    #[inline]
    pub fn transform_point(&self, pt: Point3<f64>) -> Point3<f64> {
        self.mat().transform_point(pt)
    }

    /// Transforms a vector from the node's local space to the "world" space
    /// 
    /// Different operation than `transform_point`
    #[inline]
    pub fn transform_vec(&self, pt: Vector3<f64>) -> Vector3<f64> {
        self.mat().transform_vector(pt)
    }

    /// Sets the local translation
    #[inline]
    pub fn set_pos(&mut self, pos: Point3<f64>) {
        self.pos = pos;
        self.transform.set(None);
    }

    /// Sets the nonuniform scale factors
    #[inline]
    pub fn set_scale(&mut self, scale: Vector3<f64>) {
        self.scale = scale;
        self.transform.set(None);
    }

    /// Sets the uniform scale factor
    #[inline]
    pub fn set_u_scale(&mut self, scale: f64) {
        self.scale = vec3(scale, scale, scale);
        self.transform.set(None);
    }

    /// Sets the orientation
    #[inline]
    pub fn set_rot(&mut self, rot: Quaternion<f64>) {
        self.orientation = rot;
        self.transform.set(None);
    }

    /// Sets the anchor shift
    #[inline]
    pub fn set_anchor(&mut self, anchor: Point3<f64>) {
        self.anchor = anchor;
        self.transform.set(None);
    }

    /// Returns `true` if we need to recompute the cached transformation matrix
    fn needs_to_recompute(&self) -> bool {
        let t = self.transform.take();
        let local_recompute = t.is_none();
        self.transform.set(t);
        let last_par_ver = self.last_parent_ver.take();
        local_recompute || 
            self.parent.as_ref().map(|parent| {
                let ver = parent.borrow().trans_ver.take();
                let recompute = last_par_ver != ver || 
                    parent.borrow().needs_to_recompute();
                parent.borrow().trans_ver.set(ver);
                recompute
            })
            .unwrap_or(false)
    }

    /// Updates `self.transform` and returns the new matrix
    fn update_matrix(&self) -> Matrix4<f64> {
        let mat = transform_matrix(&self.anchor, &self.pos,
            &self.orientation, &self.scale);
        let t_prime = match &self.parent {
            Some(node) => {
                let parent = node.borrow().mat();
                parent * mat
            },
            None => mat,
        };
        self.transform.set(Some(t_prime));
        self.last_parent_ver.set(self.parent.as_ref().map(|parent| {
            let t = parent.borrow().trans_ver.take();
            parent.borrow().trans_ver.set(t);
            t
        }).unwrap_or(0));
        self.trans_ver.set(self.trans_ver.take() + 1);
        t_prime
    }

    /// Gets the scale factor, ignoring any parent transforms
    #[inline]
    pub fn local_scale(&self) -> Vector3<f64> {
        self.scale
    }

    /// Gets the rotation, ignoring any parent transforms
    #[inline]
    pub fn local_rot(&self) -> Quaternion<f64> {
        self.orientation
    }

    /// Gets the node position, ignoring any parent transforms
    #[inline]
    pub fn local_pos(&self) -> Point3<f64> {
        self.pos
    }

    /// Rotates the node by `rot`, which is respect to the world axis
    /// 
    /// Sets `orientation` to `orientation * rot`
    #[inline]
    pub fn rotate_world(&mut self, rot: Quaternion<f64>) {
        self.orientation = self.orientation * rot;
        self.transform.set(None);
    }

    /// Rotate the node by `rot`, which is respect to the node's local space
    /// 
    /// Sets `orientation` to `rot * orientation`
    #[inline]
    pub fn rotate_local(&mut self, rot: Quaternion<f64>) {
        self.orientation = rot * self.orientation;
        self.transform.set(None);
    }

    /// Translates the node by `translation` units in world space
    #[inline]
    pub fn translate(&mut self, translation: Vector3<f64>) {
        self.pos += translation;
        self.transform.set(None);
    }

    #[inline]
    /// Gets the point that the origin is mapped to
    /// 
    /// Convenience method for `transform_point(point3(0., 0., 0.))`
    pub fn get_pos(&self) -> Point3<f64> {
        self.transform_point(point3(0., 0., 0.))
    }

    #[inline]
    pub fn get_parent(&self) -> Option<Rc<RefCell<Node>>>
    { self.parent.clone() }
}

impl From<&'_ Node> for Matrix4<f64> {
    fn from(node: &'_ Node) -> Matrix4<f64> {
        node.mat()
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
        node.mat().cast::<f32>().unwrap()
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
        t.set_pos(point3(10., 0., 0.));
        t.set_rot(From::from(Euler::new(Deg(0.), Deg(0f64), Deg(-60.))));
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
        parent.borrow_mut().set_scale(vec3(2., 1., 1.));
        let p = child.mat().transform_point(point3(2., 0., 0.));
        assert_eq!(p, point3(14., 0., 0.));
        child.set_pos(point3(0., 3., 0.));
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
    }
}