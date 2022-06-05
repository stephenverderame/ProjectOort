use super::shader;
#[derive(Clone, Copy)]
pub struct Vertex2D {
    pub pos: [f32; 2],
    pub tex_coords: [f32; 2],
}

glium::implement_vertex!(Vertex2D, pos, tex_coords);

#[derive(Clone, Copy)]
pub struct VertexPos {
    pub pos: [f32; 3],
}

glium::implement_vertex!(VertexPos, pos);

pub const MAX_BONES_PER_VERTEX : usize = 4;

#[derive(Clone, Copy)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub tex_coords: [f32; 2],
    pub tangent: [f32; 3], 
    // don't need bitangent since we can compute that as normal x tangent
    pub bone_ids: [i32; MAX_BONES_PER_VERTEX],
    pub bone_weights: [f32; MAX_BONES_PER_VERTEX],
}
glium::implement_vertex!(Vertex, pos, normal, tex_coords, tangent, bone_ids, bone_weights);

#[derive(Clone, Copy)]
pub struct VertexSimple {
    pub pos: [f32; 3],
    pub tex_coords: [f32; 2],
}

glium::implement_vertex!(VertexSimple, pos, tex_coords);

/// Sources of vertex data
pub enum VertexSourceData<'a> {
    Single(glium::vertex::VerticesSource<'a>),
    Multi(Vec<glium::vertex::VerticesSource<'a>>),
}

impl<'a> VertexSourceData<'a> {
    pub fn len(&self) -> usize {
        use VertexSourceData::*;
        match self {
            Single(_) => 1,
            Multi(arr) => arr.len(),
        }
    }

    /// Gets the vertex source at the specified index
    /// Requires idx < len
    pub fn index(&self, idx: usize) -> glium::vertex::VerticesSource<'a> {
        use VertexSourceData::*;
        match self {
            Single(a) => a.clone(),
            Multi(arr) => arr[idx].clone(),
        }
    }

    /// Creates a new vertex source data by adding a new data source to the existing ones
    pub fn append(self, data: glium::vertex::VerticesSource<'a>) -> Self {
        use VertexSourceData::*;
        match self {
            Single(x) => Multi(vec![x, data]),
            Multi(mut arr) => {
                arr.push(data);
                Multi(arr)
            },
        }
    }

    /// Creates a new vertex source data by adding a multi vertices source to the existing ones
    #[allow(dead_code)]
    pub fn append_flat(self, data: &mut dyn Iterator<Item = glium::vertex::VerticesSource<'a>>) -> Self {
        use VertexSourceData::*;
        let mut data : Vec<glium::vertex::VerticesSource<'a>> = data.collect();
        match self {
            Single(x) => {
                data.insert(0, x);
                Multi(data)
            },
            Multi(mut v) => {
                v.append(&mut data);
                Multi(v)
            },
        }
    }
}

/// Encapsulates one of multiple vertex data sources
pub struct VertexHolder<'a> {
    data: VertexSourceData<'a>,
    iter_count: usize,
}

impl<'a> VertexHolder<'a> {
    pub fn new(data: VertexSourceData<'a>) -> Self {
        VertexHolder {
            iter_count: 0,
            data,
        }
    }

    /// See `VertexSourceData::append`
    pub fn append(self, data: glium::vertex::VerticesSource<'a>) -> Self {
        VertexHolder {
            data: self.data.append(data),
            iter_count: self.iter_count,
        }
    }

    /// See `VertexSourceData::append_flat`
    #[allow(dead_code)]
    pub fn append_flat(mut self, data: &'a mut dyn Iterator<Item = glium::vertex::VerticesSource<'a>>) -> Self {
        self.data = self.data.append_flat(data);
        self
    }
}

impl<'a> Iterator for VertexHolder<'a> {
    type Item = glium::vertex::VerticesSource<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.iter_count >= self.data.len() { None }
        else {
            let data = self.data.index(self.iter_count);
            self.iter_count += 1;
            Some(data)
        }
    }
}

impl<'a> glium::vertex::MultiVerticesSource<'a> for VertexHolder<'a> {
    type Iterator = Self;

    fn iter(self) -> Self {
        self
    }
}

