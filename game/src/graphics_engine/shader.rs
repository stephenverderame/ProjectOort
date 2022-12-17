#![allow(clippy::transmute_ptr_to_ptr)]
use crate::cg_support::ssbo;
use cgmath::*;
use glium::implement_uniform_block;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
enum ShaderType {
    Laser,
    Pbr,
    Skybox,
    EquiRect,
    CompositeShader,
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
    ParallelPbr,
    ParallelInstancePbr,
    ParallelAnimPbr,
    ParallelLaser,
    ParallelSky,
    ParallelEqRect,
    ParallelPrefilter,
    Cloud,
    Line,
    Text,
    Minimap,
    Icon,
}

/// The type of objects that should be rendered to a render target
#[derive(Eq, Copy, Clone, Debug)]
pub enum RenderPassType {
    Visual,
    /// Render objects with depth information only
    Depth,
    /// Transparency pass of entity with specified pointer.
    /// The transparency pass is a layered pass
    Transparent(usize),
    /// Render a pass with multiple layers at once
    LayeredVisual,
    /// Render depth information of transparent or semi-transparent objects
    TransparentDepth,
}

impl PartialEq for RenderPassType {
    fn eq(&self, other: &Self) -> bool {
        use RenderPassType::*;
        matches!(
            (self, other),
            (Visual, Visual)
                | (Depth, Depth)
                | (Transparent(_), Transparent(_))
                | (LayeredVisual, LayeredVisual)
                | (TransparentDepth, TransparentDepth)
        )
    }
}

impl RenderPassType {
    /// Render pass that is equivalent to all transparency render passes
    ///
    /// Used to indicate that an object should be drawn on
    /// a reflection of another object
    pub const fn transparent_tag() -> Self {
        Self::Transparent(0)
    }
}

impl ShaderType {
    /// Gets the draw parameters for the shader type and render pass
    fn get_draw_params(
        self,
        _pass: RenderPassType,
    ) -> glium::DrawParameters<'static> {
        use glium::draw_parameters::*;
        use ShaderType::*;
        match self {
            Pbr | PbrInstancedShader | DepthShader | DepthInstancedShader
            | Laser | PbrAnim | DepthAnim | ParallelInstancePbr
            | ParallelLaser | ParallelPbr => {
                glium::DrawParameters {
                    depth: glium::Depth {
                        test: DepthTest::IfLess,
                        write: true,
                        ..Default::default()
                    },
                    backface_culling: glium::BackfaceCullingMode::CullClockwise,
                    //polygon_mode: glium::PolygonMode::Line,
                    ..Default::default()
                }
            }
            CollisionDebug | Line => glium::DrawParameters {
                depth: glium::Depth {
                    test: DepthTest::IfLess,
                    write: true,
                    ..Default::default()
                },
                polygon_mode: glium::PolygonMode::Line,
                line_width: Some(2.),
                ..Default::default()
            },
            Billboard | Minimap | Icon => glium::DrawParameters {
                blend: glium::Blend::alpha_blending(),
                backface_culling: glium::BackfaceCullingMode::CullClockwise,
                ..Default::default()
            },
            Cloud => glium::DrawParameters {
                blend: glium::Blend::alpha_blending(),
                backface_culling:
                    glium::BackfaceCullingMode::CullCounterClockwise,
                ..Default::default()
            },
            Text => glium::DrawParameters {
                blend: glium::Blend::alpha_blending(),
                //polygon_mode: glium::PolygonMode::Line,
                ..Default::default()
            },
            _ => glium::DrawParameters::default(),
        }
    }
}

/// Instance lighting data for each laser
#[derive(Copy, Clone)]
#[repr(C)]
pub struct LightData {
    _light_start: [f32; 3],
    _radius: f32,
    _light_end: [f32; 3],
    _luminance: f32,
    _color: [f32; 3],
    _mode: u32,
}

impl LightData {
    pub fn tube_light(
        start: Point3<f32>,
        end: Point3<f32>,
        radius: f32,
        luminance: f32,
        color: Vector3<f32>,
    ) -> Self {
        Self {
            _light_start: start.into(),
            _light_end: end.into(),
            _radius: radius,
            _luminance: luminance,
            _color: color.into(),
            _mode: 1,
        }
    }

    #[allow(dead_code)]
    pub fn sphere_light(
        pos: Point3<f32>,
        radius: f32,
        luminance: f32,
        color: Vector3<f32>,
    ) -> Self {
        Self {
            _light_start: pos.into(),
            _light_end: pos.into(),
            _radius: radius,
            _luminance: luminance,
            _color: color.into(),
            _mode: 0,
        }
    }

    pub fn point_light(
        pos: Point3<f32>,
        luminance: f32,
        color: Vector3<f32>,
    ) -> Self {
        Self {
            _light_start: pos.into(),
            _light_end: pos.into(),
            _radius: 1.,
            _luminance: luminance,
            _color: color.into(),
            _mode: 2,
        }
    }
}

