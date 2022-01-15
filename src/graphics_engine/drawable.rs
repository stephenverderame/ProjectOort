use super::shader;
#[derive(Clone, Copy)]
pub struct Vertex2D {
    pos: [f32; 2],
    tex_coords: [f32; 2],
}

glium::implement_vertex!(Vertex2D, pos, tex_coords);

#[derive(Clone, Copy)]
pub struct VertexPos {
    pos: [f32; 3],
}

glium::implement_vertex!(VertexPos, pos);

pub const MAX_BONES_PER_VERTEX : usize = 4;

#[derive(Clone, Copy)]
pub struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    tex_coords: [f32; 2],
    tangent: [f32; 3], 
    // don't need bitangent since we can compute that as normal x tangent
    bone_ids: [i32; MAX_BONES_PER_VERTEX],
    bone_weights: [f32; MAX_BONES_PER_VERTEX],
}
glium::implement_vertex!(Vertex, pos, normal, tex_coords, tangent, bone_ids, bone_weights);

/// Sources of vertex data
pub enum VertexSourceData<'a> {
    Single(glium::vertex::VerticesSource<'a>),
    Double([glium::vertex::VerticesSource<'a>; 2]),
}

impl<'a> VertexSourceData<'a> {
    pub fn len(&self) -> u8 {
        use VertexSourceData::*;
        match self {
            Single(_) => 1,
            Double(arr) => arr.len() as u8,
        }
    }

    /// Requires idx < len
    pub fn index(&self, idx: u8) -> glium::vertex::VerticesSource<'a> {
        use VertexSourceData::*;
        match self {
            Single(a) => *a,
            Double(arr) => arr[idx as usize],
        }
    }

    /// Creates a new vertex source data by adding a new data source to the existing ones
    pub fn append(self, data: glium::vertex::VerticesSource<'a>) -> Self {
        use VertexSourceData::*;
        match self {
            Single(x) => Double([x, data]),
            _ => panic!("Attempt to append more vertex sources than supported"),
        }
    }
}

/// Encapsulates one of multiple vertex data sources
pub struct VertexHolder<'a> {
    data: VertexSourceData<'a>,
    iter_count: u8,
}

impl<'a> VertexHolder<'a> {
    pub fn new(data: VertexSourceData<'a>) -> Self {
        VertexHolder {
            iter_count: 0,
            data,
        }
    }

    /// Creates a new VertexHolder by appending a data source to the current data source(s)
    pub fn append(self, data: glium::vertex::VerticesSource<'a>) -> Self {
        VertexHolder {
            data: self.data.append(data),
            iter_count: self.iter_count,
        }
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
    /// Draws the drawable to the given surface `frame`, with the provided scene information
    /// and shader manager.
    fn render_args<'a>(&'a self, positions: &[[[f32; 4]; 4]]) 
        -> Vec<(shader::UniformInfo, VertexHolder<'a>, glium::index::IndicesSource<'a>)>;

    /// Returns `true` if we should render this object during a pass of type `pass`
    fn should_render(&self, pass: &shader::RenderPassType) -> bool;
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