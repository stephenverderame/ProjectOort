

use crate::cg_support::node::*;
use crate::model::Model;
use crate::graphics_engine::{drawable, entity, shader};
use crate::controls;
use std::rc::{Rc};
use std::cell::RefCell;
use crate::camera;
use drawable::Viewer;
use crate::collisions;
use crate::physics;
use super::object;

use cgmath::*;

const FAR_PLANE : f32 = 1000.;

/// The player is the combination of the player's entity and the player's camera
pub struct Player {
    cam: Node,
    entity: Rc<RefCell<entity::Entity>>,
    pub aspect: f32,
    body: physics::RigidBody<object::ObjectType>,
    pub inv_fac: Rc<RefCell<f32>>,
    em_fac: Rc<RefCell<f32>>,
}

impl Player {
    /// Creates a new player
    /// 
    /// `model` - the player model
    /// 
    /// `view_aspect` - the screen aspect ratio to control the player perspective camera
    /// 
    /// `c_str` - collision string, the path of the collision mesh
    pub fn new(model: Model, view_aspect: f32, c_str: &str) -> Player {
        let root_node = Rc::new(RefCell::new(Node::default().pos(point3(100., 100., 100.))));
        let mut cam = Node::new(Some(point3(0., 15., -25.)), None, None, None);
        cam.set_parent(root_node.clone());
        let mut model = model.with_transparency(0.99, 0);
        let inv_fac = model.trans_fac();
        Player {
            cam: cam,
            aspect: view_aspect,
            em_fac: model.emissive_strength.clone(),
            entity: Rc::new(RefCell::new(entity::Entity {
                geometry: Box::new(model),
                locations: vec![root_node.clone()],
                render_passes: vec![shader::RenderPassType::Visual, 
                    shader::RenderPassType::Depth, 
                    shader::RenderPassType::transparent_tag(), 
                    shader::RenderPassType::TransparentDepth],
                order: entity::RenderOrder::Unordered,
            })),
            body: physics::RigidBody::new(root_node.clone(), Some(
                collisions::CollisionObject::new(root_node, c_str, 
                    collisions::TreeStopCriteria::default())),
                physics::BodyType::Controlled, object::ObjectType::Ship)
                    .with_density(0.88),
            inv_fac,
        }
    }

    /// Updates the players' forces based on the input controls and returns the rigid body
    pub fn as_rigid_body(&mut self, input: &controls::PlayerControls) 
        -> &mut physics::RigidBody<object::ObjectType> 
    {
        use cgmath::*;
        {
            let model : cgmath::Matrix4<f64> = 
                std::convert::From::from(&*self.body.base.transform.borrow());
            let forward = model.transform_vector(cgmath::vec3(0., 0., 1.));
            self.body.base.velocity += 
                match input.movement {
                    controls::Movement::Forward => {
                        *self.em_fac.borrow_mut() = 4.;
                        forward
                    },
                    controls::Movement::Backwards => {
                        *self.em_fac.borrow_mut() = 4.;
                        -forward
                    },
                    _ => {
                        *self.em_fac.borrow_mut() = 2.5;
                        vec3(0., 0., 0.)
                    },
                };
            self.body.base.rot_vel = vec3(input.pitch, 0., input.roll) / 10000.;
        }
        &mut self.body
        
    }

    #[inline(always)]
    #[allow(unused)]
    pub fn node(&self) -> &Rc<RefCell<Node>> {
        &self.body.base.transform
    }

    /// Gets the player's forward vector
    /// This is not the camera's forward vector, it is the player ship's
    pub fn forward(&self) -> cgmath::Vector3<f64> {
        let model : cgmath::Matrix4<f64> = 
            std::convert::From::from(&*self.body.base.transform.borrow());
        model.transform_vector(cgmath::vec3(0., 0., 1.))
    }

    /// Constructs a new perspective camera so that it has the exact 
    /// same view as the player's camera
    pub fn get_cam(&self) -> camera::PerspectiveCamera {
        camera::PerspectiveCamera {
            fov_deg: 60.,
            aspect: self.aspect,
            near: 0.1,
            far: FAR_PLANE,
            cam: self.cam_pos(),
            up: self.cam.transform_vec(vec3(0., 1., 0.))
                .cast::<f32>().unwrap(),
            target: self.body.base.transform.borrow()
                .local_pos().cast::<f32>().unwrap(),

        }
    }

    #[inline(always)]
    pub fn as_entity(&self) -> Rc<RefCell<entity::Entity>>
    {
        self.entity.clone()
    }

    /// Gets the ship transform/player root node
    #[inline(always)]
    pub fn root(&self) -> &Rc<RefCell<Node>>
    {
        &self.body.base.transform
    }

    #[inline]
    pub fn trans_fac(&self) -> std::cell::RefMut<f32> {
        self.inv_fac.borrow_mut()
    }



}
impl drawable::Viewer for Player {
    fn proj_mat(&self) -> cgmath::Matrix4<f32> {
        cgmath::perspective(cgmath::Deg::<f32>(60f32), 
            self.aspect, 0.1, FAR_PLANE)
    }

    fn cam_pos(&self) -> cgmath::Point3<f32> {
        self.cam.transform_point(point3(0., 0., 0.)).cast().unwrap()
    }

    fn view_mat(&self) -> Matrix4<f32> {
        let cam_pos = self.cam_pos();
        let view_pos = self.body.base.transform.borrow().local_pos();
        let up = self.cam.transform_vec(vec3(0., 1., 0.))
            .cast::<f32>().unwrap();
        Matrix4::look_at_rh(cam_pos, view_pos.cast::<f32>().unwrap(), up)
    }

    fn view_dist(&self) -> (f32, f32) {
        (0.1, FAR_PLANE)
    }
}