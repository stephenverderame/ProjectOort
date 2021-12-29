use std::collections::BTreeMap;

#[derive(Copy, Clone, PartialEq, Eq)]
enum ShaderType {
    Laser,
    Pbr,
    Skybox,
    EquiRect,
    UiShader,
    BlurShader,
    BloomShader,
    PrefilterHdrShader,
    GenLutShader,
}

/// Converts a shader type to an integer
/// This is to allow shader types to be keys in maps
fn shader_type_to_int(typ: &ShaderType) -> i32 {
    match typ {
        &ShaderType::Laser => 0,
        &ShaderType::Skybox => 1,
        &ShaderType::Pbr => 2,
        &ShaderType::EquiRect => 3,
        &ShaderType::UiShader => 4,
        &ShaderType::BloomShader => 5,
        &ShaderType::BlurShader => 6,
        &ShaderType::PrefilterHdrShader => 7,
        &ShaderType::GenLutShader => 8,
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

//glium::implement_buffer_content!(LightInfo);
//glium::implement_uniform_block!(LightInfo, pos);

#[derive(Copy, Clone)]
pub struct LightBuffer {
    pub light_num: u32,
    pub padding: [u32; 3],
    pub light_starts: [[f32; 4]; 1024],
    pub light_ends: [[f32; 4]; 1024],
}

//glium::implement_buffer_content!(LightBuffer);
glium::implement_uniform_block!(LightBuffer, light_num, light_starts, light_ends);

/// The ShaderManager stores all shaders and all draw parameters for each shader
/// It converts shader inputs to OpenGL uniform parameters and selects the shader
/// based on those shader inputs
pub struct ShaderManager {
    shaders: BTreeMap<ShaderType, (glium::Program, glium::DrawParameters<'static>)>,
    empty_srgb: glium::texture::SrgbTexture2d,
    empty_2d: glium::texture::Texture2d,
}

/// Precomputed integrals for PBR environment maps
pub struct PbrMaps {
    pub diffuse_ibl: glium::texture::Cubemap,
    pub spec_ibl: glium::texture::Cubemap,
    pub brdf_lut: glium::texture::Texture2d,
}

/// Stores scene-wide information such as view and projection matrices
/// and IBL images
pub struct SceneData<'a> {
    pub viewproj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    pub cam_pos: [f32; 3],
    pub ibl_maps: Option<&'a PbrMaps>,
    pub lights: Option<&'a glium::uniforms::UniformBuffer<LightBuffer>>,
}
/// Shader inputs for PBR shader
pub struct PBRData<'a> {
    pub scene_data: &'a SceneData<'a>,
    pub model: [[f32; 4]; 4],
    pub diffuse_tex: &'a glium::texture::SrgbTexture2d,
    pub roughness_map: Option<&'a glium::texture::Texture2d>,
    pub metallic_map: Option<&'a glium::texture::Texture2d>,
    pub normal_map: Option<&'a glium::texture::Texture2d>,
    pub emission_map: Option<&'a glium::texture::SrgbTexture2d>,
    pub ao_map: Option<&'a glium::texture::Texture2d>,
}
/// Shader inputs for Spherical Texture shader
pub struct EqRectData<'a> {
    pub scene_data: &'a SceneData<'a>,
    pub env_map: &'a glium::texture::Texture2d,
}
/// Shader inputs for Skybox shader
pub struct SkyboxData<'a> {
    pub scene_data: &'a SceneData<'a>,
    pub env_map: &'a glium::texture::Cubemap,
}
/// Shader inputs for Ui shader
pub struct UiData<'a> {
    pub model: [[f32; 4]; 4],
    pub diffuse: &'a glium::texture::Texture2d,
    pub do_blend: bool,
    pub blend_tex: Option<&'a glium::texture::Texture2d>,
}
/// Shader inputs for seperable convolutions
pub struct SepConvData<'a> {
    pub tex: &'a glium::texture::Texture2d,
    pub horizontal_pass: bool,
}
/// Shader inputs for extracting bright colors
pub struct ExtractBrightData<'a> {
    pub tex: &'a glium::texture::Texture2d,
}
/// Shader inputs for prefiltering the environment map
pub struct PrefilterHdrEnvData<'a> {
    pub scene_data: &'a SceneData<'a>,
    pub env_map: &'a glium::texture::Cubemap,
    pub roughness: f32,
}
/// Shader inputs for laser shader
pub struct LaserData<'a> {
    pub scene_data: &'a SceneData<'a>
}
/// Shader inputs passed from a rendering object to the shader manager
pub enum UniformInfo<'a> {
    PBRInfo(PBRData<'a>),
    EquiRectInfo(EqRectData<'a>),
    SkyboxInfo(SkyboxData<'a>),
    UiInfo(UiData<'a>),
    SepConvInfo(SepConvData<'a>),
    ExtractBrightInfo(ExtractBrightData<'a>),
    PrefilterHdrEnvInfo(PrefilterHdrEnvData<'a>),
    GenLutInfo,
    LaserInfo(LaserData<'a>),

}

impl<'a> UniformInfo<'a> {
    /// Gets the corresponding shader type based on the type of 
    /// shader inputs
    fn corresp_shader_type(&self) -> ShaderType {
        use UniformInfo::*;
        match &self {
            PBRInfo(_) => ShaderType::Pbr,
            EquiRectInfo(_) => ShaderType::EquiRect,
            SkyboxInfo(_) => ShaderType::Skybox,
            UiInfo(_) => ShaderType::UiShader,
            SepConvInfo(_) => ShaderType::BlurShader,
            ExtractBrightInfo(_) => ShaderType::BloomShader,
            PrefilterHdrEnvInfo(_) => ShaderType::PrefilterHdrShader,
            GenLutInfo => ShaderType::GenLutShader,
            LaserInfo(_) => ShaderType::Laser,
        }
    }
}

use glium::uniforms::*;
pub enum UniformType<'a> {
    LaserUniform(UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>),
    SkyboxUniform(UniformsStorage<'a, Sampler<'a, glium::texture::Cubemap>, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    PbrUniform(UniformsStorage<'a, &'a UniformBuffer<LightBuffer>, UniformsStorage<'a, bool, UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, 
        UniformsStorage<'a, Sampler<'a, glium::texture::Cubemap>, UniformsStorage<'a, 
        Sampler<'a, glium::texture::Cubemap>, UniformsStorage<'a, Sampler<'a, glium::texture::SrgbTexture2d>, 
        UniformsStorage<'a, [f32; 3], UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, 
        UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, 
        UniformsStorage<'a, Sampler<'a, glium::texture::SrgbTexture2d>, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>>>>>>>>>>>>),
    EqRectUniform(UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    ExtractBrightUniform(UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>),
    UiUniform(UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, bool, UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, 
        UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>>),
    SepConvUniform(UniformsStorage<'a, bool, UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    PrefilterHdrEnvUniform(UniformsStorage<'a, f32, UniformsStorage<'a, Sampler<'a, glium::texture::Cubemap>, UniformsStorage<'a, [[f32; 4]; 4], 
        UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>>),
    BrdfLutUniform(UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>),
    
}
/// Samples a texture with LinearMipmapLinear minification, repeat wrapping, and linear magnification
macro_rules! sample_mip_repeat {
    ($tex_name:expr) => {
        $tex_name.sampled().magnify_filter(glium::uniforms::MagnifySamplerFilter::Linear)
        .minify_filter(glium::uniforms::MinifySamplerFilter::LinearMipmapLinear)
        .wrap_function(glium::uniforms::SamplerWrapFunction::Repeat)
    }
}
/// Samples a texture with linear mag and minification and clamp wrapping
macro_rules! sample_linear_clamp {
    ($tex_name:expr) => {
        $tex_name.sampled().magnify_filter(glium::uniforms::MagnifySamplerFilter::Linear)
        .minify_filter(glium::uniforms::MinifySamplerFilter::Linear)
        .wrap_function(glium::uniforms::SamplerWrapFunction::Clamp)
    }
}

macro_rules! sample_mip_clamp {
    ($tex:expr) => {
        $tex.sampled().magnify_filter(glium::uniforms::MagnifySamplerFilter::Linear)
        .minify_filter(glium::uniforms::MinifySamplerFilter::LinearMipmapLinear)
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

/// Loads the shader source for a shader that outputs to srgb
/// If the output of this shader is stored in an sRGB framebuffer,
/// OpenGL does not do the srgb conversion for us
/// Basically has the effect of calling `glDisable(GL_FRAMEBUFFER_SRGB)`
/// for this shader
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
        let laser_shader = load_shader_source!(facade, 
            "shaders/laserVert.glsl", "shaders/laserFrag.glsl").unwrap();
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
        let prefilter_shader = load_shader_source!(facade,
            "shaders/skyVert.glsl", "shaders/prefilterEnvFrag.glsl").unwrap();
        let brdf_lut_shader = load_shader_source!(facade,
            "shaders/hdrVert.glsl", "shaders/specLutFrag.glsl").unwrap();
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
        shaders.insert(ShaderType::Laser, (laser_shader, ship_params.clone()));
        shaders.insert(ShaderType::Skybox, (skybox_shader, Default::default()));
        shaders.insert(ShaderType::Pbr, (pbr_shader, ship_params));
        shaders.insert(ShaderType::EquiRect, (equirect_shader, Default::default()));
        shaders.insert(ShaderType::UiShader, (ui_shader, Default::default()));
        shaders.insert(ShaderType::BlurShader, (blur_shader, Default::default()));
        shaders.insert(ShaderType::BloomShader, (bloom_shader, Default::default()));
        shaders.insert(ShaderType::PrefilterHdrShader, (prefilter_shader, Default::default()));
        shaders.insert(ShaderType::GenLutShader, (brdf_lut_shader, Default::default()));
        ShaderManager {
            shaders: shaders,
            empty_srgb: glium::texture::SrgbTexture2d::empty(facade, 0, 0).unwrap(),
            empty_2d: glium::texture::Texture2d::empty(facade, 0, 0).unwrap(),
        }
    }

    /// Selects a shader to use based on `data`. Returns the selected shader,
    /// the shader's draw parameters, and `data` converted to a uniform
    /// Panics if `data` is missing required fields or if `data` does not match a 
    /// shader
    pub fn use_shader<'b>(&'b self, data: &'b UniformInfo) 
        -> (&'b glium::Program, &'b glium::DrawParameters, UniformType<'b>)
    {
        use UniformInfo::*;
        let typ = data.corresp_shader_type();
        let (shader, params) = self.shaders.get(&typ).unwrap();
        let uniform = match (typ, data) {
            (ShaderType::Laser, LaserInfo(LaserData {scene_data })) => 
                UniformType::LaserUniform(glium::uniform! {
                    viewproj: scene_data.viewproj
                }),
            (ShaderType::Skybox,  SkyboxInfo(SkyboxData {scene_data, env_map})) 
            => UniformType::SkyboxUniform(glium::uniform! {
                view: scene_data.view,
                proj: scene_data.proj,
                skybox: sample_linear_clamp!(env_map),
            }),
            (ShaderType::EquiRect, EquiRectInfo(EqRectData{scene_data, env_map})) 
            => UniformType::EqRectUniform(glium::uniform! {
                view: scene_data.view,
                proj: scene_data.proj,
                equirectangular_map: sample_linear_clamp!(env_map),
            }),
            (ShaderType::Pbr, PBRInfo(PBRData { scene_data, model, 
                diffuse_tex, roughness_map, metallic_map, emission_map, normal_map, ao_map })) 
            => UniformType::PbrUniform(glium::uniform! {
                viewproj: scene_data.viewproj,
                model: model.clone(),
                albedo_map: sample_mip_repeat!(diffuse_tex),
                roughness_map: sample_mip_repeat!(roughness_map.unwrap()),
                normal_map: sample_mip_repeat!(normal_map.unwrap()),
                metallic_map: sample_mip_repeat!(metallic_map.unwrap()),
                cam_pos: scene_data.cam_pos,
                emission_map: sample_mip_repeat!(emission_map.unwrap_or(&self.empty_srgb)),
                irradiance_map: sample_linear_clamp!(scene_data.ibl_maps.unwrap().diffuse_ibl),
                prefilter_map: sample_mip_clamp!(scene_data.ibl_maps.unwrap().spec_ibl),
                brdf_lut: sample_linear_clamp!(scene_data.ibl_maps.unwrap().brdf_lut),
                ao_map: sample_mip_repeat!(ao_map.unwrap_or(&self.empty_2d)),
                use_ao: ao_map.is_some(),
                LightUniform: scene_data.lights.unwrap(),
            }),
            (ShaderType::UiShader, UiInfo(UiData {model, diffuse, do_blend, blend_tex })) => UniformType::UiUniform(glium::uniform! {
                model: *model,
                diffuse: sample_linear_clamp!(diffuse),
                do_blend: *do_blend,
                bloom_tex: sample_linear_clamp!(blend_tex.unwrap_or(&self.empty_2d)),
            }),
            (ShaderType::BlurShader, SepConvInfo(SepConvData {tex, horizontal_pass})) => UniformType::SepConvUniform(glium::uniform! {
                model: cgmath::Matrix4::from_scale(1f32).into(),
                diffuse: sample_linear_clamp!(tex),
                horizontal_pass: *horizontal_pass,
            }),
            (ShaderType::BloomShader, ExtractBrightInfo(ExtractBrightData {tex})) => UniformType::ExtractBrightUniform(glium::uniform! {
                model: cgmath::Matrix4::from_scale(1f32).into(),
                diffuse: sample_linear_clamp!(tex),
            }),
            (ShaderType::PrefilterHdrShader, PrefilterHdrEnvInfo(PrefilterHdrEnvData {
                scene_data, env_map, roughness })) 
            => UniformType::PrefilterHdrEnvUniform(glium::uniform! {
                view: scene_data.view,
                proj: scene_data.proj,
                env_map: sample_linear_clamp!(env_map),
                roughness: *roughness,
            }),
            (ShaderType::GenLutShader, _) => UniformType::BrdfLutUniform(glium::uniform! {
                model: cgmath::Matrix4::from_scale(1f32).into(),
            }),
            (_, _) => panic!("Invalid shader/shader data combination"),
        };
        (shader, params, uniform)
    }
}