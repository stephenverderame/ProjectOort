use super::bvh::Triangle;
/// Performs the high-precision collision test
pub trait HighPCollision {
    fn collide(&self, this_triangles: &[Triangle<f32>], this_transform: &cgmath::Matrix4<f64>,
        other_triangles: &[Triangle<f32>], other_transform: &cgmath::Matrix4<f64>) -> bool;
}

pub struct HighPNone {}

impl HighPCollision for HighPNone {
    fn collide(&self, _: &[Triangle<f32>], 
        _: &cgmath::Matrix4<f64>, _: &[Triangle<f32>], _: &cgmath::Matrix4<f64>) -> bool { true }
}

use crate::shader;
use crate::ssbo;
use cgmath::*;

#[derive(Clone, Copy, Debug)]
struct ShaderTriangle {
    _a: [f32; 4],
    _b: [f32; 4],
    _c: [f32; 4],
}
pub struct TriangleTriangleGPU<'a> {
    shader_manager: &'a shader::ShaderManager,
}

impl<'a> TriangleTriangleGPU<'a> {
    const WORK_GROUP_SIZE : u32 = 8;

    pub fn new(shader_manager: &'a shader::ShaderManager) -> TriangleTriangleGPU<'a> {
        TriangleTriangleGPU {
            shader_manager,
        }
    }
}

impl<'a> HighPCollision for TriangleTriangleGPU<'a> {

    fn collide(&self, a_triangles: &[Triangle<f32>], a_mat: &Matrix4<f64>, 
        b_triangles: &[Triangle<f32>], b_mat: &Matrix4<f64>) -> bool 
    {
        let map_func = |mat: Matrix4<f64>| {
            move |x: &Triangle<f32>| {
                let verts = x.verts();
                ShaderTriangle {
                    _a: (mat * vec4(verts[0].x, verts[0].y, verts[0].z, 1.0).cast().unwrap()).cast().unwrap().into(),
                    _b: (mat * vec4(verts[1].x, verts[1].y, verts[1].z, 1.0).cast().unwrap()).cast().unwrap().into(),
                    _c: (mat * vec4(verts[2].x, verts[2].y, verts[2].z, 1.0).cast().unwrap()).cast().unwrap().into(),
                }
            }
        };

        let a_triangles : Vec<ShaderTriangle> = a_triangles.iter().map(map_func(*a_mat)).collect();
        let b_triangles : Vec<ShaderTriangle> = b_triangles.iter().map(map_func(*b_mat)).collect();

        let a_len = a_triangles.len() as u32;
        let b_len = b_triangles.len() as u32;

        let input_a = ssbo::SSBO::create_static(a_triangles);
        let input_b = ssbo::SSBO::create_static(b_triangles);

        let work_groups_x = ((a_len + a_len % TriangleTriangleGPU::WORK_GROUP_SIZE) / 
            TriangleTriangleGPU::WORK_GROUP_SIZE).max(1);
        let work_groups_y = ((b_len + b_len % TriangleTriangleGPU::WORK_GROUP_SIZE) / 
            TriangleTriangleGPU::WORK_GROUP_SIZE).max(1);

        let output : ssbo::SSBO<[f32; 4]> 
            = ssbo::SSBO::static_empty(work_groups_x * work_groups_y);
            //= ssbo::SSBO::create_static(out);
        
        input_a.bind(5);
        input_b.bind(6);
        output.bind(7);
        self.shader_manager.execute_compute(work_groups_x, work_groups_y, 1, 
            shader::UniformInfo::TriangleCollisionsInfo, None);

        for e in output.map_read().as_slice().iter() {
            //println!("Output got {:?}", e);
            if e[0] > 0. { 
                return true; 
            }
        }
        false

    }
}

pub struct TriangleTriangleCPU {}

impl TriangleTriangleCPU {

    /// gets the t value of the intersection point of the parameterized line
    /// vert[index] to vert[0] and the triangle intersection line
    /// requires index is 1 or 2
    fn get_t(verts_on_l: &Vector3<f64>, dist_to_plane: &Vector3<f64>, 
        opposite_idx: usize, vert_idx: usize) -> f64 
    {
        verts_on_l[vert_idx] + (verts_on_l[opposite_idx] - verts_on_l[vert_idx]) *
        dist_to_plane[vert_idx] / (dist_to_plane[vert_idx] - dist_to_plane[opposite_idx])
    }

