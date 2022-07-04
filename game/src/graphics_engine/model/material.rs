
use std::collections::BTreeMap;
use std::io::BufRead;
use super::super::textures;
use super::super::shader;
use crate::cg_support::ssbo;

/// Either a texture or constant factor
enum TexOrConst {
    Tex(glium::texture::Texture2d),
    Fac(f32),
}

struct PBRData {
    roughness: TexOrConst,
    metalness: TexOrConst,
    ao_tex: Option<glium::texture::Texture2d>,
}

/// We can specify non-pbr textures in the pbr file in case
/// assimp doesn't find any. 
struct ExtraTexData {
    albedo_tex: Option<glium::texture::SrgbTexture2d>,
    normal_tex: Option<glium::texture::Texture2d>,
}

/// Reads pbr textures from an externam file names `[mat_name]-pbr.yml` that resides in
/// directory `dir`
/// 
/// Configuration information in this file must be specified in `key: value` pairs with each
/// key being on a separate line
/// 
/// Returns the map of key value pairs as strings
/// 
/// ### Valid Keys
/// 
/// * `roughness` - texture path or non-negative float
/// * `metalness` - texture path or non-negative float
/// * `ao` - texture path [optional]
/// * `albedo` - texture path [optional if read by assimp]
/// * `normal` - texture path [optional if read by assimp]
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
/// directory of `dir`. Also returns an extra tex data which are optionally specified textures
/// that don't have to go in the pbr file.
fn get_pbr_textures<F>(dir: &str, mat_name: &str, facade: &F) 
    -> (Option<PBRData>, Option<ExtraTexData>) where F : glium::backend::Facade 
{
    match get_pbr_data(dir, mat_name) {
        Some(tex_maps) => {
            println!("{}", tex_maps["roughness"]);
            println!("{}", tex_maps["metalness"]);
            if tex_maps.contains_key("ao") {
                println!("ao: {}", tex_maps["ao"]);
            }
            let rough_fac = tex_maps["roughness"].parse::<f32>();
            let metal_fac = tex_maps["metalness"].parse::<f32>();
            (Some(PBRData {
                ao_tex: if tex_maps.contains_key("ao") {
                    Some(textures::load_texture_2d(&format!("{}{}", dir, tex_maps["ao"]), facade))
                } else { None },
                roughness: match rough_fac {
                    Ok(f) => TexOrConst::Fac(f),
                    _ => TexOrConst::Tex(textures::load_texture_2d(
                        &format!("{}{}", dir, tex_maps["roughness"]), facade)),
                },
                metalness: match metal_fac {
                    Ok(f) => TexOrConst::Fac(f),
                    _ => TexOrConst::Tex(textures::load_texture_2d(
                        &format!("{}{}", dir, tex_maps["metalness"]), facade))
                }
            }),
            Some(ExtraTexData {
                albedo_tex: if tex_maps.contains_key("albedo") {
                    Some(textures::load_texture_srgb(
                        &format!("{}{}", dir, tex_maps["albedo"]), facade))
                } else { None },
                normal_tex: if tex_maps.contains_key("normal") {
                    Some(textures::load_texture_2d(
                        &format!("{}{}", dir, tex_maps["normal"]), facade))
                } else { None },
            }))
        },
        _ => (None, None),
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
const fn null<T>() -> *mut T {
    0 as *mut T
}
/// Gets a constant nullptr
const fn null_c<T>() -> *const T {
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
    /// Gets a material property with the key `property` as an ascii string
    /// Non alphanumeric/punctuation ascii characters are stripped
    fn get_property(mat: &assimp_sys::AiMaterial, property: &str) -> Option<String> {
        for i in 0 .. mat.num_properties {
            let prop = unsafe {&**mat.properties.add(i as usize)};
            if prop.key.data[.. prop.key.length] == *property.as_bytes() {
                let len = prop.data_length as usize;
                let mut res = Vec::<u8>::new();
                res.resize(len + 1, 0);
                unsafe { std::ptr::copy_nonoverlapping(prop.data as *const u8, res.as_mut_ptr(), len); }
                let mut name = String::from_utf8_lossy(&res).into_owned();
                name.retain(|c| c.is_ascii_alphanumeric() || c.is_ascii_punctuation());
                return Some(name);

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
        normal.append(&mut Material::get_textures(mat, assimp_sys::AiTextureType::Height, dir, &load_rgb));
        let mut ao = Material::get_textures(mat, assimp_sys::AiTextureType::Lightmap, dir, &load_rgb);
        let name = Material::get_property(mat, "?mat.name").expect("No material name!");
        let (pbr, extras) = get_pbr_textures(dir, &name, ctx);
        let pbr = pbr.map(|mut pbr| {
            if pbr.ao_tex.is_none() && ao.len() > 0 {
                pbr.ao_tex = Some(ao.swap_remove(0));
            }
            pbr
        });
        extras.map(|ExtraTexData {albedo_tex, normal_tex}| {
            if albedo_tex.is_some() { diffuse.push(albedo_tex.unwrap()); }
            if normal_tex.is_some() { normal.push(normal_tex.unwrap()); }
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
            pbr_data: get_pbr_textures(dir, &mat.name, ctx).0,
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
    pub fn to_uniform_args<'a>(&'a self, instancing: bool, model: Option<[[f32; 4]; 4]>, 
        bones: Option<&'a ssbo::SSBO<[[f32; 4]; 4]>>, 
        trans_data: Option<&'a shader::TransparencyData>, 
        emission_strength: f32) -> shader::UniformInfo 
    {
        match &self.name[..] {
            "Laser" => shader::UniformInfo::LaserInfo,
            _ if self.pbr_data.is_some() => shader::UniformInfo::PBRInfo(shader::PBRData {
                diffuse_tex: self.diffuse_tex.as_ref().unwrap(),
                model: model.unwrap_or_else(|| cgmath::Matrix4::from_scale(1f32).into()),
                roughness_map: self.pbr_data.as_ref().and_then(|data| { 
                    match &data.roughness {
                        TexOrConst::Fac(_) => None,
                        TexOrConst::Tex(t) => Some(t),
                } }),
                metallic_map: self.pbr_data.as_ref().and_then(|data| { 
                    match &data.metalness {
                        TexOrConst::Fac(_) => None,
                        TexOrConst::Tex(t) => Some(t),
                } }),
                normal_map: self.normal_tex.as_ref(),
                emission_map: self.emission_tex.as_ref(),
                ao_map: self.pbr_data.as_ref().and_then(|data| { data.ao_tex.as_ref() }),
                instancing, bone_mats: bones,
                trans_data,
                emission_strength,
                roughness_fac: self.pbr_data.as_ref().map(|data| { 
                    match &data.roughness {
                        TexOrConst::Fac(f) => *f,
                        TexOrConst::Tex(_) => -2.0,
                } }).unwrap(),
                metallic_fac: self.pbr_data.as_ref().map(|data| { 
                    match &data.metalness {
                        TexOrConst::Fac(f) => *f,
                        TexOrConst::Tex(_) => -2.,
                } }).unwrap(),
            }),
            x => panic!("Unimplemented texture with name: {}", x),
        }  
    }
}