use cgmath::*;
/// AABB
///
/// An OBB that, in local space, is an AABB.
/// So the x, y, and z basis vectors are the unit basis vectors
#[derive(Clone)]
pub struct Aabb {
    pub center: Point3<f64>,
    pub extents: Vector3<f64>,
}

/// A fully defined OBB with arbitrary basis vectors
#[derive(Clone)]
pub struct Obb {
    pub center: Point3<f64>,
    /// The length of the x, y, and z basis vectors
    pub extents: Vector3<f64>,
    /// The x basis vector
    pub x: Vector3<f64>,
    /// The y basis vector
    pub y: Vector3<f64>,
    /// The z basis vector
    pub z: Vector3<f64>,
}

impl Obb {
    /// Creates an OBB by applying a world transformation matrix to an AABB
    fn from_local_aligned(aabb: &Aabb, model: Matrix4<f64>) -> Self {
        let center = model.transform_point(aabb.center);

        let x = model.x.truncate();
        // x.magnitude() is the scale of the model in the x direction
        let ex = model.x.magnitude() * aabb.extents.x;
        let x = x.normalize();
        let y = model.y.truncate();
        let ey = model.y.magnitude() * aabb.extents.y;
        let y = y.normalize();
        let z = model.z.truncate();
        let ez = model.z.magnitude() * aabb.extents.z;
        let z = z.normalize();

        Self {
            center,
            extents: vec3(ex, ey, ez),
            x,
            y,
            z,
        }
    }

    /// Requires `axis` is not 0 and is normalized
    /// Gets the projected min and max projection coefficients of the OBB onto the axis
    fn project_onto(&self, axis: &Vector3<f64>) -> (f64, f64) {
        let pts = [
            self.center
                + self.extents.x * self.x
                + self.extents.y * self.y
                + self.extents.z * self.z,
            self.center
                + self.extents.x * self.x
                + self.extents.y * self.y
                + self.extents.z * -self.z,
            self.center
                + self.extents.x * self.x
                + self.extents.y * -self.y
                + self.extents.z * self.z,
            self.center
                + self.extents.x * self.x
                + self.extents.y * -self.y
                + self.extents.z * -self.z,
            self.center
                + self.extents.x * -self.x
                + self.extents.y * self.y
                + self.extents.z * self.z,
            self.center
                + self.extents.x * -self.x
                + self.extents.y * self.y
                + self.extents.z * -self.z,
            self.center
                + self.extents.x * -self.x
                + self.extents.y * -self.y
                + self.extents.z * self.z,
            self.center
                + self.extents.x * -self.x
                + self.extents.y * -self.y
                + self.extents.z * -self.z,
        ];
        // let a_dot = axis.dot(*axis);
        // precondition that axis is normalized so we don't need to divide by a_dot
        let mut min = f64::MAX;
        let mut max = f64::MIN;
        for pt in pts {
            let r = axis.dot(pt.to_vec()); // / a_dot;
            min = min.min(r);
            max = max.max(r);
        }
        (min, max)
    }

    /// requires axis normalized
    /// returns true if tests passes (no collision)
    ///
    /// `axis` - either a tuple of an axis to test and `None`, or a tuple of an axis from this OBB, and an axis
    /// from `other`'s OBB whose cross product is the axis to test
    fn sat_test(
        &self,
        other: &Self,
        axis: (Vector3<f64>, Option<Vector3<f64>>),
    ) -> bool {
        let (axis_a, axis_b) = axis;
        let axis = match axis_b {
            None => axis_a,
            Some(axis_b) => {
                let a = axis_a.cross(axis_b);
                if a.magnitude() < f64::EPSILON {
                    // // axes are parallel, and lie in some plane P
                    // // choose a new axis perpendicular to that plane
                    // let n = axis_a
                    //     .cross(self.center + axis_a - (other.center + axis_b));
                    // if n.magnitude() < f64::EPSILON {
                    //     return false;
                    // }
                    // n.normalize()
                    return false;
                }
                a.normalize()
            }
        };
        let (min1, max1) = self.project_onto(&axis);
        let (min2, max2) = other.project_onto(&axis);
        min1 > max2 || min2 > max1
    }

