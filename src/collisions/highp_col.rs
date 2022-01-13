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
#[repr(align(64))]
struct ShaderTriangle {
    _a: [f32; 4],
    _b: [f32; 4],
    _c: [f32; 4],
    _d: [f32; 4],
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
                    _d: [0., 0., 0., 0.],
                }
            }
        };

        let a_triangles : Vec<ShaderTriangle> = a_triangles.iter().map(map_func(*a_mat)).collect();
        let b_triangles : Vec<ShaderTriangle> = b_triangles.iter().map(map_func(*b_mat)).collect();

        let a_len = a_triangles.len() as u32;
        let b_len = b_triangles.len() as u32;

        let input_a = ssbo::SSBO::create_static(a_triangles/*.clone()*/);
        let input_b = ssbo::SSBO::create_static(b_triangles);

        let work_groups_x = ((a_len + a_len % TriangleTriangleGPU::WORK_GROUP_SIZE) / 
            TriangleTriangleGPU::WORK_GROUP_SIZE).max(1);
        let work_groups_y = ((b_len + b_len % TriangleTriangleGPU::WORK_GROUP_SIZE) / 
            TriangleTriangleGPU::WORK_GROUP_SIZE).max(1);

        let output : ssbo::SSBO<[f32; 4]> 
            = ssbo::SSBO::static_empty(work_groups_x * work_groups_y);
        
        input_a.bind(5);
        input_b.bind(6);
        output.bind(7);
        self.shader_manager.execute_compute(work_groups_x, work_groups_y, 1, 
            shader::UniformInfo::TriangleCollisionsInfo, None);

        /*let a_data = input_a.get_data();
        assert_eq!(a_data.len(), a_triangles.len());
        let rng = 0 .. 2;
        for (e, idx) in a_data.iter().zip(rng.clone()) {
            println!("GPU {:?}\n", e);
            println!("CPU {:?}\n\n", a_triangles[idx]);
        }
        for (e, idx) in a_data.into_iter().zip(rng) {
            assert_relative_eq!(point3(e._a[0], e._a[1], e._a[2]), 
                point3(a_triangles[idx]._a[0], a_triangles[idx]._a[1], a_triangles[idx]._a[2]));
            assert_relative_eq!(point3(e._b[0], e._b[1], e._b[2]), 
                point3(a_triangles[idx]._b[0], a_triangles[idx]._b[1], a_triangles[idx]._b[2]));
            assert_relative_eq!(point3(e._c[0], e._c[1], e._c[2]), 
                point3(a_triangles[idx]._c[0], a_triangles[idx]._c[1], a_triangles[idx]._c[2]));
        }*/

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
    // see triTriComp.glsl (this is a translation of the compute shader)

    fn get_t(verts_on_l: &Vector3<f64>, dist_to_plane: &Vector3<f64>, 
        opposite_idx: usize, vert_idx: usize) -> f64 
    {
        verts_on_l[vert_idx] + (verts_on_l[opposite_idx] - verts_on_l[vert_idx]) *
        dist_to_plane[vert_idx] / (dist_to_plane[vert_idx] - dist_to_plane[opposite_idx])
    }

    fn get_interval(project_on_l: &Vector3<f64>, signed_dists: &Vector3<f64>, 
        vert_indices: (usize, usize, usize)) -> (f64, f64) 
    {
        (TriangleTriangleCPU::get_t(project_on_l, signed_dists, vert_indices.0, vert_indices.1),
        TriangleTriangleCPU::get_t(project_on_l, signed_dists, vert_indices.0, vert_indices.2))
    }

    fn order_interval(interval: (f64, f64)) -> (f64, f64) {
        if interval.0 > interval.1 {
            (interval.1, interval.0)
        } else { interval }
    }

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

    fn opp_vert(v: &Vector3<f64>) -> (usize, usize, usize) {
        if v[0] * v[1] > 0. {
            (2, 0, 1)
        } else if v[0] * v[2] > 0. {
            (1, 0, 2)
        } else {
            (0, 1, 2)
        }
    }

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
            let l = a.normalize();
            let t_a = (dot(start_a.to_vec(), l), dot(end_a.to_vec(), l));
            let t_b = (dot(start_b.to_vec(), l), dot(end_b.to_vec(), l));
            return TriangleTriangleCPU::interval_overlap(t_a, t_b)
        }
        else if rs.abs() < f64::EPSILON {
            return false; 
        } 
    
        let t = cross_2d(&(start_b - start_a), &b) / rs;
        let u = qpr / rs;
    
        return t >= -f64::EPSILON && t <= 1. + f64::EPSILON 
            && u >= -f64::EPSILON && u <= 1. + f64::EPSILON;
    }

    fn coplanar_test(plane_norm: Vector3<f64>, a_verts: &Vec<Point3<f64>>, b_verts: &Vec<Point3<f64>>) -> bool {
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
            if TriangleTriangleCPU::is_coplanar(&b_dist_to_a) {
                return TriangleTriangleCPU::coplanar_test(a_norm, a_verts, b_verts)
            }
            let line = a_norm.cross(b_norm).normalize();
            let idx = TriangleTriangleCPU::abs_max_dim(&line);
            let a_onto_line = vec3(a_verts[0][idx], a_verts[1][idx], a_verts[2][idx]);
            let b_onto_line = vec3(b_verts[0][idx], b_verts[1][idx], b_verts[2][idx]);
            let a_int = TriangleTriangleCPU::get_interval(&a_onto_line, &a_dist_to_b, 
                TriangleTriangleCPU::opp_vert(&a_dist_to_b));
            let b_int = TriangleTriangleCPU::get_interval(&b_onto_line, &b_dist_to_a, 
                TriangleTriangleCPU::opp_vert(&b_dist_to_a));
            TriangleTriangleCPU::interval_overlap(a_int, b_int)
        } else {
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