    /// gets the overlap interval of a triangle
    /// `project_on_l` - the values of vertex 0, 1, 2 projected onto the line
    /// `signed_dists` - the signed distance of vertex 0, 1, 2 to the other triangle's plane
    /// `vertices` - the index of the vertex on the opposisite side of the other triangle's plane
    ///  followed by the indices of the vertices on the same side of the plane
    ///  so signed_dists[vertices.x] should have opposite sign as signed_dists[vertices.y] and
    ///  signed_dists[vertices.z]
    fn get_interval(project_on_l: &Vector3<f64>, signed_dists: &Vector3<f64>, 
        vert_indices: (usize, usize, usize)) -> (f64, f64) 
    {
        (TriangleTriangleCPU::get_t(project_on_l, signed_dists, vert_indices.0, vert_indices.1),
        TriangleTriangleCPU::get_t(project_on_l, signed_dists, vert_indices.0, vert_indices.2))
    }

    /// orders v so that v.x <= v.y
    fn order_interval(interval: (f64, f64)) -> (f64, f64) {
        if interval.0 > interval.1 {
            (interval.1, interval.0)
        } else { interval }
    }

    /// Tests wether the intervals defined by t_a and t_b overlap
    /// Intervals do not need to be in ascending order (ie. x, does not need to be the min)
    fn interval_overlap(a_t: (f64, f64), b_t: (f64, f64)) -> bool {
        let a_t = TriangleTriangleCPU::order_interval(a_t);
        let b_t = TriangleTriangleCPU::order_interval(b_t);
        a_t.0 - f64::EPSILON <= b_t.0 && a_t.1 + f64::EPSILON >= b_t.0 || 
        a_t.0 - f64::EPSILON <= b_t.1 && a_t.1 + f64::EPSILON >= b_t.1 || 
        b_t.0 - f64::EPSILON <= a_t.0 && b_t.1 + f64::EPSILON >= a_t.0 || 
        b_t.0 - f64::EPSILON <= a_t.1 && b_t.1 + f64::EPSILON >= a_t.1 
    }

    fn abs_max_dim(v: &Vector3<f64>) -> usize {
        let mut max = 0f64;
        let mut idx = 0usize;
        for i in 0 .. 3 {
            let abs = v[i].abs();
            if abs > max {
                max = abs;
                idx = i;
            }
        }
        idx
    }

    /// gets index of element with opposite sign followed by indices of elements with same sign
    fn opp_vert(v: &Vector3<f64>) -> (usize, usize, usize) {
        if v[0] * v[1] > 0. {
            (2, 0, 1)
        } else if v[0] * v[2] > 0. {
            (1, 0, 2)
        } else {
            (0, 1, 2)
        }
    }

    /// Gets a tuple of true/false if all points of b are on the same side of plane of a
    /// and signed distances of all of `b_verts` to plane of A
    fn plane_test(pt_on_a: &Point3<f64>, b_verts: &Vec<Point3<f64>>, norm_a: &Vector3<f64>) -> (bool, Vector3<f64>) {
        let d = dot(-1. * norm_a, pt_on_a.to_vec());
        let signed_dists = vec3(d, d, d) + vec3(norm_a.dot(b_verts[0].to_vec()),
            norm_a.dot(b_verts[1].to_vec()),
            norm_a.dot(b_verts[2].to_vec()));
        let all_same_side = signed_dists.x < 0. && signed_dists.y < 0. && signed_dists.z < 0. ||
            signed_dists.x > 0. && signed_dists.y > 0. && signed_dists.z > 0.;
        (all_same_side, signed_dists)
    }

    fn is_coplanar(signed_dists: &Vector3<f64>) -> bool {
        signed_dists.x.abs() < f64::EPSILON && signed_dists.y.abs() < f64::EPSILON &&
            signed_dists.z.abs() < f64::EPSILON
    }

    fn line_intersection_2d(start_a: &Point2<f64>, end_a: &Point2<f64>, 
        start_b: &Point2<f64>, end_b: &Point2<f64>) -> bool
    {
        let a = end_a - start_a;
        let b = end_b - start_b;
        let cross_2d = |a: &Vector2<f64>, b: &Vector2<f64>| {
            a.x * b.y - a.y * b.x
        };
        
        let rs = cross_2d(&a, &b);
        let qpr = cross_2d(&(start_b - start_a), &a);
    
        if rs.abs() < f64::EPSILON && qpr.abs() < f64::EPSILON {
            // colinear, project onto line and test overlap
            let l = a.normalize();
            let t_a = (dot(start_a.to_vec(), l), dot(end_a.to_vec(), l));
            let t_b = (dot(start_b.to_vec(), l), dot(end_b.to_vec(), l));
            return TriangleTriangleCPU::interval_overlap(t_a, t_b)
        }
        else if rs.abs() < f64::EPSILON {
            return false; // parallel
        } 
    
        let t = cross_2d(&(start_b - start_a), &b) / rs;
        let u = qpr / rs;
    
        return t >= -f64::EPSILON && t <= 1. + f64::EPSILON 
            && u >= -f64::EPSILON && u <= 1. + f64::EPSILON;
    }

