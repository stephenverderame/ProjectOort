

use crate::node::*;
use crate::model::Model;
use crate::shader;
use std::rc::{Rc};
use std::cell::RefCell;

use cgmath::*;

pub struct Player {
    root: Rc<RefCell<Node>>,
    cam: Node,
    geom: Model,
}

impl Player {
    pub fn new(model: Model) -> Player {
        let root_node = Rc::new(RefCell::new(Node::new(None, None, None, None)));
        let mut cam = Node::new(Some(point3(0., 20., -20.)), None, None, None);
        cam.set_parent(root_node.clone());
        Player {
            root: root_node,
            cam: cam,
            geom: model,
        }
    }

    pub fn view_mat(&self) -> Matrix4<f32> {
        let cam_pos = self.cam_pos();
        let view_pos = self.root.borrow().pos;
        Matrix4::look_at_rh(cam_pos, view_pos, vec3(0., 1., 0.))
    }

    pub fn set_rot(&mut self, rot: Quaternion<f32>) {
        self.root.borrow_mut().orientation = Rot::Quat(rot);
    }

    pub fn render(&self, display: &mut glium::Frame, mats: &shader::Matrices, shaders: &shader::ShaderManager) {
        let model : Matrix4<f32> = Matrix4::<f32>::from(&*self.root.borrow());
        self.geom.render(display, mats, model.into(), shaders)
    }

    pub fn cam_pos(&self) -> cgmath::Point3<f32> {
        let trans = Matrix4::<f32>::from(&self.cam);
        trans.transform_point(point3(0., 0., 0.))
    }
}