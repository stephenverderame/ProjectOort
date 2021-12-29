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

/// A model is geometry loaded from the filesystem
/// Each model must have a main obj file with a material file at the specified path
/// relative to the obj file's directory. The name of the material controls which
/// shader is used for it. Texture files specified in the material file are relative to the obj file's
/// directory.
/// 
/// # Special Materials
/// 
/// * **PBR** - Materials that contain "pbr" are PBR materials. PBR textures are loaded from the file
/// `[material_name]-pbr.yml` which is expected to be in the same directory as the main obj file. This file must define
/// a `roughness`, `metalness`, and optionally, `ao` parameter. Once again, these textures should be relative to the 
/// obj file's directory
pub struct Model {
    mesh_geom: Vec<MMesh>,

}

struct PBRData {
    roughness_tex: glium::texture::Texture2d,
    metalness_tex: glium::texture::Texture2d,
    ao_tex: Option<glium::texture::Texture2d>,
}

/// Material data for a mesh
struct MMaterial {
    diffuse_tex: Option<glium::texture::SrgbTexture2d>,
    name: String,
    pbr_data: Option<PBRData>,
    normal_tex: Option<glium::texture::Texture2d>,
    emission_tex: Option<glium::texture::SrgbTexture2d>,
    
}

/// A Mesh is a part of a model with its own vao and material
struct MMesh {
    verts: glium::VertexBuffer<Vertex>,
    indices: glium::IndexBuffer<u32>,
    material: Option<MMaterial>,

}

/// Gets the vertices and indices from a TOBJ mesh
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

