use std::collections::HashMap;
use crate::cg_support::ssbo;
use glium::implement_uniform_block;
use cgmath::*;
use super::entity::Entity;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
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
    CullLightsCompute,
    DepthShader,
    DepthInstancedShader,
    PbrInstancedShader,
    PbrAnim,
    DepthAnim,
    TriIntersectionCompute,
    CollisionDebug,
    Billboard,
}

/// The type of objects that should be rendered to a render target
#[derive(Eq, Copy, Clone, Debug)]
pub enum RenderPassType {
    Visual,
    /// Render objects with depth information only
    Depth,
    /// Transparency pass of entity with specified pointer
    Transparent(*const Entity),
}

impl PartialEq for RenderPassType {
    fn eq(&self, other: &RenderPassType) -> bool {
        use RenderPassType::*;
        match (self, other) {
            (Visual, Visual) => true,
            (Depth, Depth) => true,
            (Transparent(_), Transparent(_)) => true,
            _ => false,
        }
    }
}

impl RenderPassType {
    /// Render pass that is equivalent to all transparency render passes
    /// 
    /// Used to indicate that an object should be drawn on a reflection of another object
    pub fn transparent_tag() -> Self {
        RenderPassType::Transparent(0 as *const Entity)
    }
}

impl ShaderType {
    /// Gets the draw parameters for the shader type and render pass
    fn get_draw_params(&self, _pass: RenderPassType) -> glium::DrawParameters<'static> {
        use ShaderType::*;
        use glium::draw_parameters::*;
        match self {
            Pbr | PbrInstancedShader | DepthShader | DepthInstancedShader | Laser 
            | PbrAnim | DepthAnim => 
                glium::DrawParameters {
                    depth: glium::Depth {
                        test: DepthTest::IfLess,
                        write: true,
                        .. Default::default()
                    },
                    backface_culling: glium::BackfaceCullingMode::CullClockwise,
                    //polygon_mode: glium::PolygonMode::Line,
                    .. Default::default()
                },
            CollisionDebug => 
                glium::DrawParameters {
                    depth: glium::Depth {
                        test: DepthTest::IfLess,
                        write: true,
                        .. Default::default()
                    },
                    polygon_mode: glium::PolygonMode::Line,
                    line_width: Some(2.),
                    .. Default::default()
                },
            Billboard =>
                glium::DrawParameters {
                    depth: glium::Depth {
                        test: DepthTest::IfLess,
                        write: false,
                        .. Default::default()
                    },
                    blend: glium::Blend::alpha_blending(),
                    backface_culling: glium::BackfaceCullingMode::CullClockwise,
                    .. Default::default()
                },
            _ => glium::DrawParameters::default(),
        }
    }
}

/// Instance lighting data for each laser
#[derive(Copy, Clone)]
pub struct LightData {
    _light_start: [f32; 3],
    _radius: f32,
    _light_end: [f32; 3],
    _luminance: f32,
    _color: [f32; 3],
    _mode: u32,
}

impl LightData {
    pub fn tube_light(start: Point3<f32>, end: Point3<f32>, radius: f32, luminance: f32,
        color: Vector3<f32>) -> Self {
            Self {
                _light_start: start.into(),
                _light_end: end.into(),
                _radius: radius, _luminance: luminance,
                _color: color.into(),
                _mode: 1,
            }
    }

    #[allow(dead_code)]
    pub fn sphere_light(pos: Point3<f32>, radius: f32, luminance: f32, color: Vector3<f32>) -> Self {
        Self {
            _light_start: pos.into(),
            _light_end: pos.into(),
            _radius: radius, _luminance: luminance,
            _color: color.into(),
            _mode: 0,
        }
    }

    pub fn point_light(pos: Point3<f32>, luminance: f32, color: Vector3<f32>) -> Self {
        Self {
            _light_start: pos.into(),
            _light_end: pos.into(),
            _radius: 1., _luminance: luminance,
            _color: color.into(),
            _mode: 2,
        }
    }
}

/// The ShaderManager stores all shaders and all draw parameters for each shader
/// It converts shader inputs to OpenGL uniform parameters and selects the shader
/// based on those shader inputs
pub struct ShaderManager {
    shaders: HashMap<ShaderType, glium::Program>,
    compute_shaders: HashMap<ShaderType, glium::program::ComputeShader>,
    empty_srgb: glium::texture::SrgbTexture2d,
    empty_2d: glium::texture::Texture2d,
    empty_cube: glium::texture::Cubemap,
}

