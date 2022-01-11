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
#[derive(Copy, Clone)]
struct CompOut {
    v: [u32; 4],
}
pub struct TriangleTriangleGPU<'a, F : glium::backend::Facade> {
    facade: &'a F,
    shader_manager: &'a shader::ShaderManager,
}

impl<'a, F : glium::backend::Facade> TriangleTriangleGPU<'a, F> {
    pub fn new(shader_manager: &'a shader::ShaderManager, facade: &'a F) -> TriangleTriangleGPU<'a, F> {
        TriangleTriangleGPU {
            facade,
            shader_manager,
        }
    }
}

impl<'a, F : glium::backend::Facade> HighPCollision for TriangleTriangleGPU<'a, F> {

    fn collide(&self, a_triangles: &[Triangle<f32>], a_mat: &Matrix4<f64>, 
        b_triangles: &[Triangle<f32>], b_mat: &Matrix4<f64>) -> bool 
    {
        let a_mat : Matrix4<f32> = a_mat.cast().unwrap();
        let b_mat : Matrix4<f32> = b_mat.cast().unwrap();

        let map_func = |mat: Matrix4<f32>| {
            move |x: &Triangle<f32>| {
                let verts = x.verts();
                ShaderTriangle {
                    _a: (mat * vec4(verts[0].x, verts[0].y, verts[0].z, 1.0)).into(),
                    _b: (mat * vec4(verts[1].x, verts[1].y, verts[1].z, 1.0)).into(),
                    _c: (mat * vec4(verts[1].x, verts[1].y, verts[1].z, 1.0)).into(),
                }
            }
        };

        let a_triangles : Vec<ShaderTriangle> = a_triangles.iter().map(map_func(a_mat)).collect();
        let b_triangles : Vec<ShaderTriangle> = b_triangles.iter().map(map_func(b_mat)).collect();

        let a_len = a_triangles.len() as u32;
        let b_len = b_triangles.len() as u32;

        let input_a = ssbo::SSBO::create_static(a_triangles);
        let input_b = ssbo::SSBO::create_static(b_triangles);
        
        let work_group_size = 8;

        let work_groups_x = ((a_len + a_len % work_group_size) / work_group_size).max(1);
        let work_groups_y = ((b_len + b_len % work_group_size) / work_group_size).max(1);

        let output : ssbo::SSBO<CompOut> 
            = ssbo::SSBO::static_empty(work_groups_x * work_groups_y);
            //= ssbo::SSBO::create_static(out);
        
        input_a.bind(5);
        input_b.bind(6);
        output.bind(7);
        self.shader_manager.execute_compute(work_groups_x, work_groups_y, 1, 
            shader::UniformInfo::TriangleCollisionsInfo, None);

        for e in output.map_read().as_slice().iter() {
            println!("Output got {:?}", e.v);
            if e.v[0] > 0 { 
                return true; 
            }
        }
        false

    }
}