use super::bvh::Triangle;

/// Encapsulates collision information
pub struct HitData {
    /// The point of collision and the impact normal on the first collider's mesh
    pub pos_norm_a: (Point3<f64>, Vector3<f64>),
    /// The point of collision (and the normal computed from it) on the second collider's mesh
    pub pos_norm_b: (Point3<f64>, Vector3<f64>),
}

pub enum Hit {
    /// A collision ocurred, but we have no resolution data about it
    NoData,
    Hit(HitData),
}

/// Performs the high-precision collision test
pub trait HighPCollision {
    /// Determines if there is a collision between `this_triangles` and `other_triangles`
    ///
    /// Triangles are specified in local object space
    ///
    /// Returns `None` if there is no collision, otherwise returns a `Hit` that may contain more information about the
    /// collision
    fn collide(
        &self,
        this_triangles: &[Triangle<f32>],
        this_transform: &cgmath::Matrix4<f64>,
        other_triangles: &[Triangle<f32>],
        other_transform: &cgmath::Matrix4<f64>,
    ) -> Option<Hit>;
}

/// Implementation which does not perform any higher precision collision tests
pub struct HighPNone {}

impl HighPCollision for HighPNone {
    fn collide(
        &self,
        _: &[Triangle<f32>],
        _: &cgmath::Matrix4<f64>,
        _: &[Triangle<f32>],
        _: &cgmath::Matrix4<f64>,
    ) -> Option<Hit> {
        Some(Hit::NoData)
    }
}

use crate::cg_support::ssbo;
use crate::graphics_engine::shader;
use cgmath::*;

/// Hit data from triangles that a high-precision test determined are colliding
///
/// Requires that for all triangles in `colliders_a`, there exists a triangle that intersects it in `colliders_b` and
/// vice versa
fn hit_from_colliders(
    colliders_a: &[&Triangle<f32>],
    colliders_b: &[&Triangle<f32>],
    model_a: &Matrix4<f64>,
    model_b: &Matrix4<f64>,
) -> HitData {
    let avg_pt_norm = |colliders: &[&Triangle<f32>], model: &Matrix4<f64>| {
        let mut avg_point = point3(0f64, 0., 0.);
        let mut avg_norm = vec3(0f64, 0., 0.);
        for t in colliders {
            avg_point += t.centroid().to_vec();
            let sum: Vector3<f32> = t.norms().into_iter().sum();
            avg_norm += sum.cast().unwrap() / 3f64;
        }
        avg_point /= colliders_a.len() as f64;
        avg_norm /= colliders_a.len() as f64;
        let norm_trans = Matrix3::new(
            model.x.x, model.x.y, model.x.z, model.y.x, model.y.y, model.y.z,
            model.z.x, model.z.y, model.z.z,
        );
        (
            model.transform_point(avg_point),
            (norm_trans * avg_norm).normalize(),
        )
    };
    HitData {
        pos_norm_a: avg_pt_norm(colliders_a, model_a),
        pos_norm_b: avg_pt_norm(colliders_b, model_b),
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
#[repr(align(64))]
struct ShaderTriangle {
    _a: [f32; 4],
    _b: [f32; 4],
    _c: [f32; 4],
    _d: [f32; 4],
}

/// Performs GPU accelerated triangle-triangle high-precision collision tests
pub struct TriangleTriangleGPU<'a> {
    shader_manager: Option<&'a shader::ShaderManager>,
}

struct ComputeInputBuffer {
    buf: ssbo::Ssbo<ShaderTriangle>,
    len: u32,
}

impl<'a> TriangleTriangleGPU<'a> {
    const WORK_GROUP_SIZE: u32 = 8;

    #[allow(dead_code)]
    pub const fn new(
        shader_manager: &'a shader::ShaderManager,
    ) -> TriangleTriangleGPU<'a> {
        TriangleTriangleGPU {
            shader_manager: Some(shader_manager),
        }
    }

    pub const fn from_active_ctx() -> TriangleTriangleGPU<'static> {
        TriangleTriangleGPU {
            shader_manager: None,
        }
    }

    fn get_triangle_to_mat_func(
        mat: Matrix4<f64>,
    ) -> impl Fn(&Triangle<f32>) -> ShaderTriangle {
        move |x: &Triangle<f32>| {
            let verts = x.verts();
            ShaderTriangle {
                _a: (mat
                    * vec4(verts[0].x, verts[0].y, verts[0].z, 1.0)
                        .cast()
                        .unwrap())
                .cast()
                .unwrap()
                .into(),
                _b: (mat
                    * vec4(verts[1].x, verts[1].y, verts[1].z, 1.0)
                        .cast()
                        .unwrap())
                .cast()
                .unwrap()
                .into(),
                _c: (mat
                    * vec4(verts[2].x, verts[2].y, verts[2].z, 1.0)
                        .cast()
                        .unwrap())
                .cast()
                .unwrap()
                .into(),
                _d: [0., 0., 0., 0.],
            }
        }
    }

    /// Executes the collision compute shader
    /// # Arguments
    /// * `a` - The first set of triangles to test
    /// * `b` - The second set of triangles to test
    /// * `work_groups_x` - The number of work groups to execute in the x direction
    /// * `work_groups_y` - The number of work groups to execute in the y direction
    fn execute_collision_compute(
        &self,
        a: &ComputeInputBuffer,
        b: &ComputeInputBuffer,
        work_groups_x: u32,
        work_groups_y: u32,
    ) -> ssbo::Ssbo<[u32; 4]> {
        let output: ssbo::Ssbo<[u32; 4]> =
            ssbo::Ssbo::static_empty(a.len + b.len);
        output.zero_bytes();

        a.buf.bind(5);
        b.buf.bind(6);
        output.bind(7);
        if self.shader_manager.is_none() {
            let ctx = crate::graphics_engine::get_active_ctx();
            ctx.shader.execute_compute(
                work_groups_x,
                work_groups_y,
                1,
                &shader::UniformInfo::TriangleCollisions,
                None,
            );
        } else {
            self.shader_manager.as_ref().unwrap().execute_compute(
                work_groups_x,
                work_groups_y,
                1,
                &shader::UniformInfo::TriangleCollisions,
                None,
            );
        }

        output
    }

    /// Converts the output of the computer shader into a `Hit` structure
    fn colliders_from_output(
        a_mat: &Matrix4<f64>,
        b_mat: &Matrix4<f64>,
        a_colliders: &[&Triangle<f32>],
        b_colliders: &[&Triangle<f32>],
    ) -> Option<Hit> {
        if !a_colliders.is_empty() && !b_colliders.is_empty() {
            Some(Hit::Hit(hit_from_colliders(
                a_colliders,
                b_colliders,
                a_mat,
                b_mat,
            )))
        } else {
            None
        }
    }
}