/// Precomputed integrals for PBR environment maps
pub struct PbrMaps {
    pub diffuse_ibl: glium::texture::Cubemap,
    pub spec_ibl: glium::texture::Cubemap,
    pub brdf_lut: glium::texture::Texture2d,
}

pub struct ViewerData {
    pub viewproj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    pub cam_pos: [f32; 3],
}

/// Stores scene-wide information passed to uniforms such as view and projection matrices
/// and IBL images
pub struct SceneData<'a> {
    pub viewer: ViewerData,
    pub ibl_maps: Option<&'a PbrMaps>,
    pub lights: Option<&'a ssbo::SSBO<LightData>>,
    pub pass_type: RenderPassType,
    pub light_pos: Option<[f32; 3]>,
}
pub struct TransparencyData {
    pub trans_fac: f32,
    pub refraction_idx: f32,
    pub object_id: u32,
}

impl Default for TransparencyData {
    fn default() -> Self {
        TransparencyData {
            trans_fac: 0.,
            refraction_idx: 1.,
            object_id: 0,
        }
    }
}
/// Shader inputs for PBR shader
pub struct PBRData<'a> {
    pub model: [[f32; 4]; 4],
    pub diffuse_tex: &'a glium::texture::SrgbTexture2d,
    pub roughness_map: Option<&'a glium::texture::Texture2d>,
    pub metallic_map: Option<&'a glium::texture::Texture2d>,
    pub normal_map: Option<&'a glium::texture::Texture2d>,
    pub emission_map: Option<&'a glium::texture::SrgbTexture2d>,
    pub ao_map: Option<&'a glium::texture::Texture2d>,
    pub instancing: bool,
    pub bone_mats: Option<&'a ssbo::SSBO<[[f32; 4]; 4]>>,
    pub trans_data: Option<&'a TransparencyData>,
}
/// Shader inputs for Spherical Texture shader
pub struct EqRectData<'a> {
    pub env_map: &'a glium::texture::Texture2d,
}
/// Shader inputs for Skybox shader
pub struct SkyboxData<'a> {
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
    pub env_map: &'a glium::texture::Cubemap,
    pub roughness: f32,
}
/// Compute shader inputs for light culling
pub struct LightCullData<'a> {
    pub depth_tex: &'a glium::texture::DepthTexture2d,
    pub scr_width: u32,
    pub scr_height: u32,
}
#[derive(Clone, Copy)]
pub struct CascadeUniform {
    pub far_planes: [f32; 4],
    pub viewproj_mats: [[[f32; 4]; 4]; 5],
    //pub depth_maps: [glium::texture::TextureHandle<'a>; 5], //texture handle is 64 bits
}
implement_uniform_block!(CascadeUniform, far_planes, viewproj_mats);

/// A uniform array of `T` with the remaining uniform values `R`
/// `name` is the base name of uniform array, without the `[]`
/// 
/// So if a uniform is defined as `uniform sampler2D depthMaps[10]`, then 
/// `depthMaps` should be `name`
pub struct UniformsArray<'s, T : AsUniformValue, R : Uniforms> {
    pub vals: Vec<T>,
    pub name: &'s str,
    pub rest: R,
}

impl<'s, T : AsUniformValue, R : Uniforms> Uniforms for UniformsArray<'s, T, R> {
    fn visit_values<'a, F: FnMut(&str, UniformValue<'a>)>(&'a self, mut set_uniform: F) {
        for (val, idx) in self.vals.iter().zip(0 .. self.vals.len()) {
            set_uniform(&format!("{}[{}]", self.name, idx), val.as_uniform_value());
        }
        self.rest.visit_values(set_uniform);
    }
}

/// A uniform struct named `name` enclosing the fields in `data`
pub struct UniformsStruct<'s, T: Uniforms, R : Uniforms> {
    pub name: &'s str,
    pub data: T,
    pub rest: R,
}

impl<'s, T : Uniforms, R : Uniforms> Uniforms for UniformsStruct<'s, T, R> {
    fn visit_values<'a, F: FnMut(&str, UniformValue<'a>)>(&'a self, mut set_uniform: F) {
        self.data.visit_values(|name, val| {
            set_uniform(&format!("{}.{}", self.name, name), val)
        });
        self.rest.visit_values(set_uniform)
    }
}

