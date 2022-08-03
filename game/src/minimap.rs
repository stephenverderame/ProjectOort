use super::physics;
use super::object;
use crate::graphics_engine::{textures, instancing, shader, entity};
use crate::cg_support::{node, Transformation};
use super::drawable::*;
use VertexSimple as Vertex;
use std::rc::Rc;
use std::cell::RefCell;

// TODO: consolidate with Billboard.rs

const RECT_VERTS : [Vertex; 4] = [
    Vertex { pos: [1., 1., 0.], tex_coords: [1., 1.]},
    Vertex { pos: [-1., 1., 0.], tex_coords: [0., 1.]},
    Vertex { pos: [-1., -1., 0.], tex_coords: [0., 0.]},
    Vertex { pos: [1., -1., 0.], tex_coords: [1., 0.]}
];

const RECT_INDICES : [u8; 6] = [0, 1, 3, 3, 1, 2];

struct MinimapBlip {
    color: [f32; 4],
    tex_index: usize,
    pos: node::Node,

}

pub struct Minimap {
    textures: [glium::texture::Texture2d; 3],
    center: Rc<RefCell<node::Node>>,
    view_dist: f64,
    blips: Vec<MinimapBlip>,
    vertices: glium::VertexBuffer<Vertex>,
    indicies: glium::IndexBuffer<u8>,
    instances: instancing::InstanceBuffer<instancing::ParticleAttributes>,
    instance_pos: instancing::InstanceBuffer<instancing::InstancePosition>,
}

impl Minimap {
    /// Creates a new minimap that is centered on `center` and can see `view_dist`
    /// away from `center` in all directions
    pub fn new<F : glium::backend::Facade>(center: Rc<RefCell<node::Node>>, 
        view_dist: f64, facade: &F) -> Self 
    {
        Self {
            center, view_dist,
            textures: [
                textures::load_texture_2d("assets/particles/circle_05.png", facade),
                textures::load_texture_2d("assets/particles/trace_02.png", facade),
                textures::load_texture_2d("assets/particles/star_07.png", facade)],
            blips: Vec::new(),
            vertices: glium::VertexBuffer::immutable(facade, &RECT_VERTS).unwrap(),
            indicies: glium::IndexBuffer::immutable(facade, 
                glium::index::PrimitiveType::TrianglesList, &RECT_INDICES).unwrap(),
            instances: instancing::InstanceBuffer::new(),
            instance_pos: instancing::InstanceBuffer::new(),
        }

    }

    /// Adds `body` to the minimap. Does nothing if `body` should not be shown
    pub fn add_item(&mut self, body: &physics::RigidBody<object::ObjectData>) {
        use object::ObjectType::*;
        use cgmath::*;
        let (color, tex_index, scale) = match body.metadata.0 {
            Asteroid => ([0.517f32, 0.282, 0.082, 1.0], 0usize, 
                (body.base.extents().unwrap_or(0.) / self.view_dist).max(0.05)),
            Laser => ([0.5451f32, 0.0, 0.5451, 1.0], 1usize, 0.1),
            Ship => ([0.960f32, 0.623, 0.141, 1.0], 2usize, 0.1),
            _ => return,
        };
        let center_inv = self.center.borrow().mat().invert().unwrap();
        let minimap_pos = center_inv.transform_point(body.base.center()) / self.view_dist;
        let pos : [f64; 3] = minimap_pos.into();
        self.blips.push(MinimapBlip {
            color, tex_index, pos: node::Node::default()
                .pos(pos.into())
                .u_scale(scale)
        })
    }

    /// Removes all items on the minimap
    pub fn clear_items(&mut self) {
        self.blips.clear();
        // add center icon (self)
        self.blips.push(MinimapBlip {
            color: [0.0, 1.0, 0.0, 1.0], 
            tex_index: 2, 
            pos: node::Node::default().u_scale(0.1)
        });
    }
}

impl Drawable for Minimap {
    fn render_args<'a>(&'a mut self, _positions: &[[[f32; 4]; 4]]) 
        -> Vec<(shader::UniformInfo, VertexHolder<'a>, glium::index::IndicesSource<'a>)>
    {
        if self.blips.is_empty() { return Vec::new() };

        use glium::*;
        let attribs : Vec<_> = self.blips.iter().map(|b| {
            instancing::ParticleAttributes {
                color: b.color,
                tex_idx: b.tex_index as u32,
            }
        }).collect();

        let positions : Vec<[[f32; 4]; 4]> = self.blips.iter()
            .map(|b| { b.pos.mat().cast().unwrap().into() }).collect();
        let positions = instancing::model_mats_to_vertex(&positions);

        {
            let ctx = crate::graphics_engine::get_active_ctx();
            let facade = ctx.ctx.borrow();
            self.instances.update_buffer(&attribs, &*facade);
            self.instance_pos.update_buffer(&positions, &*facade);
        }

        let uniform = shader::UniformInfo::MinimapInfo(shader::MinimapData {
            textures: [&self.textures[0], &self.textures[1], &self.textures[2]]
        });

        let inst_attribs : vertex::VerticesSource<'a> =
            From::from(self.instances.get_stored_buffer().unwrap().per_instance().unwrap());
        let inst_pos : vertex::VerticesSource<'a> = 
            From::from(self.instance_pos.get_stored_buffer().unwrap().per_instance().unwrap());
        let v = VertexHolder::new(
            VertexSourceData::Single(From::from(&self.vertices)))
            .append(inst_attribs)
            .append(inst_pos);
        vec![(uniform, v, From::from(&self.indicies))]
    }

    fn transparency(&self) -> Option<f32> { None }
}

impl entity::AbstractEntity for Minimap {

    fn transformations(&self) -> Option<&[Rc<RefCell<dyn Transformation>>]> {
        None
    }

    fn drawable(&mut self) -> &mut dyn Drawable {
        self
    }

    fn should_render(&self, pass: shader::RenderPassType) -> bool {
        match pass {
            shader::RenderPassType::Visual => true,
            _ => false,
        }

    }


    fn render_order(&self) -> entity::RenderOrder {
        entity::RenderOrder::Unordered
    }
}