use super::animation::Bone;
use std::collections::HashMap;
use super::{to_m4, to_v3, to_v2};
use crate::{shader, ssbo};
use super::material::*;

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
glium::implement_vertex!(Vertex, pos, normal, tex_coords, tangent, bone_ids, bone_weights);

/// Creates a OpenGL vbo and ebo for the vertices and indices
#[inline]
fn get_vbo_ebo<F : glium::backend::Facade>(verts: Vec<Vertex>, indices: Vec<u32>, ctx: &F) 
    -> (glium::VertexBuffer<Vertex>, glium::IndexBuffer<u32>) 
{
    (glium::VertexBuffer::immutable(ctx, &verts).unwrap(),
    glium::IndexBuffer::immutable(ctx, glium::index::PrimitiveType::TrianglesList, &indices).unwrap())
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
                        offset_matrix: to_m4(*bone.offset_matrix()),
                    });
                    unique_bones += 1;
                    id
                },
                Some(bone) => bone.id
            };

            // Assimp weight_iter is broken
            for i in 0 .. bone.num_weights as usize {
                let weight = unsafe { *bone.weights.add(i) };
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
        model: Option<[[f32; 4]; 4]>, instancing: bool, mats: &Vec<Material>, bones: Option<&ssbo::SSBO::<[[f32; 4]; 4]>>, mut draw_func: F) 
        where F : FnMut(&glium::VertexBuffer<Vertex>, &glium::IndexBuffer<u32>, &glium::Program, &glium::DrawParameters, &shader::UniformType), 
    {
        let mat = mats[self.mat_idx.min(mats.len() - 1)].to_uniform_args(instancing, model, bones);
        let (shader, params, uniform) = manager.use_shader(&mat, Some(scene_data), Some(local_data));
        draw_func(&self.vbo, &self.ebo, shader, &params, &uniform)
    }

    pub fn render<S : glium::Surface>(&self, wnd: &mut S, mats: &shader::SceneData, local_data: &shader::PipelineCache, model: [[f32; 4]; 4], 
        manager: &shader::ShaderManager, materials: &Vec<Material>, bones: Option<&ssbo::SSBO::<[[f32; 4]; 4]>>) {
        self.render_helper(mats, manager, local_data, Some(model), false, materials, bones,
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
        self.render_helper(mats, manager, local_data, None, true, materials, None,
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