    /// Returns true if there is a collision between this obb and `other`
    fn collision(&self, other: &Self) -> bool {
        let axes = [
            (self.x, None),
            (self.y, None),
            (self.z, None),
            (other.x, None),
            (other.y, None),
            (other.z, None),
            (self.x, Some(other.x)),
            (self.x, Some(other.y)),
            (self.x, Some(other.z)),
            (self.y, Some(other.x)),
            (self.y, Some(other.y)),
            (self.y, Some(other.z)),
            (self.z, Some(other.x)),
            (self.z, Some(other.y)),
            (self.z, Some(other.z)),
        ];
        for a in axes {
            if self.sat_test(other, a) {
                return false;
            }
        }
        true
    }

    fn vol(&self) -> f64 {
        8. * self.extents.x * self.extents.y * self.extents.z
    }
}

impl Aabb {
    /// Computes an AABB from points
    /// `points` - local space point cloud
    /// `model` - world space transformation
    pub fn from<T: BaseNum>(points: &[Point3<T>]) -> Self {
        let mut mins = vec3(f64::MAX, f64::MAX, f64::MAX);
        let mut maxs = vec3(f64::MIN, f64::MIN, f64::MIN);
        for pt in points {
            let pt = pt.cast().unwrap();
            mins.x = mins.x.min(pt.x);
            mins.y = mins.y.min(pt.y);
            mins.z = mins.z.min(pt.z);

            maxs.x = maxs.x.max(pt.x);
            maxs.y = maxs.y.max(pt.y);
            maxs.z = maxs.z.max(pt.z);
        }
        let center = (mins + maxs) / 2.0;
        let extents = maxs - center;
        Self {
            center: point3(center.x, center.y, center.z),
            extents,
        }
    }

    /// Createa a new AABB that encloses `a` and `b`
    #[allow(dead_code)]
    pub fn combine(a: &Self, b: &Self) -> Self {
        let center = (a.center.to_vec() + b.center.to_vec()) / 2.0;
        let dist = b.center + b.extents - (a.center + a.extents);
        let extents = {
            let mut extents = vec3(0., 0., 0.);
            for i in 0..3 {
                extents[i] = dist[i].abs() / 2.0;
            }
            extents
        };
        Self {
            center: point3(center.x, center.y, center.z),
            extents,
        }
    }

    /// Returns true if this OBB collides with `other`
    ///
    /// `self_transform` - the matrix transform this obb to world coordinates
    ///
    /// `other_transform` - the matrix transforming `other` to world coordinates
    pub fn collide(
        &self,
        self_transform: &Matrix4<f64>,
        other: &Self,
        other_transform: &Matrix4<f64>,
    ) -> bool {
        let other = Obb::from_local_aligned(
            other,
            self_transform.invert().unwrap() * other_transform,
        );
        /* Gist of SAT (separating axis theorem): if a line can be drawn between two obb's, they don't collide
         We check each axis of both obb's, then check the 9 combinations of cross products of these axes
         Checking an axis basically entails projecting the points of each obb onto the line, and checking for overlap on that
         line

         It follows from separating hyperplane theorem, for convex objects, there either exists a separating plane
         or the objects are intersecting. The separating axis is a line perpendicular to this plane
         Orthogonal projections of each object onto this line results in non-overlapping intervals (for no collision)

         Objects can intersect face-face, face-edge, or edge-edge. For intersections involving faces -> test
         face normals of both objects as separating axis. For edges, we use axes that are cross products of all edges in A
         with all edges in B
        */
        let this = Obb {
            x: vec3(1., 0., 0.),
            y: vec3(0., 1., 0.),
            z: vec3(0., 0., 1.),
            center: self.center,
            extents: self.extents,
        };
        this.collision(&other)
    }

    /// Returns true if this bounding box collides with `obb`
    pub fn obb_collide(
        &self,
        self_transform: &Matrix4<f64>,
        obb: &Obb,
    ) -> bool {
        let this = Obb::from_local_aligned(self, *self_transform);
        this.collision(obb)
    }

