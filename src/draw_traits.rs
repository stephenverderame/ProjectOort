use crate::shader;
/// Something that can be drawn
pub trait Drawable {
    /// Draws the drawable to the given surface `frame`, with the provided scene information
    /// and shader manager.
    fn render<S>(&self, frame: &mut S, mats: &shader::SceneData, shader: &shader::ShaderManager)
        where S : glium::Surface;
}

/// Something that encapsulates control of a view of the scene
pub trait Viewer {
    /// Gets the viewer's projection matrix
    fn proj_mat(&self, aspect: f32) -> cgmath::Matrix4<f32>;

    /// Gets the viewer's position
    fn cam_pos(&self) -> cgmath::Point3<f32>;

    /// Gets the viewer's view matrix
    fn view_mat(&self) -> cgmath::Matrix4<f32>;
}