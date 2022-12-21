#![allow(clippy::unreadable_literal)]
use super::bvh::{CollisionVertex, OBBTree, TreeStopCriteria};
use super::highp_col::{HighPCollision, Hit};
use super::obb::{self, BoundingVolume};
use cgmath::*;

pub struct CollisionMesh {
    sub_meshes: Vec<OBBTree<f32>>,
}

fn get_mesh_data(mesh: &tobj::Mesh) -> (Vec<CollisionVertex<f32>>, Vec<u32>) {
    let mut verts = Vec::new();
    let indices = mesh.indices.clone();
    for idx in 0..mesh.positions.len() / 3 {
        let idx = idx as usize;
        let norm = if mesh.normals.is_empty() {
            vec3(0., 0., 0.)
        } else {
            vec3(
                mesh.normals[idx * 3],
                mesh.normals[idx * 3 + 1],
                mesh.normals[idx * 3 + 2],
            )
        };

        let pos = point3(
            mesh.positions[idx * 3],
            mesh.positions[idx * 3 + 1],
            mesh.positions[idx * 3 + 2],
        );
        verts.push(CollisionVertex { pos, norm });
    }
    (verts, indices)
}

impl CollisionMesh {
    pub fn new(file: &str, stop_method: TreeStopCriteria) -> Self {
        let (meshes, _) = tobj::load_obj(
            file,
            &tobj::LoadOptions {
                triangulate: true,
                single_index: true,
                ..Default::default()
            },
        )
        .unwrap();
        let meshes: Vec<OBBTree<f32>> = meshes
            .into_iter()
            .map(|x| {
                let (verts, indices) = get_mesh_data(&x.mesh);
                println!("{} triangles", indices.len() / 3);
                OBBTree::from(indices, verts, stop_method)
            })
            .collect();
        println!("Created mesh");
        Self { sub_meshes: meshes }
    }

    /// See `HighPStrat::collision`
    pub fn collision(
        &self,
        self_transform: &Matrix4<f64>,
        other: &Self,
        other_transform: &Matrix4<f64>,
        highp_strat: &dyn HighPCollision,
    ) -> Option<Hit> {
        let mut our_tris = Vec::new();
        let mut other_tris = Vec::new();
        for m in &self.sub_meshes {
            for i in &other.sub_meshes {
                if let Some((mut a, mut b)) =
                    m.collision(self_transform, i, other_transform)
                {
                    our_tris.append(&mut a);
                    other_tris.append(&mut b);
                }
            }
        }
        //println!("Triangle checks: {} x {}", our_tris.len(), other_tris.len());
        if our_tris.is_empty() || other_tris.is_empty() {
            None
        } else {
            highp_strat.collide(
                &our_tris,
                self_transform,
                &other_tris,
                other_transform,
            )
        }
    }

    /// Gets a sphere that encloses the entire collision mesh, in local space
    pub fn bounding_sphere(&self) -> (Point3<f64>, f64) {
        let mut c = vec3(0f64, 0., 0.);
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        let mut max_z = f64::MIN;
        for mesh in &self.sub_meshes {
            let aabb = mesh.bounding_box();
            assert!(matches!(aabb, obb::BoundingVolume::Aabb(_)));
            let center = aabb.center().to_vec();
            let extents = aabb.extents();
            c += center;
            max_x = max_x.max(center.x + extents.x);
            max_y = max_y.max(center.y + extents.y);
            max_z = max_z.max(center.z + extents.z);
        }
        c /= self.sub_meshes.len() as f64;
        let center = point3(c.x, c.y, c.z);
        let extents =
            vec3(max_x - center.x, max_y - center.y, max_z - center.z);
        (center, extents.x.max(extents.y.max(extents.z)))
    }

