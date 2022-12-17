use crate::cg_support::*;
use crate::collisions;
use cgmath::*;
use std::cell::RefCell;
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A bare-bones cell that is really really unsafe
/// MUST use from a single thread, and MUST ensure no `borrow_mut` occurs
/// at the same time as borrow
struct RaceyCell<T> {
    cell: UnsafeCell<T>,
}

impl<T> RaceyCell<T> {
    const fn new(t: T) -> Self {
        Self {
            cell: UnsafeCell::new(t),
        }
    }

    #[allow(clippy::missing_const_for_fn)]
    unsafe fn borrow(&self) -> &T {
        &*self.cell.get()
    }

    #[allow(clippy::mut_from_ref)]
    unsafe fn borrow_mut(&self) -> &mut T {
        &mut *self.cell.get()
    }
}

unsafe impl<T> Sync for RaceyCell<T> {}

#[derive(PartialEq, Eq, Copy, Clone, Hash)]
pub enum BodyType {
    Static,
    Dynamic,
    Controlled,
}

#[derive(PartialEq, Eq, Hash, Copy, Clone)]
pub enum CollisionMethod {
    Triangle,
}

/// Rigid body data that is shared among all bodies with the same collision mesh
struct SharedRigidBody {
    collision_method: CollisionMethod,
    inertial_tensor: Matrix3<f64>,
}

lazy_static! {
    static ref SHARED_BODIES: RaceyCell<HashMap<usize, SharedRigidBody>> =
        RaceyCell::new(HashMap::new());
}
const INVALID_SHARED_BODY_ID: usize = 0;

impl SharedRigidBody {
    /// Computes the "unit" inertial tensor
    /// (must be multiplied by mass prior to usage)
    fn calc_inertial_tensor(
        collider: &collisions::CollisionObject,
    ) -> Matrix3<f64> {
        let mut ixx = 0.;
        let mut iyy = 0.;
        let mut izz = 0.;
        let mut ixy = 0.;
        let mut ixz = 0.;
        let mut iyz = 0.;
        collider.forall_verts(|pt| {
            let pt: Point3<f64> = pt.pos.cast().unwrap();
            ixx += pt.y.mul_add(pt.y, pt.z * pt.z);
            iyy += pt.x.mul_add(pt.x, pt.z * pt.z);
            izz += pt.x.mul_add(pt.x, pt.y * pt.y);
            ixy += pt.x * pt.y;
            ixz += pt.x * pt.z;
            iyz += pt.y * pt.z;
        });
        // column major
        Matrix3::new(ixx, -ixy, -ixz, -ixy, iyy, -iyz, -ixz, -iyz, izz)
    }

    fn new(collider: &collisions::CollisionObject) -> Self {
        unsafe { SHARED_BODIES.borrow_mut() }
            .entry(INVALID_SHARED_BODY_ID)
            .or_insert_with(|| Self {
                collision_method: CollisionMethod::Triangle,
                inertial_tensor: Matrix3::from_diagonal(Vector3::new(
                    1., 1., 1.,
                )),
            });
        Self {
            collision_method: CollisionMethod::Triangle,
            inertial_tensor: Self::calc_inertial_tensor(collider),
        }
    }

    fn get_ptr_id(collider: &Option<collisions::CollisionObject>) -> usize {
        collider
            .as_ref()
            .map_or(INVALID_SHARED_BODY_ID, |collider| {
                let id = collider.geometry_id();
                unsafe { SHARED_BODIES.borrow_mut() }
                    .entry(id)
                    .or_insert_with(|| Self::new(collider));
                id
            })
    }
}

/// Contains the data for a rigid body simulation
/// MUST use from a single thread
pub struct BaseRigidBody {
    pub transform: Rc<RefCell<node::Node>>,
    pub velocity: cgmath::Vector3<f64>,
    pub rot_vel: Vector3<f64>,
    pub collider: Option<collisions::CollisionObject>,
    pub body_type: BodyType,
    pub mass: f64,
    shared_body_ptr: usize,
}

impl BaseRigidBody {
    /// Get's the world space center of this rigid body
    pub fn center(&self) -> Point3<f64> {
        self.collider.as_ref().map_or_else(
            || {
                self.transform
                    .borrow()
                    .mat()
                    .transform_point(point3(0., 0., 0.))
            },
            |collider| collider.bounding_sphere().0,
        )
    }

    /// Gets the maximum distance from the object center a point on the
    /// object can be
    pub fn extents(&self) -> Option<f64> {
        self.collider
            .as_ref()
            .map(|collider| collider.bounding_sphere().1)
    }

    /// Sets the density of this body. Uses this to recompute the mass from the
    /// supplied density and volume of the body
    ///
    /// If this object doesn't have a collision body, sets the mass to the supplied density value
    pub fn density(&mut self, density: f64) {
        let scale = self.transform.borrow().local_scale();
        let scale = scale.x * scale.y * scale.z;
        let mass = self.collider.as_ref().map_or(density, |collider| {
            density * collider.aabb_volume() * scale
        });
        self.mass = mass;
    }

    /// The returned reference should not persist, whatever information that's
    /// needed should be copied
    /// This function must also be called from a single thread
    unsafe fn get_shared_body(&self) -> &SharedRigidBody {
        &(SHARED_BODIES.borrow()[&self.shared_body_ptr])
    }

    /// Gets the moment of inertia tensor
    pub fn moment_inertia(&self) -> Matrix3<f64> {
        self.mass * unsafe { self.get_shared_body() }.inertial_tensor
    }

    /// Gets the body's collision method
    pub fn col_meth(&self) -> CollisionMethod {
        unsafe { self.get_shared_body() }.collision_method
    }
}

/// Contains the physical information for a rigid body simulation and the body's
/// metadata
/// MUST use from a single thread
pub struct RigidBody<T> {
    pub base: BaseRigidBody,
    pub metadata: T,
}

impl<T> RigidBody<T> {
    pub fn new(
        transform: Rc<RefCell<node::Node>>,
        collider: Option<collisions::CollisionObject>,
        body_type: BodyType,
        metadata: T,
    ) -> Self {
        use collisions::CollisionObject;
        let mass = collider.as_ref().map_or(0., CollisionObject::aabb_volume);
        Self {
            base: BaseRigidBody {
                transform,
                mass,
                shared_body_ptr: SharedRigidBody::get_ptr_id(&collider),
                collider,
                velocity: vec3(0., 0., 0.),
                rot_vel: vec3(0., 0., 0.),
                body_type,
            },
            metadata,
        }
    }

    /// Changes the density of this body
    pub fn with_density(mut self, density: f64) -> Self {
        self.base.density(density);
        self
    }
}
