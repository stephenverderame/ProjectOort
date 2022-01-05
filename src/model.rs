extern crate assimp;
extern crate assimp_sys;
extern crate tobj;

use assimp::*;
use crate::textures;
use crate::shader;
use std::collections::BTreeMap;
use std::io::BufRead;

#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    tex_coords: [f32; 2],
    tangent: [f32; 3],
}
glium::implement_vertex!(Vertex, pos, normal, tex_coords, tangent);

/// Assimp Vector3D to f32 array
fn to_v3(v: Vector3D) -> [f32; 3] {
    [(*v).x, (*v).y, (*v).z]
}
/// Takes the `x` and `y` coordinates of an assimp `Vector3D`
fn to_v2(v: Vector3D) -> [f32; 2] {
    [(*v).x, (*v).y]
}

/// Creates a OpenGL vbo and ebo for the vertices and indices
fn get_vbo_ebo<F : glium::backend::Facade>(verts: Vec<Vertex>, indices: Vec<u32>, ctx: &F) 
    -> (glium::VertexBuffer<Vertex>, glium::IndexBuffer<u32>) 
{
    (glium::VertexBuffer::immutable(ctx, &verts).unwrap(),
    glium::IndexBuffer::immutable(ctx, glium::index::PrimitiveType::TrianglesList, &indices).unwrap())
}

struct PBRData {
    roughness_tex: glium::texture::Texture2d,
    metalness_tex: glium::texture::Texture2d,
    ao_tex: Option<glium::texture::Texture2d>,
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
/// Texture information for a mesh
/// Currently, a material can only have 1 texture of each type
pub struct Material {
    diffuse_tex: Option<glium::texture::SrgbTexture2d>,
    name: String,
    pbr_data: Option<PBRData>,
    normal_tex: Option<glium::texture::Texture2d>,
    emission_tex: Option<glium::texture::SrgbTexture2d>,
}
/// Gets a mutable nullptr
fn null<T>() -> *mut T {
    0 as *mut T
}
/// Gets a constant nullptr
fn null_c<T>() -> *const T {
    0 as *const T
}

impl Material {
    /// Finds all textures of the specified type and loads them using the specified function
    /// 
    /// `dir` - main directory of model where texture paths are relative to
    /// 
    /// `load_func` - function taking the path to the texture as a String and returning the loaded texture
    fn get_textures<T, G : Fn(String) -> T>(mat: &assimp_sys::AiMaterial, tex_type: assimp_sys::AiTextureType, 
        dir: &str, load_func: &G) -> Vec<T>
    {
        let mut path = assimp_sys::AiString::default();
        let tex_num = unsafe { assimp_sys::aiGetMaterialTextureCount(mat as *const assimp_sys::AiMaterial, tex_type) };
        let mut textures = Vec::<T>::new();
        for i in 0 .. tex_num {
            unsafe { 
                assimp_sys::aiGetMaterialTexture(mat as *const assimp_sys::AiMaterial, tex_type, i, &mut path as *mut assimp_sys::AiString,
                    null_c(), null(), null(), null(), null(), null()); 
            }
            let tex = format!("{}{}", dir, String::from_utf8_lossy(&path.data[.. path.length]));
            println!("Assimp loaded: {}", tex);
            textures.push(load_func(tex));
            
        }
        textures
    }
    /// Gets a material property with the key `property` as a utf8 string
    fn get_property(mat: &assimp_sys::AiMaterial, property: &str) -> Option<String> {
        for i in 0 .. mat.num_properties {
            let prop = unsafe {&**mat.properties.add(i as usize)};
            if prop.key.data[.. prop.key.length] == *property.as_bytes() {
                let len = prop.data_length as usize;
                let mut res = Vec::<u8>::new();
                res.resize(len + 1, 0);
                unsafe { std::ptr::copy_nonoverlapping(prop.data as *const u8, res.as_mut_ptr(), len); }
                return Some(String::from_utf8_lossy(&res).into_owned());

            }
        }
        None
    }
    /// Creates a material from an Assimp material
    /// 
    /// `dir` - the directory of the model file where textures are relative to
    pub fn new<F : glium::backend::Facade>(mat: &assimp_sys::AiMaterial, dir: &str, ctx: &F) -> Material {
        let load_srgb = |path: String| textures::load_texture_srgb(&path, ctx);
        let load_rgb = |path: String| textures::load_texture_2d(&path, ctx);
        let mut diffuse = Material::get_textures(mat, assimp_sys::AiTextureType::Diffuse, dir, &load_srgb);
        let mut emissive = Material::get_textures(mat, assimp_sys::AiTextureType::Emissive, dir, &load_srgb);
        let mut normal = Material::get_textures(mat, assimp_sys::AiTextureType::Normals, dir, &load_rgb);
        let mut ao = Material::get_textures(mat, assimp_sys::AiTextureType::Lightmap, dir, &load_rgb);
        let name = Material::get_property(mat, "?mat.name").expect("No material name!");
        let pbr = get_pbr_textures(dir, &name, ctx).map(|mut pbr| {
            if pbr.ao_tex.is_none() && ao.len() > 0 {
                pbr.ao_tex = Some(ao.swap_remove(0));
            }
            pbr
        });
        Material {
            diffuse_tex: Some(diffuse.swap_remove(0)),
            name, pbr_data: pbr,
            normal_tex: if normal.len() > 0 { Some(normal.swap_remove(0)) } else { None },
            emission_tex: if emissive.len() > 0 { Some(emissive.swap_remove(0)) } else { None },
        }

    }