    /// Gets a tuple of the largest bounding volume in the tree and the leaf bounding volumes
    #[allow(dead_code)]
    pub fn main_and_leaf_boxes(
        &self,
    ) -> (Vec<obb::BoundingVolume>, Vec<obb::BoundingVolume>) {
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
    pub fn get_colliding_volumes(
        &self,
        self_transform: &Matrix4<f64>,
        other: &Self,
        other_transform: &Matrix4<f64>,
    ) -> (Vec<obb::BoundingVolume>, Vec<obb::BoundingVolume>) {
        let mut our_v = Vec::new();
        let mut other_v = Vec::new();
        for sb in &self.sub_meshes {
            for o_sb in &other.sub_meshes {
                let (mut our, mut other) = sb.get_colliding_volumes(
                    self_transform,
                    o_sb,
                    other_transform,
                );
                our_v.append(&mut our);
                other_v.append(&mut other);
            }
        }
        (our_v, other_v)
    }

    /// Gets the volume of the bounding box that encloses the entire tree
    pub fn aabb_volume(&self) -> f64 {
        let mut vol = 0.;
        for mesh in &self.sub_meshes {
            vol += mesh.bounding_box().vol();
        }
        vol
    }

    /// Iterates over all the vertices of this mesh
    pub fn forall_verts<F: FnMut(&CollisionVertex<f32>)>(&self, func: &mut F) {
        for mesh in &self.sub_meshes {
            mesh.forall_verts(func);
        }
    }

    /// Determines if there is a collision between this mesh and another bounding volume
    ///
    /// Performs no high precision collision detection
    pub fn bounding_volume_collision(
        &self,
        self_transform: &Matrix4<f64>,
        other: BoundingVolume,
        other_transform: &Matrix4<f64>,
    ) -> bool {
        let tree = OBBTree::from_volume(other);
        self.sub_meshes.iter().any(|m| {
            m.collision(self_transform, &tree, other_transform)
                .is_some()
        })
    }
}

#[cfg(test)]
mod test {
    use super::super::bvh;
    use super::*;
    use crate::cg_support::node;
    use crate::collisions::highp_col;
    use crate::graphics_engine::shader;
    use serial_test::serial;

    #[cfg(unix)]
    fn get_event_loop() -> glutin::event_loop::EventLoop<()> {
        use glutin::platform::unix::EventLoopExtUnix;
        glutin::event_loop::EventLoop::new_any_thread()
    }

    #[cfg(windows)]
    fn get_event_loop() -> glutin::event_loop::EventLoop<()> {
        use glutin::platform::windows::EventLoopExtWindows;
        glutin::event_loop::EventLoop::new_any_thread()
    }

    fn init() -> (shader::ShaderManager, glium::Display) {
        use glium::*;
        use glutin::window::WindowBuilder;
        use glutin::ContextBuilder;
        let e_loop = get_event_loop();
        let window_builder = WindowBuilder::new()
            .with_visible(false)
            .with_inner_size(glium::glutin::dpi::PhysicalSize::<u32> {
                width: 128,
                height: 128,
            });
        let wnd_ctx = ContextBuilder::new();
        let wnd_ctx = Display::new(window_builder, wnd_ctx, &e_loop).unwrap();
        gl::load_with(|s| wnd_ctx.gl_window().get_proc_address(s));
        (shader::ShaderManager::init(&wnd_ctx), wnd_ctx)
    }

    #[test]
    #[serial]
    fn bvh_collisions() {
        let method = highp_col::HighPNone {};
        let plane_mesh = CollisionMesh::new(
            "assets/Ships/StarSparrow01.obj",
            TreeStopCriteria::default(),
        );
        assert!(plane_mesh
            .collision(
                &Matrix4::from_scale(1.),
                &plane_mesh,
                &Matrix4::from_translation(vec3(3., 4., 1.)),
                &method
            )
            .is_none());
        assert!(plane_mesh
            .collision(
                &Matrix4::from_scale(1.),
                &plane_mesh,
                &Matrix4::from_translation(vec3(3., 3., 1.)),
                &method
            )
            .is_some());

        let mut t_ast = node::Node::default();
        let mut t_ship = node::Node::default();
        let asteroid = CollisionMesh::new(
            "assets/asteroid1/Asteroid.obj",
            TreeStopCriteria::default(),
        );
        t_ast = t_ast
            .pos(point3(
                11.743949077465658,
                19.97710245003765,
                16.749212434348635,
            ))
            .scale(vec3(
                0.2025860334703863,
                0.2025860334703863,
                0.2025860334703863,
            ))
            .rot(Quaternion::new(
                0.9213945937844363,
                0.07290361996544982,
                -0.10036152351261705,
                0.3682996461293077,
            ));

        t_ship = t_ship
            .pos(point3(
                10.87532384616883,
                11.219439807187339,
                2.8101362590310326,
            ))
            .rot(Quaternion::new(
                -0.0025265890813806324,
                -0.4370424383838227,
                -0.4747224546633822,
                -0.7639542620062583,
            ));
        assert!(asteroid
            .collision(&t_ast.mat(), &plane_mesh, &t_ship.mat(), &method)
            .is_some());

        t_ship = t_ship
            .pos(point3(
                -69.25926888264416,
                92.04805170691911,
                -11.333046808157235,
            ))
            .rot(Quaternion::new(
                0.6138717343783247,
                0.7250881014216262,
                -0.31210219023218827,
                0.0009806938413965951,
            ));

        t_ast = t_ast
            .pos(point3(
                -69.70801643768598,
                71.32888312152988,
                -19.245189840990307,
            ))
            .scale(vec3(
                0.29053803758358265,
                0.29053803758358265,
                0.29053803758358265,
            ))
            .rot(Quaternion::new(
                0.6138717343783247,
                0.7250881014216262,
                -0.31210219023218827,
                0.0009806938413965951,
            ));
        assert!(asteroid
            .collision(&t_ast.mat(), &plane_mesh, &t_ship.mat(), &method)
            .is_some());
        assert!(plane_mesh
            .collision(&t_ship.mat(), &asteroid, &t_ast.mat(), &method)
            .is_some());
    }

