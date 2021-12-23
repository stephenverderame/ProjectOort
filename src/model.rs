use tobj::*;
use std::collections::BTreeMap;
use std::io::BufRead;

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

struct PBRData {
    roughness_tex: glium::texture::Texture2d,
    metalness_tex: glium::texture::Texture2d,
}

struct MMaterial {
    diffuse_tex: glium::texture::SrgbTexture2d,
    name: String,
    pbr_data: Option<PBRData>,
    normal_tex: Option<glium::texture::Texture2d>,
    emission_tex: Option<glium::texture::SrgbTexture2d>,
    
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

fn get_pbr_data(dir: &str, mat_name: &str) -> Option<BTreeMap<String, String>> {
    match std::fs::File::open(format!("{}{}-pbr.yml", dir, mat_name)) {
        Ok(file) => {
            let mut map = BTreeMap::<String, String>::new();
            let line_iter = std::io::BufReader::new(file).lines();
            for line in line_iter {
                if let Ok(ln_str) = line {
                    let (key, val) = ln_str.split_at(ln_str.find(':').unwrap());
                    map.insert(key.trim().to_string(), val[1 .. val.len()].trim().to_string());
                }
            }
            Some(map)
        },
        _ => None,
    }
}

fn get_pbr_textures<F>(dir: &str, mat_name: &str, facade: &F) 
    -> Option<PBRData> where F : glium::backend::Facade 
{
    match get_pbr_data(dir, mat_name) {
        Some(tex_maps) => {
            println!("{}", tex_maps["roughness"]);
            println!("{}", tex_maps["metalness"]);
            Some(PBRData {
                roughness_tex: textures::load_texture_2d(&format!("{}{}", dir, tex_maps["roughness"]), facade),
                metalness_tex: textures::load_texture_2d(&format!("{}{}", dir, tex_maps["metalness"]), facade),
            })
        },
        _ => None,
    }
}

fn get_material_data<F>(dir: &str, mat: &Material, facade: &F)
    -> MMaterial where F : glium::backend::Facade
{
    if mat.name.find("pbr").is_some() {
        println!("{}", dir);
        println!("{}", mat.diffuse_texture);
        println!("{}", mat.normal_texture);
        println!("{}", mat.dissolve_texture);
    }
    MMaterial {
        diffuse_tex: textures::load_tex_srgb_or_empty(&format!("{}{}", dir, mat.diffuse_texture), facade),
        pbr_data: get_pbr_textures(dir, &mat.name, facade),
        normal_tex: if mat.normal_texture.is_empty() { None } else { 
            Some(textures::load_texture_2d(&format!("{}{}", dir, mat.normal_texture), facade))
        },
        emission_tex: if mat.dissolve_texture.is_empty() { None } else {
            Some(textures::load_texture_srgb(&format!("{}{}", dir, mat.dissolve_texture), facade))
        },
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

fn mat_to_uniform_data<'a>(material: &'a MMaterial, mats: &'a shader::SceneData, 
    model: [[f32; 4]; 4]) -> shader::UniformData<'a>
{
    shader::UniformData {
        diffuse_tex: Some(&material.diffuse_tex),
        model: model,
        scene_data: mats,
        roughness_map: match &material.pbr_data {
            Some(pbr) => Some(&pbr.roughness_tex),
            _ => None,
        },
        metallic_map: match &material.pbr_data {
            Some(pbr) => Some(&pbr.metalness_tex),
            _ => None,
        },
        normal_map: match &material.normal_tex {
            Some(tex) => Some(tex),
            _ => None,
        },
        emission_map: match &material.emission_tex {
            Some(tex) => Some(tex),
            _ => None,
        },
        env_map: None,
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

    pub fn render<S : glium::Surface>(&self, wnd: &mut S, mats: &shader::SceneData, model: [[f32; 4]; 4], manager: &shader::ShaderManager) {
        for mesh in &self.mesh_geom {
            let mat_name : String;
            let data = match &mesh.material {
                Some(mat) => {
                    mat_name = mat.name.clone();
                    mat_to_uniform_data(mat, mats, model)
                },
                _ => panic!("No material"),
            };
            let (shader, params, uniform) = manager.use_shader(&mat_name, &data);
            match uniform {
                shader::UniformType::BSUniform(uniform) => 
                    wnd.draw(&mesh.verts, &mesh.indices, &shader, &uniform, &params),
                shader::UniformType::PbrUniform(uniform) => 
                    wnd.draw(&mesh.verts, &mesh.indices, &shader, &uniform, &params),
                shader::UniformType::EqRectUniform(_) | shader::UniformType::SkyboxUniform(_) 
                 | shader::UniformType::UiUniform(_) => 
                    panic!("Model get invalid uniform type"),
            }.unwrap()
        }
    }
}