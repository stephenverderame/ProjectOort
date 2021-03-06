use cgmath::*;
/// AABB
/// 
/// An OBB that, in local space, is an AABB.
/// So the x, y, and z basis vectors are the unit basis vectors
#[derive(Clone)]
pub struct AABB {
    pub center: Point3<f64>,
    pub extents: Vector3<f64>,
}

/// A fully defined OBB with arbitrary basis vectors
#[derive(Clone)]
struct OBB {
    center: Point3<f64>,
    extents: Vector3<f64>,
    x: Vector3<f64>,
    y: Vector3<f64>,
    z: Vector3<f64>,
}

impl OBB {
    /// Creates an OBB by applying a world transformation matrix to an AABB
    fn from_local_aligned(obb: &AABB, model: Matrix4<f64>) -> OBB {
        let mut pts = [
            obb.center + vec3(obb.extents.x, obb.extents.y, obb.extents.z),
            obb.center + vec3(obb.extents.x, obb.extents.y, -obb.extents.z),
            obb.center + vec3(obb.extents.x, -obb.extents.y, obb.extents.z),
            obb.center + vec3(obb.extents.x, -obb.extents.y, -obb.extents.z),
            obb.center + vec3(-obb.extents.x, obb.extents.y, obb.extents.z),
            obb.center + vec3(-obb.extents.x, obb.extents.y, -obb.extents.z),
            obb.center + vec3(-obb.extents.x, -obb.extents.y, obb.extents.z),
            obb.center + vec3(-obb.extents.x, -obb.extents.y, -obb.extents.z),
        ];
        let mut center = point3(0f64, 0., 0.);
        for pt in &mut pts {
            *pt = model.transform_point(*pt);
            center += pt.to_vec();
        };
        center /= 8.0;

        let mut x = pts[0] - pts[4];
        let ex = x.magnitude() / 2.0;
        x = x.normalize();

        let mut y = pts[0] - pts[2];
        let ey = y.magnitude() / 2.0;
        y = y.normalize();

        let mut z = pts[0] - pts[1];
        let ez = z.magnitude() / 2.0;
        z = z.normalize();

        OBB {
            center, extents: vec3(ex, ey, ez),
            x, y, z
        }
        
    }

    /// Requires `axis` is not 0 and is normalized
    /// Gets the projected center and radius
    fn project_onto(&self, axis: &Vector3<f64>) -> (f64, f64) {
        let pts = [
            self.center + self.extents.x * self.x + self.extents.y * self.y + self.extents.z * self.z,
            self.center + self.extents.x * self.x + self.extents.y * self.y + self.extents.z * -self.z,
            self.center + self.extents.x * self.x + self.extents.y * -self.y + self.extents.z * self.z,
            self.center + self.extents.x * self.x + self.extents.y * -self.y + self.extents.z * -self.z,
            self.center + self.extents.x * -self.x + self.extents.y * self.y + self.extents.z * self.z,
            self.center + self.extents.x * -self.x + self.extents.y * self.y + self.extents.z * -self.z,
            self.center + self.extents.x * -self.x + self.extents.y * -self.y + self.extents.z * self.z,
            self.center + self.extents.x * -self.x + self.extents.y * -self.y + self.extents.z * -self.z,
        ];
        let mut radius = 0f64;
        let a_dot = axis.dot(*axis);
        let center = axis.dot(self.center.to_vec()) / a_dot;
        for pt in pts {
            let r = axis.dot(pt.to_vec()) / a_dot;
            radius = radius.max((r - center).abs());
        }
        (center, radius)
        
    }

    /// requires axis normalized
    /// returns true if tests passes (no collision)
    /// 
    /// `axis` - either a tuple of an axis to test and `None`, or a tuple of an axis from this OBB, and an axis
    /// from `other`'s OBB whose cross product is the axis to test
    fn sat_test(&self, other: &OBB, axis: (Vector3<f64>, Option<Vector3<f64>>)) -> bool {
        let (axis_a, axis_b) = axis;
        let axis = match axis_b {
            None => axis_a,
            Some(axis_b) => {
                let a = axis_a.cross(axis_b);
                if a.magnitude2() < 5. * f64::EPSILON {
                    // axes are parallel, and lie in some plane P
                    // choose a new axis perpendicular to that plane
                    let n = axis_a.cross(self.center + axis_a - (other.center + axis_b));
                    if n.magnitude2() < 5. * f64::EPSILON {
                        return false
                    } else { n.normalize() }
                } else { a.normalize() }
            }
        };
        let (c, r) = self.project_onto(&axis);
        let (c2, r2) = other.project_onto(&axis);
        (c2 - c).abs() > r + r2
    }

    /// Returns true if there is a collision between this obb and `other`
    fn collision(&self, other: &OBB) -> bool {
        let axes = [
            (self.x, None), (self.y, None), (self.z, None),
            (other.x, None), (other.y, None), (other.z, None),
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
                return false
            }
        }
        true
    }
}

impl AABB {
    /// Computes an AABB from points
    /// `points` - local space point cloud
    /// `model` - world space transformation
    pub fn from<T : BaseNum>(points: &[Point3<T>]) -> AABB {
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
        let extents = vec3(maxs.x - center.x, maxs.y - center.y, maxs.z - center.z);
        AABB {
            center: point3(center.x, center.y, center.z),
            extents
        }
    }

    /// Createa a new AABB that encloses `a` and `b`
    #[allow(dead_code)]
    pub fn combine(a: &AABB, b: &AABB) -> AABB {
        let center = (a.center.to_vec() + b.center.to_vec()) / 2.0;
        let dist = b.center + b.extents - (a.center + a.extents);
        let extents = {
            let mut extents = vec3(0., 0., 0.);
            for i in 0 .. 3 {
                extents[i] = dist[i].abs() / 2.0;
            }
            extents
        };
        AABB { center: point3(center.x, center.y, center.z), extents }
    }