    #[test]
    #[serial]
    fn triangle_tests() {
        let (shader, _) = init();
        let strat = highp_col::TriangleTriangleGPU::new(&shader);
        let vertices = vec![
            CollisionVertex {
                pos: point3(1f32, 0., 0.),
                norm: vec3(0., 0., 1.),
            },
            CollisionVertex {
                pos: point3(0., 1., 0.),
                norm: vec3(0., 0., 1.),
            },
            CollisionVertex {
                pos: point3(-1., 0., 0.),
                norm: vec3(0., 0., 1.),
            },
        ];
        let triangle = bvh::Triangle::array_from(
            vec![0, 1, 2],
            std::ptr::addr_of!(vertices),
        );
        let mut t_b = node::Node::default();
        let mut t_a = node::Node::default();
        assert!(strat
            .collide(
                &triangle,
                &Matrix4::from_scale(1.),
                &triangle,
                &Matrix4::from_translation(vec3(0., 0., 1.))
            )
            .is_none()); //plane test reject
        t_b = t_b
            .rot(Matrix3::from_angle_y(Deg(70f64)).into())
            .pos(point3(0., 0., 0.8));
        assert!(strat
            .collide(&triangle, &t_a.mat(), &triangle, &t_b.mat())
            .is_some()); //small intersect
        t_a = t_a.pos(point3(1., 0., 0.));
        assert!(strat
            .collide(&triangle, &t_a.mat(), &triangle, &t_b.mat())
            .is_some()); //small intersect off origin
        t_b = t_b.pos(point3(-0.3, 0., 0.5));
        assert!(strat
            .collide(&triangle, &t_a.mat(), &triangle, &t_b.mat())
            .is_none()); //plane test pass, no intersect
        t_b = t_b
            .pos(point3(-0.3, 1., 0.5))
            .rot(Matrix3::from_angle_y(Deg(120f64)).into());
        t_a = t_a.rot(From::from(Euler::new(Deg(20f64), Deg(0.), Deg(53.))));
        assert!(strat
            .collide(&triangle, &t_a.mat(), &triangle, &t_b.mat())
            .is_none()); //plane test pass, no intersect (more transforms)
        t_a = t_a.scale(vec3(1.895f64, 1.895, 1.895));
        assert!(strat
            .collide(&triangle, &t_a.mat(), &triangle, &t_b.mat())
            .is_some()); // intersect via scale

        let vertices2 = vec![
            CollisionVertex {
                pos: point3(-0.292f32, -0.0536, 0.00074),
                norm: vec3(0., 0., 0.),
            },
            CollisionVertex {
                pos: point3(-0.392, 0.273, 0.296),
                norm: vec3(0., 0., 0.),
            },
            CollisionVertex {
                pos: point3(0.747, 0.515, 0.255),
                norm: vec3(0., 0., 0.),
            },
        ];
        t_a = t_a
            .pos(point3(0., 0f64, 0.))
            .rot(Matrix3::from_angle_x(Deg(0.)).into());
        let triangle2 = bvh::Triangle::array_from(
            vec![0, 1, 2],
            std::ptr::addr_of!(vertices2),
        );
        assert!(strat
            .collide(&triangle2, &t_a.mat(), &triangle, &t_b.mat())
            .is_none()); // random triangle no intersect
        t_a = t_a.pos(point3(-0.16618f64, 0.97175, 0.65434));
        assert!(strat
            .collide(&triangle2, &t_a.mat(), &triangle, &t_b.mat())
            .is_some()); // random triangle intersect
        t_a = node::Node::default();
        t_b = node::Node::default();
        t_a = t_a.pos(point3(1.06508f64, 0.559814, 0.));
        assert!(strat
            .collide(&triangle, &t_a.mat(), &triangle, &t_b.mat())
            .is_some()); //coplanar intersect
        t_a = t_a.pos(point3(3f64, 0., 0.));
        assert!(strat
            .collide(&triangle, &t_a.mat(), &triangle, &t_b.mat())
            .is_none()); //coplanar no intersect
    }

