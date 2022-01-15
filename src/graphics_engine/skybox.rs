use super::shader;
use super::drawable::*;
use VertexPos as Vertex;

const CUBE_VERTS: [Vertex; 8] = [Vertex { pos: [-1.0, -1.0, 1.0] },
    Vertex { pos: [1.0, -1.0, 1.0] },
    Vertex { pos: [1.0, 1.0, 1.0] },
    Vertex { pos: [-1.0, 1.0, 1.0] },
    Vertex { pos: [-1.0, -1.0, -1.0] },
    Vertex { pos: [1.0, -1.0, -1.0] },
    Vertex { pos: [1.0, 1.0, -1.0] },
    Vertex { pos: [-1.0, 1.0, -1.0] }];

const CUBE_INDICES: [u16; 36] = [0, 1, 2, 2, 3, 0, 1, 5, 6, 6, 2, 1, 7, 6, 5, 5, 4, 7, 4, 0, 3, 3, 7,
    4, 4, 5, 1, 1, 0, 4, 3, 2, 6, 6, 7, 3];

/// The type of texture for the skybox. Either a cubemap or a 2d texture
/// storing an equirectangular spherical image
pub enum SkyboxTex {
    Cube(glium::texture::Cubemap),
    Sphere(glium::texture::Texture2d), //equirectangular spherical texture
}

/// A cube textured by a cubemap or equirectangular texture that is always centered around the camera
pub struct Skybox {
    vbo: glium::VertexBuffer<Vertex>,
    ebo: glium::IndexBuffer<u16>,
    tex: SkyboxTex,
    mip_progress: Option<f32>,
}

impl Skybox {
    pub fn new<F>(tex: SkyboxTex, facade: &F) -> Skybox where F : glium::backend::Facade {
        Skybox {
            vbo: glium::VertexBuffer::new(facade, &CUBE_VERTS).unwrap(),
            ebo: glium::IndexBuffer::new(facade, glium::index::PrimitiveType::TrianglesList, &CUBE_INDICES).unwrap(),
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

impl Drawable for Skybox {
    /*fn render<S : glium::Surface>(&self, frame: &mut S, mats: &shader::SceneData, local_data: &shader::PipelineCache, shader: &shader::ShaderManager) {
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
    }*/
    fn render_args<'a>(&'a self, _: &[[[f32; 4]; 4]]) -> Vec<(shader::UniformInfo, VertexHolder<'a>, glium::index::IndicesSource<'a>)>
    {
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
        vec![(args, VertexHolder::new(VertexSourceData::Single(From::from(&self.vbo))), From::from(&self.ebo))]
    }

    fn should_render(&self, pass: &shader::RenderPassType) -> bool {
        pass == shader::RenderPassType::Visual
    }
}

pub struct DebugCube {
    vbo: glium::VertexBuffer<Vertex>,
    ebo: glium::IndexBuffer<u16>,
}

impl DebugCube {
    pub fn new<F : glium::backend::Facade>(facade: &F) -> DebugCube {
        DebugCube {
            vbo: glium::VertexBuffer::new(facade, &CUBE_VERTS).unwrap(),
            ebo: glium::IndexBuffer::new(facade, glium::index::PrimitiveType::TrianglesList, &CUBE_INDICES).unwrap(),
        }
    }
}

impl Drawable for DebugCube {
    /*fn render<S : glium::Surface>(&self, frame: &mut S, mats: &shader::SceneData, local_data: &shader::PipelineCache, shader: &shader::ShaderManager) {
        let args = shader::UniformInfo::CollisionDebugInfo(self.transform.cast::<f32>().unwrap().into());
        let (program, params, uniform) = shader.use_shader(&args, Some(mats), Some(local_data));
        match uniform {
            shader::UniformType::DepthUniform(uniform) =>
                frame.draw(&self.vbo, &self.ebo, program, &uniform, &params).unwrap(),
            _ => panic!("Invalid uniform type returned for skybox"),
        }
    }*/
    fn render_args<'a>(&'a self, models: &[[[f32; 4]; 4]]) -> Vec<(shader::UniformInfo, VertexHolder<'a>, glium::index::IndicesSource<'a>)>
    {
        models.into_iter().map(|x| {
            let args = shader::UniformInfo::CollisionDebugInfo(*x);
            (args, VertexHolder::new(VertexSourceData::Single(From::from(&self.vbo))), From::from(&self.ebo))
        }).collect()
    }

    fn should_render(&self, pass: &shader::RenderPassType) -> bool {
        pass == shader::RenderPassType::Visual
    }
}