/// The `ShaderManager` stores all shaders and all draw parameters for each shader
/// It converts shader inputs to OpenGL uniform parameters and selects the shader
/// based on those shader inputs
pub struct ShaderManager {
    shaders: HashMap<ShaderType, glium::Program>,
    compute_shaders: HashMap<ShaderType, glium::program::ComputeShader>,
    empty_srgb: glium::texture::SrgbTexture2d,
    empty_2d: glium::texture::Texture2d,
    empty_cube: glium::texture::Cubemap,
    empty_depth: glium::texture::DepthTexture2d,
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
    pub lights: Option<&'a ssbo::Ssbo<LightData>>,
    pub pass_type: RenderPassType,
    pub light_pos: Option<[f32; 3]>,
}
pub struct TransparencyData {
    pub trans_fac: Rc<RefCell<f32>>,
    pub refraction_idx: f32,
    pub object_id: u32,
}

impl Default for TransparencyData {
    fn default() -> Self {
        Self {
            trans_fac: Rc::new(RefCell::new(0.)),
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
    pub bone_mats: Option<&'a ssbo::Ssbo<[[f32; 4]; 4]>>,
    pub trans_data: Option<&'a TransparencyData>,
    pub emission_strength: f32,
    pub metallic_fac: f32,
    pub roughness_fac: f32,
}
/// Shader inputs for Spherical Texture shader
pub struct EqRectData<'a> {
    pub env_map: &'a glium::texture::Texture2d,
}
/// Shader inputs for Skybox shader
pub struct SkyboxData<'a> {
    pub env_map: &'a glium::texture::Cubemap,
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
/// Data for cloud rendering
pub struct CloudData<'a> {
    pub volume: &'a glium::texture::Texture3d,
    pub model: [[f32; 4]; 4],
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
pub struct UniformsArray<'s, T: AsUniformValue, R: Uniforms> {
    pub vals: Vec<T>,
    pub name: &'s str,
    pub rest: R,
}

impl<'s, T: AsUniformValue, R: Uniforms> Uniforms for UniformsArray<'s, T, R> {
    fn visit_values<'a, F: FnMut(&str, UniformValue<'a>)>(
        &'a self,
        mut set_uniform: F,
    ) {
        for (val, idx) in self.vals.iter().zip(0..self.vals.len()) {
            set_uniform(
                &format!("{}[{}]", self.name, idx),
                val.as_uniform_value(),
            );
        }
        self.rest.visit_values(set_uniform);
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum BlendFn {
    Add,
    Overlay,
}

/// Shader inputs for Composite shader
pub struct CompositeData<'a> {
    pub model: [[f32; 4]; 4],
    pub textures: Vec<&'a glium::texture::Texture2d>,
    pub transforms: Vec<[[f32; 3]; 3]>,
    pub blend_function: (BlendFn, glium::program::ShaderStage),
}

/// A uniform struct named `name` enclosing the fields in `data`
pub struct UniformsStruct<'s, T: Uniforms, R: Uniforms> {
    pub name: &'s str,
    pub data: T,
    pub rest: R,
}

impl<'s, T: Uniforms, R: Uniforms> Uniforms for UniformsStruct<'s, T, R> {
    fn visit_values<'a, F: FnMut(&str, UniformValue<'a>)>(
        &'a self,
        mut set_uniform: F,
    ) {
        self.data.visit_values(|name, val| {
            set_uniform(&format!("{}.{}", self.name, name), val);
        });
        self.rest.visit_values(set_uniform);
    }
}

/// Stores shader inputs that can change from stage to stage within a
/// render pass. Shader stages can read and write from the pipeline chache,
/// which is reset every iteration of a render pass
#[derive(Default)]
pub struct PipelineCache<'a> {
    pub cascade_ubo: Option<glium::uniforms::UniformBuffer<CascadeUniform>>,
    pub tiles_x: Option<u32>,
    pub cascade_maps: Option<Vec<&'a glium::texture::DepthTexture2d>>,
    pub trans_cascade_maps: Option<
        Vec<(
            &'a glium::texture::DepthTexture2d,
            &'a glium::texture::Texture2d,
        )>,
    >,
    pub obj_cubemaps: HashMap<u32, &'a glium::texture::Cubemap>,
    pub cam_depth: Option<&'a glium::texture::DepthTexture2d>,
}

pub struct MinimapData<'a> {
    pub textures: [&'a glium::texture::Texture2d; 3],
}

