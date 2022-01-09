use super::bvh::OBBTree;
use crate::node;
use std::rc::Rc;
use std::cell::RefCell;
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
    pub fn new(file: &str) -> CollisionMesh {
        let (meshes, _) = tobj::load_obj(file, &tobj::LoadOptions {
            triangulate: true,
            single_index: true,
            .. Default::default()
        }).unwrap();
        let meshes : Vec<OBBTree<f32>> = meshes.into_iter().map(|x| {
            let (verts, indices) = get_mesh_data(&x.mesh);
            println!("{} triangles", indices.len() / 3);
            OBBTree::from(indices, verts)
        }).collect();
        println!("Created mesh");
        CollisionMesh { sub_meshes: meshes }
    }

    pub fn collision(&self, self_transform: &Matrix4<f64>, other: &CollisionMesh,
        other_transform: &Matrix4<f64>) -> bool 
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
        println!("Triangle checks: {} x {}", our_tris.len(), other_tris.len());
        !our_tris.is_empty()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use once_cell::sync::Lazy;
    use cgmath::*;

    #[test]
    fn basic_collisions() {
        let plane_mesh = CollisionMesh::new("assets/Ships/StarSparrow01.obj");
        assert_eq!(plane_mesh.collision(&Matrix4::from_scale(1.), 
            &plane_mesh, &Matrix4::from_translation(vec3(3., 4., 1.))), false);
        assert_eq!(plane_mesh.collision(&Matrix4::from_scale(1.), 
            &plane_mesh, &Matrix4::from_translation(vec3(3., 3., 1.))), true);
        assert_eq!(plane_mesh.collision(&Matrix4::from_scale(1.), 
            &plane_mesh, &(Matrix4::from_translation(vec3(10.5, -0.5, 1.7)) * Matrix4::from_angle_y(Deg(-90f64)))
            ), false);
    }
}