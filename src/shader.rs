use std::collections::BTreeMap;
use crate::textures;

#[derive(Copy, Clone)]
enum ShaderType {
    Ship,
    Skybox,
}
pub struct ShaderManager {
    shaders: BTreeMap<i32, (glium::Program, glium::DrawParameters<'static>)>,
    skybox: glium::texture::Cubemap,
}

#[derive(Clone)]
pub struct Matrices {
    pub viewproj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
}

pub struct UniformData<'a> {
    pub matrices: &'a Matrices,
    pub model: [[f32; 4]; 4],
    pub diffuse_tex: &'a glium::texture::SrgbTexture2d,

}

fn str_to_shader_type(material_name: &str) -> ShaderType {
    match material_name {
        "skybox" => ShaderType::Skybox,
        _ => ShaderType::Ship,
    }
}

fn shader_type_to_int(typ: &ShaderType) -> i32 {
    match typ {
        &ShaderType::Ship => 0,
        &ShaderType::Skybox => 1,
    }
}

use glium::uniforms::*;
pub enum UniformType<'a> {
    ShipUniform(UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, &'a glium::texture::SrgbTexture2d, UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    SkyboxUniform(UniformsStorage<'a, Sampler<'a, glium::texture::Cubemap>, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
}


impl ShaderManager {
    pub fn init<F : glium::backend::Facade>(facade: &F) -> ShaderManager {
        let ship_shader = glium::Program::from_source(facade, 
            &String::from_utf8_lossy(include_bytes!("shaders/basicVert.glsl")),
            &String::from_utf8_lossy(include_bytes!("shaders/basicFrag.glsl")), 
            None).unwrap();
        let skybox_shader = glium::Program::from_source(facade, 
            &String::from_utf8_lossy(include_bytes!("shaders/skyVert.glsl")),
            &String::from_utf8_lossy(include_bytes!("shaders/skyFrag.glsl")), 
            None).unwrap();
        let ship_params = glium::DrawParameters::<'static> {
            depth: glium::Depth {
                test: glium::draw_parameters::DepthTest::IfLess,
                write: true,
                .. Default::default()
            },
            backface_culling: glium::draw_parameters::BackfaceCullingMode::CullClockwise,
            .. Default::default()
        };
        let mut shaders = BTreeMap::<i32, (glium::Program, glium::DrawParameters)>::new();
        shaders.insert(0, (ship_shader, ship_params));
        shaders.insert(1, (skybox_shader, Default::default()));
        ShaderManager {
            shaders: shaders,
            skybox: textures::load_cubemap("assets/skybox/right.png", facade),
        }
    }

    pub fn use_shader<'b>(&'b self, shader: &str, data: &'b UniformData) 
        -> (&'b glium::Program, &'b glium::DrawParameters, UniformType<'b>)
    {
        let typ = str_to_shader_type(shader);
        let (shader, params) = self.shaders.get(&shader_type_to_int(&typ)).unwrap();
        let uniform = match typ {
            ShaderType::Ship => UniformType::ShipUniform(glium::uniform! {
                viewproj: data.matrices.viewproj,
                diffuse_tex: data.diffuse_tex,
                model: data.model,
            }),
            ShaderType::Skybox => UniformType::SkyboxUniform(glium::uniform! {
                view: data.matrices.view,
                proj: data.matrices.proj,
                skybox: self.skybox.sampled().magnify_filter(glium::uniforms::MagnifySamplerFilter::Linear),
            })
        };
        (shader, params, uniform)
    }
}