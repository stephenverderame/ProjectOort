use std::rc::Rc;
use std::cell::RefCell;
use crate::collisions;
use crate::cg_support::*;
use cgmath::*;

#[derive(PartialEq, Eq, Copy, Clone, Hash)]
pub enum BodyType {
    Static, Dynamic, Controlled
}

#[derive(PartialEq, Eq, Hash, Copy, Clone)]
pub enum CollisionMethod {
    Triangle
}

pub struct RigidBody<T> {
    pub transform: Rc<RefCell<node::Node>>,
    pub velocity: cgmath::Vector3<f64>,
    pub rot_vel: Vector3<f64>,
    pub collider: Option<collisions::CollisionObject>,
    pub body_type: BodyType,
    pub col_type: CollisionMethod,
    pub metadata: T,
    pub mass: f64,
    inertial_tensor: Matrix3<f64>,
}

impl<T> RigidBody<T> {
    pub fn new(transform: Rc<RefCell<node::Node>>, collider: Option<collisions::CollisionObject>,
        body_type: BodyType, metadata: T) -> Self
    {
        let mass = collider.as_ref().map(|collider| collider.aabb_volume()).unwrap_or(0.);
        Self {
            transform,
            mass,
            inertial_tensor: collider.as_ref()
                .map(|collider| Self::calc_inertial_tensor(collider))
                .unwrap_or(Matrix3::from_scale(1.0)),
            collider,
            velocity: vec3(0., 0., 0.),
            rot_vel: vec3(0., 0., 0.),
            body_type,
            col_type: CollisionMethod::Triangle,
            metadata,
        }
    }

    /// Get's the world space center of this rigid body
    pub fn center(&self) -> Point3<f64> {
        if let Some(collider) = &self.collider {
            collider.bounding_sphere().0
        } else {
            self.transform.borrow().mat().transform_point(point3(0., 0., 0.))
        }
    }

    /// Sets the density of this body. Uses this to recompute the mass from the
    /// supplied density and volume of the body
    /// 
    /// If this object doesn't have a collision body, sets the mass to the supplied density value
    pub fn density(&mut self, density: f64) {
        let scale = self.transform.borrow().scale;
        let scale = scale.x * scale.y * scale.z;
        let mass = self.collider.as_ref().map(|collider| density * collider.aabb_volume() * scale).unwrap_or(density);
        self.mass = mass;
    }

    /// @see `density`
    pub fn with_density(mut self, density: f64) -> Self {
        self.density(density);
        self
    }

    /// Gets the moment of inertia tensor
    pub fn moment_inertia(&self) -> Matrix3<f64> {
        self.mass * self.inertial_tensor
    }

    /// Computes the "unit" inertial tensor 
    /// (must be multiplied by mass prior to usage)
    fn calc_inertial_tensor(collider: &collisions::CollisionObject) -> Matrix3<f64> {
        let mut ixx = 0.;
        let mut iyy = 0.;
        let mut izz = 0.;
        let mut ixy = 0.;
        let mut ixz = 0.;
        let mut iyz = 0.;
        collider.forall_verts(|pt| {
            let pt : Point3<f64> = pt.pos.cast().unwrap();
            ixx += pt.y * pt.y + pt.z * pt.z;
            iyy += pt.x * pt.x + pt.z * pt.z;
            izz += pt.x * pt.x + pt.y * pt.y;
            ixy += pt.x * pt.y;
            ixz += pt.x * pt.z;
            iyz += pt.y * pt.z;
        });
        // column major
        Matrix3::new(
            ixx, -ixy, -ixz,
            -ixy, iyy, -iyz,
            -ixz, -iyz, izz
        )
    }
}