/// Reads pbr textures from an externam file names `[mat_name]-pbr.yml` that resides in
/// directory `dir`
/// 
/// Configuration information in this file must be specified in `key: value` pairs with each
/// key being on a separate line
/// 
/// Returns the map of key value pairs as strings
fn get_pbr_data(dir: &str, mat_name: &str) -> Option<BTreeMap<String, String>> {
    let file = format!("{}{}-pbr.yml", dir, mat_name);
    println!("{}", file);
    match std::fs::File::open(file) {
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

/// Loads the pbr textures for the material with name `mat_name` which has a home
/// directory of `dir`
fn get_pbr_textures<F>(dir: &str, mat_name: &str, facade: &F) 
    -> Option<PBRData> where F : glium::backend::Facade 
{
    match get_pbr_data(dir, mat_name) {
        Some(tex_maps) => {
            println!("{}", tex_maps["roughness"]);
            println!("{}", tex_maps["metalness"]);
            if tex_maps.contains_key("ao") {
                println!("ao: {}", tex_maps["ao"]);
            }
            Some(PBRData {
                roughness_tex: textures::load_texture_2d(&format!("{}{}", dir, tex_maps["roughness"]), facade),
                metalness_tex: textures::load_texture_2d(&format!("{}{}", dir, tex_maps["metalness"]), facade),
                ao_tex: if tex_maps.contains_key("ao") {
                    Some(textures::load_texture_2d(&format!("{}{}", dir, tex_maps["ao"]), facade))
                } else { None },
            })
        },
        _ => None,
    }
}

/// Creates an `MMaterial` from a TOBJ `Material` and loads extra data
/// from other files, if any exists
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
        diffuse_tex: if mat.diffuse_texture.is_empty() { None } else {
            Some(textures::load_texture_srgb(&format!("{}{}", dir, mat.diffuse_texture), facade))
        },
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

/// Converts a material to its relevant UniformType based on what material information is present
fn mat_to_uniform_data<'a>(material: &'a MMaterial, mats: &'a shader::SceneData, 
    model: Option<[[f32; 4]; 4]>) -> shader::UniformInfo<'a>
{
    match &material.name[..] {
        "Laser" => shader::UniformInfo::LaserInfo(shader::LaserData {
            scene_data: mats,
        }),
        x if x.find("pbr").is_some() => shader::UniformInfo::PBRInfo(shader::PBRData {
            diffuse_tex: material.diffuse_tex.as_ref().unwrap(),
            model: model.unwrap(),
            scene_data: mats,
            roughness_map: material.pbr_data.as_ref().map(|data| { &data.roughness_tex }),
            metallic_map: material.pbr_data.as_ref().map(|data| { &data.metalness_tex }),
            normal_map: material.normal_tex.as_ref(),
            emission_map: material.emission_tex.as_ref(),
            ao_map: material.pbr_data.as_ref().and_then(|data| { data.ao_tex.as_ref() }),
        }),
        x => panic!("Unimplemented texture with name: {}", x),
    }   
}

impl Model {
    /// Loads a model from `file`. If any extra data exists for the model, it must reside in the same directory
    /// as the corresponding `.mtl` file for the mesh
    pub fn load<F>(file: &str, facade: &F) -> Model where F : glium::backend::Facade {
        let (models, materials) = load_obj(file, &LoadOptions {
            triangulate: true,
            single_index: true,
            ..Default::default()
        }).expect(&format!("Could not open model file '{}'", file));
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

    fn render_helper<F, S>(&self, scene_data: &shader::SceneData, manager: &shader::ShaderManager, model: Option<[[f32; 4]; 4]>,
        surface: &mut S, draw_func: F) where F : Fn(&MMesh, &glium::Program, &glium::DrawParameters, &shader::UniformType, &mut S), S : glium::Surface
    {
        for mesh in &self.mesh_geom {
            let data = match &mesh.material {
                Some(mat) => {
                    mat_to_uniform_data(mat, scene_data, model)
                },
                _ => panic!("No material"),
            };
            let (shader, params, uniform) = manager.use_shader(&data);
            draw_func(mesh, shader, params, &uniform, surface);
        }
    }

    pub fn render<S : glium::Surface>(&self, wnd: &mut S, mats: &shader::SceneData, model: [[f32; 4]; 4], manager: &shader::ShaderManager) {
        self.render_helper(mats, manager, Some(model), wnd,
        |mesh, shader, params, uniform, surface| {
            match uniform {
               shader::UniformType::LaserUniform(uniform) => 
                    surface.draw(&mesh.verts, &mesh.indices, &shader, uniform, &params),
                shader::UniformType::PbrUniform(uniform) => 
                    surface.draw(&mesh.verts, &mesh.indices, &shader, uniform, &params),
                shader::UniformType::EqRectUniform(_) | shader::UniformType::SkyboxUniform(_) 
                 | shader::UniformType::UiUniform(_) | shader::UniformType::SepConvUniform(_) 
                 | shader::UniformType::ExtractBrightUniform(_) 
                 | shader::UniformType::PrefilterHdrEnvUniform(_)
                 | shader::UniformType::BrdfLutUniform(_) => 
                    panic!("Model get invalid uniform type"),
            }.unwrap()
        });
    }

    pub fn render_instanced<S : glium::Surface, T : Copy>(&self, wnd: &mut S, mats: &shader::SceneData, manager: &shader::ShaderManager, 
        instance_buffer: glium::vertex::VertexBufferSlice<T>) 
    {
        self.render_helper(mats, manager, None, wnd,
        |mesh, shader, params, uniform, surface| {
            match uniform {
               shader::UniformType::LaserUniform(uniform) => 
                    surface.draw((&mesh.verts, instance_buffer.per_instance().unwrap()), 
                        &mesh.indices, &shader, uniform, &params),
                shader::UniformType::PbrUniform(uniform) => 
                    surface.draw((&mesh.verts, instance_buffer.per_instance().unwrap()), 
                        &mesh.indices, &shader, uniform, &params),
                shader::UniformType::EqRectUniform(_) | shader::UniformType::SkyboxUniform(_) 
                 | shader::UniformType::UiUniform(_) | shader::UniformType::SepConvUniform(_) 
                 | shader::UniformType::ExtractBrightUniform(_) 
                 | shader::UniformType::PrefilterHdrEnvUniform(_)
                 | shader::UniformType::BrdfLutUniform(_) => 
                    panic!("Model get invalid uniform type"),
            }.unwrap()
        });
    }
}