    /// Volume of the OBB
    #[inline]
    pub fn vol(&self) -> f64 {
        self.extents.x * self.extents.y * self.extents.z * 8.0
    }
}

#[derive(Clone)]
pub enum BoundingVolume {
    Aabb(Aabb),
    Obb(Obb),
}

impl BoundingVolume {
    /// Returns true if this bounding volume collides with `other`
    /// `self_transform` - the matrix transform this bounding volume to
    /// world coordinates
    /// `other_transform` - the matrix transforming `other` to world coordinates
    ///  If `self` or `other` is an AABB, `self_transform` and `other_transform`
    /// must be provided, respectively
    pub fn is_colliding(
        &self,
        self_transform: Option<&Matrix4<f64>>,
        other: &Self,
        other_transform: Option<&Matrix4<f64>>,
    ) -> bool {
        match (self, other) {
            (BoundingVolume::Aabb(a), BoundingVolume::Aabb(b)) => {
                a.collide(self_transform.unwrap(), b, other_transform.unwrap())
            }
            (BoundingVolume::Aabb(a), BoundingVolume::Obb(b)) => {
                a.obb_collide(self_transform.unwrap(), b)
            }
            (BoundingVolume::Obb(a), BoundingVolume::Aabb(b)) => {
                b.obb_collide(other_transform.unwrap(), a)
            }
            (BoundingVolume::Obb(a), BoundingVolume::Obb(b)) => a.collision(b),
        }
    }

    /// Gets the center of the bounding volume
    pub const fn center(&self) -> Point3<f64> {
        match self {
            BoundingVolume::Aabb(a) => a.center,
            BoundingVolume::Obb(o) => o.center,
        }
    }

    /// Volume of the bounding volume
    pub fn vol(&self) -> f64 {
        match self {
            BoundingVolume::Aabb(a) => a.vol(),
            BoundingVolume::Obb(o) => o.vol(),
        }
    }