/// Shader inputs passed from a rendering object to the shader manager
pub enum UniformInfo<'a> {
    Pbr(PBRData<'a>),
    EquiRect(EqRectData<'a>),
    Skybox(SkyboxData<'a>),
    Composite(CompositeData<'a>),
    SepConv(SepConvData<'a>),
    ExtractBright(ExtractBrightData<'a>),
    PrefilterHdrEnv(PrefilterHdrEnvData<'a>),
    GenLut,
    Laser,
    TriangleCollisions,
    LightCull(LightCullData<'a>),
    /// Arg - model matrix
    CollisionDebug([[f32; 4]; 4]),
    /// Args - billboard texture, spherical billboard density
    Billboard(&'a glium::texture::SrgbTexture2d, f32),
    Cloud(CloudData<'a>),
    Line,
    /// Args - SDF texture, `[tex_width, tex_height]`
    Text(&'a glium::texture::Texture2d, [i32; 2]),
    Minimap(MinimapData<'a>),
    /// Args - Icon texture, model matrix
    Icon(&'a glium::texture::SrgbTexture2d, [[f32; 4]; 4]),
}

impl<'a> std::fmt::Debug for UniformInfo<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use UniformInfo::*;
        let name = match self {
            Pbr(_) => "PBR",
            EquiRect(_) => "EQ Rect",
            Skybox(_) => "Skybox",
            Composite(_) => "Compositor",
            SepConv(_) => "Sep Conv",
            ExtractBright(_) => "Extract Bright",
            PrefilterHdrEnv(_) => "Prefilter HDR",
            GenLut => "Gen BRDF Lut",
            Laser => "Laser",
            TriangleCollisions => "Compute triangle",
            LightCull(_) => "Compute light cull",
            CollisionDebug(_) => "Collision debug",
            Billboard(_, _) => "Billboard",
            Cloud(_) => "Cloud",
            Line => "Line",
            Text(_, _) => "Text",
            Minimap(_) => "Minimap",
            Icon(_, _) => "Icon",
        };
        f.write_str(name)
    }
}

impl<'a> UniformInfo<'a> {
    /// Gets the corresponding shader type based on the type of
    /// shader inputs
    #[allow(clippy::too_many_lines)]
    fn corresp_shader_type(&self, pass: RenderPassType) -> ShaderType {
        use RenderPassType::*;
        use UniformInfo::*;
        match (self, pass) {
            // solid passes
            (
                Pbr(PBRData {
                    instancing: true, ..
                }),
                Visual,
            ) => ShaderType::PbrInstancedShader,
            (
                Pbr(PBRData {
                    instancing: true, ..
                }),
                Transparent(_) | LayeredVisual,
            ) => ShaderType::ParallelInstancePbr,
            (
                Pbr(PBRData {
                    bone_mats: Some(_), ..
                }),
                Visual,
            ) => ShaderType::PbrAnim,
            (
                Pbr(PBRData {
                    bone_mats: Some(_), ..
                }),
                LayeredVisual | Transparent(_),
            ) => ShaderType::ParallelAnimPbr,
            (Pbr(_), Visual) => ShaderType::Pbr,
            (Pbr(_), LayeredVisual | Transparent(_)) => ShaderType::ParallelPbr,
            (
                Pbr(PBRData {
                    instancing: true, ..
                }),
                Depth,
            ) => ShaderType::DepthInstancedShader,
            (
                Pbr(PBRData {
                    bone_mats: Some(_), ..
                }),
                Depth,
            ) => ShaderType::DepthAnim,
            (Laser, Visual) => ShaderType::Laser,
            (Laser, LayeredVisual | Transparent(_)) => {
                ShaderType::ParallelLaser
            }
            (Laser, Depth) | (Pbr(_), Depth | TransparentDepth) => {
                ShaderType::DepthShader
            }
            (CollisionDebug(_), Visual) => ShaderType::CollisionDebug,
            (Cloud(_), Visual) => ShaderType::Cloud,
            (Line, Visual | Transparent(_)) => ShaderType::Line,
            //(CloudInfo(_), Depth) => ShaderType::CloudDepth,

            // game objects
            (EquiRect(_), Visual) => ShaderType::EquiRect,
            (Skybox(_), Visual) => ShaderType::Skybox,
            (EquiRect(_), LayeredVisual | Transparent(_)) => {
                ShaderType::ParallelEqRect
            }
            (Skybox(_), LayeredVisual | Transparent(_)) => {
                ShaderType::ParallelSky
            }
            (Billboard(_, _), Visual) => ShaderType::Billboard,
            (Text(_, _), Visual) => ShaderType::Text,
            (Minimap(_), Visual) => ShaderType::Minimap,
            (Icon(_, _), Visual) => ShaderType::Icon,

            // tex processors
            (Composite(_), Visual) => ShaderType::CompositeShader,
            (SepConv(_), Visual) => ShaderType::BlurShader,
            (ExtractBright(_), Visual) => ShaderType::BloomShader,
            (PrefilterHdrEnv(_), Visual) => ShaderType::PrefilterHdrShader,
            (PrefilterHdrEnv(_), LayeredVisual) => {
                ShaderType::ParallelPrefilter
            }
            (GenLut, Visual) => ShaderType::GenLutShader,

            // compute shaders
            (LightCull(_), Visual) => ShaderType::CullLightsCompute,
            (TriangleCollisions, Visual) => ShaderType::TriIntersectionCompute,
            (typ, pass) => panic!(
                "Unknown shader-pass combination ({:?}, {:?})",
                typ, pass
            ),
        }
    }
}

use glium::uniforms::*;
#[allow(clippy::type_complexity, clippy::large_enum_variant)]
pub enum UniformType<'a> {
    Laser(UniformsStorage<'a, bool, UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>),
    Skybox(UniformsStorage<'a, Sampler<'a, glium::texture::Cubemap>, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    Pbr(UniformsArray<'static, Sampler<'a, glium::texture::DepthTexture2d>,
        UniformsArray<'static, Sampler<'a, glium::texture::DepthTexture2d>,
        UniformsArray<'static, Sampler<'a, glium::texture::Texture2d>,
        UniformsStruct<'static, UniformsStorage<'a,
        glium::uniforms::Sampler<'a, glium::texture::Cubemap>, glium::uniforms::UniformsStorage<'a, f32,
        glium::uniforms::UniformsStorage<'a, f32, glium::uniforms::EmptyUniforms>>>,
        glium::uniforms::UniformsStorage<'a, [f32; 3], glium::uniforms::UniformsStorage<'a, [[f32; 4]; 4],
        glium::uniforms::UniformsStorage<'a, &'a glium::uniforms::UniformBuffer<CascadeUniform>,
        glium::uniforms::UniformsStorage<'a, i32, glium::uniforms::UniformsStorage<'a, bool,
        glium::uniforms::UniformsStorage<'a, glium::uniforms::Sampler<'a, glium::Texture2d>,
        glium::uniforms::UniformsStorage<'a, glium::uniforms::Sampler<'a, glium::Texture2d>,
        glium::uniforms::UniformsStorage<'a, glium::uniforms::Sampler<'a, glium::texture::Cubemap>,
        glium::uniforms::UniformsStorage<'a, glium::uniforms::Sampler<'a, glium::texture::Cubemap>,
        glium::uniforms::UniformsStorage<'a, glium::uniforms::Sampler<'a, glium::texture::SrgbTexture2d>,
        glium::uniforms::UniformsStorage<'a, [f32; 3], glium::uniforms::UniformsStorage<'a, glium::uniforms::Sampler<'a,
        glium::Texture2d>, glium::uniforms::UniformsStorage<'a, glium::uniforms::Sampler<'a, glium::Texture2d>,
        glium::uniforms::UniformsStorage<'a, glium::uniforms::Sampler<'a, glium::Texture2d>,
        glium::uniforms::UniformsStorage<'a, glium::uniforms::Sampler<'a, glium::texture::SrgbTexture2d>,
        glium::uniforms::UniformsStorage<'a, [[f32; 4]; 4], glium::uniforms::UniformsStorage<'a, [[f32; 4]; 4],
        UniformsStorage<'a, f32, UniformsStorage< 'a, f32, UniformsStorage<'a, f32, glium::uniforms::EmptyUniforms>>>>>>>>>>>>>>>>>>>>>>>>),
    EqRect(UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    ExtractBright(UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>),
    Composite(UniformsArray<'static, Sampler<'a, glium::texture::Texture2d>, UniformsArray<'static, [[f32; 3]; 3], UniformsStorage<'a, (&'a str, glium::program::ShaderStage), UniformsStorage<'a, [[f32; 4]; 4],
        UniformsStorage<'a, u32, EmptyUniforms>>>>>),
    SepConv(UniformsStorage<'a, bool, UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    PrefilterHdrEnv(UniformsStorage<'a, f32, UniformsStorage<'a, Sampler<'a, glium::texture::Cubemap>, UniformsStorage<'a, [[f32; 4]; 4],
        UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>>),
    BrdfLut(UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>),
    Depth(UniformsStorage<'a, f32, UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    Color(UniformsStorage<'a, [f32; 4], UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>),
    Billboard(UniformsStorage<'a, f32, UniformsStorage<'a, Sampler<'a, glium::texture::DepthTexture2d>, UniformsStorage<'a, Sampler<'a, glium::texture::SrgbTexture2d>,
        UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>>>),
    Cloud(UniformsStorage<'a, Sampler<'a, glium::texture::DepthTexture2d>, UniformsStorage<'a, [[f32; 4]; 4],
        UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, i32,
        UniformsStorage<'a, Sampler<'a, glium::texture::Texture3d>, UniformsStorage<'a, [f32; 3],
        UniformsStorage<'a, [f32; 3], UniformsStorage<'a, [[f32; 4]; 4], UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>>>>>>>>),
    Line(UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>),
    Text(UniformsStorage<'a, Sampler<'a, glium::texture::Texture2d>, UniformsStorage<'a, [f32; 2], UniformsStorage<'a, [[f32; 4]; 4],
        EmptyUniforms>>>),
    Minimap(UniformsArray<'static, Sampler<'a, glium::texture::Texture2d>, EmptyUniforms>),
    Icon(UniformsStorage<'a, Sampler<'a, glium::texture::SrgbTexture2d>, UniformsStorage<'a, [[f32; 4]; 4], EmptyUniforms>>), 
}
/// Samples a texture with `LinearMipmapLinear` minification, repeat wrapping, and linear magnification
macro_rules! sample_mip_repeat {
    ($tex_name:expr) => {
        $tex_name
            .sampled()
            .magnify_filter(glium::uniforms::MagnifySamplerFilter::Linear)
            .minify_filter(
                glium::uniforms::MinifySamplerFilter::LinearMipmapLinear,
            )
            .wrap_function(glium::uniforms::SamplerWrapFunction::Repeat)
    };
}
/// Samples a texture with linear mag and minification and clamp wrapping
macro_rules! sample_linear_clamp {
    ($tex_name:expr) => {
        $tex_name
            .sampled()
            .magnify_filter(glium::uniforms::MagnifySamplerFilter::Linear)
            .minify_filter(glium::uniforms::MinifySamplerFilter::Linear)
            .wrap_function(glium::uniforms::SamplerWrapFunction::Clamp)
    };
}

/*
macro_rules! sample_linear_repeat {
    ($tex_name:expr) => {
        $tex_name.sampled().magnify_filter(glium::uniforms::MagnifySamplerFilter::Linear)
        .minify_filter(glium::uniforms::MinifySamplerFilter::Linear)
        .wrap_function(glium::uniforms::SamplerWrapFunction::Repeat)
    }
}*/

macro_rules! sample_linear_b_clamp {
    ($tex_name:expr) => {
        $tex_name
            .sampled()
            .magnify_filter(glium::uniforms::MagnifySamplerFilter::Linear)
            .minify_filter(glium::uniforms::MinifySamplerFilter::Linear)
            .wrap_function(glium::uniforms::SamplerWrapFunction::BorderClamp)
    };
}

macro_rules! sample_mip_clamp {
    ($tex:expr) => {
        $tex.sampled()
            .magnify_filter(glium::uniforms::MagnifySamplerFilter::Linear)
            .minify_filter(
                glium::uniforms::MinifySamplerFilter::LinearMipmapLinear,
            )
            .wrap_function(glium::uniforms::SamplerWrapFunction::Clamp)
    };
}

macro_rules! sample_nearest_border {
    ($tex:expr) => {
        $tex.sampled()
            .magnify_filter(glium::uniforms::MagnifySamplerFilter::Nearest)
            .minify_filter(glium::uniforms::MinifySamplerFilter::Nearest)
            .wrap_function(glium::uniforms::SamplerWrapFunction::BorderClamp)
    };
}

macro_rules! sample_linear_border {
    ($tex:expr) => {
        $tex.sampled()
            .magnify_filter(glium::uniforms::MagnifySamplerFilter::Linear)
            .minify_filter(glium::uniforms::MinifySamplerFilter::Linear)
            .wrap_function(glium::uniforms::SamplerWrapFunction::BorderClamp)
    };
}

macro_rules! include_str {
    ($file:literal) => {
        &String::from_utf8_lossy(include_bytes!($file))
    };
}

macro_rules! load_shader_source {
    ($facade:expr, $vertex_file:literal, $fragment_file:literal, $geom_file:literal) => {
        glium::Program::from_source(
            $facade,
            include_str!($vertex_file),
            include_str!($fragment_file),
            Some(include_str!($geom_file)),
        )
    };
    ($facade:expr, $vertex_file:literal, $fragment_file:literal) => {
        glium::Program::from_source(
            $facade,
            include_str!($vertex_file),
            include_str!($fragment_file),
            None,
        )
    };
}

/// Loads the shader source for a shader that outputs to srgb
/// If the output of this shader is stored in an `sRGB` framebuffer,
/// OpenGL does not do the srgb conversion for us
/// Basically has the effect of calling `glDisable(GL_FRAMEBUFFER_SRGB)`
/// for this shader
macro_rules! load_shader_srgb {
    ($facade:expr, $vertex_file:literal, $fragment_file:literal) => {
        glium::Program::new(
            $facade,
            glium::program::ProgramCreationInput::SourceCode {
                vertex_shader: include_str!($vertex_file),
                tessellation_control_shader: None,
                tessellation_evaluation_shader: None,
                geometry_shader: None,
                fragment_shader: include_str!($fragment_file),
                transform_feedback_varyings: None,
                outputs_srgb: true,
                uses_point_size: false,
            },
        )
    };
}

impl ShaderManager {
    /// Initializes the shader manager and loads all shaders
    #[allow(clippy::too_many_lines)]
    pub fn init<F: glium::backend::Facade>(facade: &F) -> Self {
        let laser_shader =
            load_shader_source!(facade, "shaders/laser.vs", "shaders/laser.fs")
                .unwrap();
        let skybox_shader =
            load_shader_source!(facade, "shaders/sky.vs", "shaders/sky.fs")
                .unwrap();
        let pbr_shader =
            load_shader_source!(facade, "shaders/pbr.vs", "shaders/pbr.fs")
                .unwrap();
        let equirect_shader =
            load_shader_srgb!(facade, "shaders/sky.vs", "shaders/eqRect.fs")
                .unwrap();
        let ui_shader =
            load_shader_source!(facade, "shaders/hdr.vs", "shaders/hdr.fs")
                .unwrap();
        let bloom_shader =
            load_shader_source!(facade, "shaders/hdr.vs", "shaders/bloom.fs")
                .unwrap();
        let blur_shader =
            load_shader_source!(facade, "shaders/hdr.vs", "shaders/blur.fs")
                .unwrap();
        let prefilter_shader = load_shader_source!(
            facade,
            "shaders/sky.vs",
            "shaders/prefilterEnv.fs"
        )
        .unwrap();
        let brdf_lut_shader =
            load_shader_source!(facade, "shaders/hdr.vs", "shaders/specLut.fs")
                .unwrap();
        let depth_shader =
            load_shader_source!(facade, "shaders/depth.vs", "shaders/depth.fs")
                .unwrap();
        let depth_instanced = load_shader_source!(
            facade,
            "shaders/instanceDepth.vs",
            "shaders/depth.fs"
        )
        .unwrap();
        let pbr_instanced = load_shader_source!(
            facade,
            "shaders/instancePbr.vs",
            "shaders/pbr.fs"
        )
        .unwrap();
        let pbr_anim =
            load_shader_source!(facade, "shaders/pbrAnim.vs", "shaders/pbr.fs")
                .unwrap();
        let depth_anim = load_shader_source!(
            facade,
            "shaders/depthAnim.vs",
            "shaders/depth.fs"
        )
        .unwrap();
        let debug = load_shader_source!(
            facade,
            "shaders/depth.vs",
            "shaders/constantColor.fs"
        )
        .unwrap();
        let billboard = load_shader_source!(
            facade,
            "shaders/billboard.vs",
            "shaders/billboard.fs"
        )
        .unwrap();
        let parallel_pbr = load_shader_source!(
            facade,
            "shaders/pbr.vs",
            "shaders/pbr.fs",
            "shaders/parallelPbr.gs"
        )
        .unwrap();
        let parallel_instance_pbr = load_shader_source!(
            facade,
            "shaders/instancePbr.vs",
            "shaders/pbr.fs",
            "shaders/parallelPbr.gs"
        )
        .unwrap();
        let parallel_laser = load_shader_source!(
            facade,
            "shaders/laser.vs",
            "shaders/laser.fs",
            "shaders/parallelLaser.gs"
        )
        .unwrap();
        let parallel_sky = load_shader_source!(
            facade,
            "shaders/sky.vs",
            "shaders/sky.fs",
            "shaders/parallelSky.gs"
        )
        .unwrap();
        let parallel_eq_rect = load_shader_source!(
            facade,
            "shaders/sky.vs",
            "shaders/eqRect.fs",
            "shaders/parallelSky.gs"
        )
        .unwrap();
        let parallel_anim_pbr = load_shader_source!(
            facade,
            "shaders/pbrAnim.vs",
            "shaders/pbr.fs",
            "shaders/parallelPbr.gs"
        )
        .unwrap();
        let parallel_prefilter = load_shader_source!(
            facade,
            "shaders/sky.vs",
            "shaders/prefilterEnv.fs",
            "shaders/parallelSky.gs"
        )
        .unwrap();
        let cloud_shader =
            load_shader_source!(facade, "shaders/cloud.vs", "shaders/cloud.fs")
                .unwrap();
        let line_shader =
            load_shader_source!(facade, "shaders/line.vs", "shaders/line.fs")
                .unwrap();
        let text_shader =
            load_shader_source!(facade, "shaders/text.vs", "shaders/text.fs")
                .unwrap();
        let minimap_shader = load_shader_source!(
            facade,
            "shaders/minimap.vs",
            "shaders/minimap.fs"
        )
        .unwrap();
        let icon_shader =
            load_shader_source!(facade, "shaders/icon.vs", "shaders/icon.fs")
                .unwrap();
        let light_cull = glium::program::ComputeShader::from_source(
            facade,
            include_str!("shaders/lightCull.comp"),
        )
        .unwrap();
        let triangle_test = glium::program::ComputeShader::from_source(
            facade,
            include_str!("shaders/triTriCollision.comp"),
        )
        .unwrap();
        let mut shaders = HashMap::<ShaderType, glium::Program>::new();
        shaders.insert(ShaderType::Laser, laser_shader);
        shaders.insert(ShaderType::Skybox, skybox_shader);
        shaders.insert(ShaderType::Pbr, pbr_shader);
        shaders.insert(ShaderType::EquiRect, equirect_shader);
        shaders.insert(ShaderType::CompositeShader, ui_shader);
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
        shaders.insert(ShaderType::ParallelPbr, parallel_pbr);
        shaders.insert(ShaderType::ParallelLaser, parallel_laser);
        shaders.insert(ShaderType::ParallelInstancePbr, parallel_instance_pbr);
        shaders.insert(ShaderType::ParallelSky, parallel_sky);
        shaders.insert(ShaderType::ParallelEqRect, parallel_eq_rect);
        shaders.insert(ShaderType::ParallelAnimPbr, parallel_anim_pbr);
        shaders.insert(ShaderType::ParallelPrefilter, parallel_prefilter);
        shaders.insert(ShaderType::Cloud, cloud_shader);
        shaders.insert(ShaderType::Line, line_shader);
        shaders.insert(ShaderType::Text, text_shader);
        shaders.insert(ShaderType::Minimap, minimap_shader);
        shaders.insert(ShaderType::Icon, icon_shader);
        let mut compute_shaders =
            HashMap::<ShaderType, glium::program::ComputeShader>::new();
        compute_shaders.insert(ShaderType::CullLightsCompute, light_cull);
        compute_shaders
            .insert(ShaderType::TriIntersectionCompute, triangle_test);
        Self {
            shaders,
            compute_shaders,
            empty_srgb: glium::texture::SrgbTexture2d::empty(facade, 0, 0)
                .unwrap(),
            empty_2d: glium::texture::Texture2d::empty(facade, 0, 0).unwrap(),
            empty_cube: glium::texture::Cubemap::empty(facade, 0).unwrap(),
            empty_depth: glium::texture::DepthTexture2d::empty(facade, 0, 0)
                .unwrap(),
        }
    }

    /// Selects a shader to use based on `data`. Returns the selected shader,
    /// the shader's draw parameters, and `data` converted to a uniform
    /// Panics if `data` is missing required fields or if `data` does not match a
    /// shader
    #[allow(clippy::too_many_lines)]
    pub fn use_shader<'b>(
        &'b self,
        data: &'b UniformInfo,
        scene_data: Option<&'b SceneData<'b>>,
        cache: Option<&'b PipelineCache<'b>>,
    ) -> (&'b glium::Program, glium::DrawParameters, UniformType<'b>) {
        use RenderPassType::*;
        use UniformInfo::*;
        let pass_tp = scene_data.map_or(Visual, |sd| sd.pass_type);
        let typ = data.corresp_shader_type(pass_tp);
        let params = typ.get_draw_params(pass_tp);
        let shader = self.shaders.get(&typ).unwrap();
        let uniform = match (data, pass_tp) {
            (Laser, Visual) =>
                UniformType::Laser(glium::uniform! {
                    viewproj: scene_data.unwrap().viewer.viewproj,
                    layered: false,
                }),
            (Laser, Transparent(_) | LayeredVisual) =>
                UniformType::Laser(glium::uniform! {
                    viewproj: scene_data.unwrap().viewer.viewproj,
                    layered: true,
                }),
            (Skybox(SkyboxData { env_map }), Visual | Transparent(_) | LayeredVisual)
            => UniformType::Skybox(glium::uniform! {
                view: scene_data.unwrap().viewer.view,
                proj: scene_data.unwrap().viewer.proj,
                skybox: sample_linear_clamp!(env_map),
            }),
            (EquiRect(EqRectData { env_map }), Visual | Transparent(_) | LayeredVisual)
            => UniformType::EqRect(glium::uniform! {
                view: scene_data.unwrap().viewer.view,
                proj: scene_data.unwrap().viewer.proj,
                equirectangular_map: sample_linear_clamp!(env_map),
            }),
            (Pbr(PBRData {
                model,
                diffuse_tex,
                roughness_map,
                metallic_map,
                emission_map,
                normal_map,
                ao_map,
                instancing: _,
                bone_mats,
                trans_data,
                emission_strength,
                roughness_fac,
                metallic_fac }), Visual | Transparent(_) | LayeredVisual)
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
                UniformType::Pbr(UniformsArray { name: "cascadeDepthMaps",
                vals: maps.iter().map(|x|
                    sample_nearest_border!(*x)).collect::<Vec<Sampler<'b, glium::texture::DepthTexture2d>>>(),
                rest: UniformsArray { name: "cascadeTransMaps",
                vals: cache.trans_cascade_maps.as_ref().map_or_else(
                    || vec![sample_nearest_border!(self.empty_depth)],
                    |v| {
                        v.iter().map(|(d, _)| sample_nearest_border!(*d))
                        .collect::<Vec<Sampler<'b, glium::texture::DepthTexture2d>>>()}),
                rest: UniformsArray { name: "cascadeTransFacs",
                vals: cache.trans_cascade_maps.as_ref().map_or_else(
                    || vec![sample_nearest_border!(self.empty_2d)],
                    |v| {
                    v.iter().map(|(_, c)| sample_nearest_border!(*c))
                    .collect::<Vec<Sampler<'b, glium::texture::Texture2d>>>()}),
                rest: UniformsStruct { name: "transparencyData", 
                data: glium::uniform! {
                    trans_fac: *trans_data.trans_fac.borrow(),
                    refraction_idx: trans_data.refraction_idx,
                    tex:
                        sample_linear_clamp!(cache.obj_cubemaps.get(&trans_data.object_id).unwrap_or(&&self.empty_cube)),
                },
                rest: glium::uniform! {
                    roughness_fac: *roughness_fac,
                    metallic_fac: *metallic_fac,
                    emission_strength: *emission_strength,
                    viewproj: sd.viewer.viewproj,
                    model: *model,
                    albedo_map: sample_mip_repeat!(diffuse_tex),
                    roughness_map: sample_mip_repeat!(roughness_map.unwrap_or(&self.empty_2d)),
                    normal_map: sample_mip_repeat!(normal_map.unwrap()),
                    metallic_map: sample_mip_repeat!(metallic_map.unwrap_or(&self.empty_2d)),
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
            }}
                    }
                }})
            },
            (Composite(CompositeData {model, textures, blend_function, transforms}), _) => {
                let subroutine_name = match blend_function.0 {
                    BlendFn::Overlay => "blendOverlay",
                    BlendFn::Add => "blendAdd",
                };
                UniformType::Composite(UniformsArray {
                    vals: textures.iter().map(|tex| sample_linear_clamp!(tex)).collect(),
                    name: "textures",
                    rest: UniformsArray {
                        name: "models",
                        vals: transforms.clone(),
                        rest: glium::uniform! {
                            tex_count: textures.len() as u32,
                            model: *model,
                            blend_function: (subroutine_name, blend_function.1),
                        },
                    },
                })
            },
            (SepConv(SepConvData {tex, horizontal_pass}), _) => UniformType::SepConv(glium::uniform! {
                model: cgmath::Matrix4::from_scale(1f32).into(),
                diffuse: sample_linear_clamp!(tex),
                horizontal_pass: *horizontal_pass,
            }),
            (ExtractBright(ExtractBrightData {tex}), _) => UniformType::ExtractBright(glium::uniform! {
                model: cgmath::Matrix4::from_scale(1f32).into(),
                diffuse: sample_linear_clamp!(tex),
            }),
            (PrefilterHdrEnv(PrefilterHdrEnvData {
                env_map, roughness }), _)
            => UniformType::PrefilterHdrEnv(glium::uniform! {
                view: scene_data.unwrap().viewer.view,
                proj: scene_data.unwrap().viewer.proj,
                env_map: sample_linear_clamp!(env_map),
                roughness: *roughness,
            }),
            (GenLut, _) => UniformType::BrdfLut(glium::uniform! {
                model: cgmath::Matrix4::from_scale(1f32).into(),
            }),
            (Pbr(PBRData { model, bone_mats, trans_data, .. }), Depth | TransparentDepth) => {
                bone_mats.map(|x| x.bind(4));
                UniformType::Depth(glium::uniform! {
                    viewproj: scene_data.unwrap().viewer.viewproj,
                    model: *model,
                    inv_fac: trans_data.as_ref().map_or(0., |x| *x.trans_fac.borrow()),
                })
            },
            (CollisionDebug(model), _) => UniformType::Color(glium::uniform! {
                viewproj: scene_data.unwrap().viewer.viewproj,
                model: *model,
                color: [1.0, 0.0, 0.0, 1.0],
            }),
            (Billboard(tex, density), Visual) => UniformType::Billboard(glium::uniform! {
                view: scene_data.unwrap().viewer.view,
                proj: scene_data.unwrap().viewer.proj,
                tex: sample_mip_repeat!(tex),
                cam_depth: sample_linear_clamp!(cache.unwrap().cam_depth.unwrap()),
                particle_density: *density,
            }),
            (Cloud(CloudData{volume, model}), Visual) => UniformType::Cloud(glium::uniform! {
                viewproj: scene_data.unwrap().viewer.viewproj,
                model: *model,
                light_dir: scene_data.unwrap().light_pos.unwrap_or([1f32, 0., 0.]),
                cam_pos: scene_data.unwrap().viewer.cam_pos,
                volume: sample_linear_b_clamp!(volume),
                tile_num_x: cache.as_ref().map(|x| x.tiles_x).unwrap().unwrap() as i32,
                view: scene_data.unwrap().viewer.view,
                proj: scene_data.unwrap().viewer.proj,
                cam_depth: sample_linear_clamp!(cache.unwrap().cam_depth.unwrap()),
            }),
            (Line, Visual | Transparent(_)) => UniformType::Line(glium::uniform! {
                viewproj: scene_data.unwrap().viewer.viewproj,
            }),
            (Text(tex, tex_width_height), Visual) => UniformType::Text(glium::uniform! {
                viewproj: scene_data.unwrap().viewer.viewproj,
                tex_width_height: [tex_width_height[0] as f32, tex_width_height[1] as f32],
                tex: sample_mip_clamp!(tex),
            }),
            (Minimap(MinimapData{ textures }), Visual) => UniformType::Minimap(UniformsArray {
                name: "textures",
                vals: textures.iter().map(|t| sample_linear_border!(t)).collect(),
                rest: EmptyUniforms,
            }),
            (Icon(texture, model), Visual) => UniformType::Icon(glium::uniform! {
                model: *model,
                tex: sample_linear_clamp!(texture),
            }),
            (data, pass) =>
                panic!("Invalid shader/shader data combination with shader (Args: `{:?}` '{:?}') during pass '{:?}'", data, typ, pass),
        };
        (shader, params, uniform)
    }

    /// Executes a computer shader with `x * y * z` working groups
    pub fn execute_compute(
        &self,
        x: u32,
        y: u32,
        z: u32,
        args: &UniformInfo,
        scene_data: Option<&SceneData>,
    ) {
        match args {
            UniformInfo::LightCull(LightCullData {
                depth_tex,
                scr_width,
                scr_height,
            }) => {
                let scene_data = scene_data.unwrap();
                let uniform = glium::uniform! {
                    view: scene_data.viewer.view,
                    proj: scene_data.viewer.proj,
                    depth_tex: *depth_tex,
                    viewproj: scene_data.viewer.viewproj,
                    screen_size: [*scr_width as i32, *scr_height as i32],
                };
                let compute = self
                    .compute_shaders
                    .get(&ShaderType::CullLightsCompute)
                    .unwrap();
                scene_data.lights.unwrap().bind(0);
                compute.execute(uniform, x, y, z);
            }
            UniformInfo::TriangleCollisions => {
                let compute = self
                    .compute_shaders
                    .get(&ShaderType::TriIntersectionCompute)
                    .unwrap();
                compute.execute(EmptyUniforms, x, y, z);
            }
            _ => panic!("Unknown compute shader args"),
        }
    }
}
