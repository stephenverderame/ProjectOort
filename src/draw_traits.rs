use crate::shader;
/// Something that can be drawn
pub trait Drawable {
    /// Draws the drawable to the given surface `frame`, with the provided scene information
    /// and shader manager.
    fn render<S>(&self, frame: &mut S, mats: &shader::SceneData, local_data: &shader::PipelineCache, shader: &shader::ShaderManager)
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

pub fn viewer_data_from(viewer: &dyn Viewer, aspect: f32) -> shader::ViewerData {
    let view = viewer.view_mat();
    let proj = viewer.proj_mat(aspect);
    shader::ViewerData {
        viewproj: (proj * view).into(),
        view: view.into(),
        proj: proj.into(),
        cam_pos: viewer.cam_pos().into(),
    }
}

/// Gets the default scene data filled with the relevant matrices according to
/// `viewer` and the aspect ratio `aspect`.
/// 
/// All other scene information is set to `None`
pub fn default_scene_data(viewer: &dyn Viewer, aspect: f32) -> shader::SceneData {
    
    shader::SceneData {
        viewer: viewer_data_from(viewer, aspect),
        ibl_maps: None,
        lights: None,
        pass_type: shader::RenderPassType::Visual,
        light_viewproj: None,
        light_pos: None,
    }
}