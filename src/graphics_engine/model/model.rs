

use assimp::*;
use super::super::textures;
use super::super::shader;
use cgmath::*;
use std::collections::HashMap;
use std::rc::Rc;
use crate::cg_support::ssbo;
use super::mesh::Mesh;
use super::material::Material;
use super::animation::{Animator, Bone, AssimpNode};
use super::super::drawable::*;
use super::super::instancing;


/// A model is geometry loaded from the filesystem
/// Each model must have a main obj file with a material file at the specified path
/// relative to the obj file's directory. The name of the material controls which
/// shader is used for it. Texture files specified in the material file are relative to the obj file's
/// directory.
/// 
/// # Special Materials
/// 
/// * **PBR** - Materials that have a corresponding PBR file are PBR materials. PBR textures are loaded from the file
/// `[material_name]-pbr.yml` which is expected to be in the same directory as the main obj file. This file must define
/// a `roughness`, `metalness`, and optionally, `ao` parameter. Once again, these textures should be relative to the 
/// obj file's directory. This file can also define `albedo`, `normal`, and `emission` which will be added **IN ADDITION**
/// to whatever diffuse, normal, and emission textures are loaded by assimp/tobj. Use this if texture loading is working
/// properly
/// 
/// * **Lasers** - Materials with the name "Laser" are lasers. These are objects that are simply colored
/// with one uniform color and do not use textures
pub struct Model {
    meshes: Vec<Mesh>,
    materials: Vec<Material>,
    animator: Animator,
    bone_buffer: Option<ssbo::SSBO<[[f32; 4]; 4]>>,
    instances: instancing::InstanceBuffer,
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
    fn load_missing_bones(scene: &Scene, mut bone_map: HashMap<String, Bone>) -> HashMap<String, Bone> {
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
        bone_map
    }

    /// Gets the materials for `scene`
    /// 
    /// If `path` is an `.obj` file, or we find an `.mtl` file with the same name as `path` in the same
    /// directory, loads the textures from there instead
    /// ## Supported MTL arguments
    /// * `map_Kd` - diffuse texture
    /// * `map_bump` or `bump` - normal map
    /// * `map_Ke` - emission map
    /// 
    /// PBR textures are specified in a YAML file with the `-pbr.yml` file ending. They should be named
    /// the same as their corresponding materials
    fn process_materials<F : glium::backend::Facade>(path: &str, scene: &Scene, ctx: &F) -> Vec<Material> {
        let backup_mtl = format!("{}{}.mtl", 
            textures::dir_stem(path),
            std::path::Path::new(path).file_stem().map(|x| x.to_str().unwrap()).unwrap());
        if path.find(".obj").is_some() {
            Model::process_obj_mats(path, ctx)
        } else if std::path::Path::new(&backup_mtl).exists() {
            Model::process_obj_mats(&backup_mtl, ctx)
        } else {
            Model::process_mats(&scene, &textures::dir_stem(path), ctx)
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
        println!("Loaded model");
        let mut bone_map = HashMap::<String, Bone>::new();
        let root_node = AssimpNode::new(&scene.root_node());
        let meshes = Model::process_node(scene.root_node(), &scene, &mut bone_map, ctx);
        let materials = Model::process_materials(path, &scene, ctx);
        bone_map = Model::load_missing_bones(&scene, bone_map);
        let bone_buffer = if !bone_map.is_empty() {
            Some(ssbo::SSBO::<[[f32; 4]; 4]>::static_alloc_dyn(bone_map.len(), None))
        } else { None };
        let animator = Animator::new(scene.animation_iter(), Rc::new(bone_map), Rc::new(root_node));
        Model { meshes, materials, animator, 
            bone_buffer, 
            instances: instancing::InstanceBuffer::new() }
    }

    /// Render this model once, animating if there is one
    fn render<'a>(&'a mut self, model: [[f32; 4]; 4]) -> Vec<(shader::UniformInfo, VertexHolder<'a>, glium::index::IndicesSource<'a>)>
    {
        let bones = self.animator.animate(std::time::Instant::now());
        match (&bones, self.bone_buffer.as_mut()) {
            (Some(mats), Some(buf)) => {
                buf.update(mats)
            },
            _ => (),
        };
        let mut v = Vec::new();
        let bones = self.bone_buffer.as_ref();
        for mesh in &self.meshes {
            v.push(mesh.render_args(Some(model), &self.materials, bones.clone()));
        }
        v
    }

    /// Render multiple instances of this model
    /// 
    /// `instance_buffer` - VertexBuffer where each element in it is passed to each rendered copy of this model. So this will render an amount of copies equal to elements
    /// in this buffer
    fn render_instanced<'a>(&'a mut self, positions: &[[[f32; 4]; 4]]) 
        -> Vec<(shader::UniformInfo, VertexHolder<'a>, glium::index::IndicesSource<'a>)>
    {
        let mut v = Vec::new();
        if !positions.is_empty() {
            {
                let ctx = super::super::get_active_ctx();
                let ctx = ctx.ctx.borrow();
                self.instances.update_buffer(positions, &*ctx);
            }
            let data : glium::vertex::VerticesSource<'a> 
                = From::from(self.instances.get_stored_buffer().unwrap().per_instance().unwrap());
            for mesh in &self.meshes {
                let (uniform, vertices, indices) = mesh.render_args(None, &self.materials, None);
                v.push((uniform, vertices.append(data.clone()), indices));
            }
        }
        v
    }

    pub fn get_animator(&mut self) -> &mut Animator {
        &mut self.animator
    }
}

impl Drawable for Model {
    fn render_args<'a>(&'a mut self, positions: &[[[f32; 4]; 4]]) 
        -> Vec<(shader::UniformInfo, VertexHolder<'a>, glium::index::IndicesSource<'a>)>
    {
        if positions.len() == 1 {
            self.render(positions[0])
        } else {
            self.render_instanced(positions)
        }
    }
}