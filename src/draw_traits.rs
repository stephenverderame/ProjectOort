use crate::shader;
pub trait Drawable {
    fn render<S>(&self, frame: &mut S, mats: &shader::SceneData, shader: &shader::ShaderManager)
        where S : glium::Surface;
}

pub trait Viewer {
    fn proj_mat(&self, aspect: f32) -> cgmath::Matrix4<f32>;

    fn cam_pos(&self) -> cgmath::Point3<f32>;

    fn view_mat(&self) -> cgmath::Matrix4<f32>;
}