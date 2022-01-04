

use crate::node::*;
use crate::model::Model;
use crate::shader;
use crate::draw_traits;
use crate::controls;
use std::rc::{Rc};
use std::cell::RefCell;
use crate::camera;
use draw_traits::Viewer;

use cgmath::*;

fn far() -> f32 { 200f32 }

/// The player is the combination of the player's entity and the player's camera
pub struct Player {
    pub root: Rc<RefCell<Node>>,
    cam: Node,
    geom: Model,
    pub aspect: f32,
}

impl Player {
    pub fn new(model: Model, view_aspect: f32) -> Player {
        let root_node = Rc::new(RefCell::new(Node::new(None, None, None, None)));
        let mut cam = Node::new(Some(point3(0., 15., -25.)), None, None, None);
        cam.set_parent(root_node.clone());
        Player {
            root: root_node,
            cam: cam,
            geom: model,
            aspect: view_aspect,
        }
    }
    /// Moves the player based on user input
    /// 
    /// `dt` - seconds per frame
    pub fn move_player(&mut self, input: &controls::PlayerControls, dt: f64) {
        use cgmath::*;
        let model : cgmath::Matrix4<f64> = std::convert::From::from(&*self.root.borrow());
        let transform = &mut *self.root.borrow_mut();
        let forward = model.transform_vector(cgmath::vec3(0., 0., 1.) * dt);
        match input.movement {
            controls::Movement::Forward => transform.pos += forward * 30f64,
            controls::Movement::Backwards => transform.pos -= forward * 10f64,
            _ => (),
        }
        let q : Quaternion<f64> = Euler::<Deg<f64>>::new(Deg::<f64>(input.pitch), 
            Deg::<f64>(0.), Deg::<f64>(input.roll)).into();
        transform.orientation = transform.orientation * q;
    }

    pub fn forward(&self) -> cgmath::Vector3<f64> {
        let model : cgmath::Matrix4<f64> = std::convert::From::from(&*self.root.borrow());
        model.transform_vector(cgmath::vec3(0., 0., 1.))
    }

    pub fn get_cam(&self) -> camera::PerspectiveCamera {
        camera::PerspectiveCamera {
            fov_deg: 60.,
            aspect: self.aspect,
            near: 0.1,
            far: far(),
            cam: self.cam_pos(),
            up: Matrix4::<f64>::from(&self.cam).transform_vector(vec3(0., 1., 0.)).cast::<f32>().unwrap(),
            target: self.root.borrow().pos.cast::<f32>().unwrap(),

        }
    }


}
impl draw_traits::Viewer for Player {
    fn proj_mat(&self) -> cgmath::Matrix4<f32> {
        cgmath::perspective(cgmath::Deg::<f32>(60f32), self.aspect, 0.1, far())
    }

    fn cam_pos(&self) -> cgmath::Point3<f32> {
        let trans = Matrix4::<f32>::from(&self.cam);
        trans.transform_point(point3(0., 0., 0.))
    }

    fn view_mat(&self) -> Matrix4<f32> {
        let cam_pos = self.cam_pos();
        let view_pos = self.root.borrow().pos;
        let up = Matrix4::<f64>::from(&self.cam).transform_vector(vec3(0., 1., 0.))
            .cast::<f32>().unwrap();
        Matrix4::look_at_rh(cam_pos, view_pos.cast::<f32>().unwrap(), up)
    }

    fn view_dist(&self) -> (f32, f32) {
        (0.1, far())
    }
}

impl draw_traits::Drawable for Player {
    fn render<S : glium::Surface>(&self, display: &mut S, mats: &shader::SceneData, local_data: &shader::PipelineCache, shaders: &shader::ShaderManager) {
        let model : Matrix4<f32> = Matrix4::<f32>::from(&*self.root.borrow());
        self.geom.render(display, mats, local_data, model.into(), shaders)
    }
}