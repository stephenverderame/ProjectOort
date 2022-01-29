use super::drawable::*;
use VertexSimple as Vertex;
use glium::*;
use super::shader;

const RECT_VERTS : [Vertex; 4] = [
    Vertex { pos: [1., 1., 0.], tex_coords: [1., 1.]},
    Vertex { pos: [-1., 1., 0.], tex_coords: [0., 1.]},
    Vertex { pos: [-1., -1., 0.], tex_coords: [0., 0.]},
    Vertex { pos: [1., -1., 0.], tex_coords: [1., 0.]}
];

const RECT_INDICES : [u32; 6] = [0, 1, 3, 3, 1, 2];

/// A textured 2D quad in 3D that provides billboarding support
/// 
/// Independent of positional arguments to allow control
pub struct Rect3D {
    vertices: glium::VertexBuffer<Vertex>,
    indices: glium::IndexBuffer<u32>,
    tex: glium::texture::SrgbTexture2d,
}

impl Rect3D {
    pub fn new<F : backend::Facade>(tex: glium::texture::SrgbTexture2d, facade: &F) -> Self {
        Self {
            vertices: vertex::VertexBuffer::immutable(facade, &RECT_VERTS).unwrap(),
            indices: index::IndexBuffer::immutable(facade, index::PrimitiveType::TrianglesList, &RECT_INDICES).unwrap(),
            tex,
        }
    }
}

impl Drawable for Rect3D {
    fn render_args<'a>(&'a mut self, _: &[[[f32; 4]; 4]]) -> Vec<(shader::UniformInfo, VertexHolder<'a>, glium::index::IndicesSource<'a>)>
    {
        let info = shader::UniformInfo::BillboardInfo(&self.tex);
        vec![(info, VertexHolder::new(VertexSourceData::Single(From::from(&self.vertices))), 
            From::from(&self.indices))]
    }

    fn transparency(&self) -> Option<f32> { None }
}