    /// Gets the extents (half-widths) of the bounding volume
    pub const fn extents(&self) -> Vector3<f64> {
        match self {
            BoundingVolume::Aabb(a) => a.extents,
            BoundingVolume::Obb(o) => o.extents,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::cg_support::node;
    use assertables::*;

    #[allow(clippy::many_single_char_names)]
    #[test]
    fn basic_collision() {
        let ident = node::Node::new(None, None, None, None);
        let a = Aabb::from(&[point3(1., 1., 1.), point3(-1., -1., -1.)]);
        let b = Aabb::from(&[point3(0.5, 0.5, 0.5), point3(0., 0., 0.)]);
        assert!(a.collide(&ident.mat(), &b, &ident.mat()));
        let c =
            Aabb::from(&[point3(100., 100., 100.), point3(102., 102., 102.)]);
        assert!(!a.collide(&ident.mat(), &c, &ident.mat()));
        assert!(!b.collide(&ident.mat(), &c, &ident.mat()));
        let g = Aabb::from(&[point3(8., 0., 0.), point3(-2., 6., 6.)]);
        let h = Aabb::from(&[point3(-2., 0., 0.), point3(4., -6., 6.)]);
        assert!(g.collide(&ident.mat(), &h, &ident.mat()));
    }

    #[test]
    fn rotation_collisions() {
        let t_a = node::Node::new(None, None, None, None);
        let mut t_b = node::Node::default();
        let a = Aabb::from(&[point3(5., -2., 2.), point3(3.0, 0., 0.)]);
        let b = Aabb::from(&[point3(6., -1., 1.), point3(7., 0., 0.)]);
        assert!(!a.collide(&t_a.mat(), &b, &t_b.mat()));
        t_b = t_b.anchor(point3(6., -1., 1.)).rot(From::from(Euler::new(
            Deg(0.),
            Deg(-180.),
            Deg(0f64),
        )));
        assert!(a.collide(&t_a.mat(), &b, &t_b.mat()));
        t_b = t_b.anchor(point3(6., 0., 0.)).rot(From::from(Euler::new(
            Deg(-180f64),
            Deg(0.),
            Deg(0.),
        )));
        assert!(!a.collide(&t_a.mat(), &b, &t_b.mat()));
    }

    #[test]
    fn transformation_collisions() {
        let mut t_a = node::Node::default();
        let mut t_b = node::Node::default();
        let a = Aabb::from(&[point3(4., -1., 0.), point3(6., 1., 2.)]);
        let b = Aabb::from(&[point3(-1., -1., -1.), point3(1., 1., 1.)]);
        t_a = t_a.anchor(point3(4., -1., 0.));
        assert!(!a.collide(&t_a.mat(), &b, &t_b.mat()));
        t_b = t_b.pos(point3(3.5, 0., 0.));
        assert!(a.collide(&t_a.mat(), &b, &t_b.mat()));
        t_b = t_b.pos(point3(2.5, 0., 0.));
        t_a = t_a.rot(From::from(Euler::new(Deg(0.), Deg(0.), Deg(70.))));
        assert!(a.collide(&t_a.mat(), &b, &t_b.mat()));
        t_a = t_a.rot(From::from(Euler::new(Deg(0.), Deg(0.), Deg(0.))));
        t_b = t_b.scale(vec3(1., 1., 2.));
        assert!(!a.collide(&t_a.mat(), &b, &t_b.mat()));
        t_b = t_b.rot(From::from(Euler::new(Deg(0.), Deg(30.), Deg(0.))));
        assert!(a.collide(&t_a.mat(), &b, &t_b.mat()));
    }

    #[test]
    fn edge_edge_collision() {
        let mut t_a = node::Node::default();
        let mut t_b = node::Node::default();
        let a = Aabb::from(&[point3(-1., -1., -1.), point3(1., 1., 1.)]);
        let b = Aabb::from(&[point3(-1., -1., -1.), point3(1., 1., 1.)]);
        t_a = t_a.pos(point3(3., 0., 3.));
        t_b = t_b.pos(point3(5., 0., 1.));
        assert!(a.collide(&t_a.mat(), &b, &t_b.mat()));
        t_a = t_a.rot(From::from(Euler::new(Deg(0f64), Deg(10.), Deg(0f64))));
        assert!(!a.collide(&t_a.mat(), &b, &t_b.mat()));
        t_b = t_b.scale(vec3(1.144, 1.144, 1.144));
        assert!(a.collide(&t_a.mat(), &b, &t_b.mat()));
        t_b = t_b
            .anchor(point3(6.1437, -1.1437, 2.1437))
            .rot(From::from(Euler::new(Deg(0f64), Deg(-9.), Deg(0f64))));
        assert!(!a.collide(&t_a.mat(), &b, &t_b.mat()));
        t_b = t_b.anchor(point3(0., 0., 0.));
        assert!(a.collide(&t_a.mat(), &b, &t_b.mat()));
    }

    #[allow(clippy::unreadable_literal)]
    #[test]
    fn obb_aabb_test() {
        let test_cube = Obb {
            center: point3(-5., -5., -6.),
            extents: vec3(0.5, 0.5, 0.5),
            x: vec3(1., 0., 0.),
            y: vec3(0., 1., 0.),
            z: vec3(0., 0., 1.),
        };

        let test_aabb = Aabb {
            center: point3(0., 0., 0.),
            extents: vec3(1., 1., 1.),
        };

        let mut n = node::Node::default();
        n.set_pos(point3(
            -3.0430575401731623,
            -2.2713675814903596,
            -3.811697260699404,
        ));
        n.set_scale(vec3(
            0.7901060645496794,
            0.5186714524834486,
            1.2765283791294075,
        ));
        n.set_rot(Quaternion::new(
            0.1383083848590439,
            -0.9795480588051816,
            -0.5675755819185906,
            0.9629267337964779,
        ));

        assert_gt!(
            n.get_pos().distance(test_cube.center),
            (f64::sqrt(3.) / 2.0).mul_add(1.0, (f64::sqrt(3.) * 2.27) / 2.0)
        );

        // ?? I don't think these should collide
        // it doesn't in Blender, but it kind of looked like it did
        // when rendered in this engine
        assert!(!test_aabb.obb_collide(&n.mat(), &test_cube));
    }
}
