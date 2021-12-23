use crate::model;
use crate::node;
use crate::draw_traits;
use crate::shader;

pub struct Entity {
    pub transform: node::Node,
    geometry: model::Model,
}

impl Entity {
    pub fn new(model: model::Model) -> Entity {
        Entity {
            transform: node::Node::new(None, None, None, None),
            geometry: model,
        }
    }

    pub fn from(model: model::Model, transform: node::Node) -> Entity {
        Entity {
            transform: transform,
            geometry: model,
        }
    }
}

impl draw_traits::Drawable for Entity {
    fn render<S>(&self, frame: &mut S, mats: &shader::SceneData, shader: &shader::ShaderManager) 
        where S : glium::Surface
    {
        let mat : cgmath::Matrix4<f32> = std::convert::From::from(&self.transform);
        self.geometry.render(frame, mats, mat.into(), shader)
    }
}

