extern crate assimp;
extern crate assimp_sys;
extern crate tobj;

use assimp::*;
use crate::textures;
use crate::shader;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::io::BufRead;
use cgmath::*;
use cgmath::Quaternion;
use std::cell::RefCell;
use std::rc::Rc;

const MAX_BONES_PER_VERTEX : usize = 4;

#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    tex_coords: [f32; 2],
    tangent: [f32; 3], 
    // don't need bitangent since we can compute that as normal x tangent
    bone_ids: [i32; MAX_BONES_PER_VERTEX],
    bone_weights: [f32; MAX_BONES_PER_VERTEX],
}
glium::implement_vertex!(Vertex, pos, normal, tex_coords, tangent);

/// Assimp Vector3D to f32 array
fn to_v3(v: Vector3D) -> Vector3<f32> {
    vec3((*v).x, (*v).y, (*v).z)
}
/// Takes the `x` and `y` coordinates of an assimp `Vector3D`
fn to_v2(v: Vector3D) -> [f32; 2] {
    [(*v).x, (*v).y]
}

fn to_m4(m: assimp_sys::AiMatrix4x4) -> cgmath::Matrix4<f64> {
    cgmath::Matrix4::new(m.a1, m.b1, m.c1, m.d1, m.a2, m.b2, m.c2, m.d2,
        m.a3, m.b3, m.c3, m.d3, m.a4, m.b4, m.c4, m.d4).cast().unwrap()
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
pub struct Bone {
    pub id: i32,
    /// Matrix to transform a vector into bone space
    pub offset_matrix: Matrix4<f64>,
}

/// Stores the sequence of keyframes for a particular bone
pub struct BoneAnim {
    /// vector of pos keyframes and tick tuples
    positions: Vec<(Vector3<f64>, f64)>,
    rotations: Vec<(Quaternion<f64>, f64)>,
    scales: Vec<(Vector3<f64>, f64)>,
    id: i32,
    last_pos: RefCell<usize>, //interior mutability
    last_rot: RefCell<usize>,
    last_scale: RefCell<usize>,
}

impl BoneAnim {
    /// `id` - the id of the given Bone this represents
    pub fn new(id: i32, anim: &scene::NodeAnim) -> BoneAnim {
        let mut positions = Vec::<(Vector3<f64>, f64)>::new();
        let mut scales = Vec::<(Vector3<f64>, f64)>::new();
        let mut rotations = Vec::<(Quaternion<f64>, f64)>::new();
        for pos_idx in 0 .. (*anim).num_position_keys {
            let key = anim.get_position_key(pos_idx as usize).unwrap();
            positions.push((vec3(key.value.x, key.value.y, key.value.z).cast().unwrap(), key.time));
        }
        for rot_idx in 0 .. (*anim).num_rotation_keys {
            let key = anim.get_rotation_key(rot_idx as usize).unwrap();
            rotations.push((Quaternion::new(key.value.w, key.value.x, key.value.y, key.value.z).cast().unwrap(), key.time));
        }
        for scale_idx in 0 .. (*anim).num_scaling_keys {
            let key = anim.get_scaling_key(scale_idx as usize).unwrap();
            scales.push((vec3(key.value.x, key.value.y, key.value.z).cast().unwrap(), key.time));
        }
        BoneAnim {
            positions, rotations, scales, id,
            last_pos: RefCell::new(0), last_rot: RefCell::new(0), last_scale: RefCell::new(0),
        }
    }

    /// Gets the last keyframe, next keyframe, `0 - 1` factor to lerp between the two, and index of last keyframe
    /// Requires that `anim_time >= vec[last_idx].1`
    /// 
    /// `last_idx` - the last used keyframe in `vec`. Will start searching for the next keyframe in `vec` from
    /// `last_idx`. Panics if there is no next keyframe
    fn get_last_next_lerp<T : Clone>(vec: &Vec<(T, f64)>, anim_time: f64, last_idx: usize) -> (T, T, f64, usize) {
        for idx in last_idx.max(1) .. vec.len() {
            if anim_time < vec[idx].1  {
                let lerp_fac = (anim_time - vec[idx - 1].1) / (vec[idx].1 - vec[idx - 1].1);
                return (vec[idx - 1].0.clone(), vec[idx].0.clone(), lerp_fac, idx - 1);
            }
        }
        panic!("Animation out of bounds!")
    }

    /// Interpolates to get current position for `anim_ticks`. Updates 
    /// `self.last_pos`, looping if necessary
    fn get_cur_pos(&self, anim_ticks: f64) -> Vector3<f64> {
        if self.positions.len() == 1 { return self.positions[0].0; }
        if anim_ticks < self.positions[*self.last_pos.borrow()].1 {
            *self.last_pos.borrow_mut() = 0;
        }
        let (last, next, lerp_fac, last_idx) 
            = BoneAnim::get_last_next_lerp(&self.positions, anim_ticks, *self.last_pos.borrow());
        *self.last_pos.borrow_mut() = last_idx;
        next * lerp_fac + last * (1f64 - lerp_fac)
    }

    /// Interpolates to get current scale for `anim_ticks`. Updates 
    /// `self.last_scale`, looping if necessary
    fn get_cur_scale(&self, anim_ticks: f64) -> Vector3<f64> {
        if self.scales.len() == 1 { return self.scales[0].0; }
        if anim_ticks < self.scales[*self.last_scale.borrow()].1 {
            *self.last_scale.borrow_mut() = 0;
        }
        let (last, next, lerp_fac, last_idx) 
            = BoneAnim::get_last_next_lerp(&self.scales, anim_ticks, *self.last_scale.borrow());
        *self.last_scale.borrow_mut() = last_idx;
        next * lerp_fac + last * (1f64 - lerp_fac)
    }

    /// Interpolates to get current rotation for `anim_ticks`. Updates 
    /// `self.last_rot`, looping if necessary
    fn get_cur_rot(&self, anim_ticks: f64) -> Quaternion<f64> {
        if self.rotations.len() == 1 { return self.rotations[0].0; }
        if anim_ticks < self.rotations[*self.last_rot.borrow()].1 {
            *self.last_rot.borrow_mut() = 0;
        }
        let (last, next, lerp_fac, last_idx) 
            = BoneAnim::get_last_next_lerp(&self.rotations, anim_ticks, *self.last_rot.borrow());
        *self.last_rot.borrow_mut() = last_idx;
        last.normalize().slerp(next.normalize(), lerp_fac)
    }

    /// Gets the current transformation matrix for the bone at the time
    /// `anim_ticks`
    pub fn get_bone_matrix(&self, anim_ticks: f64) -> Matrix4<f64> {
        let scale = self.get_cur_scale(anim_ticks);
        Matrix4::from_translation(self.get_cur_pos(anim_ticks)) *
        Matrix4::from(self.get_cur_rot(anim_ticks)) *
        Matrix4::from_nonuniform_scale(scale.x, scale.y, scale.z)
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
    /// Gets a vector indexable by mesh vertex index which returns a vector of tuples of corresponding
    /// scene bone ids and bone weights
    /// 
    /// `bone_map` - scene wide storage of bones to keep track of unique ids for each bone across the model
    /// and reuse the same `Bone` `struct` per bone
    /// 
    /// `num_vertices` - the number of vertices in the mesh and size of the return vector
    fn get_bones(mesh: &assimp::Mesh, bone_map: &mut HashMap<String, Bone>, num_vertices: usize) 
        -> Vec<Vec<(i32, f32)>> 
    {
        let mut unique_bones = bone_map.len() as i32;
        let mut vertex_bone_data = Vec::<Vec<(i32, f32)>>::new();
        vertex_bone_data.resize(num_vertices, Vec::new());
        for bone in mesh.bone_iter() {
            let name = bone.name().to_owned();
            let bone_id = match bone_map.get(&name) {
                None => {
                    let id = unique_bones;
                    bone_map.insert(name, Bone {
                        id,
                        offset_matrix: to_m4(*bone.offset_matrix()).into(),
                    });
                    unique_bones += 1;
                    id
                },
                Some(bone) => bone.id
            };

            for weight in bone.weight_iter().map(|vw| *vw) {
                vertex_bone_data[weight.vertex_id as usize].push((bone_id, weight.weight));
            }
        }
        vertex_bone_data
    }
    
    /// Splits an iterator over bone id, bone weight tuples into respective bone id and bone weight arrays
    /// If there are less weights than `max_bones_per_vertex`, the bone id will be `-1` and weight will be `0`
    fn to_bone_weight_arrays(bone_weights: &mut dyn Iterator<Item = &(i32, f32)>) 
        -> ([i32; MAX_BONES_PER_VERTEX], [f32; MAX_BONES_PER_VERTEX]) 
    {
        // set associated type, Item, for Iterator
        use std::mem::MaybeUninit;
        let mut id_array : [MaybeUninit<i32>; MAX_BONES_PER_VERTEX] = unsafe { MaybeUninit::uninit().assume_init() };
        let mut weight_array : [MaybeUninit<f32>; MAX_BONES_PER_VERTEX] = unsafe { MaybeUninit::uninit().assume_init() };
        let mut idx : usize = 0;
        while idx < MAX_BONES_PER_VERTEX {
            match bone_weights.next() {
                Some((id, weight)) => {
                    id_array[idx].write(*id);
                    weight_array[idx].write(*weight);
                    idx += 1;
                },
                _ => break,
            }
        }
        for i in idx .. MAX_BONES_PER_VERTEX {
            id_array[i].write(-1);
            weight_array[i].write(0.0);
        }
        unsafe {
            (std::mem::transmute(id_array), std::mem::transmute(weight_array))
        }
    }
    /// Creates a new mesh 
    /// 
    /// `bone_map` - map of already loaded bones by other meshes in the model. Will be updated if this mesh contains
    /// new bones
    pub fn new<F : glium::backend::Facade>(mesh: &assimp::Mesh, bone_map: &mut HashMap<String, Bone>, ctx: &F) -> Mesh {
        let mut vertices = Vec::<Vertex>::new();
        let mut indices = Vec::<u32>::new();
        let bones = Mesh::get_bones(mesh, bone_map, mesh.num_vertices() as usize);
        for (vert, norm, tex_coord, tan, bone_weights) in mesh.vertex_iter().zip(mesh.normal_iter()).zip(mesh.texture_coords_iter(0))
            .zip(mesh.tangent_iter()).zip(bones.iter()).map(|((((v, n), t), ta), b)| (v, n, t, ta, b))
        {
            let (bone_ids, bone_weights) = Mesh::to_bone_weight_arrays(&mut bone_weights.iter());
            vertices.push(Vertex {
                pos: to_v3(vert).into(),
                normal: to_v3(norm).into(),
                tex_coords: to_v2(tex_coord),
                tangent: to_v3(tan).into(),
                bone_ids, bone_weights,
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

/// Encapsulates transformation information of an AiNode.
/// Essentially represents a node in the scene graph for the model
pub struct AssimpNode {
    transformation: Matrix4<f64>,
    name: String,
    children: Vec<Box<AssimpNode>>,
}

impl AssimpNode {
    /// Creates a new scene heirarchy tree from a scene graph node and all its descendants
    pub fn new(node: &assimp::Node) -> AssimpNode {
        AssimpNode {
            name: node.name().to_owned(),
            transformation: to_m4(*node.transformation()),
            children: node.child_iter().map(|c| Box::new(AssimpNode::new(&c))).collect()
        }
    }
}

/// Stores a single animation
pub struct Animation {
    ticks_per_sec: f64,
    duration: f64,
    root_node: Rc<AssimpNode>,
    bone_map: Rc<HashMap<String, Bone>>,
    name: String,
    anim_bones: HashMap<String, BoneAnim>,
}

impl Animation {
    /// Requires there are no missing bones from `bone_map`
    pub fn new(anim: &assimp::Animation, root_node: Rc<AssimpNode>, bone_map: Rc<HashMap<String, Bone>>) -> Animation {
        let mut used_bones = HashMap::<String, BoneAnim>::new();
        for i in 0 .. anim.num_channels as usize {
            let node = anim.get_node_anim(i).unwrap();
            let bone_info = bone_map.get((*node).node_name.as_ref()).unwrap();
            used_bones.insert((*node).node_name.as_ref().to_owned(),
                BoneAnim::new(bone_info.id, &node));
        }
        Animation {
            ticks_per_sec: anim.ticks_per_second,
            duration: anim.duration,
            name: anim.name.as_ref().to_owned(),
            root_node, bone_map, anim_bones: used_bones,
        }
    }

    /// Plays the animations and gets the bone matrices
    /// 
    /// `dt` - seconds since animations has begun. If `dt > duration`
    /// animation loops to beginning
    pub fn play(&self, dt: f64) -> Vec<Matrix4<f32>> {
        let ticks = self.ticks_per_sec * dt;
        let iterations = (ticks / self.duration).round() as i32;
        let ticks = ticks - iterations as f64 * self.duration;

        let mut final_mats = Vec::<Matrix4<f32>>::new();
        final_mats.resize(self.bone_map.len(), Matrix4::from_scale(1.));
        let identity = Matrix4::from_scale(1f64);
        self.get_bone_transforms(ticks, &self.root_node, &identity, &mut final_mats);
        final_mats

    }

    /// Computes the bone transforms recursively done the node tree and stores them in `out_bone_matrices`
    /// 
    /// `parent_transform` - the matrix to transfrom from parent space to world space
    /// 
    /// `out_bone_matrices` - the vector storing final bone transformation matrices. Required to have size equal
    /// to the number of bones
    /// 
    /// `anim_time` - the duration the animation has been running. Required to be between `0` and `duration`
    fn get_bone_transforms(&self, anim_time: f64, ai_node: &AssimpNode, parent_transform: &Matrix4<f64>,
        mut out_bone_matrices: &mut Vec<Matrix4<f32>>) 
    {
        let bone = self.anim_bones.get(&ai_node.name).unwrap();
        let bone_transform = bone.get_bone_matrix(anim_time);
        let bone = self.bone_map.get(&ai_node.name).unwrap();
        let to_world_space = parent_transform * bone_transform;
        out_bone_matrices[bone.id as usize] = (to_world_space * bone.offset_matrix).cast().unwrap();

        for child in ai_node.children.iter().map(|c| &*c) {
            self.get_bone_transforms(anim_time, child, &to_world_space, &mut out_bone_matrices);
        }
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
    root_node: AssimpNode,
}

impl Model {
    fn process_node<F : glium::backend::Facade>(node: assimp::Node, scene: &Scene, 
        bone_map: &mut HashMap<String, Bone>, ctx: &F) -> Vec<Mesh> 
    {
        let mut meshes = Vec::<Mesh>::new();
        for i in 0 .. node.num_meshes() {
            let mesh = scene.mesh(i as usize).unwrap();
            meshes.push(Mesh::new(&mesh, bone_map, ctx));
        }
        for n in node.child_iter() {
            meshes.append(&mut Model::process_node(n, scene, bone_map, ctx));
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

    /// In case an animation somehow contains bones that none of the meshes do
    fn load_missing_bones(scene: &Scene, bone_map: &mut HashMap<String, Bone>) {
        for anim in scene.animation_iter() {
            let anim = &*anim;
            for i in 0 .. anim.num_channels as usize {
                let channel = unsafe { &**anim.channels.add(i) };
                if bone_map.get(channel.node_name.as_ref()).is_none() {
                    println!("Missing bone!");
                    let bone_id = bone_map.len() as i32;
                    bone_map.insert(channel.node_name.as_ref().to_owned(), Bone {
                        id: bone_id,
                        offset_matrix: Matrix4::from_scale(1f64).into(),
                    });
                }
            }

        }
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
        let mut bone_map = HashMap::<String, Bone>::new();
        let root_node = AssimpNode::new(&scene.root_node());
        let meshes = Model::process_node(scene.root_node(), &scene, &mut bone_map, ctx);
        let materials = if path.find(".obj").is_some() {
            Model::process_obj_mats(path, ctx)
        } else {
            Model::process_mats(&scene, &textures::dir_stem(path), ctx)
        };
        Model { meshes, materials, root_node }
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