    /// For some reason Assimp is having trouble loading mtl data from obj files
    /// Creates a material from the tobj material loaded from an mtl file
    /// 
    /// `dir` - the directory of the model file where textures are relative to
    pub fn from_mtl<F : glium::backend::Facade>(mat: &tobj::Material, dir: &str, ctx: &F) -> Material {
        Material {
            diffuse_tex: if mat.diffuse_texture.is_empty() { None } else {
                Some(textures::load_texture_srgb(&format!("{}{}", dir, mat.diffuse_texture), ctx))
            },
            pbr_data: get_pbr_textures(dir, &mat.name, ctx),
            normal_tex: if mat.normal_texture.is_empty() { None } else { 
                Some(textures::load_texture_2d(&format!("{}{}", dir, mat.normal_texture), ctx))
            },
            emission_tex: mat.unknown_param.get("map_Ke").map(|x| {
                println!("Ke: {}", x);
                textures::load_texture_srgb(&format!("{}{}", dir, x), ctx)
            }),
            name: mat.name.clone(),
        }
    }

    /// Converts the material to shader uniform arguments
    /// 
    /// `instancing` - if instanced rendering is being used
    /// 
    /// `model` - model matrix if available. If `None`, the identity matrix is used
    pub fn to_uniform_args(&self, instancing: bool, model: Option<[[f32; 4]; 4]>) -> shader::UniformInfo {
        match &self.name[..] {
            "Laser" => shader::UniformInfo::LaserInfo,
            x if x.find("pbr").is_some() => shader::UniformInfo::PBRInfo(shader::PBRData {
                diffuse_tex: self.diffuse_tex.as_ref().unwrap(),
                model: model.unwrap_or_else(|| cgmath::Matrix4::from_scale(1f32).into()),
                roughness_map: self.pbr_data.as_ref().map(|data| { &data.roughness_tex }),
                metallic_map: self.pbr_data.as_ref().map(|data| { &data.metalness_tex }),
                normal_map: self.normal_tex.as_ref(),
                emission_map: self.emission_tex.as_ref(),
                ao_map: self.pbr_data.as_ref().and_then(|data| { data.ao_tex.as_ref() }),
                instancing,
            }),
            x => panic!("Unimplemented texture with name: {}", x),
        }  
    }
}

/// A component of a model with its own material, vertices, and indices
/// Currently, every mesh face must be a triangle
pub struct Mesh {
    vbo: glium::VertexBuffer<Vertex>,
    ebo: glium::IndexBuffer<u32>,
    mat_idx: usize,
}

impl Mesh {
    pub fn new<F : glium::backend::Facade>(mesh: &assimp::Mesh, ctx: &F) -> Mesh {
        let mut vertices = Vec::<Vertex>::new();
        let mut indices = Vec::<u32>::new();
        for (vert, norm, tex_coord, tan, _bitan) in mesh.vertex_iter().zip(mesh.normal_iter()).zip(mesh.texture_coords_iter(0))
            .zip(mesh.tangent_iter()).zip(mesh.bitangent_iter()).map(|((((v, n), t), ta), bi)| (v, n, t, ta, bi))
        {
            vertices.push(Vertex {
                pos: to_v3(vert),
                normal: to_v3(norm),
                tex_coords: to_v2(tex_coord),
                tangent: to_v3(tan),
                //bitangent: to_v3(bitan),
            });
        }
        for face in mesh.face_iter() {
            for idx in 0 .. (*face).num_indices {
                unsafe { indices.push(*(*face).indices.add(idx as usize)); }
            }
        }
        let (vbo, ebo) = get_vbo_ebo(vertices, indices, ctx);
        Mesh { vbo, ebo, mat_idx: (*mesh).material_index as usize }
    }

