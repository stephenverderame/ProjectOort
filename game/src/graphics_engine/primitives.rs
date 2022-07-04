use super::shader;
use super::drawable::*;
use super::entity::*;
use glium::*;
use VertexPos as Vertex;
use super::instancing::*;
use std::collections::HashMap;
use crate::node;
use cgmath::*;

const LINE_VERTS : [Vertex; 2] = 
    [Vertex {pos: [0., 0., 0.]}, Vertex {pos: [1., 0., 0.]}];

const LINE_INDICES : [u8; 2] = [0, 1];

/// Encapsulates information to draw a line
pub struct LineData {
    pub start: node::Node,
    pub end: node::Node,
    pub color: [f32; 4],
}

/// A collection of lines
/// 
/// This is both an entity and a drawable
/// If used as a drawable, the render locations are ignored. This will
/// only render lines added with the `add_line` method
/// 
/// If used as an entity, the `transformations` method will always return
/// nothing regardless of how many lines will actually be drawn
/// 
/// This is bad design, but rihgt now, I think it keeps usage a bit simpler
pub struct Lines {
    vertices: VertexBuffer<Vertex>,
    indices: IndexBuffer<u8>,
    instances: InstanceBuffer<LineAttributes>,
    lines: HashMap<u32, LineData>
}

impl Lines {
    pub fn new<F : backend::Facade>(ctx: &F) -> Self {
        Self {
            vertices: VertexBuffer::new(ctx, &LINE_VERTS).unwrap(),
            indices: IndexBuffer::new(ctx, index::PrimitiveType::LinesList, 
                &LINE_INDICES).unwrap(),
            instances: InstanceBuffer::new(),
            lines: HashMap::new(),
        }
    }

    /// Adds a new line to this collection of lines
    /// 
    /// `key` - the unique id for this line
    pub fn add_line(&mut self, key: u32, line: LineData) {
        self.lines.insert(key, line);
    }

    /// Removes a line by its unique identification
    pub fn remove_line(&mut self, key: u32) {
        self.lines.remove(&key);
    }
}

fn pt_to_gl_v4(pt: Point3<f64>) -> [f32; 4] {
    let pt = pt.cast().unwrap();
    [pt.x, pt.y, pt.z, 1.0]
}

impl Drawable for Lines {
    fn render_args<'a>(&'a mut self, _positions: &[[[f32; 4]; 4]]) 
        -> Vec<(shader::UniformInfo, VertexHolder<'a>, glium::index::IndicesSource<'a>)>
    {
        if !self.lines.is_empty() {
            {
                let ctx = super::super::get_active_ctx();
                let ctx = ctx.ctx.borrow();
                let vals : Vec<LineAttributes> = self.lines.values()
                    .map(|v| LineAttributes {
                        start_pos: pt_to_gl_v4(v.start.get_pos()),
                        end_pos: pt_to_gl_v4(v.end.get_pos()),
                        color: v.color,
                    }).collect();
                self.instances.update_buffer(&vals, &*ctx);
            }
            let data : glium::vertex::VerticesSource<'a> 
                = From::from(self.instances.get_stored_buffer().unwrap()
                    .per_instance().unwrap());
            let vertices = VertexHolder::new(VertexSourceData::Single(
                From::from(&self.vertices))).append(data);
            vec![(shader::UniformInfo::LineInfo, vertices, 
                From::from(&self.indices))]
        } else {
            Vec::new()
        }
    }

    fn transparency(&self) -> Option<f32> { None }
}

use std::rc::Rc;
use std::cell::RefCell;
use crate::cg_support::Transformation;

impl AbstractEntity for Lines {

    fn transformations(&self) -> Option<&[Rc<RefCell<dyn Transformation>>]> {
        None
    }

    fn drawable(&mut self) -> &mut dyn Drawable {
        self
    }

    fn should_render(&self, pass: shader::RenderPassType) -> bool {
        match pass {
            shader::RenderPassType::Visual | 
            shader::RenderPassType::LayeredVisual |
            shader::RenderPassType::Transparent(_) => true,
            _ => false,
        }

    }


    fn render_order(&self) -> RenderOrder {
        RenderOrder::Unordered
    }
}