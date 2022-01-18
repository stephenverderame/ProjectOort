

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

use cgmath::*;

const FAR_PLANE : f32 = 1000.;

/// The player is the combination of the player's entity and the player's camera
pub struct Player {
    cam: Node,
    entity: Rc<RefCell<entity::Entity>>,
    pub aspect: f32,
    body: physics::RigidBody,
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
        let root_node = Rc::new(RefCell::new(Node::new(None, None, None, None)));
        let mut cam = Node::new(Some(point3(0., 15., -25.)), None, None, None);
        cam.set_parent(root_node.clone());
        Player {
            cam: cam,
            aspect: view_aspect,
            entity: Rc::new(RefCell::new(entity::Entity {
                geometry: Box::new(model),
                locations: vec![root_node.clone()],
                render_passes: vec![shader::RenderPassType::Visual, shader::RenderPassType::Depth],
            })),
            body: physics::RigidBody::new(root_node, Some(
                collisions::CollisionObject::new(root_node.clone(), c_str, collisions::TreeStopCriteria::default())),
                physics::BodyType::Dynamic),
        }
    }

    /// Updates the players' forces based on the input controls and returns the rigid body
    pub fn as_rigid_body(&mut self, input: &controls::PlayerControls) -> &physics::RigidBody {
        use cgmath::*;
        let model : cgmath::Matrix4<f64> = std::convert::From::from(&*self.body.transform.borrow());
        let transform = &mut *self.body.transform.borrow_mut();
        let forward = model.transform_vector(cgmath::vec3(0., 0., 1.));
        self.body.velocity = 
            match input.movement {
                controls::Movement::Forward => forward * 30f64,
                controls::Movement::Backwards => forward * -10f64,
                _ => vec3(0., 0., 0.),
            };
        self.body.rot_vel = Euler::<Deg<f64>>::new(Deg::<f64>(input.pitch), 
            Deg::<f64>(0.), Deg::<f64>(input.roll)).into();
        &self.body
        
    }

    /// Gets the player's forward vector
    /// This is not the camera's forward vector, it is the player ship's
    pub fn forward(&self) -> cgmath::Vector3<f64> {
        let model : cgmath::Matrix4<f64> = std::convert::From::from(&*self.body.transform.borrow());
        model.transform_vector(cgmath::vec3(0., 0., 1.))
    }

    /// Constructs a new perspective camera so that it has the exact same view as the player's camera
    pub fn get_cam(&self) -> camera::PerspectiveCamera {
        camera::PerspectiveCamera {
            fov_deg: 60.,
            aspect: self.aspect,
            near: 0.1,
            far: FAR_PLANE,
            cam: self.cam_pos(),
            up: Matrix4::<f64>::from(&self.cam).transform_vector(vec3(0., 1., 0.)).cast::<f32>().unwrap(),
            target: self.body.transform.borrow().pos.cast::<f32>().unwrap(),

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
        &self.body.transform
    }



}
impl drawable::Viewer for Player {
    fn proj_mat(&self) -> cgmath::Matrix4<f32> {
        cgmath::perspective(cgmath::Deg::<f32>(60f32), self.aspect, 0.1, FAR_PLANE)
    }

    fn cam_pos(&self) -> cgmath::Point3<f32> {
        let trans = Matrix4::<f32>::from(&self.cam);
        trans.transform_point(point3(0., 0., 0.))
    }

    fn view_mat(&self) -> Matrix4<f32> {
        let cam_pos = self.cam_pos();
        let view_pos = self.body.transform.borrow().pos;
        let up = Matrix4::<f64>::from(&self.cam).transform_vector(vec3(0., 1., 0.))
            .cast::<f32>().unwrap();
        Matrix4::look_at_rh(cam_pos, view_pos.cast::<f32>().unwrap(), up)
    }

    fn view_dist(&self) -> (f32, f32) {
        (0.1, FAR_PLANE)
    }
}