    /// Gets the correct shader program, parameters, and uniform from the shader manager based on this mesh's material. The calls
    /// the supplied draw function with these arguments and the mesh's vbo and ebo
    fn render_helper<F>(&self, scene_data: &shader::SceneData, manager: &shader::ShaderManager, local_data: &shader::PipelineCache,
        model: Option<[[f32; 4]; 4]>, instancing: bool, mats: &Vec<Material>, mut draw_func: F) 
        where F : FnMut(&glium::VertexBuffer<Vertex>, &glium::IndexBuffer<u32>, &glium::Program, &glium::DrawParameters, &shader::UniformType), 
    {
        let mat = mats[self.mat_idx.min(mats.len() - 1)].to_uniform_args(instancing, model);
        let (shader, params, uniform) = manager.use_shader(&mat, Some(scene_data), Some(local_data));
        draw_func(&self.vbo, &self.ebo, shader, &params, &uniform)
    }

    pub fn render<S : glium::Surface>(&self, wnd: &mut S, mats: &shader::SceneData, local_data: &shader::PipelineCache, model: [[f32; 4]; 4], 
        manager: &shader::ShaderManager, materials: &Vec<Material>) {
        self.render_helper(mats, manager, local_data, Some(model), false, materials,
        |vbo, ebo, shader, params, uniform| {
            match uniform {
               shader::UniformType::LaserUniform(uniform) => 
                    wnd.draw(vbo, ebo, shader, uniform, params),
                shader::UniformType::PbrUniform(uniform) => 
                    wnd.draw(vbo, ebo, shader, uniform, params),
                shader::UniformType::DepthUniform(uniform) =>
                    wnd.draw(vbo, ebo, shader, uniform, params),
                shader::UniformType::EqRectUniform(_) | shader::UniformType::SkyboxUniform(_) 
                 | shader::UniformType::UiUniform(_) | shader::UniformType::SepConvUniform(_) 
                 | shader::UniformType::ExtractBrightUniform(_) 
                 | shader::UniformType::PrefilterHdrEnvUniform(_)
                 | shader::UniformType::BrdfLutUniform(_) => 
                    panic!("Model get invalid uniform type"),
            }.unwrap()
        });
    }