impl<'a> HighPCollision for TriangleTriangleGPU<'a> {
    fn collide(
        &self,
        a_triangles: &[Triangle<f32>],
        a_mat: &Matrix4<f64>,
        b_triangles: &[Triangle<f32>],
        b_mat: &Matrix4<f64>,
    ) -> Option<Hit> {
        let a_in_triangles: Vec<ShaderTriangle> = a_triangles
            .iter()
            .map(Self::get_triangle_to_mat_func(*a_mat))
            .collect();
        let b_in_triangles: Vec<ShaderTriangle> = b_triangles
            .iter()
            .map(Self::get_triangle_to_mat_func(*b_mat))
            .collect();

        let a = ComputeInputBuffer {
            buf: ssbo::Ssbo::create_static(&a_in_triangles),
            len: a_triangles.len() as u32,
        };
        let b = ComputeInputBuffer {
            buf: ssbo::Ssbo::create_static(&b_in_triangles),
            len: b_triangles.len() as u32,
        };

        let work_groups_x = ((a.len
            + a.len % TriangleTriangleGPU::WORK_GROUP_SIZE)
            / TriangleTriangleGPU::WORK_GROUP_SIZE)
            .max(1);
        let work_groups_y = ((b.len
            + b.len % TriangleTriangleGPU::WORK_GROUP_SIZE)
            / TriangleTriangleGPU::WORK_GROUP_SIZE)
            .max(1);

        let output = self.execute_collision_compute(
            &a,
            &b,
            work_groups_x,
            work_groups_y,
        );

        let mut a_colliders = Vec::new();
        let mut b_colliders = Vec::new();
        for (e, idx) in
            output.map_read().as_slice().iter().zip(0..a.len + b.len)
        {
            //println!("Output got {:?}", e);
            if e[0] > 0 {
                if idx < a.len {
                    a_colliders.push(&a_triangles[idx as usize]);
                } else {
                    b_colliders.push(&b_triangles[(idx - a.len) as usize]);
                }
            }
        }
        Self::colliders_from_output(a_mat, b_mat, &a_colliders, &b_colliders)
    }
}

/// Performs CPU triangle-triangle collision detection.
pub struct TriangleTriangleCPU {}

impl TriangleTriangleCPU {
    // see triTriComp.glsl (this is a translation of the compute shader)

