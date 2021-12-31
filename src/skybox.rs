use crate::shader;
use crate::draw_traits;

#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 3],
}

glium::implement_vertex!(Vertex, pos);


pub enum SkyboxTex {
    Cube(glium::texture::Cubemap),
    Sphere(glium::texture::Texture2d), //equirectangular spherical texture
}

pub struct Skybox {
    vbo: glium::VertexBuffer<Vertex>,
    ebo: glium::IndexBuffer<u16>,
    tex: SkyboxTex,
    mip_progress: Option<f32>,
}

impl Skybox {
    pub fn new<F>(tex: SkyboxTex, facade: &F) -> Skybox where F : glium::backend::Facade {
        let verts: [Vertex; 8] = [Vertex { pos: [-1.0, -1.0, 1.0] },
            Vertex { pos: [1.0, -1.0, 1.0] },
            Vertex { pos: [1.0, 1.0, 1.0] },
            Vertex { pos: [-1.0, 1.0, 1.0] },
            Vertex { pos: [-1.0, -1.0, -1.0] },
            Vertex { pos: [1.0, -1.0, -1.0] },
            Vertex { pos: [1.0, 1.0, -1.0] },
            Vertex { pos: [-1.0, 1.0, -1.0] }];
        let indices: [u16; 36] = [0, 1, 2, 2, 3, 0, 1, 5, 6, 6, 2, 1, 7, 6, 5, 5, 4, 7, 4, 0, 3, 3, 7,
            4, 4, 5, 1, 1, 0, 4, 3, 2, 6, 6, 7, 3];
        Skybox {
            vbo: glium::VertexBuffer::new(facade, &verts).unwrap(),
            ebo: glium::IndexBuffer::new(facade, glium::index::PrimitiveType::TrianglesList, &indices).unwrap(),
            tex: tex, mip_progress: None,
        }
    }

    /// Sets the progress value of the mipmap progress
    /// If this function is used, skybox will render to a filtered skybox shader
    /// which takes this parameter as an argument to control different outputs based on the
    /// mip level of the render target. This does nothing if using a spherical equirectangular texture
    /// 
    /// If `progress` is none, disables mipping. Otherwise `0 <= progress <= 1`
    pub fn set_mip_progress(&mut self, progress: Option<f32>) {
        self.mip_progress = progress;
    }
}

impl draw_traits::Drawable for Skybox {
    fn render<S : glium::Surface>(&self, frame: &mut S, mats: &shader::SceneData, local_data: &shader::PipelineCache, shader: &shader::ShaderManager) {
        let args = match (&self.tex, self.mip_progress) {
            (SkyboxTex::Sphere(map), _) => shader::UniformInfo::EquiRectInfo(shader::EqRectData {
                env_map: map,
            }),
            (SkyboxTex::Cube(map), None) => shader::UniformInfo::SkyboxInfo(shader::SkyboxData {
                env_map: map,
            }),
            (SkyboxTex::Cube(map), Some(progress)) => shader::UniformInfo::PrefilterHdrEnvInfo(
                shader::PrefilterHdrEnvData {
                env_map: map,
                roughness: progress,
            }),
        };
        let (program, params, uniform) = shader.use_shader(&args, Some(mats), Some(local_data));
        match uniform {
            shader::UniformType::SkyboxUniform(uniform) =>
                frame.draw(&self.vbo, &self.ebo, program, &uniform, &params).unwrap(),
            shader::UniformType::EqRectUniform(uniform) =>
                frame.draw(&self.vbo, &self.ebo, program, &uniform, &params).unwrap(),
            shader::UniformType::PrefilterHdrEnvUniform(uniform) =>
                frame.draw(&self.vbo, &self.ebo, program, &uniform, &params).unwrap(),
            _ => panic!("Invalid uniform type returned for skybox"),
        }
    }
}