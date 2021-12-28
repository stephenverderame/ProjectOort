use crate::model;
use crate::node;
use crate::draw_traits;
use crate::shader;

/// An entity is a renderable geometry with a node in the
/// scene transformation heirarchy and collision and physics
/// behavior.
pub struct Entity {
    pub transform: node::Node,
    pub visible: bool,
    geometry: model::Model,
}

impl Entity {
    pub fn new(model: model::Model) -> Entity {
        Entity {
            transform: node::Node::new(None, None, None, None),
            geometry: model, visible: true,
        }
    }

    pub fn from(model: model::Model, transform: node::Node) -> Entity {
        Entity {
            transform: transform,
            geometry: model,
            visible: true,
        }
    }
}

impl draw_traits::Drawable for Entity {
    fn render<S>(&self, frame: &mut S, mats: &shader::SceneData, shader: &shader::ShaderManager) 
        where S : glium::Surface
    {
        if self.visible {
            let mat : cgmath::Matrix4<f32> = std::convert::From::from(&self.transform);
            self.geometry.render(frame, mats, mat.into(), shader)
        }
    }
}