    fn get_t(
        verts_on_l: &Vector3<f64>,
        dist_to_plane: &Vector3<f64>,
        opposite_idx: usize,
        vert_idx: usize,
    ) -> f64 {
        verts_on_l[vert_idx]
            + (verts_on_l[opposite_idx] - verts_on_l[vert_idx])
                * dist_to_plane[vert_idx]
                / (dist_to_plane[vert_idx] - dist_to_plane[opposite_idx])
    }

    fn get_interval(
        project_on_l: &Vector3<f64>,
        signed_dists: &Vector3<f64>,
        vert_indices: (usize, usize, usize),
    ) -> (f64, f64) {
        (
            Self::get_t(
                project_on_l,
                signed_dists,
                vert_indices.0,
                vert_indices.1,
            ),
            Self::get_t(
                project_on_l,
                signed_dists,
                vert_indices.0,
                vert_indices.2,
            ),
        )
    }

    fn order_interval(interval: (f64, f64)) -> (f64, f64) {
        if interval.0 > interval.1 {
            (interval.1, interval.0)
        } else {
            interval
        }
    }

    fn interval_overlap(a_t: (f64, f64), b_t: (f64, f64)) -> bool {
        let a_t = Self::order_interval(a_t);
        let b_t = Self::order_interval(b_t);
        a_t.0 - f64::EPSILON <= b_t.0 && a_t.1 + f64::EPSILON >= b_t.0
            || a_t.0 - f64::EPSILON <= b_t.1 && a_t.1 + f64::EPSILON >= b_t.1
            || b_t.0 - f64::EPSILON <= a_t.0 && b_t.1 + f64::EPSILON >= a_t.0
            || b_t.0 - f64::EPSILON <= a_t.1 && b_t.1 + f64::EPSILON >= a_t.1
    }

    fn abs_max_dim(v: &Vector3<f64>) -> usize {
        let mut max = 0f64;
        let mut idx = 0usize;
        for i in 0..3 {
            let abs = v[i].abs();
            if abs > max {
                max = abs;
                idx = i;
            }
        }
        idx
    }

    fn opp_vert(v: &Vector3<f64>) -> (usize, usize, usize) {
        if v[0] * v[1] > 0. {
            (2, 0, 1)
        } else if v[0] * v[2] > 0. {
            (1, 0, 2)
        } else {
            (0, 1, 2)
        }
    }

    fn plane_test(
        pt_on_a: &Point3<f64>,
        b_verts: &[Point3<f64>],
        norm_a: &Vector3<f64>,
    ) -> (bool, Vector3<f64>) {
        let d = dot(-1. * norm_a, pt_on_a.to_vec());
        let signed_dists = vec3(d, d, d)
            + vec3(
                norm_a.dot(b_verts[0].to_vec()),
                norm_a.dot(b_verts[1].to_vec()),
                norm_a.dot(b_verts[2].to_vec()),
            );
        let all_same_side =
            signed_dists.x < 0. && signed_dists.y < 0. && signed_dists.z < 0.
                || signed_dists.x > 0.
                    && signed_dists.y > 0.
                    && signed_dists.z > 0.;
        (all_same_side, signed_dists)
    }

    fn is_coplanar(signed_dists: &Vector3<f64>) -> bool {
        signed_dists.x.abs() < f64::EPSILON
            && signed_dists.y.abs() < f64::EPSILON
            && signed_dists.z.abs() < f64::EPSILON
    }

    fn line_intersection_2d(
        start_a: &Point2<f64>,
        end_a: &Point2<f64>,
        start_b: &Point2<f64>,
        end_b: &Point2<f64>,
    ) -> bool {
        let a = end_a - start_a;
        let b = end_b - start_b;
        let cross_2d =
            |a: &Vector2<f64>, b: &Vector2<f64>| a.x * b.y - a.y * b.x;

        let rs = cross_2d(&a, &b);
        let qpr = cross_2d(&(start_b - start_a), &a);

        if rs.abs() < f64::EPSILON && qpr.abs() < f64::EPSILON {
            let l = a.normalize();
            let t_a = (dot(start_a.to_vec(), l), dot(end_a.to_vec(), l));
            let t_b = (dot(start_b.to_vec(), l), dot(end_b.to_vec(), l));
            return Self::interval_overlap(t_a, t_b);
        } else if rs.abs() < f64::EPSILON {
            return false;
        }

        let t = cross_2d(&(start_b - start_a), &b) / rs;
        let u = qpr / rs;

        (-f64::EPSILON..=1. + f64::EPSILON).contains(&t)
            && (-f64::EPSILON..=1. + f64::EPSILON).contains(&u)
    }

