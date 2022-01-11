use crate::model;
use crate::node;
use crate::draw_traits;
use crate::shader;
use std::rc::Rc;
use std::cell::RefCell;
use crate::collisions;

/// The transformation data for an entity
pub struct EntityInstanceData {
    pub transform: Rc<RefCell<node::Node>>,
    pub velocity: cgmath::Vector3<f64>,
    pub visible: bool,
    pub collider: Option<collisions::CollisionObject>,
}

/// An entity is a renderable geometry with a node in the
/// scene transformation heirarchy and collision and physics
/// behavior.
pub struct Entity {
    pub data: EntityInstanceData,
    geometry: model::Model,
}

impl Entity {
    #[allow(dead_code)]
    pub fn new(model: model::Model) -> Entity {
        Entity::from(model, node::Node::new(None, None, None, None))
    }

    pub fn from(model: model::Model, transform: node::Node) -> Entity {
        Entity {
            data: EntityInstanceData {
                transform: Rc::new(RefCell::new(transform)),
                velocity: cgmath::vec3(0., 0., 0.),
                visible: true,
                collider: None,
            },
            geometry: model,
        }
    }

    /*
    pub fn with_collisions(model: model::Model, transform: node::Node, collision_mesh: &str) -> Entity {
        let transform = Rc::new(RefCell::new(transform));
        Entity {
            data: EntityInstanceData {
                velocity: cgmath::vec3(0., 0., 0.),
                visible: true,
                collider: Some(collisions::CollisionObject::new(transform.clone(), collision_mesh, 
                    collisions::TreeStopCriteria::default())),
                transform,
            },
            geometry: model,
        }
    }*/

    pub fn get_animator(&mut self) -> &mut model::Animator {
        self.geometry.get_animator()
    }
}

impl draw_traits::Drawable for Entity {
    fn render<S>(&self, frame: &mut S, mats: &shader::SceneData, local_data: &shader::PipelineCache, shader: &shader::ShaderManager) 
        where S : glium::Surface
    {
        if self.data.visible {
            let mat : cgmath::Matrix4<f32> = std::convert::From::from(&*self.data.transform.borrow());
            self.geometry.render(frame, mats, local_data, mat.into(), shader)
        }
    }
}
#[derive(Copy, Clone)]
struct InstanceAttribute {
    //instance_model: [[f32; 4]; 4],
    instance_model_col0: [f32; 4],
    instance_model_col1: [f32; 4],
    instance_model_col2: [f32; 4],
    instance_model_col3: [f32; 4],
    instance_color: [f32; 3],
}

glium::implement_vertex!(InstanceAttribute, instance_model_col0, instance_model_col1, instance_model_col2, 
    instance_model_col3, instance_color);

/// Entities with shared geometry
/// 
/// TODO: free unnecessary instances
/// TODO: make private
pub struct EntityFlyweight {
    pub instances: Vec<EntityInstanceData>,
    geometry: model::Model,
    instance_data: Option<glium::VertexBuffer<InstanceAttribute>>,
    buffer_count: usize,
}

impl EntityFlyweight {
    pub fn new(model: model::Model) -> EntityFlyweight {
        EntityFlyweight {
            instances: Vec::<EntityInstanceData>::new(),
            geometry: model,
            instance_data: None,
            buffer_count: 0,
        }
    }

    fn resize_buffer<F : glium::backend::Facade>(instances: &Vec<EntityInstanceData>, facade: &F) 
    -> (glium::VertexBuffer<InstanceAttribute>, usize)
    {
        let new_size = instances.len() * 2;
        let data : Vec<InstanceAttribute> = instances.iter().map(|data| {
            let mat : cgmath::Matrix4<f32> = From::from(&*data.transform.borrow());
            InstanceAttribute {
                instance_model_col0: mat.x.into(),
                instance_model_col1: mat.y.into(),
                instance_model_col2: mat.z.into(),
                instance_model_col3: mat.w.into(),
                instance_color: [0.5451, 0f32, 0.5451],
            }
        }).chain((0 .. instances.len()).map(|_| {
            InstanceAttribute {
                instance_model_col0: [0f32, 0f32, 0f32, 0f32],
                instance_model_col1: [0f32, 0f32, 0f32, 0f32],
                instance_model_col2: [0f32, 0f32, 0f32, 0f32],
                instance_model_col3: [0f32, 0f32, 0f32, 0f32],
                instance_color: [0.5451, 0f32, 0.5451],
            }
        })).collect();
        (glium::VertexBuffer::dynamic(facade, &data).unwrap(), new_size)
    }

    /// Creates a new instance
    pub fn new_instance<F : glium::backend::Facade>(&mut self, instance: EntityInstanceData, facade: &F) {
        self.instances.push(instance);
        if self.instances.len() >= self.buffer_count {
            let (buffer, new_count) = EntityFlyweight::resize_buffer(&self.instances, facade);
            self.instance_data = Some(buffer);
            self.buffer_count = new_count;
        }
    }

    /// Moves all instances based on their current velocity
    /// 
    /// TODO: Combine into Physics engine
    pub fn instance_motion(&mut self, dt: f64) {
        if let Some(instance_data) = &mut self.instance_data {
            let mut mapping = instance_data.map();
            let mut count = 0usize;
            for (src, dst) in self.instances.iter_mut().zip(mapping.iter_mut()) {
                src.transform.borrow_mut().pos += src.velocity * dt;
                let mat : cgmath::Matrix4<f32> = From::from(&*src.transform.borrow());
                dst.instance_model_col0 = mat.x.into();
                dst.instance_model_col1 = mat.y.into();
                dst.instance_model_col2 = mat.z.into();
                dst.instance_model_col3 = mat.w.into();
                count += 1;
            }
            assert_eq!(count, self.instances.len());
        } else { assert!(false) }
    }

    pub fn iter_positions<F : FnMut(&node::Node)>(&self, mut cb: F) {
        for instance in &self.instances {
            cb(&*instance.transform.borrow())
        }
    }
}

impl draw_traits::Drawable for EntityFlyweight {
    fn render<S>(&self, frame: &mut S, scene_data: &shader::SceneData, local_data: &shader::PipelineCache, shader: &shader::ShaderManager)
        where S : glium::Surface
    {
        if let Some(instance_data) = &self.instance_data {
            self.geometry.render_instanced(frame, scene_data, local_data, shader, 
                &instance_data.slice(.. self.instances.len()).unwrap());
        }
    }
}

