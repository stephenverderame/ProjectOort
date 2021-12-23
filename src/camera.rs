use crate::node;
use crate::draw_traits;
use cgmath::*;

pub struct PerspectiveCamera {
    pub cam: node::Node,
    pub aspect: f32,
    pub fov_deg: f32,
    pub target: cgmath::Point3<f32>,
    pub near: f32,
    pub far: f32,
    pub up: cgmath::Vector3<f32>,

}

impl draw_traits::Viewer for PerspectiveCamera {
    fn proj_mat(&self, aspect: f32) -> cgmath::Matrix4<f32> {
        cgmath::perspective(cgmath::Deg::<f32>(self.fov_deg), aspect, self.near, self.far)
    }

    fn cam_pos(&self) -> cgmath::Point3<f32> {
        let trans = Matrix4::<f32>::from(&self.cam);
        trans.transform_point(point3(0., 0., 0.))
    }

    fn view_mat(&self) -> Matrix4<f32> {
        let cam_pos = self.cam_pos();
        Matrix4::look_at_rh(cam_pos, self.target, self.up)
    }
}