    fn triangle_intersection(
        a_verts: &[Point2<f64>],
        b_verts: &[Point2<f64>],
    ) -> bool {
        Self::line_intersection_2d(
            &a_verts[0],
            &a_verts[1],
            &b_verts[0],
            &b_verts[1],
        ) || Self::line_intersection_2d(
            &a_verts[0],
            &a_verts[1],
            &b_verts[0],
            &b_verts[2],
        ) || Self::line_intersection_2d(
            &a_verts[0],
            &a_verts[1],
            &b_verts[1],
            &b_verts[2],
        ) || Self::line_intersection_2d(
            &a_verts[0],
            &a_verts[2],
            &b_verts[0],
            &b_verts[1],
        ) || Self::line_intersection_2d(
            &a_verts[0],
            &a_verts[2],
            &b_verts[0],
            &b_verts[2],
        ) || Self::line_intersection_2d(
            &a_verts[0],
            &a_verts[2],
            &b_verts[1],
            &b_verts[2],
        ) || Self::line_intersection_2d(
            &a_verts[1],
            &a_verts[2],
            &b_verts[0],
            &b_verts[0],
        ) || Self::line_intersection_2d(
            &a_verts[1],
            &a_verts[2],
            &b_verts[0],
            &b_verts[2],
        ) || Self::line_intersection_2d(
            &a_verts[1],
            &a_verts[2],
            &b_verts[1],
            &b_verts[2],
        )
    }

    fn coplanar_test(
        plane_norm: Vector3<f64>,
        a_verts: &[Point3<f64>],
        b_verts: &[Point3<f64>],
    ) -> bool {
        let axis = Self::abs_max_dim(&plane_norm);
        let x = (axis + 1) % 3;
        let y = (axis + 2) % 3;
        let a_verts: Vec<Point2<f64>> =
            a_verts.iter().map(|p| point2(p[x], p[y])).collect();
        let b_verts: Vec<Point2<f64>> =
            b_verts.iter().map(|p| point2(p[x], p[y])).collect();

        Self::triangle_intersection(&a_verts, &b_verts)
    }

    fn moller_test(a_verts: &[Point3<f64>], b_verts: &[Point3<f64>]) -> bool {
        let a_norm = (a_verts[2] - a_verts[0])
            .cross(a_verts[1] - a_verts[0])
            .normalize();
        let b_norm = (b_verts[2] - b_verts[0])
            .cross(b_verts[1] - b_verts[0])
            .normalize();

        let (b_same_side, b_dist_to_a) =
            Self::plane_test(&a_verts[0], b_verts, &a_norm);
        let (a_same_side, a_dist_to_b) =
            Self::plane_test(&b_verts[0], a_verts, &b_norm);
        if !b_same_side && !a_same_side {
            if Self::is_coplanar(&b_dist_to_a) {
                return Self::coplanar_test(a_norm, a_verts, b_verts);
            }
            let line = a_norm.cross(b_norm).normalize();
            let idx = Self::abs_max_dim(&line);
            let a_onto_line =
                vec3(a_verts[0][idx], a_verts[1][idx], a_verts[2][idx]);
            let b_onto_line =
                vec3(b_verts[0][idx], b_verts[1][idx], b_verts[2][idx]);
            let a_int = Self::get_interval(
                &a_onto_line,
                &a_dist_to_b,
                Self::opp_vert(&a_dist_to_b),
            );
            let b_int = Self::get_interval(
                &b_onto_line,
                &b_dist_to_a,
                Self::opp_vert(&b_dist_to_a),
            );
            Self::interval_overlap(a_int, b_int)
        } else {
            false
        }
    }
}

impl HighPCollision for TriangleTriangleCPU {
    fn collide(
        &self,
        a_triangles: &[Triangle<f32>],
        a_mat: &Matrix4<f64>,
        b_triangles: &[Triangle<f32>],
        b_mat: &Matrix4<f64>,
    ) -> Option<Hit> {
        for a in a_triangles {
            let a_verts: Vec<Point3<f64>> = a
                .verts()
                .into_iter()
                .map(|x| a_mat.transform_point(x.cast().unwrap()))
                .collect();
            for b in b_triangles {
                let b_verts: Vec<Point3<f64>> = b
                    .verts()
                    .into_iter()
                    .map(|x| b_mat.transform_point(x.cast().unwrap()))
                    .collect();
                if Self::moller_test(&a_verts, &b_verts) {
                    return Some(Hit::NoData);
                }
            }
        }
        None
    }
}
