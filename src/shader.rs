use std::collections::BTreeMap;

#[derive(Copy, Clone)]
enum ShaderType {
    BlinnPhong,
    Pbr,
    Skybox,
    EquiRect,
}
pub struct ShaderManager {
    shaders: BTreeMap<i32, (glium::Program, glium::DrawParameters<'static>)>,
    empty_srgb: glium::texture::SrgbTexture2d,
}

#[derive(Clone)]
pub struct Matrices {
    pub viewproj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    pub cam_pos: [f32; 3],
}

pub struct UniformData<'a> {
    pub matrices: &'a Matrices,
    pub model: [[f32; 4]; 4],
    pub diffuse_tex: Option<&'a glium::texture::SrgbTexture2d>,
    pub roughness_map: Option<&'a glium::texture::Texture2d>,
    pub metallic_map: Option<&'a glium::texture::Texture2d>,
    pub normal_map: Option<&'a glium::texture::Texture2d>,
    pub emission_map: Option<&'a glium::texture::SrgbTexture2d>,
    pub env_map: Option<&'a glium::texture::Cubemap>,


}

fn str_to_shader_type(material_name: &str) -> ShaderType {
    match material_name {
        "skybox" => ShaderType::Skybox,
        x if x.find("pbr").is_some() => ShaderType::Pbr,
        "equirectangular" => ShaderType::EquiRect,
        _ => ShaderType::BlinnPhong,
    }
}

fn shader_type_to_int(typ: &ShaderType) -> i32 {
    match typ {
        &ShaderType::BlinnPhong => 0,
        &ShaderType::Skybox => 1,
        &ShaderType::Pbr => 2,
        &ShaderType::EquiRect => 3,
    }
}

use glium::uniforms::*;
pub enum UniformType<'a> {
    BSUniform(UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, &'a glium::texture::SrgbTexture2d, UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    SkyboxUniform(UniformsStorage<'a, Sampler<'a, glium::texture::Cubemap>, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    PbrUniform(UniformsStorage<'a, &'a glium::texture::SrgbTexture2d, UniformsStorage<'a, [f32; 3], UniformsStorage<'a, &'a glium::texture::Texture2d, 
        UniformsStorage<'a, &'a glium::texture::Texture2d, UniformsStorage<'a, &'a glium::texture::Texture2d, 
        UniformsStorage<'a, &'a glium::texture::SrgbTexture2d, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>>>>>>),
    EqRectUniform(UniformsStorage<'a, &'a glium::texture::Texture2d, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
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
        let pbr_shader = glium::Program::from_source(facade,
            &String::from_utf8_lossy(include_bytes!("shaders/pbrVert.glsl")),
            &String::from_utf8_lossy(include_bytes!("shaders/pbrFrag.glsl")), 
            None).unwrap();
        let equirect_shader = glium::Program::from_source(facade, 
            &String::from_utf8_lossy(include_bytes!("shaders/skyVert.glsl")),
            &String::from_utf8_lossy(include_bytes!("shaders/eqRectFrag.glsl")), 
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
        shaders.insert(0, (ship_shader, ship_params.clone()));
        shaders.insert(1, (skybox_shader, Default::default()));
        shaders.insert(2, (pbr_shader, ship_params));
        shaders.insert(3, (equirect_shader, Default::default()));
        ShaderManager {
            shaders: shaders,
            empty_srgb: glium::texture::SrgbTexture2d::empty(facade, 0, 0).unwrap(),
        }
    }

    pub fn use_shader<'b>(&'b self, shader: &str, data: &'b UniformData) 
        -> (&'b glium::Program, &'b glium::DrawParameters, UniformType<'b>)
    {
        let typ = str_to_shader_type(shader);
        let (shader, params) = self.shaders.get(&shader_type_to_int(&typ)).unwrap();
        let uniform = match typ {
            ShaderType::BlinnPhong => UniformType::BSUniform(glium::uniform! {
                viewproj: data.matrices.viewproj,
                diffuse_tex: data.diffuse_tex.unwrap(),
                model: data.model,
            }),
            ShaderType::Skybox => UniformType::SkyboxUniform(glium::uniform! {
                view: data.matrices.view,
                proj: data.matrices.proj,
                skybox: data.env_map.unwrap().sampled().magnify_filter(glium::uniforms::MagnifySamplerFilter::Linear),
            }),
            ShaderType::EquiRect => UniformType::EqRectUniform(glium::uniform! {
                view: data.matrices.view,
                proj: data.matrices.proj,
                equirectangular_map: data.normal_map.unwrap(),
            }),
            ShaderType::Pbr => UniformType::PbrUniform(glium::uniform! {
                viewproj: data.matrices.viewproj,
                model: data.model,
                albedo_map: data.diffuse_tex.unwrap(),
                roughness_map: data.roughness_map.unwrap(),
                normal_map: data.normal_map.unwrap(),
                metallic_map: data.metallic_map.unwrap(),
                cam_pos: data.matrices.cam_pos,
                emission_map: data.emission_map.unwrap_or(&self.empty_srgb),
            })
        };
        (shader, params, uniform)
    }
}