/// Stores shader inputs that can change from stage to stage within a 
/// render pass. Shader stages can read and write from the pipeline chache,
/// which is reset every iteration of a render pass
pub struct PipelineCache<'a> {
    pub cascade_ubo: Option<glium::uniforms::UniformBuffer<CascadeUniform>>,
    pub tiles_x: Option<u32>,
    pub cascade_maps: Option<Vec<&'a glium::texture::DepthTexture2d>>,
    pub obj_cubemaps: HashMap<u32, &'a glium::texture::Cubemap>,
}

impl<'a> std::default::Default for PipelineCache<'a> {
    fn default() -> PipelineCache<'a> {
        PipelineCache {
            cascade_ubo: None,
            tiles_x: None,
            cascade_maps: None,
            obj_cubemaps: HashMap::new(),
        }
    }
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
    LaserInfo,
    TriangleCollisionsInfo,
    LightCullInfo(LightCullData<'a>),
    CollisionDebugInfo([[f32; 4]; 4]),
    BillboardInfo(&'a glium::texture::SrgbTexture2d),
}

impl<'a> std::fmt::Debug for UniformInfo<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use UniformInfo::*;
        let name = match self {
            PBRInfo(_) => "PBR",
            EquiRectInfo(_) => "EQ Rect",
            SkyboxInfo(_) => "Skybox",
            UiInfo(_) => "UI",
            SepConvInfo(_) => "Sep Conv",
            ExtractBrightInfo(_) => "Extract Bright",
            PrefilterHdrEnvInfo(_) => "Prefilter HDR",
            GenLutInfo => "Gen BRDF Lut",
            LaserInfo => "Laser",
            TriangleCollisionsInfo => "Compute triangle",
            LightCullInfo(_) => "Compute light cull",
            CollisionDebugInfo(_) => "Collision debug",
            BillboardInfo(_) => "Billboard",
        };
        f.write_str(name)
    }
}

impl<'a> UniformInfo<'a> {
    /// Gets the corresponding shader type based on the type of 
    /// shader inputs
    fn corresp_shader_type(&self, pass: RenderPassType) -> ShaderType {
        use UniformInfo::*;
        use RenderPassType::*;
        match (self, pass) {
            (PBRInfo(PBRData {instancing, ..}), Visual) | 
            (PBRInfo(PBRData {instancing, ..}), Transparent(_)) 
                if *instancing => ShaderType::PbrInstancedShader,
            (PBRInfo(PBRData {bone_mats: Some(_), ..}), Visual) |
            (PBRInfo(PBRData {bone_mats: Some(_), ..}), Transparent(_)) => ShaderType::PbrAnim,
            (PBRInfo(_), Visual) | (PBRInfo(_), Transparent(_)) => ShaderType::Pbr,
            (PBRInfo(PBRData {instancing, ..}), Depth) if *instancing => 
                ShaderType::DepthInstancedShader,
            (PBRInfo(PBRData {bone_mats: Some(_), ..}), Depth) => 
                ShaderType::DepthAnim,
            (PBRInfo(_), Depth) => ShaderType::DepthShader,
            (EquiRectInfo(_), _) => ShaderType::EquiRect,
            (SkyboxInfo(_), _) => ShaderType::Skybox,
            (UiInfo(_), _) => ShaderType::UiShader,
            (SepConvInfo(_), _) => ShaderType::BlurShader,
            (ExtractBrightInfo(_), _) => ShaderType::BloomShader,
            (PrefilterHdrEnvInfo(_), _) => ShaderType::PrefilterHdrShader,
            (GenLutInfo, _) => ShaderType::GenLutShader,
            (LaserInfo, Visual) | (LaserInfo, Transparent(_)) => ShaderType::Laser,
            (LaserInfo, Depth) => ShaderType::DepthShader,
            (LightCullInfo(_), _) => ShaderType::CullLightsCompute,
            (TriangleCollisionsInfo, _) => ShaderType::TriIntersectionCompute,
            (CollisionDebugInfo(_), _) => ShaderType::CollisionDebug,
            (BillboardInfo(_), _) => ShaderType::Billboard,
        }
    }
}

