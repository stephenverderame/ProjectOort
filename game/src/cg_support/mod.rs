pub mod ssbo;
mod transformation;
use cgmath::*;
pub use transformation::*;

pub mod node {
    pub use shared_types::node::*;
}

/// Creates a rotation matrix to orient the object to face the given direction
/// `view_dir` - the direction the object should face
/// `up` - the direction that should be considered "up" for the object, must be
/// normalized
///
/// Returns None if `view_dir` or `up` are zero vectors, or if the rotation
/// is otherwise malformed
pub fn look_at(
    view_dir: Vector3<f64>,
    up: &Vector3<f64>,
) -> Option<Matrix3<f64>> {
    if view_dir.is_zero() || up.is_zero() {
        return None;
    }
    // positive z is oriented out of the screen
    let z = view_dir.normalize();
    let x = up.cross(z).normalize();
    let y = z.cross(x).normalize();
    Some(Matrix3::from_cols(x, y, z))
}