    #[test]
    #[serial]
    fn collision_tests() {
        let (shader, _) = init();
        let strat = highp_col::TriangleTriangleGPU::new(&shader);
        let ship = CollisionMesh::new(
            "assets/Ships/StarSparrow01.obj",
            TreeStopCriteria::AlwaysStop,
        );
        let asteroid = CollisionMesh::new(
            "assets/asteroid1/Asteroid.obj",
            TreeStopCriteria::AlwaysStop,
        );
        let mut t_ship = node::Node::new(
            Some(point3(-14.2537f64, 32.5402f64, -39.6763)),
            Some(Quaternion::new(0.016473, 0.091357, 0.971325, 0.218883)),
            None,
            None,
        );
        let mut t_ast = node::Node::new(
            Some(point3(-7.56163f64, 50.2958, -20.4725)),
            Some(Quaternion::new(0.142821f64, 0.96663, -0.146196, -0.154451)),
            Some(vec3(0.375723f64, 0.375723, 0.375723)),
            None,
        );

        assert!(asteroid
            .collision(&t_ast.mat(), &ship, &t_ship.mat(), &strat)
            .is_none());

        t_ship = node::Node::new(
            Some(point3(
                -3.095402600732103f64,
                9.244842371955391,
                8.95973740527222,
            )),
            Some(Quaternion::new(
                0.8425839759656641f64,
                -0.4024286753361133,
                0.03029610177936822,
                0.3566308328371091,
            )),
            None,
            None,
        );
        t_ast = node::Node::new(
            Some(point3(
                -22.790083507848195f64,
                12.473857310034916,
                10.514631403774104,
            )),
            Some(Quaternion::new(
                0.9999590317878406f64,
                0.007635050334856993,
                -0.0012676281267735207,
                -0.004694025057539509,
            )),
            Some(vec3(
                0.23743170979346961f64,
                0.23743170979346961,
                0.23743170979346961,
            )),
            None,
        );
        assert!(asteroid
            .collision(&t_ast.mat(), &ship, &t_ship.mat(), &strat)
            .is_none());

        t_ship = t_ship.pos(point3(
            -25.20556890402142,
            34.70378892431485,
            33.41363920806364,
        ));
        t_ship = t_ship.rot(Quaternion::new(
            0.20591263538805038,
            0.27125974748507087,
            -0.09978353992557279,
            -0.9349124991900651,
        ));
        t_ast = t_ast.pos(point3(
            -37.86042001868104,
            17.471324865149157,
            39.919355753951976,
        ));
        t_ast = t_ast.rot(Quaternion::new(
            0.7633320035030866,
            -0.41610255061356016,
            -0.27499327009999497,
            -0.4105625667307782,
        ));
        t_ast = t_ast.scale(vec3(
            0.23288870438198583,
            0.23288870438198583,
            0.23288870438198583,
        ));
        assert!(asteroid
            .collision(&t_ast.mat(), &ship, &t_ship.mat(), &strat)
            .is_none());

        t_ast = node::Node::default();
        t_ship = node::Node::default();
        t_ast = t_ast
            .pos(point3(
                11.743949077465658,
                19.97710245003765,
                16.749212434348635,
            ))
            .scale(vec3(
                0.2025860334703863,
                0.2025860334703863,
                0.2025860334703863,
            ))
            .rot(Quaternion::new(
                0.9213945937844363,
                0.07290361996544982,
                -0.10036152351261705,
                0.3682996461293077,
            ));

        t_ship = t_ship
            .pos(point3(
                10.87532384616883,
                11.219439807187339,
                2.8101362590310326,
            ))
            .rot(Quaternion::new(
                -0.0025265890813806324,
                -0.4370424383838227,
                -0.4747224546633822,
                -0.7639542620062583,
            ));
        assert!(asteroid
            .collision(&t_ast.mat(), &ship, &t_ship.mat(), &strat)
            .is_some());

        t_ship = t_ship
            .pos(point3(
                -69.25926888264416,
                92.04805170691911,
                -11.333046808157235,
            ))
            .rot(Quaternion::new(
                0.6138717343783247,
                0.7250881014216262,
                -0.31210219023218827,
                0.0009806938413965951,
            ));

        t_ast = t_ast
            .pos(point3(
                -69.70801643768598,
                71.32888312152988,
                -19.245189840990307,
            ))
            .scale(vec3(
                0.29053803758358265,
                0.29053803758358265,
                0.29053803758358265,
            ))
            .rot(Quaternion::new(
                0.6138717343783247,
                0.7250881014216262,
                -0.31210219023218827,
                0.0009806938413965951,
            ));
        assert!(asteroid
            .collision(&t_ast.mat(), &ship, &t_ship.mat(), &strat)
            .is_some());
    }
}
