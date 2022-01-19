use super::bvh::{OBBTree, TreeStopCriteria, CollisionVertex};
use super::highp_col::{HighPCollision, Hit};
use super::obb;
use tobj;
use cgmath::*;

pub struct CollisionMesh {
    sub_meshes: Vec<OBBTree<f32>>,
}

fn get_mesh_data(mesh: &tobj::Mesh) -> (Vec<CollisionVertex<f32>>, Vec<u32>) {
    let mut verts = Vec::new();
    let indices = mesh.indices.clone();
    for idx in 0 .. mesh.positions.len() / 3 {
        let idx = idx as usize;
        let norm = if mesh.normals.is_empty() { vec3(0., 0., 0.) } else 
        { vec3(mesh.normals[idx * 3], mesh.normals[idx * 3 + 1], mesh.normals[idx * 3 + 2]) };
        let pos = point3(mesh.positions[idx * 3], mesh.positions[idx * 3 + 1], mesh.positions[idx * 3 + 2]);
        verts.push(CollisionVertex{ pos, norm });
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
        other_transform: &Matrix4<f64>, highp_strat: &dyn HighPCollision) -> Option<Hit>
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
        if our_tris.is_empty() || other_tris.is_empty() { None }
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

    /// Gets a tuple of the largest bounding volume in the tree and the leaf bounding volumes
    #[allow(dead_code)]
    pub fn main_and_leaf_boxes(&self) -> (Vec<obb::AABB>, Vec<obb::AABB>) {
        let mut main_boxes = Vec::new();
        let mut leaf_boxes = Vec::new();
        for sb in &self.sub_meshes {
            let mut v = sb.main_and_leaf_bounding_boxes();
            main_boxes.push(v.swap_remove(0));
            leaf_boxes.append(&mut v);
        }
        (main_boxes, leaf_boxes)
    }

    /// Testing method to get colliding bounding boxes
    #[allow(dead_code)]
    pub fn get_colliding_volumes(&self, self_transform: &Matrix4<f64>, other: &CollisionMesh,
        other_transform: &Matrix4<f64>) -> (Vec<obb::AABB>, Vec<obb::AABB>) {
        let mut our_v = Vec::new();
        let mut other_v = Vec::new();
        for sb in &self.sub_meshes {
            for o_sb in &other.sub_meshes {
                let (mut our, mut other) = sb.get_colliding_volumes(self_transform, o_sb, other_transform);
                our_v.append(&mut our);
                other_v.append(&mut other);
            }
        }
        (our_v, other_v)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::collisions::highp_col;
    use crate::graphics_engine::shader;
    use super::super::bvh;
    use crate::cg_support::node;
    use serial_test::serial;
    
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
    #[serial]
    fn basic_collisions() {
        let method = highp_col::HighPNone {};
        let plane_mesh = CollisionMesh::new("assets/Ships/StarSparrow01.obj", TreeStopCriteria::default());
        assert_eq!(plane_mesh.collision(&Matrix4::from_scale(1.), 
            &plane_mesh, &Matrix4::from_translation(vec3(3., 4., 1.)), &method).is_some(), false);
        assert_eq!(plane_mesh.collision(&Matrix4::from_scale(1.), 
            &plane_mesh, &Matrix4::from_translation(vec3(3., 3., 1.)), &method).is_some(), true);
    }

    #[test]
    #[serial]
    fn triangle_tests() {
        let (shader, _) = init();
        let strat = highp_col::TriangleTriangleGPU::new(&shader);
        let vertices = vec![CollisionVertex{ pos: point3(1f32, 0., 0.), norm: vec3(0., 0., 1.) }, 
            CollisionVertex{ pos: point3(0., 1., 0.), norm: vec3(0., 0., 1.) }, 
            CollisionVertex{ pos: point3(-1., 0., 0.), norm: vec3(0., 0., 1.)}];
        let triangle = bvh::Triangle::array_from(vec![0, 1, 2], &vertices as *const Vec<CollisionVertex<f32>>);
        let mut t_b = node::Node::default();
        let mut t_a = node::Node::default();
        assert_eq!(strat.collide(&triangle, &Matrix4::from_scale(1.), 
            &triangle, &Matrix4::from_translation(vec3(0., 0., 1.))).is_some(), false); //plane test reject
        t_b.orientation = Matrix3::from_angle_y(Deg(70f64)).into();
        t_b.pos = point3(0., 0., 0.8);
        assert_eq!(strat.collide(&triangle, &t_a.mat(), &triangle, &t_b.mat()).is_some(), true); //small intersect
        t_a.pos = point3(1., 0., 0.);
        assert_eq!(strat.collide(&triangle, &t_a.mat(), &triangle, &t_b.mat()).is_some(), true); //small intersect off origin
        t_b.pos = point3(-0.3, 0., 0.5);
        assert_eq!(strat.collide(&triangle, &t_a.mat(), &triangle, &t_b.mat()).is_some(), false); //plane test pass, no intersect
        t_b.pos = point3(-0.3, 1., 0.5);
        t_b.orientation = Matrix3::from_angle_y(Deg(120f64)).into();
        t_a.orientation = From::from(Euler::new(Deg(20f64), Deg(0.), Deg(53.)));
        assert_eq!(strat.collide(&triangle, &t_a.mat(), &triangle, &t_b.mat()).is_some(), false); //plane test pass, no intersect (more transforms)
        t_a.scale = vec3(1.895f64, 1.895, 1.895);
        assert_eq!(strat.collide(&triangle, &t_a.mat(), &triangle, &t_b.mat()).is_some(), true); // intersect via scale

        let vertices2 = vec![
            CollisionVertex{pos: point3(-0.292f32, -0.0536, 0.00074), norm: vec3(0., 0., 0.) }, 
            CollisionVertex{pos: point3(-0.392, 0.273, 0.296), norm: vec3(0., 0., 0.)}, 
            CollisionVertex{pos: point3(0.747, 0.515, 0.255), norm: vec3(0., 0., 0.) }];
        t_a.pos = point3(0., 0f64, 0.);
        t_a.orientation = Matrix3::from_angle_x(Deg(0.)).into();
        let triangle2 = bvh::Triangle::array_from(vec![0, 1, 2], &vertices2 as *const Vec<CollisionVertex<f32>>);
        assert_eq!(strat.collide(&triangle2, &t_a.mat(), &triangle, &t_b.mat()).is_some(), false); // random triangle no intersect
        t_a.pos = point3(-0.16618f64, 0.97175, 0.65434);
        assert_eq!(strat.collide(&triangle2, &t_a.mat(), &triangle, &t_b.mat()).is_some(), true); // random triangle intersect
        t_a = node::Node::default();
        t_b = node::Node::default();
        t_a.pos = point3(1.06508f64, 0.559814, 0.);
        assert_eq!(strat.collide(&triangle, &t_a.mat(), &triangle, &t_b.mat()).is_some(), true); //coplanar intersect
        t_a.pos = point3(3f64, 0., 0.);
        assert_eq!(strat.collide(&triangle, &t_a.mat(), &triangle, &t_b.mat()).is_some(), false); //coplanar no intersect
    }

    #[test]
    #[serial]
    fn collision_tests() {
        let (shader, _) = init();
        let strat = /*highp_col::TriangleTriangleCPU {};*/highp_col::TriangleTriangleGPU::new(&shader);
        let ship = CollisionMesh::new("assets/Ships/StarSparrow01.obj", TreeStopCriteria::AlwaysStop);
        let asteroid = CollisionMesh::new("assets/asteroid1/Asteroid.obj", TreeStopCriteria::AlwaysStop);
        let mut t_ship = node::Node::new(Some(point3(-14.2537f64, 32.5402f64, -39.6763)),
            Some(Quaternion::new(0.016473, 0.091357, 0.971325, 0.218883)),
            None, None);
        let mut t_ast = node::Node::new(Some(point3(-7.56163f64, 50.2958, -20.4725)), 
            Some(Quaternion::new(0.142821f64, 0.96663, -0.146196, -0.154451)),
            Some(vec3(0.375723f64, 0.375723, 0.375723)), None);

        assert_eq!(asteroid.collision(&t_ast.mat(), &ship, &t_ship.mat(), &strat).is_some(), false);

        t_ship = node::Node::new(Some(point3(-3.095402600732103f64, 9.244842371955391, 8.95973740527222)),
            Some(Quaternion::new(0.8425839759656641f64, -0.4024286753361133, 0.03029610177936822, 0.3566308328371091)),
            None, None);
        t_ast = node::Node::new(Some(point3(-22.790083507848195f64, 12.473857310034916, 10.514631403774104)),
            Some(Quaternion::new(0.9999590317878406f64, 0.007635050334856993, -0.0012676281267735207, -0.004694025057539509)),
            Some(vec3(0.23743170979346961f64, 0.23743170979346961, 0.23743170979346961)), None);       
        assert_eq!(asteroid.collision(&t_ast.mat(), &ship, &t_ship.mat(), &strat).is_some(), false);


        t_ship.pos = point3(-25.20556890402142, 34.70378892431485, 33.41363920806364);
        t_ship.orientation = Quaternion::new(0.20591263538805038, 0.27125974748507087, -0.09978353992557279, -0.9349124991900651);
        t_ast.pos = point3(-37.86042001868104, 17.471324865149157, 39.919355753951976);
        t_ast.orientation = Quaternion::new(0.7633320035030866, -0.41610255061356016, -0.27499327009999497, -0.4105625667307782);
        t_ast.scale = vec3(0.23288870438198583, 0.23288870438198583, 0.23288870438198583);
        assert_eq!(asteroid.collision(&t_ast.mat(), &ship, &t_ship.mat(), &strat).is_some(), false);
    }
}