    pub fn render_instanced<S : glium::Surface, T : Copy>(&self, wnd: &mut S, mats: &shader::SceneData, local_data: &shader::PipelineCache, manager: &shader::ShaderManager, 
        instance_buffer: &glium::vertex::VertexBufferSlice<T>, materials: &Vec<Material>) 
    {
        self.render_helper(mats, manager, local_data, None, true, materials,
        |vbo, ebo, shader, params, uniform| {
            match uniform {
               shader::UniformType::LaserUniform(uniform) => 
                    wnd.draw((vbo, instance_buffer.per_instance().unwrap()), 
                        ebo, shader, uniform, params),
                shader::UniformType::PbrUniform(uniform) => 
                    wnd.draw((vbo, instance_buffer.per_instance().unwrap()), 
                        ebo, shader, uniform, params),
                shader::UniformType::DepthUniform(uniform) =>
                    wnd.draw((vbo, instance_buffer.per_instance().unwrap()), 
                        ebo, shader, uniform, params),
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
/// 
/// * **Lasers** - Materials with the name "Laser" are lasers. These are objects that are simply colored
/// with one uniform color and do not use textures
pub struct Model {
    meshes: Vec<Mesh>,
    materials: Vec<Material>,
}

impl Model {
    fn process_node<F : glium::backend::Facade>(node: assimp::Node, scene: &Scene, ctx: &F) -> Vec<Mesh> {
        let mut meshes = Vec::<Mesh>::new();
        for i in 0 .. node.num_meshes() {
            let mesh = scene.mesh(i as usize).unwrap();
            meshes.push(Mesh::new(&mesh, ctx));
        }
        for n in node.child_iter() {
            meshes.append(&mut Model::process_node(n, scene, ctx));
        }
        meshes
    }

    /// Use assimp to load all scene materials
    fn process_mats<F : glium::backend::Facade>(scene: &Scene, dir: &str, ctx: &F) -> Vec<Material> {
        scene.material_iter().map(|x| Material::new(&*x, dir, ctx)).collect()
    }

    /// Assimp is being weird with mtl files. If we load an obj file, use tobj to load
    /// its corresponding material file
    fn process_obj_mats<F : glium::backend::Facade>(path: &str, ctx: &F) -> Vec<Material> {
        let dir = textures::dir_stem(path);
        let (mats, _) = tobj::load_mtl(path.replace(".obj", ".mtl")).unwrap();
        mats.iter().map(|x| Material::from_mtl(&*x, &dir, ctx)).collect()
        
    }

    pub fn new<F : glium::backend::Facade>(path: &str, ctx: &F) -> Model {
        let mut importer = Importer::new();
        importer.join_identical_vertices(true);
        importer.triangulate(true);
        //importer.flip_uvs(true);
        importer.optimize_meshes(true);
        importer.calc_tangent_space(|mut tan_space_args| {
            tan_space_args.enable = true;
        });
        let scene = importer.read_file(path).unwrap();
        assert_eq!(scene.is_incomplete(), false);
        let meshes = Model::process_node(scene.root_node(), &scene, ctx);
        let materials = if path.find(".obj").is_some() {
            Model::process_obj_mats(path, ctx)
        } else {
            Model::process_mats(&scene, &textures::dir_stem(path), ctx)
        };
        Model { meshes, materials }
    }

    /// Render this model with the given scene and pipeline data and model matrix
    pub fn render<S : glium::Surface>(&self, wnd: &mut S, mats: &shader::SceneData, local_data: &shader::PipelineCache, model: [[f32; 4]; 4], 
        manager: &shader::ShaderManager) 
    {
        for mesh in &self.meshes {
            mesh.render(wnd, mats, local_data, model, manager, &self.materials);
        }
    }

    /// Render multiple instances of this model
    /// 
    /// `instance_buffer` - VertexBuffer where each element in it is passed to each rendered copy of this model. So this will render an amount of copies equal to elements
    /// in this buffer
    pub fn render_instanced<S : glium::Surface, T : Copy>(&self, wnd: &mut S, mats: &shader::SceneData, local_data: &shader::PipelineCache, manager: &shader::ShaderManager, 
        instance_buffer: glium::vertex::VertexBufferSlice<T>) 
    {
        for mesh in &self.meshes {
            mesh.render_instanced(wnd, mats, local_data, manager, &instance_buffer, &self.materials);
        }
    }
}