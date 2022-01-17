use cgmath::{Matrix4};
/// A transformation is something that can be converted into a matrix
pub trait Transformation {
    fn as_transform(&self) -> Matrix4<f64>;
}

impl Transformation for super::node::Node {
    fn as_transform(&self) -> Matrix4<f64> {
        From::from(self)
    }
}
impl Transformation for Matrix4<f64> {
    fn as_transform(&self) -> Self {
        self.clone()
    }
}

impl Transformation for Matrix4<f32> {
    fn as_transform(&self) -> Matrix4<f64> {
        self.cast().unwrap()
    }
}