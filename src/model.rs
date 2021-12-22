use tobj::*;

use glium::{Surface};
use crate::textures;
use crate::shader;

#[derive(Clone, Copy)]
pub struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    tex_coords: [f32; 2],
}

glium::implement_vertex!(Vertex, pos, normal, tex_coords);

pub struct Model {
    mesh_geom: Vec<MMesh>,

}

struct MMaterial {
    diffuse_tex: glium::texture::SrgbTexture2d,
    name: String,
}

struct MMesh {
    verts: glium::VertexBuffer<Vertex>,
    indices: glium::IndexBuffer<u32>,
    material: Option<MMaterial>,

}

fn get_mesh_data(mesh: &Mesh) -> (Vec<Vertex>, Vec<u32>) {
    let mut verts = Vec::<Vertex>::new();
    let indices = mesh.indices.clone();
    for idx in 0 .. mesh.positions.len() / 3 {
        let idx = idx as usize;
        let normal = if mesh.normals.is_empty() { [0f32, 0f32, 0f32] } else 
        { [mesh.normals[idx * 3], mesh.normals[idx * 3 + 1], mesh.normals[idx * 3 + 2]] };
        let texcoords = if mesh.texcoords.is_empty() { [0f32, 0f32] } else 
        { [mesh.texcoords[idx * 2], mesh.texcoords[idx * 2 + 1]] };
        let vert = Vertex {
            pos: [mesh.positions[idx * 3], mesh.positions[idx * 3 + 1], mesh.positions[idx * 3 + 2]],
            normal: normal,
            tex_coords: texcoords,
        };
        verts.push(vert);
    }
    (verts, indices)
}

fn get_material_data<F>(dir: &str, mat: &Material, facade: &F)
    -> MMaterial where F : glium::backend::Facade
{
    MMaterial {
        diffuse_tex: textures::load_tex2d_or_empty(&format!("{}{}", dir, mat.diffuse_texture), facade),
        name: mat.name.clone(),
    }
}

fn get_material_or_none<F>(dir: &str, mesh: &Mesh, mats: &Vec<Material>, facade: &F) 
    -> Option<MMaterial> where F : glium::backend::Facade
{
    match mesh.material_id {
        Some(id) => Some(get_material_data(dir, &mats[id], facade)),
        None => None,
    }
}

impl Model {
    pub fn load<F>(file: &str, facade: &F) -> Model where F : glium::backend::Facade {
        let (models, materials) = load_obj(file, &LoadOptions {
            triangulate: true,
            single_index: true,
            ..Default::default()
        }).unwrap();
        let mut meshes = Vec::<MMesh>::new();
        let mats = materials.unwrap();
        let dir = textures::dir_stem(file);
        for model in models {
            let (verts, indices) = get_mesh_data(&model.mesh);
            let mat = get_material_or_none(&dir, &model.mesh, &mats, facade);
            meshes.push(MMesh {
                verts: glium::VertexBuffer::new(facade, &verts).unwrap(),
                indices: glium::IndexBuffer::new(facade, glium::index::PrimitiveType::TrianglesList, &indices).unwrap(),
                material: mat,
            });

        }
        return Model {
            mesh_geom: meshes
        }
        
    }

    pub fn render(&self, wnd: &mut glium::Frame, mats: &shader::Matrices, model: [[f32; 4]; 4], manager: &shader::ShaderManager) {
        for mesh in &self.mesh_geom {
            let mut mat_name : String;
            let data = match &mesh.material {
                Some(MMaterial {diffuse_tex, name}) => {
                    mat_name = name.clone();
                    shader::UniformData {
                        diffuse_tex: diffuse_tex,
                        model: model,
                        matrices: mats,
                    }
                },
                _ => panic!("No material"),
            };
            let (shader, params, uniform) = manager.use_shader(&mat_name, &data);
            match uniform {
                shader::UniformType::ShipUniform(uniform) => 
                    wnd.draw(&mesh.verts, &mesh.indices, &shader, &uniform, &params),
                shader::UniformType::SkyboxUniform(uniform) => 
                    wnd.draw(&mesh.verts, &mesh.indices, &shader, &uniform, &params),
            }.unwrap()
        }
    }
}