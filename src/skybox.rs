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
            tex: tex,
        }
    }
}

impl draw_traits::Drawable for Skybox {
    fn render<S : glium::Surface>(&self, frame: &mut S, mats: &shader::SceneData, shader: &shader::ShaderManager) {
        let args = shader::UniformData {
            scene_data: mats,
            model: cgmath::Matrix4::from_scale(1f32).into(),
            diffuse_tex: None,
            roughness_map: None,
            metallic_map: None,
            normal_map: match &self.tex {
                SkyboxTex::Sphere(map) => Some(map),
                SkyboxTex::Cube(_) => None,
            },
            emission_map: None,
            env_map: match &self.tex {
                SkyboxTex::Cube(map) => Some(map),
                SkyboxTex::Sphere(_) => None,
            },
        };
        let shader_name = match &self.tex {
            SkyboxTex::Cube(_) => "skybox",
            SkyboxTex::Sphere(_) => "equirectangular",
        };
        let (program, params, uniform) = shader.use_shader(shader_name, &args);
        match uniform {
            shader::UniformType::SkyboxUniform(uniform) =>
                frame.draw(&self.vbo, &self.ebo, program, &uniform, &params).unwrap(),
            shader::UniformType::EqRectUniform(uniform) =>
                frame.draw(&self.vbo, &self.ebo, program, &uniform, &params).unwrap(),
            _ => panic!("Invalid uniform type returned for skybox"),
        }
    }
}