use std::collections::BTreeMap;

#[derive(Copy, Clone, PartialEq, Eq)]
enum ShaderType {
    BlinnPhong,
    Pbr,
    Skybox,
    EquiRect,
    UiShader,
    BlurShader,
    BloomShader,
}

fn shader_type_to_int(typ: &ShaderType) -> i32 {
    match typ {
        &ShaderType::BlinnPhong => 0,
        &ShaderType::Skybox => 1,
        &ShaderType::Pbr => 2,
        &ShaderType::EquiRect => 3,
        &ShaderType::UiShader => 4,
        &ShaderType::BloomShader => 5,
        &ShaderType::BlurShader => 6,
    }
}

impl std::cmp::Ord for ShaderType {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        shader_type_to_int(&self).cmp(&shader_type_to_int(other))
    }
}

impl std::cmp::PartialOrd for ShaderType {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub struct ShaderManager {
    shaders: BTreeMap<ShaderType, (glium::Program, glium::DrawParameters<'static>)>,
    empty_srgb: glium::texture::SrgbTexture2d,
    empty_2d: glium::texture::Texture2d,
}

#[derive(Clone)]
pub struct SceneData<'a> {
    pub viewproj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    pub cam_pos: [f32; 3],
    pub ibl_map: Option<&'a glium::texture::Cubemap>,
}

pub struct UniformData<'a> {
    pub scene_data: &'a SceneData<'a>,
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
        "ui" => ShaderType::UiShader,
        "ui-bloom" => ShaderType::BloomShader,
        "ui-blur" => ShaderType::BlurShader,
        _ => ShaderType::BlinnPhong,
    }
}

