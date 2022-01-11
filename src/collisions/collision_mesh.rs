use super::bvh::{OBBTree, TreeStopCriteria};
use super::highp_col::HighPCollision;
use std::rc::Rc;
use tobj;
use cgmath::*;

pub struct CollisionMesh {
    sub_meshes: Vec<OBBTree<f32>>,

}

fn get_mesh_data(mesh: &tobj::Mesh) -> (Vec<Point3<f32>>, Vec<u32>) {
    let mut verts = Vec::<Point3<f32>>::new();
    let indices = mesh.indices.clone();
    for idx in 0 .. mesh.positions.len() / 3 {
        let idx = idx as usize;
        /*let normal = if mesh.normals.is_empty() { point3(0., 0., 0.) } else 
        { point3(mesh.normals[idx * 3], mesh.normals[idx * 3 + 1], mesh.normals[idx * 3 + 2]) };*/
        let vert = point3(mesh.positions[idx * 3], mesh.positions[idx * 3 + 1], mesh.positions[idx * 3 + 2]);
        verts.push(vert);
    }
    (verts, indices)
}

impl CollisionMesh {
    pub fn new(file: &str, stop_method: TreeStopCriteria) -> CollisionMesh {
        let (meshes, _) = tobj::load_obj(file, &tobj::LoadOptions {
            triangulate: true,
            single_index: true,
            .. Default::default()
        }).unwrap();
        let meshes : Vec<OBBTree<f32>> = meshes.into_iter().map(|x| {
            let (verts, indices) = get_mesh_data(&x.mesh);
            println!("{} triangles", indices.len() / 3);
            OBBTree::from(indices, verts, stop_method)
        }).collect();
        println!("Created mesh");
        CollisionMesh { sub_meshes: meshes, }
    }

    pub fn collision(&self, self_transform: &Matrix4<f64>, other: &CollisionMesh,
        other_transform: &Matrix4<f64>, highp_strat: &dyn HighPCollision) -> bool 
    {
        let mut our_tris = Vec::new();
        let mut other_tris = Vec::new();
        for m in &self.sub_meshes {
            for i in &other.sub_meshes {
                m.collision(self_transform, i, other_transform).map(|(mut a, mut b)| {
                    our_tris.append(&mut a);
                    other_tris.append(&mut b);
                });
            }
        }
        //println!("Triangle checks: {} x {}", our_tris.len(), other_tris.len());
        if our_tris.is_empty() || other_tris.is_empty() { false }
        else {
            highp_strat.collide(&our_tris, &self_transform, 
                &other_tris, &other_transform)
        }
    }

    pub fn bounding_sphere(&self) -> (Point3<f64>, f64) {
        let mut c = vec3(0f64, 0., 0.);
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        let mut max_z = f64::MIN;
        for mesh in &self.sub_meshes {
            let aabb = mesh.bounding_box();
            c += aabb.center.to_vec();
            max_x = max_x.max(aabb.center.x + aabb.extents.x);
            max_y = max_y.max(aabb.center.y + aabb.extents.y);
            max_z = max_z.max(aabb.center.z + aabb.extents.z);
        }
        c /= self.sub_meshes.len() as f64;
        let center = point3(c.x, c.y, c.z);
        let extents = vec3(max_x - center.x, max_y - center.y, max_z - center.z);
        (center, extents.x.max(extents.y.max(extents.z)))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::collisions::highp_col;
    use crate::shader;
    use super::super::bvh;
    use crate::node;
    
    fn init() -> (shader::ShaderManager, glium::Display) {
        use glium::*;
        use glutin::window::{WindowBuilder};
        use glutin::ContextBuilder;
        use glutin::platform::windows::EventLoopExtWindows;
        let e_loop : glutin::event_loop::EventLoop<()> = glutin::event_loop::EventLoop::new_any_thread();
        let window_builder = WindowBuilder::new().with_visible(false).with_inner_size(glium::glutin::dpi::PhysicalSize::<u32>{
            width: 128, height: 128,
        });
        let wnd_ctx = ContextBuilder::new();//.build_headless(&e_loop, glutin::dpi::PhysicalSize::from((128, 128)));
        let wnd_ctx = Display::new(window_builder, wnd_ctx, &e_loop).unwrap();
        gl::load_with(|s| wnd_ctx.gl_window().get_proc_address(s));
        (shader::ShaderManager::init(&wnd_ctx), wnd_ctx)
    }

    #[test]
    fn basic_collisions() {
        let method = highp_col::HighPNone {};
        let plane_mesh = CollisionMesh::new("assets/Ships/StarSparrow01.obj", TreeStopCriteria::default());
        assert_eq!(plane_mesh.collision(&Matrix4::from_scale(1.), 
            &plane_mesh, &Matrix4::from_translation(vec3(3., 4., 1.)), &method), false);
        assert_eq!(plane_mesh.collision(&Matrix4::from_scale(1.), 
            &plane_mesh, &Matrix4::from_translation(vec3(3., 3., 1.)), &method), true);
    }

    #[test]
    fn triangle_tests() {
        let (shader, disp) = init();
        let strat = highp_col::TriangleTriangleGPU::new(&shader, &disp);
        let vertices = vec![point3(1f32, 0., 0.), point3(0., 1., 0.), point3(-1., 0., 0.)];
        let triangle = bvh::Triangle::array_from(vec![0, 1, 2], &vertices as *const Vec<Point3<f32>>);
        let mut t_b = node::Node::default();
        let mut t_a = node::Node::default();
        //assert_eq!(strat.collide(&triangle, &Matrix4::from_scale(1.), &triangle, &Matrix4::from_translation(vec3(0., 0., 1.))), false);
        t_b.orientation = Matrix3::from_angle_y(Deg(70f64)).into();
        t_b.pos = point3(0., 0., 0.8);
        assert_eq!(strat.collide(&triangle, &t_a.mat(), &triangle, &t_b.mat()), true);
    }
}