    fn coplanar_test(plane_norm: Vector3<f64>, a_verts: &Vec<Point3<f64>>, b_verts: &Vec<Point3<f64>>) -> bool {
        // project onto axis-aligned plane that is closest to the plane
        // both triangles are on and perform 2d triangle collision detection
        let axis = TriangleTriangleCPU::abs_max_dim(&plane_norm);
        let x = (axis + 1) % 3;
        let y = (axis + 2) % 3;
        let a_verts : Vec<Point2<f64>> 
            = a_verts.into_iter().map(|p| point2(p[x], p[y])).collect();
        let b_verts : Vec<Point2<f64>> 
            = b_verts.into_iter().map(|p| point2(p[x], p[y])).collect();

        TriangleTriangleCPU::line_intersection_2d(&a_verts[0], &a_verts[1], &b_verts[0], &b_verts[1]) || 
        TriangleTriangleCPU::line_intersection_2d(&a_verts[0], &a_verts[1], &b_verts[0], &b_verts[2]) ||
        TriangleTriangleCPU::line_intersection_2d(&a_verts[0], &a_verts[1], &b_verts[1], &b_verts[2]) ||
        TriangleTriangleCPU::line_intersection_2d(&a_verts[0], &a_verts[2], &b_verts[0], &b_verts[1]) ||
        TriangleTriangleCPU::line_intersection_2d(&a_verts[0], &a_verts[2], &b_verts[0], &b_verts[2]) ||
        TriangleTriangleCPU::line_intersection_2d(&a_verts[0], &a_verts[2], &b_verts[1], &b_verts[2]) ||
        TriangleTriangleCPU::line_intersection_2d(&a_verts[1], &a_verts[2], &b_verts[0], &b_verts[0]) ||
        TriangleTriangleCPU::line_intersection_2d(&a_verts[1], &a_verts[2], &b_verts[0], &b_verts[2]) ||
        TriangleTriangleCPU::line_intersection_2d(&a_verts[1], &a_verts[2], &b_verts[1], &b_verts[2])
    }

    fn moller_test(a_verts: &Vec<Point3<f64>>, b_verts: &Vec<Point3<f64>>) -> bool {
        let a_norm = (a_verts[2] - a_verts[0]).cross(a_verts[1] - a_verts[0]).normalize();
        let b_norm = (b_verts[2] - b_verts[0]).cross(b_verts[1] - b_verts[0]).normalize();

        let (b_same_side, b_dist_to_a) = TriangleTriangleCPU::plane_test(&a_verts[0], &b_verts, &a_norm);
        let (a_same_side, a_dist_to_b) = TriangleTriangleCPU::plane_test(&b_verts[0], &a_verts, &b_norm);
        if !b_same_side && !a_same_side {
            // a line must pass through both triangles
            // this line's direction is the cross product of the normals
            if TriangleTriangleCPU::is_coplanar(&b_dist_to_a) {
                return TriangleTriangleCPU::coplanar_test(a_norm, a_verts, b_verts)
            }
            let line = a_norm.cross(b_norm).normalize();
            // overlap test doesn't change if we project not onto v, but onto
            // the coordinate axis for which v is most closely aligned
            let idx = TriangleTriangleCPU::abs_max_dim(&line);
            let a_onto_line = vec3(a_verts[0][idx], a_verts[1][idx], a_verts[2][idx]);
            let b_onto_line = vec3(b_verts[0][idx], b_verts[1][idx], b_verts[2][idx]);
            // we find intersections between the two edges connecting the vertex that
            // is on the other side of the triangle plane as the other two vertices
            // and v projected onto its world axis
            let a_int = TriangleTriangleCPU::get_interval(&a_onto_line, &a_dist_to_b, 
                TriangleTriangleCPU::opp_vert(&a_dist_to_b));
            let b_int = TriangleTriangleCPU::get_interval(&b_onto_line, &b_dist_to_a, 
                TriangleTriangleCPU::opp_vert(&b_dist_to_a));
            TriangleTriangleCPU::interval_overlap(a_int, b_int)
        } else {
            // all vertices of a triangle on the same side of the other triangle's plane
            false
        }
    }
}

impl HighPCollision for TriangleTriangleCPU {

    fn collide(&self, a_triangles: &[Triangle<f32>], a_mat: &Matrix4<f64>, 
        b_triangles: &[Triangle<f32>], b_mat: &Matrix4<f64>) -> bool 
    {
        for a in a_triangles {
            let a_verts : Vec<Point3<f64>> = 
            a.verts().into_iter().map(|x| a_mat.transform_point(x.cast().unwrap())).collect();
            for b in b_triangles {
                let b_verts : Vec<Point3<f64>> =
                b.verts().into_iter().map(|x| b_mat.transform_point(x.cast().unwrap())).collect();
                if TriangleTriangleCPU::moller_test(&a_verts, &b_verts) {
                    return true
                }
            }
        }
        false
    }
}