use glium::uniforms::*;
pub enum UniformType<'a> {
    LaserUniform(UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>),
    SkyboxUniform(UniformsStorage<'a, Sampler<'a, glium::texture::Cubemap>, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    PbrUniform(UniformsArray<'static, Sampler<'a, glium::texture::DepthTexture2d>,
        UniformsStruct<'static, UniformsStorage<'a, Sampler<'a, glium::texture::Cubemap>, 
        UniformsStorage<'a, f32, UniformsStorage<'a, f32, EmptyUniforms>>>, 

        UniformsStorage<'a, [f32; 3], UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, &'a glium::uniforms::UniformBuffer<CascadeUniform>, UniformsStorage<'a, i32, 
        UniformsStorage<'a, bool, UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, 
        UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, Sampler<'a, glium::texture::Cubemap>, UniformsStorage<'a, 
        Sampler<'a, glium::texture::Cubemap>, UniformsStorage<'a, Sampler<'a, glium::texture::SrgbTexture2d>, 
        UniformsStorage<'a, [f32; 3], UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, 
        UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, 
        UniformsStorage<'a, Sampler<'a, glium::texture::SrgbTexture2d>, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>>>>>>>>>>>>>>>>>),
    EqRectUniform(UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    ExtractBrightUniform(UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>),
    UiUniform(UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, bool, UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, 
        UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>>),
    SepConvUniform(UniformsStorage<'a, bool, UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    PrefilterHdrEnvUniform(UniformsStorage<'a, f32, UniformsStorage<'a, Sampler<'a, glium::texture::Cubemap>, UniformsStorage<'a, [[f32; 4]; 4], 
        UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>>),
    BrdfLutUniform(UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>),
    DepthUniform(UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>),
    BillboardUniform(UniformsStorage<'a, Sampler<'a, glium::texture::SrgbTexture2d>, UniformsStorage<'a, [[f32; 4]; 4], 
        UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    
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

macro_rules! sample_nearest_border {
    ($tex:expr) => {
        $tex.sampled().magnify_filter(glium::uniforms::MagnifySamplerFilter::Nearest)
        .minify_filter(glium::uniforms::MinifySamplerFilter::Nearest)
        .wrap_function(glium::uniforms::SamplerWrapFunction::BorderClamp)
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
    /// Initializes the shader manager and loads all shaders
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
        let depth_shader = load_shader_source!(facade,
            "shaders/depthVert.glsl", "shaders/depthFrag.glsl").unwrap();
        let depth_instanced = load_shader_source!(facade,
            "shaders/instanceDepthVert.glsl", "shaders/depthFrag.glsl").unwrap();
        let pbr_instanced = load_shader_source!(facade,
            "shaders/instancePbrVert.glsl", "shaders/pbrFrag.glsl").unwrap();
        let pbr_anim = load_shader_source!(facade,
            "shaders/pbrAnimVert.glsl", "shaders/pbrFrag.glsl").unwrap();
        let depth_anim = load_shader_source!(facade,
            "shaders/depthAnimVert.glsl", "shaders/depthFrag.glsl").unwrap();
        let debug = load_shader_source!(facade,
            "shaders/depthVert.glsl", "shaders/constantColor.glsl").unwrap();
        let billboard = load_shader_source!(facade,
            "shaders/billboardVert.glsl", "shaders/billboardFrag.glsl").unwrap();
        let light_cull = glium::program::ComputeShader::from_source(facade,
           include_str!("shaders/lightCullComp.glsl")).unwrap();
        let triangle_test = glium::program::ComputeShader::from_source(facade, 
            include_str!("shaders/triTriComp.glsl")).unwrap();
        let mut shaders = HashMap::<ShaderType, glium::Program>::new();
        shaders.insert(ShaderType::Laser, laser_shader);
        shaders.insert(ShaderType::Skybox, skybox_shader);
        shaders.insert(ShaderType::Pbr, pbr_shader);
        shaders.insert(ShaderType::EquiRect, equirect_shader);
        shaders.insert(ShaderType::UiShader, ui_shader);
        shaders.insert(ShaderType::BlurShader, blur_shader);
        shaders.insert(ShaderType::BloomShader, bloom_shader);
        shaders.insert(ShaderType::PrefilterHdrShader, prefilter_shader);
        shaders.insert(ShaderType::GenLutShader, brdf_lut_shader);
        shaders.insert(ShaderType::DepthShader, depth_shader);
        shaders.insert(ShaderType::DepthInstancedShader, depth_instanced);
        shaders.insert(ShaderType::PbrInstancedShader, pbr_instanced);
        shaders.insert(ShaderType::PbrAnim, pbr_anim);
        shaders.insert(ShaderType::DepthAnim, depth_anim);
        shaders.insert(ShaderType::CollisionDebug, debug);
        shaders.insert(ShaderType::Billboard, billboard);
        let mut compute_shaders = HashMap::<ShaderType, glium::program::ComputeShader>::new();
        compute_shaders.insert(ShaderType::CullLightsCompute, light_cull);
        compute_shaders.insert(ShaderType::TriIntersectionCompute, triangle_test);
        ShaderManager {
            shaders: shaders, compute_shaders,
            empty_srgb: glium::texture::SrgbTexture2d::empty(facade, 0, 0).unwrap(),
            empty_2d: glium::texture::Texture2d::empty(facade, 0, 0).unwrap(),
            empty_cube: glium::texture::Cubemap::empty(facade, 0).unwrap(),
        }
    }

    /// Selects a shader to use based on `data`. Returns the selected shader,
    /// the shader's draw parameters, and `data` converted to a uniform
    /// Panics if `data` is missing required fields or if `data` does not match a 
    /// shader
    pub fn use_shader<'b>(&'b self, data: &'b UniformInfo, scene_data: Option<&'b SceneData<'b>>, cache: Option<&'b PipelineCache<'b>>) 
        -> (&'b glium::Program, glium::DrawParameters, UniformType<'b>)
    {
        use UniformInfo::*;
        use RenderPassType::*;
        let pass_tp = scene_data.map(|sd| sd.pass_type).unwrap_or(Visual);
        let typ = data.corresp_shader_type(pass_tp);
        let params = typ.get_draw_params(pass_tp);
        let shader = self.shaders.get(&typ).unwrap();
        let uniform = match (data, pass_tp) {
            (LaserInfo, Visual) | (LaserInfo, Transparent(_)) => 
                UniformType::LaserUniform(glium::uniform! {
                    viewproj: scene_data.unwrap().viewer.viewproj
                }),
            (SkyboxInfo(SkyboxData {env_map}), Visual) | (SkyboxInfo(SkyboxData {env_map}), Transparent(_))
            => UniformType::SkyboxUniform(glium::uniform! {
                view: scene_data.unwrap().viewer.view,
                proj: scene_data.unwrap().viewer.proj,
                skybox: sample_linear_clamp!(env_map),
            }),
            (EquiRectInfo(EqRectData{env_map}), Visual) | (EquiRectInfo(EqRectData{env_map}), Transparent(_))
            => UniformType::EqRectUniform(glium::uniform! {
                view: scene_data.unwrap().viewer.view,
                proj: scene_data.unwrap().viewer.proj,
                equirectangular_map: sample_linear_clamp!(env_map),
            }),
            (PBRInfo(PBRData { model, 
                diffuse_tex, roughness_map, metallic_map, emission_map, normal_map, ao_map,
                instancing: _, bone_mats, trans_data}), Visual) |
            (PBRInfo(PBRData { model, 
                diffuse_tex, roughness_map, metallic_map, emission_map, normal_map, ao_map,
                instancing: _, bone_mats, trans_data}), Transparent(_)) 
            => {
                let sd = scene_data.unwrap();
                sd.lights.unwrap().bind(0);
                let cache = cache.unwrap();
                let maps = cache.cascade_maps.as_ref().unwrap();
                // NOTE: requires the compute shader's SSBO for visible indices is still bound
                if typ == ShaderType::PbrAnim {
                    bone_mats.unwrap().bind(4);
                }
                let default = TransparencyData::default();
                let trans_data = if pass_tp == Visual { trans_data.unwrap_or(&default) }
                else { &default };
                UniformType::PbrUniform(UniformsArray { name: "cascadeDepthMaps", 
                vals: maps.iter().map(|x| sample_nearest_border!(*x)).collect::<Vec<Sampler<'b, glium::texture::DepthTexture2d>>>(), 
                rest: UniformsStruct { name: "transparencyData", 
                    data: glium::uniform! {
                        trans_fac: trans_data.trans_fac,
                        refraction_idx: trans_data.refraction_idx,
                        tex: 
                            sample_linear_clamp!(cache.obj_cubemaps.get(&trans_data.object_id).unwrap_or(&&self.empty_cube)),
                    },
                    rest: glium::uniform! {
                        viewproj: sd.viewer.viewproj,
                        model: model.clone(),
                        albedo_map: sample_mip_repeat!(diffuse_tex),
                        roughness_map: sample_mip_repeat!(roughness_map.unwrap()),
                        normal_map: sample_mip_repeat!(normal_map.unwrap()),
                        metallic_map: sample_mip_repeat!(metallic_map.unwrap()),
                        cam_pos: sd.viewer.cam_pos,
                        emission_map: sample_mip_repeat!(emission_map.unwrap_or(&self.empty_srgb)),
                        irradiance_map: sample_linear_clamp!(sd.ibl_maps.unwrap().diffuse_ibl),
                        prefilter_map: sample_mip_clamp!(sd.ibl_maps.unwrap().spec_ibl),
                        brdf_lut: sample_linear_clamp!(sd.ibl_maps.unwrap().brdf_lut),
                        ao_map: sample_mip_repeat!(ao_map.unwrap_or(&self.empty_2d)),
                        use_ao: ao_map.is_some(),
                        tile_num_x: cache.tiles_x.unwrap() as i32,
                        CascadeUniform: cache.cascade_ubo.as_ref().unwrap(),                
                        view: sd.viewer.view,
                        dir_light_dir: sd.light_pos.unwrap(),
                    
                }}})
            },
            (UiInfo(UiData {model, diffuse, do_blend, blend_tex }), _) => {
                UniformType::UiUniform(glium::uniform! {
                    model: *model,
                    diffuse: sample_linear_clamp!(diffuse),
                    do_blend: *do_blend,
                    bloom_tex: sample_linear_clamp!(blend_tex.unwrap_or(&self.empty_2d)),
                })
            },
            (SepConvInfo(SepConvData {tex, horizontal_pass}), _) => UniformType::SepConvUniform(glium::uniform! {
                model: cgmath::Matrix4::from_scale(1f32).into(),
                diffuse: sample_linear_clamp!(tex),
                horizontal_pass: *horizontal_pass,
            }),
            (ExtractBrightInfo(ExtractBrightData {tex}), _) => UniformType::ExtractBrightUniform(glium::uniform! {
                model: cgmath::Matrix4::from_scale(1f32).into(),
                diffuse: sample_linear_clamp!(tex),
            }),
            (PrefilterHdrEnvInfo(PrefilterHdrEnvData {
                env_map, roughness }), _) 
            => UniformType::PrefilterHdrEnvUniform(glium::uniform! {
                view: scene_data.unwrap().viewer.view,
                proj: scene_data.unwrap().viewer.proj,
                env_map: sample_linear_clamp!(env_map),
                roughness: *roughness,
            }),
            (GenLutInfo, _) => UniformType::BrdfLutUniform(glium::uniform! {
                model: cgmath::Matrix4::from_scale(1f32).into(),
            }),
            (PBRInfo(PBRData {model, bone_mats, ..}), Depth) => {
                bone_mats.map(|x| x.bind(4));
                UniformType::DepthUniform(glium::uniform! {
                    viewproj: scene_data.unwrap().viewer.viewproj,
                    model: model.clone(),
                })
            },
            (CollisionDebugInfo(model), _) => UniformType::DepthUniform(glium::uniform! {
                viewproj: scene_data.unwrap().viewer.viewproj,
                model: model.clone(),
            }),
            (BillboardInfo(tex), _) => UniformType::BillboardUniform(glium::uniform! {
                view: scene_data.unwrap().viewer.view,
                proj: scene_data.unwrap().viewer.proj,
                tex: sample_mip_repeat!(tex),
            }),
            (data, pass) => 
                panic!("Invalid shader/shader data combination with shader (Args: `{:?}` '{:?}') during pass '{:?}'", data, typ, pass),
        };
        (shader, params, uniform)
    }

    /// Executes a computer shader with `x * y * z` working groups
    pub fn execute_compute(&self, x: u32, y: u32, z: u32, args: UniformInfo, scene_data: Option<&SceneData>) {
        match args {
            UniformInfo::LightCullInfo(LightCullData {depth_tex, scr_width, scr_height}) => {
                let scene_data = scene_data.unwrap();
                let uniform = glium::uniform! {
                    view: scene_data.viewer.view,
                    proj: scene_data.viewer.proj,
                    depth_tex: depth_tex,
                    viewproj: scene_data.viewer.viewproj,
                    screen_size: [scr_width as i32, scr_height as i32],
                };
                let compute = self.compute_shaders.get(&ShaderType::CullLightsCompute).unwrap();
                scene_data.lights.unwrap().bind(0);
                compute.execute(uniform, x, y, z);
            },
            UniformInfo::TriangleCollisionsInfo => {
                let compute = self.compute_shaders.get(&ShaderType::TriIntersectionCompute).unwrap();
                compute.execute(EmptyUniforms, x, y, z);
            }
            _ => panic!("Unknown compute shader args"),
        }
    }
}