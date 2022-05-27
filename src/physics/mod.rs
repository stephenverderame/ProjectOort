mod rigid_body;
mod simulation;
mod forces;

pub use rigid_body::*;
pub use simulation::Simulation;
pub use forces::*;
use cgmath::*;

/// Something that can apply a force on a rigid body
pub trait Forcer {
    /// If this forcer affects `body`, then gets the point of force application in world coordinates
    /// and the force vector
    /// 
    /// Otherwise `None`
    fn get_force(&self, body: &BaseRigidBody) -> Option<(Point3<f64>, Vector3<f64>)>;
}