/// Something that can be drawn
pub trait Drawable {
    /// Gets the shader uniform, vertices, and indices for rendering the drawable
    /// 
    /// `positions` - the model matrices to render this drawable at. If this is empty, the specific
    /// drawable may choose what to do
    fn render_args<'a>(&'a mut self, positions: &[[[f32; 4]; 4]]) 
        -> Vec<(shader::UniformInfo, VertexHolder<'a>, glium::index::IndicesSource<'a>)>;

    /// Gets the transparency of the Drawable from `0` indicating opaque to `1` indicating transparent
    /// or `None` if the drawable is opaque
    fn transparency(&self) -> Option<f32>;
}

/// Something that encapsulates control of a view of the scene
pub trait Viewer {
    /// Gets the viewer's projection matrix
    fn proj_mat(&self) -> cgmath::Matrix4<f32>;

    /// Gets the viewer's position
    fn cam_pos(&self) -> cgmath::Point3<f32>;

    /// Gets the viewer's view matrix
    fn view_mat(&self) -> cgmath::Matrix4<f32>;

    /// Gets the viewer's near and far plane as a tuple
    fn view_dist(&self) -> (f32, f32);
}

/// Constructs shader viewer matrices from a viewer
pub fn viewer_data_from(viewer: &dyn Viewer) -> shader::ViewerData {
    let view = viewer.view_mat();
    let proj = viewer.proj_mat();
    shader::ViewerData {
        viewproj: (proj * view).into(),
        view: view.into(),
        proj: proj.into(),
        cam_pos: viewer.cam_pos().into(),
    }
}

/// Gets the default scene data filled with the relevant matrices according to
/// `viewer` and the aspect ratio `aspect`.
/// 
/// All other scene information is set to `None`
pub fn default_scene_data(viewer: &dyn Viewer) -> shader::SceneData {
    
    shader::SceneData {
        viewer: viewer_data_from(viewer),
        ibl_maps: None,
        lights: None,
        pass_type: shader::RenderPassType::Visual,
        light_pos: None,
    }
}

/// Renders a drawable to the surface
/// 
/// `matrices` - model matrices to render the drawable at, or `None` to render a single drawable using the identity matrix
/// for its transformation matrix
pub fn render_drawable<S : glium::Surface>(drawable: &mut dyn Drawable, matrices: Option<&[[[f32; 4]; 4]]>,
    surface: &mut S, scene_data: &shader::SceneData, cache: &shader::PipelineCache,
    shader: &shader::ShaderManager)
{
    let v = vec![cgmath::Matrix4::from_scale(1f32).into()];
    for (args, vbo, ebo) in drawable.render_args(matrices.unwrap_or(&v)).into_iter() {
        let (shader, params, uniform) = shader.use_shader(&args, Some(scene_data), Some(cache));
        match uniform {
            shader::UniformType::LaserUniform(uniform) => 
                surface.draw(vbo, ebo, shader, &uniform, &params),
            shader::UniformType::PbrUniform(uniform) => 
                surface.draw(vbo, ebo, shader, &uniform, &params),
            shader::UniformType::DepthUniform(uniform) =>
                surface.draw(vbo, ebo, shader, &uniform, &params),
            shader::UniformType::EqRectUniform(uniform) =>
                surface.draw(vbo, ebo, shader, &uniform, &params),
            shader::UniformType::SkyboxUniform(uniform) =>
                surface.draw(vbo, ebo, shader, &uniform, &params),
            shader::UniformType::CompositeUniform(uniform) =>
                surface.draw(vbo, ebo, shader, &uniform, &params),
            shader::UniformType::SepConvUniform(uniform) =>
                surface.draw(vbo, ebo, shader, &uniform, &params),
            shader::UniformType::ExtractBrightUniform(uniform) =>
                surface.draw(vbo, ebo, shader, &uniform, &params),
            shader::UniformType::PrefilterHdrEnvUniform(uniform) =>
                surface.draw(vbo, ebo, shader, &uniform, &params),
            shader::UniformType::BrdfLutUniform(uniform) =>
                surface.draw(vbo, ebo, shader, &uniform, &params),
            shader::UniformType::BillboardUniform(uniform) =>
                surface.draw(vbo, ebo, shader, &uniform, &params),
            shader::UniformType::CloudUniform(uniform) =>
                surface.draw(vbo, ebo, shader, &uniform, &params),
            shader::UniformType::LineUniform(uniform) =>
                surface.draw(vbo, ebo, shader, &uniform, &params),
            shader::UniformType::TextUniform(uniform) =>
                surface.draw(vbo, ebo, shader, &uniform, &params),
        }.unwrap()
    }
}