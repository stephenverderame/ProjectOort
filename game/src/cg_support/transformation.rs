use cgmath::{Matrix4, Point3, Transform, Vector3};
/// A transformation is something that can be converted into a matrix
pub trait Transformation {
    fn as_transform(&self) -> Matrix4<f64>;

    fn into_transform(self) -> Matrix4<f64>;

    fn transform_pt(&self, pt: Point3<f64>) -> Point3<f64>;

    fn transform_vec(&self, v: Vector3<f64>) -> Vector3<f64>;
}

impl Transformation for super::node::Node {
    fn as_transform(&self) -> Matrix4<f64> {
        From::from(self)
    }

    fn into_transform(self) -> Matrix4<f64> {
        self.into()
    }

    fn transform_pt(&self, pt: Point3<f64>) -> Point3<f64> {
        self.transform_point(pt)
    }

    fn transform_vec(&self, v: Vector3<f64>) -> Vector3<f64> {
        Self::transform_vec(self, v)
    }
}
impl Transformation for Matrix4<f64> {
    fn as_transform(&self) -> Self {
        *self
    }

    fn into_transform(self) -> Self {
        self
    }

    fn transform_pt(&self, pt: Point3<f64>) -> Point3<f64> {
        self.transform_point(pt)
    }

    fn transform_vec(&self, v: Vector3<f64>) -> Vector3<f64> {
        self.transform_vector(v)
    }
}

impl Transformation for Matrix4<f32> {
    fn as_transform(&self) -> Matrix4<f64> {
        self.cast().unwrap()
    }

    fn into_transform(self) -> Matrix4<f64> {
        self.cast().unwrap()
    }

    fn transform_pt(&self, pt: Point3<f64>) -> Point3<f64> {
        self.as_transform().transform_point(pt)
    }

    fn transform_vec(&self, v: Vector3<f64>) -> Vector3<f64> {
        self.as_transform().transform_vector(v)
    }
}
