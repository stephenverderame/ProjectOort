use crate::model;
use crate::node;
use crate::draw_traits;
use crate::shader;

/// The transformation data for an entity
pub struct EntityInstanceData {
    pub transform: node::Node,
    pub velocity: cgmath::Vector3<f64>,
    pub visible: bool,
}

/// An entity is a renderable geometry with a node in the
/// scene transformation heirarchy and collision and physics
/// behavior.
pub struct Entity {
    pub data: EntityInstanceData,
    geometry: model::Model,
}

impl Entity {
    pub fn new(model: model::Model) -> Entity {
        Entity::from(model, node::Node::new(None, None, None, None))
    }

    pub fn from(model: model::Model, transform: node::Node) -> Entity {
        Entity {
            data: EntityInstanceData {
                transform,
                velocity: cgmath::vec3(0., 0., 0.),
                visible: true,
            },
            geometry: model,
        }
    }
}

impl draw_traits::Drawable for Entity {
    fn render<S>(&self, frame: &mut S, mats: &shader::SceneData, shader: &shader::ShaderManager) 
        where S : glium::Surface
    {
        if self.data.visible {
            let mat : cgmath::Matrix4<f32> = std::convert::From::from(&self.data.transform);
            self.geometry.render(frame, mats, mat.into(), shader)
        }
    }
}

/// Entities with shared geometry
/// 
/// TODO: free unnecessary instances
pub struct EntityFlyweight {
    instances: Vec<EntityInstanceData>,
    geometry: model::Model,
}

impl EntityFlyweight {
    pub fn new(model: model::Model) -> EntityFlyweight {
        EntityFlyweight {
            instances: Vec::<EntityInstanceData>::new(),
            geometry: model,
        }
    }

    /// Creates a new instance
    pub fn new_instance(&mut self, instance: EntityInstanceData) {
        self.instances.push(instance)
    }

    /// Moves all instances based on their current velocity
    /// 
    /// TODO: Combine into Physics engine
    pub fn instance_motion(&mut self, dt: f64) {
        for instance in &mut self.instances {
            instance.transform.pos += instance.velocity * dt;
        }
    }
}

impl draw_traits::Drawable for EntityFlyweight {
    fn render<S>(&self, frame: &mut S, scene_data: &shader::SceneData, shader: &shader::ShaderManager)
        where S : glium::Surface
    {
        for instance in &self.instances {
            if instance.visible {
                let model : cgmath::Matrix4<f32> = From::from(&instance.transform);
                self.geometry.render(frame, scene_data, model.into(), shader);
            }
        }
    }
}