    /// Returns true if this OBB collides with `other`
    /// 
    /// `self_transform` - the matrix transform this obb to world coordinates
    /// 
    /// `other_transform` - the matrix transforming `other` to world coordinates
    pub fn collide(&self, self_transform: &Matrix4<f64>, other: &AABB, other_transform: &Matrix4<f64>) -> bool {
        let other = OBB::from_local_aligned(&other, 
            self_transform.invert().unwrap() * other_transform);
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
        let this = OBB {
            x: vec3(1., 0., 0.),
            y: vec3(0., 1., 0.),
            z: vec3(0., 0., 1.),
            center: self.center,
            extents: self.extents,
        };
        this.collision(&other)
    }

    /// Volume of the OBB
    #[inline(always)]
    pub fn vol(&self) -> f64 {
        self.extents.x * self.extents.y * self.extents.z * 8.0
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::cg_support::node;

    #[test]
    fn basic_collision() {
        let ident = node::Node::new(None, None, None, None);
        let a = AABB::from(&[point3(1., 1., 1.), point3(-1., -1., -1.)]);
        let b = AABB::from(&[point3(0.5, 0.5, 0.5), point3(0., 0., 0.)]);
        assert_eq!(a.collide(&ident.mat(), &b, &ident.mat()), true);
        let c = AABB::from(&[point3(100., 100., 100.), point3(102., 102., 102.)]);
        assert_eq!(a.collide(&ident.mat(), &c, &ident.mat()), false);
        assert_eq!(b.collide(&ident.mat(), &c, &ident.mat()), false);
        let g = AABB::from(&[point3(8., 0., 0.), point3(-2., 6., 6.)]);
        let h = AABB::from(&[point3(-2., 0., 0.), point3(4., -6., 6.)]);
        assert_eq!(g.collide(&ident.mat(), &h, &ident.mat()), true);
    }

    #[test]
    fn rotation_collisions() {
        let t_a = node::Node::new(None, None, None, None);
        let mut t_b = node::Node::default();
        let a = AABB::from(&[point3(5., -2., 2.), point3(3.0, 0., 0.)]);
        let b = AABB::from(&[point3(6., -1., 1.), point3(7., 0., 0.,)]);
        assert_eq!(a.collide(&t_a.mat(), &b, &t_b.mat()), false);
        t_b = t_b.anchor(point3(6., -1., 1.))
            .rot(From::from(Euler::new(Deg(0.), Deg(-180.), Deg(0f64))));
        assert_eq!(a.collide(&t_a.mat(), &b, &t_b.mat()), true);
        t_b = t_b.anchor(point3(6., 0., 0.))
            .rot(From::from(Euler::new(Deg(-180f64), Deg(0.), Deg(0.))));
        assert_eq!(a.collide(&t_a.mat(), &b, &t_b.mat()), false);
        
    }

    #[test]
    fn transformation_collisions() {
        let mut t_a = node::Node::default();
        let mut t_b = node::Node::default();
        let a = AABB::from(&[point3(4., -1., 0.), point3(6., 1., 2.)]);
        let b = AABB::from(&[point3(-1., -1., -1.), point3(1., 1., 1.,)]);
        t_a = t_a.anchor(point3(4., -1., 0.));
        assert_eq!(a.collide(&t_a.mat(), &b, &t_b.mat()), false);
        t_b = t_b.pos(point3(3.5, 0., 0.));
        assert_eq!(a.collide(&t_a.mat(), &b, &t_b.mat()), true);
        t_b = t_b.pos(point3(2.5, 0., 0.));
        t_a = t_a.rot(From::from(Euler::new(Deg(0.), Deg(0.), Deg(70.))));
        assert_eq!(a.collide(&t_a.mat(), &b, &t_b.mat()), true);
        t_a = t_a.rot(From::from(Euler::new(Deg(0.), Deg(0.), Deg(0.))));
        t_b = t_b.scale(vec3(1., 1., 2.));
        assert_eq!(a.collide(&t_a.mat(), &b, &t_b.mat()), false);
        t_b = t_b.rot(From::from(Euler::new(Deg(0.), Deg(30.), Deg(0.))));
        assert_eq!(a.collide(&t_a.mat(), &b, &t_b.mat()), true);

    }

    #[test]
    fn edge_edge_collision() {
        let mut t_a = node::Node::default();
        let mut t_b = node::Node::default();
        let a = AABB::from(&[point3(-1., -1., -1.), point3(1., 1., 1.)]);
        let b = AABB::from(&[point3(-1., -1., -1.), point3(1., 1., 1.,)]);
        t_a = t_a.pos(point3(3., 0., 3.));
        t_b = t_b.pos(point3(5., 0., 1.));
        assert_eq!(a.collide(&t_a.mat(), &b, &t_b.mat()), true);
        t_a = t_a.rot(From::from(Euler::new(Deg(0f64), Deg(10.), Deg(0f64))));
        assert_eq!(a.collide(&t_a.mat(), &b, &t_b.mat()), false);
        t_b = t_b.scale(vec3(1.144, 1.144, 1.144));
        assert_eq!(a.collide(&t_a.mat(), &b, &t_b.mat()), true);
        t_b = t_b.anchor(point3(6.1437, -1.1437, 2.1437))
            .rot(From::from(Euler::new(Deg(0f64), Deg(-9.), Deg(0f64))));
        assert_eq!(a.collide(&t_a.mat(), &b, &t_b.mat()), false);
        t_b = t_b.anchor(point3(0., 0., 0.));
        assert_eq!(a.collide(&t_a.mat(), &b, &t_b.mat()), true);
    }
}