use glium::uniforms::*;
pub enum UniformType<'a> {
    BSUniform(UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, Sampler<'a, glium::texture::SrgbTexture2d>, UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    SkyboxUniform(UniformsStorage<'a, Sampler<'a, glium::texture::Cubemap>, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    PbrUniform(UniformsStorage<'a, Sampler<'a, glium::texture::Cubemap>, UniformsStorage<'a, Sampler<'a, glium::texture::SrgbTexture2d>, 
        UniformsStorage<'a, [f32; 3], UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, 
        UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, 
        UniformsStorage<'a, Sampler<'a, glium::texture::SrgbTexture2d>, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>>>>>>>),
    EqRectUniform(UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    BloomUniform(UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>),
    UiUniform(UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, bool, UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, 
        UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>>),
    BlurUniform(UniformsStorage<'a, bool, UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    
}

macro_rules! sample_mip_repeat {
    ($tex_name:expr) => {
        $tex_name.sampled().magnify_filter(glium::uniforms::MagnifySamplerFilter::Linear)
        .minify_filter(glium::uniforms::MinifySamplerFilter::LinearMipmapLinear)
        .wrap_function(glium::uniforms::SamplerWrapFunction::Repeat)
    }
}

macro_rules! sample_linear_clamp {
    ($tex_name:expr) => {
        $tex_name.sampled().magnify_filter(glium::uniforms::MagnifySamplerFilter::Linear)
        .minify_filter(glium::uniforms::MinifySamplerFilter::Linear)
        .wrap_function(glium::uniforms::SamplerWrapFunction::Clamp)
    }
}

macro_rules! include_str {
    ($file:literal) => {
        &String::from_utf8_lossy(include_bytes!($file))
    };
}

macro_rules! load_shader_source {
    ($facade:expr, $vertex_file:literal, $fragment_file:literal, $geom_file:literal) => {
        glium::Program::from_source($facade,
            include_str!($vertex_file), include_str!($fragment_file), include_str!($geom_option))
    };
    ($facade:expr, $vertex_file:literal, $fragment_file:literal) => {
        glium::Program::from_source($facade,
            include_str!($vertex_file), include_str!($fragment_file), None)
    };
}

macro_rules! load_shader_srgb {
    ($facade:expr, $vertex_file:literal, $fragment_file:literal) => {
        glium::Program::new($facade,
            glium::program::ProgramCreationInput::SourceCode {
                vertex_shader: include_str!($vertex_file),
                tessellation_control_shader: None,
                tessellation_evaluation_shader: None,
                geometry_shader: None,
                fragment_shader: include_str!($fragment_file),
                transform_feedback_varyings: None,
                outputs_srgb: true,
                uses_point_size: false,
            })
    };
}




impl ShaderManager {
    pub fn init<F : glium::backend::Facade>(facade: &F) -> ShaderManager {
        let ship_shader = load_shader_source!(facade, 
            "shaders/basicVert.glsl", "shaders/basicFrag.glsl").unwrap();
        let skybox_shader = load_shader_source!(facade, 
            "shaders/skyVert.glsl", "shaders/skyFrag.glsl").unwrap();
        let pbr_shader = load_shader_source!(facade,
            "shaders/pbrVert.glsl", "shaders/pbrFrag.glsl").unwrap();
        let equirect_shader = load_shader_srgb!(facade, 
            "shaders/skyVert.glsl", "shaders/eqRectFrag.glsl").unwrap();
        let ui_shader = load_shader_source!(facade, 
            "shaders/hdrVert.glsl", "shaders/hdrFrag.glsl").unwrap();
        let bloom_shader = load_shader_source!(facade,
            "shaders/hdrVert.glsl", "shaders/bloomFrag.glsl").unwrap();
        let blur_shader = load_shader_source!(facade,
            "shaders/hdrVert.glsl", "shaders/blurFrag.glsl").unwrap();
        let ship_params = glium::DrawParameters::<'static> {
            depth: glium::Depth {
                test: glium::draw_parameters::DepthTest::IfLess,
                write: true,
                .. Default::default()
            },
            backface_culling: glium::draw_parameters::BackfaceCullingMode::CullClockwise,
            .. Default::default()
        };
        let mut shaders = BTreeMap::<ShaderType, (glium::Program, glium::DrawParameters)>::new();
        shaders.insert(ShaderType::BlinnPhong, (ship_shader, ship_params.clone()));
        shaders.insert(ShaderType::Skybox, (skybox_shader, Default::default()));
        shaders.insert(ShaderType::Pbr, (pbr_shader, ship_params));
        shaders.insert(ShaderType::EquiRect, (equirect_shader, Default::default()));
        shaders.insert(ShaderType::UiShader, (ui_shader, Default::default()));
        shaders.insert(ShaderType::BlurShader, (blur_shader, Default::default()));
        shaders.insert(ShaderType::BloomShader, (bloom_shader, Default::default()));
        ShaderManager {
            shaders: shaders,
            empty_srgb: glium::texture::SrgbTexture2d::empty(facade, 0, 0).unwrap(),
            empty_2d: glium::texture::Texture2d::empty(facade, 0, 0).unwrap(),
        }
    }

    pub fn use_shader<'b>(&'b self, shader: &str, data: &'b UniformData) 
        -> (&'b glium::Program, &'b glium::DrawParameters, UniformType<'b>)
    {
        let typ = str_to_shader_type(shader);
        let (shader, params) = self.shaders.get(&typ).unwrap();
        let uniform = match typ {
            ShaderType::BlinnPhong => UniformType::BSUniform(glium::uniform! {
                viewproj: data.scene_data.viewproj,
                diffuse_tex: sample_mip_repeat!(data.diffuse_tex.unwrap()),
                model: data.model,
            }),
            ShaderType::Skybox => UniformType::SkyboxUniform(glium::uniform! {
                view: data.scene_data.view,
                proj: data.scene_data.proj,
                skybox: sample_linear_clamp!(data.env_map.unwrap()),
            }),
            ShaderType::EquiRect => UniformType::EqRectUniform(glium::uniform! {
                view: data.scene_data.view,
                proj: data.scene_data.proj,
                equirectangular_map: sample_linear_clamp!(data.normal_map.unwrap()),
            }),
            ShaderType::Pbr => UniformType::PbrUniform(glium::uniform! {
                viewproj: data.scene_data.viewproj,
                model: data.model,
                albedo_map: sample_mip_repeat!(data.diffuse_tex.unwrap()),
                roughness_map: sample_mip_repeat!(data.roughness_map.unwrap()),
                normal_map: sample_mip_repeat!(data.normal_map.unwrap()),
                metallic_map: sample_mip_repeat!(data.metallic_map.unwrap()),
                cam_pos: data.scene_data.cam_pos,
                emission_map: sample_mip_repeat!(data.emission_map.unwrap_or(&self.empty_srgb)),
                irradiance_map: sample_linear_clamp!(data.scene_data.ibl_map.unwrap()),
            }),
            ShaderType::UiShader => UniformType::UiUniform(glium::uniform! {
                model: data.model,
                diffuse: sample_linear_clamp!(data.normal_map.unwrap()),
                do_blend: data.roughness_map.is_some(),
                bloom_tex: sample_linear_clamp!(data.roughness_map.unwrap_or(&self.empty_2d)),
            }),
            ShaderType::BlurShader => UniformType::BlurUniform(glium::uniform! {
                model: data.model,
                diffuse: sample_linear_clamp!(
                    data.normal_map.unwrap_or_else(|| data.roughness_map.unwrap())),
                horizontal_pass: data.normal_map.is_some(),
            }),
            ShaderType::BloomShader => UniformType::BloomUniform(glium::uniform! {
                model: data.model,
                diffuse: sample_linear_clamp!(data.normal_map.unwrap()),
            })
        };
        (shader, params, uniform)
    }
}