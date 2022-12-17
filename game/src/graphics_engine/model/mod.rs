use assimp::Vector3D;
use cgmath::*;
extern crate assimp;
extern crate assimp_sys;
extern crate tobj;
mod animation;
mod material;
mod mesh;
#[allow(clippy::module_inception)]
mod model;
pub use animation::Animator;
pub use model::Model;

/// Assimp `Vector3D` to `f32` array
#[inline]
fn to_v3(v: Vector3D) -> Vector3<f32> {
    vec3((*v).x, (*v).y, (*v).z)
}
/// Takes the `x` and `y` coordinates of an assimp `Vector3D`
#[inline]
fn to_v2(v: Vector3D) -> [f32; 2] {
    [(*v).x, (*v).y]
}
/// Assimp to cgmath Mat4
#[inline]
fn to_m4(m: assimp_sys::AiMatrix4x4) -> cgmath::Matrix4<f64> {
    cgmath::Matrix4::new(
        m.a1, m.b1, m.c1, m.d1, m.a2, m.b2, m.c2, m.d2, m.a3, m.b3, m.c3, m.d3, m.a4, m.b4, m.c4,
        m.d4,
    )
    .cast()
    .unwrap()
}

/// Generic interpolation
trait Lerp {
    type Numeric;
    /// Interpolates between `a` to `b` using `fac`
    ///
    /// Requires `fac` is between `0` and `1` where `0` indicates `a` is returned
    /// and `1` indicates `b` is
    fn lerp(a: Self, b: Self, fac: Self::Numeric) -> Self;
}

impl<T: BaseFloat> Lerp for Vector3<T> {
    type Numeric = T;
    fn lerp(a: Self, b: Self, fac: Self::Numeric) -> Self {
        a * (Self::Numeric::from(1).unwrap() - fac) + b * fac
    }
}

impl<T: BaseFloat> Lerp for Quaternion<T> {
    type Numeric = T;
    fn lerp(a: Self, b: Self, fac: Self::Numeric) -> Self {
        a.normalize().slerp(b.normalize(), fac)
    }
}
