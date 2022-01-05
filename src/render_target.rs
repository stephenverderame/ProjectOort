#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 2],
    tex_coords: [f32; 2],
}

glium::implement_vertex!(Vertex, pos, tex_coords);

use glium::Surface;
use crate::shader;
use crate::draw_traits::*;
use crate::ssbo;
use glium::*;
use glium::framebuffer::ToDepthAttachment;
use framebuffer::ToColorAttachment;
use shader::RenderPassType;
use shader::PipelineCache;
use std::rc::Rc;
use std::cell::RefCell;

/// Gets the vertex and index buffer for a rectangle
fn get_rect_vbo_ebo<F : glium::backend::Facade>(facade: &F) 
    -> (VertexBuffer<Vertex>, IndexBuffer<u16>) 
{
    let verts: [Vertex; 4] = [Vertex { pos: [1.0, -1.0], tex_coords: [1., 0.] },
        Vertex { pos: [1.0, 1.0], tex_coords: [1., 1.] },
        Vertex { pos: [-1.0, 1.0], tex_coords: [0., 1.] },
        Vertex { pos: [-1.0, -1.0], tex_coords: [0., 0.] }];
    let indices: [u16; 6] = [2, 3, 0, 2, 0, 1];

    (VertexBuffer::new(facade, &verts).unwrap(), 
    IndexBuffer::new(facade, glium::index::PrimitiveType::TrianglesList, &indices).unwrap())
}

/// Either a `T` or `&T`
pub enum Ownership<'a, T> {
    Own(T),
    Ref(&'a T),
}

impl<'a, T> Ownership<'a, T> {
    /// Gets a reference of the data, regardless of the onwership type
    pub fn to_ref(&self) -> &T {
        match &self {
            Own(s) => &s,
            Ref(s) => s,
        }
    }
}

use Ownership::*;

pub enum StageArgs {
    CascadeArgs([[f32; 4]; 4], f32),
}

pub enum TextureType<'a> {
    Tex2d(Ownership<'a, texture::Texture2d>),
    Depth2d(Ownership<'a, texture::DepthTexture2d>),
    TexCube(Ownership<'a, texture::Cubemap>),
    #[allow(dead_code)]
    Bindless(texture::ResidentTexture),
    WithArg(Box<TextureType<'a>>, StageArgs),
}

/// A RenderTarget is something that can be rendered to and produces a texture
pub trait RenderTarget {
    /// Draws to the render target by passing a framebuffer to `func`. Must be called before `read()`.
    /// 
    /// `viewer` - the viewer for this render. May or may not be passed verbatim to `func`
    /// 
    /// `pipeline_inputs` - any texture inputs to this render target from the pipeline
    /// 
    /// `func` - the function called to render to the render target. Passed the render target
    /// framebuffer, viewer, type of the render target, and any pipeline inputs to this render target
    /// 
    /// Returns the texture output of rendering to this render target
    fn draw(&mut self, viewer: &dyn Viewer, pipeline_inputs: Option<Vec<&TextureType>>,
        cache: &mut PipelineCache,
        func: &dyn Fn(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer, RenderPassType,
        &PipelineCache, &Option<Vec<&TextureType>>)) -> Option<TextureType>;
}

/// A TextureProcessor transforms input textures into an output texture. It is basically
/// a function on textures
pub trait TextureProcessor {
    /// `source` - input textures for the processor
    /// 
    /// `shader` - shader manager
    /// 
    /// `data` - the scene data for the processor or `None`
    fn process<'a>(&mut self, source: Option<Vec<&'a TextureType>>, shader: &shader::ShaderManager,
        cache: &mut PipelineCache<'a>, data: Option<&shader::SceneData>) -> Option<TextureType>;
}

/// RenderTarget which renders to an MSAA color and depth buffer
/// 
/// ### Output
/// 2D RGBA F16 texture with multisampling already resolved
pub struct MsaaRenderTarget<'a> {
    fbo: framebuffer::SimpleFrameBuffer<'a>,
    _tex: Box<texture::Texture2dMultisample>,
    _depth_tex: Box<texture::DepthTexture2dMultisample>,
    out_fbo: framebuffer::SimpleFrameBuffer<'a>,
    out_tex: Box<texture::Texture2d>,
    width: u32,
    height: u32,
}

impl<'a> MsaaRenderTarget<'a> {
    pub fn new<F : glium::backend::Facade>(samples: u32, width: u32, height: u32, facade: &F) -> MsaaRenderTarget {
        let _depth_tex = Box::new(glium::texture::DepthTexture2dMultisample::empty(facade, width, height, samples).unwrap());
        let _tex = Box::new(glium::texture::Texture2dMultisample::empty_with_format(facade, glium::texture::UncompressedFloatFormat::F16F16F16F16,
            glium::texture::MipmapsOption::NoMipmap, width, height, samples).unwrap());
        let out_tex = Box::new(glium::texture::Texture2d::empty_with_format(facade, glium::texture::UncompressedFloatFormat::F16F16F16F16,
            glium::texture::MipmapsOption::NoMipmap, width, height).unwrap());
        let ms_tex = &*_tex as *const glium::texture::Texture2dMultisample;
        let rbo_ptr = &*_depth_tex as *const glium::texture::DepthTexture2dMultisample;
        let out_ptr = &*out_tex as *const glium::texture::Texture2d;
        unsafe {
            MsaaRenderTarget {
                fbo: glium::framebuffer::SimpleFrameBuffer::with_depth_buffer(facade, 
                    &*ms_tex, &*rbo_ptr).unwrap(),
                out_fbo: glium::framebuffer::SimpleFrameBuffer::new(facade, &*out_ptr).unwrap(),
                _tex, _depth_tex, out_tex, width, height,
            }
        }
    }
  
}

impl<'a> RenderTarget for MsaaRenderTarget<'a> {
    fn draw(&mut self, viewer: &dyn Viewer, pipeline_inputs: Option<Vec<&TextureType>>, cache: &mut PipelineCache,
        func: &dyn Fn(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer, RenderPassType, &PipelineCache,&Option<Vec<&TextureType>>)) 
        -> Option<TextureType>
    {
        func(&mut self.fbo, viewer, RenderPassType::Visual, cache, &pipeline_inputs);
        let dst_target = glium::BlitTarget {
            left: 0,
            bottom: 0,
            width: self.width as i32,
            height: self.height as i32,
        };
        self.fbo.blit_whole_color_to(&self.out_fbo, 
            &dst_target, glium::uniforms::MagnifySamplerFilter::Linear);
        Some(TextureType::Tex2d(Ref(&self.out_tex)))
    }
}

/// RenderTarget which renders to Depth buffer
/// 
/// ### Output
/// F32 2D DepthTexture
/// 
/// If a custom view getter is specified, then returns the depth texture with 
/// the used viewer's viewproj matrix
pub struct DepthRenderTarget<'a, F : backend::Facade> {
    width: u32, height: u32,
    viewer: Option<Rc<RefCell<dyn Viewer>>>,
    getter: Option<Box<dyn Fn(&dyn Viewer) -> Box<dyn Viewer>>>,
    facade: &'a F,
}

impl<'a, F : backend::Facade> DepthRenderTarget<'a, F> {
    /// `width` - width of depth texture
    /// 
    /// `height` - height of depth texture
    /// 
    /// `viewer` - custom viewer for this render target or `None` to use whatever viewer
    /// is being used in the rest of the pipeline
    pub fn new(width: u32, height: u32, 
        viewer: Option<Rc<RefCell<dyn Viewer>>>, 
        view_getter: Option<Box<dyn Fn(&dyn Viewer) -> Box<dyn Viewer>>>, facade: &'a F) -> DepthRenderTarget<'a, F> 
    {
        DepthRenderTarget {
            width, height, viewer, getter: view_getter, facade
        }
    }

    fn get_fbo_rbo<'b>(&self) -> (framebuffer::SimpleFrameBuffer<'b>, Box<texture::DepthTexture2d>) {
        let rbo = Box::new(texture::DepthTexture2d::empty_with_format(self.facade, texture::DepthFormat::F32, 
            texture::MipmapsOption::NoMipmap, self.width, self.height).unwrap());
        let rbo_ptr = &*rbo as *const texture::DepthTexture2d;
        unsafe {
            (glium::framebuffer::SimpleFrameBuffer::depth_only(self.facade, &*rbo_ptr).unwrap(), rbo)
        }
    }
  
}

impl<'a, F : backend::Facade> RenderTarget for DepthRenderTarget<'a, F> {
    fn draw(&mut self, viewer: &dyn Viewer, pipeline_inputs: Option<Vec<&TextureType>>, cache: &mut PipelineCache,
        func: &dyn Fn(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer, RenderPassType, &PipelineCache, &Option<Vec<&TextureType>>)) 
        -> Option<TextureType> 
    {
        let (mut fbo, rbo) = self.get_fbo_rbo();
        let maybe_view = self.viewer.as_ref().map(|x| x.borrow());
        let maybe_processed_view = self.getter.as_ref().map(|f| f(maybe_view.as_ref().map(|x| &**x).unwrap_or(viewer)));
        let viewer = maybe_processed_view.as_ref().map(|x| &**x).unwrap_or(maybe_view.as_ref().map(|x| &**x).unwrap_or(viewer));
        let vp : [[f32; 4]; 4] = (viewer.proj_mat() * viewer.view_mat()).into();
        func(&mut fbo, viewer, 
            RenderPassType::Depth, cache, &pipeline_inputs);
        //let tex = TextureType::Depth2d(Ref(&*self.rbo));
        let tex = TextureType::Depth2d(Own(*rbo));
        if maybe_processed_view.is_some() {
            Some(TextureType::WithArg(Box::new(/*TextureType::Bindless(rbo.resident().unwrap())*/tex), StageArgs::CascadeArgs(vp, viewer.view_dist().1)))
        } else {
            Some(tex)
        }
       
    }
}

/// Helper struct for render targets rendering to a cubemap with perspective
struct CubemapRenderBase {
    view_dist: f32,
    view_pos: cgmath::Point3<f32>,
}

impl CubemapRenderBase {
    fn new(view_dist: f32, view_pos: cgmath::Point3<f32>) -> CubemapRenderBase
    {
        CubemapRenderBase {
            view_dist, view_pos,
        }
    }

    /// Gets an array of tuples of view target direction, CubeFace, and up vector
    fn get_target_face_up() 
        -> [(cgmath::Point3<f32>, glium::texture::CubeLayer, cgmath::Vector3<f32>); 6]
    {
        use texture::CubeLayer::*;
        use cgmath::*;
        [(point3(1., 0., 0.), PositiveX, vec3(0., -1., 0.)), (point3(-1., 0., 0.), NegativeX, vec3(0., -1., 0.)),
            (point3(0., 1., 0.), PositiveY, vec3(0., 0., 1.)), (point3(0., -1., 0.), NegativeY, vec3(0., 0., -1.)),
            (point3(0., 0., 1.), PositiveZ, vec3(0., -1., 0.)), (point3(0., 0., -1.), NegativeZ, vec3(0., -1., 0.))]
    }

    /// Repeatedly calls `func` for each face of the cubemap
    /// 
    /// `func` - callable to render a single face of a cubemap. Passed a cube face and camera
    fn draw(&self, func: &dyn Fn(texture::CubeLayer, &dyn Viewer)) {
        use crate::camera::*;
        use cgmath::*;
        let mut cam = PerspectiveCamera {
            cam: self.view_pos,
            aspect: 1f32,
            fov_deg: 90f32,
            target: cgmath::point3(0., 0., 0.),
            near: 0.1,
            far: self.view_dist,
            up: cgmath::vec3(0., 1., 0.),
        };
        let target_faces = Self::get_target_face_up();
        for (target, face, up) in target_faces {
            let target : (f32, f32, f32) = (target.to_vec() + cam.cam.to_vec()).into();
            cam.target = std::convert::From::from(target);
            cam.up = up;
            func(face, &cam);
        }
    }
}

/// RenderTarget which renders to a cubemap with perspective. Can assume that `draw()` ignores its viewer argument
/// and that its called once per face
/// 
/// ### Output
/// F16 RGB cubemap
pub struct CubemapRenderTarget<'a, F : backend::Facade> {
    cubemap: CubemapRenderBase,
    cbo_tex: texture::Cubemap,
    depth_buffer: framebuffer::DepthRenderBuffer,
    _size: u32,
    facade: &'a F,
}

impl<'a, F : backend::Facade> CubemapRenderTarget<'a, F> {
    /// Creates a new CubemapRenderTarget. The cubemap is a F16 RGB texture with no mipmapping
    /// `view_dist` - the view distance for the viewer when rendering to a cubemap
    /// 
    /// `size` - the square side length of each texture face in the cubemap
    /// 
    /// `view_pos` - the position in the scene the cubemap is rendered from
    pub fn new(size: u32, view_dist: f32, view_pos: cgmath::Point3<f32>, facade: &'a F) -> CubemapRenderTarget<'a, F> {
        CubemapRenderTarget {
            _size: size, 
            cubemap: CubemapRenderBase::new(view_dist, view_pos),
            depth_buffer: glium::framebuffer::DepthRenderBuffer::new(facade, 
                glium::texture::DepthFormat::F32, size, size).unwrap(),
            facade,
            cbo_tex: texture::Cubemap::empty_with_format(facade, texture::UncompressedFloatFormat::F16F16F16,
                texture::MipmapsOption::NoMipmap, size).unwrap(),
        }
    }
}

impl<'a, F : backend::Facade> RenderTarget for CubemapRenderTarget<'a, F> {
    fn draw(&mut self, _: &dyn Viewer, pipeline_inputs: Option<Vec<&TextureType>>, cache: &mut PipelineCache,
        func: &dyn Fn(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer, RenderPassType, &PipelineCache, &Option<Vec<&TextureType>>)) 
        -> Option<TextureType>
    {
        self.cubemap.draw(&|face, cam| {
            let mut fbo = glium::framebuffer::SimpleFrameBuffer::with_depth_buffer(self.facade, 
                self.cbo_tex.main_level().image(face), self.depth_buffer.to_depth_attachment()).unwrap();
            fbo.clear_color_and_depth((0., 0., 0., 1.), 1.);
            func(&mut fbo, cam, RenderPassType::Visual, cache, &pipeline_inputs);
        });
        Some(TextureType::TexCube(Ref(&self.cbo_tex)))
    }

}

/// RenderTarget which renders to a cubemap with perspective. Can assume that `draw()` ignores its viewer argument
/// and that it is called once per face, per mipmap level, starting at level 0.
/// 
/// ### Output
/// RGB F16 Cubemap texture with mipmapping
pub struct MipCubemapRenderTarget<'a, F : backend::Facade> {
    cubemap: CubemapRenderBase,
    mip_levels: u32,
    facade: &'a F,
    size: u32,
}

impl<'a, F : backend::Facade> MipCubemapRenderTarget<'a, F> {
    /// Creates a new CubemapRenderTarget. The cubemap is a F16 RGB texture with no mipmapping
    /// `view_dist` - the view distance for the viewer when rendering to a cubemap
    /// 
    /// `size` - the square side length of each texture face in the cubemap at the highest detail mipmap (level 0)
    /// Each successive mipmap level has half the previous size
    /// 
    /// `view_pos` - the position in the scene the cubemap is rendered from
    /// 
    /// `mip_levels` - the amount of mipmaps
    pub fn new(size: u32, mip_levels: u32, view_dist: f32, view_pos: cgmath::Point3<f32>, facade: &'a F) -> MipCubemapRenderTarget<'a, F> {
        MipCubemapRenderTarget {
            mip_levels, facade, size,
            cubemap: CubemapRenderBase::new(view_dist, view_pos),
        }
    }
}

impl<'a, F : backend::Facade> RenderTarget for MipCubemapRenderTarget<'a, F> {
    fn draw(&mut self, _: &dyn Viewer, pipeline_inputs: Option<Vec<&TextureType>>, cache: &mut PipelineCache,
        func: &dyn Fn(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer, RenderPassType, &PipelineCache, &Option<Vec<&TextureType>>)) 
        -> Option<TextureType>
    {
        let cbo_tex = texture::Cubemap::empty_with_format(self.facade, texture::UncompressedFloatFormat::F16F16F16,
        texture::MipmapsOption::AutoGeneratedMipmaps, self.size).unwrap();
        for mip_level in 0 .. self.mip_levels {
            let mip_pow = 0.5f32.powi(mip_level as i32);
            let mipped_size = ((self.size as f32) * mip_pow) as u32;
            self.cubemap.draw(&|face, cam| {
                let rbo = framebuffer::DepthRenderBuffer::new(self.facade, texture::DepthFormat::I24, mipped_size, mipped_size).unwrap();
                let mut fbo = glium::framebuffer::SimpleFrameBuffer::with_depth_buffer(self.facade, 
                    cbo_tex.mipmap(mip_level).unwrap().image(face), 
                    rbo.to_depth_attachment()).unwrap();
                fbo.clear_color_and_depth((0., 0., 0., 1.), 1.);
                func(&mut fbo, cam, RenderPassType::Visual, cache, &pipeline_inputs);
            });
        }
        Some(TextureType::TexCube(Own(cbo_tex)))
    }

}

/// Texture processor which extracts bright parts of a texture for Bloom
/// 
/// ### Inputs
/// 2D texture
/// ### Outputs
/// 2D RGBA F16 texture
pub struct ExtractBrightProcessor<'a> {
    bright_color_tex: Box<glium::texture::Texture2d>,
    bright_color_fbo: framebuffer::SimpleFrameBuffer<'a>,
    vbo: VertexBuffer<Vertex>,
    ebo: IndexBuffer<u16>,
}

impl<'a> ExtractBrightProcessor<'a> {
    pub fn new<F : backend::Facade>(facade: &F, width: u32, height: u32) -> ExtractBrightProcessor<'a> {
        let bright_color_tex = Box::new(glium::texture::Texture2d::empty_with_format(facade,
            glium::texture::UncompressedFloatFormat::F16F16F16F16, glium::texture::MipmapsOption::NoMipmap,
            width, height).unwrap());
        let (vbo, ebo) = get_rect_vbo_ebo(facade);
        unsafe {
            let tex_ptr = &*bright_color_tex as *const texture::Texture2d;
            ExtractBrightProcessor {
                bright_color_tex, 
                bright_color_fbo: glium::framebuffer::SimpleFrameBuffer::new(facade, &*tex_ptr).unwrap(),
                ebo, vbo,
            }
        }
    }
}

impl<'a> TextureProcessor for ExtractBrightProcessor<'a> {
    fn process(&mut self, source: Option<Vec<&TextureType>>, shader: &shader::ShaderManager,
        pc: &mut PipelineCache, sd: Option<&shader::SceneData>) -> Option<TextureType>
    {
        if let TextureType::Tex2d(source) = source.unwrap()[0] {
            let source = source.to_ref();
            let data = shader::UniformInfo::ExtractBrightInfo(shader::ExtractBrightData {
                tex: source
            });
            let (program, params, uniform) = shader.use_shader(&data, sd, Some(pc));
            match uniform {
                shader::UniformType::ExtractBrightUniform(uniform) => {
                    let fbo = &mut self.bright_color_fbo;
                    fbo.clear_color(0., 0., 0., 1.);
                    fbo.draw(&self.vbo, &self.ebo, program, &uniform, &params).unwrap()
                },
                _ => panic!("Invalid uniform type returned for RenderTarget"),
            };
            Some(TextureType::Tex2d(Ref(&self.bright_color_tex)))
        } else {
            panic!("Invalid texture source for extract bright");
        }
    }
}

/// Texture processor which performs a separable convolution
/// 
/// ### Inputs
/// 2D texture
/// ### Outputs
/// 2D RGBA F16 Texture
pub struct SepConvProcessor<'a> {
    ping_pong_tex: [Box<texture::Texture2d>; 2],
    ping_pong_fbo: [framebuffer::SimpleFrameBuffer<'a>; 2],
    iterations: usize,
    ebo: IndexBuffer<u16>,
    vbo: VertexBuffer<Vertex>,
}

impl<'a> SepConvProcessor<'a> {
    /// Requires `iterations >= 2` because a single convolution is broken up into two passes. So an odd number
    /// for `iterations` performs a multiple of `1/2` convolutions
    pub fn new<F : backend::Facade>(width: u32, height: u32, iterations: usize, facade: &'a F) -> SepConvProcessor {
        use std::mem::MaybeUninit;
        let mut ping_pong_tex: [MaybeUninit<Box<texture::Texture2d>>; 2] = unsafe { MaybeUninit::uninit().assume_init() };
        let mut ping_pong_fbo: [MaybeUninit<framebuffer::SimpleFrameBuffer<'a>>; 2] = unsafe { MaybeUninit::uninit().assume_init() };
        let (vbo, ebo) = get_rect_vbo_ebo(facade);
        for i in 0 .. 2 {
            let tex_box = Box::new(glium::texture::Texture2d::empty_with_format(facade,
                glium::texture::UncompressedFloatFormat::F16F16F16F16, glium::texture::MipmapsOption::NoMipmap,
                width, height).unwrap());
            let tex_ptr = &*tex_box as *const texture::Texture2d;
            unsafe {
                ping_pong_tex[i].write(tex_box);
                ping_pong_fbo[i].write(glium::framebuffer::SimpleFrameBuffer::new(facade, &*tex_ptr).unwrap());
            }
        }
        unsafe {
            SepConvProcessor {
                iterations, ping_pong_fbo: std::mem::transmute::<_, [framebuffer::SimpleFrameBuffer<'a>; 2]>(ping_pong_fbo), 
                ping_pong_tex: std::mem::transmute::<_, [Box<texture::Texture2d>; 2]>(ping_pong_tex),
                vbo, ebo
            }
        }
    }

    fn pass(dst: &mut framebuffer::SimpleFrameBuffer, source: &texture::Texture2d, 
        vbo: &VertexBuffer<Vertex>, ebo: &IndexBuffer<u16>, iteration: usize, shaders: &shader::ShaderManager) 
    {
        let data = shader::UniformInfo::SepConvInfo(shader::SepConvData {
            horizontal_pass: iteration % 2 == 0, 
            tex: source
        });
        let (program, params, uniform) = shaders.use_shader(&data, None, None);
        match uniform {
            shader::UniformType::SepConvUniform(uniform) => {
                dst.draw(vbo, ebo, program, &uniform, params).unwrap();
            },
            _ => panic!("Invalid uniform type returned for RenderTarget"),
        }
    }
}

impl<'a> TextureProcessor for SepConvProcessor<'a> {
    fn process(&mut self, source: Option<Vec<&TextureType>>, shader: &shader::ShaderManager,
        _: &mut PipelineCache, _: Option<&shader::SceneData>) -> Option<TextureType>
    {
        if let TextureType::Tex2d(source) = source.unwrap()[0] {
            let source = source.to_ref();
            SepConvProcessor::pass(&mut self.ping_pong_fbo[0], source, &self.vbo, &self.ebo, 0, shader);
            for i in 1 .. self.iterations {
                let tex = &*self.ping_pong_tex[(i - 1) % 2];
                let dst = &mut self.ping_pong_fbo[i % 2];
                SepConvProcessor::pass(dst, tex, &self.vbo, &self.ebo, i, shader);
            }
            Some(TextureType::Tex2d(Ref(&*self.ping_pong_tex[(self.iterations - 1) % 2])))
        } else {
            panic!("Invalid source type for separable convolution");
        }
    }
}

/// A processor which additively blends together textures and renders them to a surface
/// 
/// ### Inputs
/// 2D Main texture
/// 2D additive texture
/// ### Outputs
/// None (result is drawn as a quad to main FBO)
pub struct UiCompositeProcessor<S : Surface, F : Fn() -> S, G : Fn(S)> {
    vbo: VertexBuffer<Vertex>,
    ebo: IndexBuffer<u16>,
    get_surface: F,
    clean_surface: G,
}

impl<S : Surface, F : Fn() -> S, G : Fn(S)> UiCompositeProcessor<S, F, G> {
    /// `get_surface` - callable that returns the surface to render to. The surface is **not** cleared
    /// 
    /// `clean_surface` - callable that accepts the returned surface and performs any necessary cleanup
    /// after drawing is finished
    pub fn new<Fac: backend::Facade>(facade: &Fac, get_surface: F, clean_surface: G) -> UiCompositeProcessor<S, F, G> {
        let (vbo, ebo) = get_rect_vbo_ebo(facade);
        UiCompositeProcessor { vbo, ebo, get_surface, clean_surface }
    }

    fn render<'a>(&self, tex_a: &Ownership<'a, texture::Texture2d>, cache: &PipelineCache,
        tex_b: Option<&Ownership<'a, texture::Texture2d>>, shader: &shader::ShaderManager) 
    {
        let diffuse = tex_a.to_ref();
        let blend_tex = tex_b.map(|tex| tex.to_ref());
        let args = shader::UniformInfo::UiInfo(shader::UiData {
            diffuse, do_blend: blend_tex.is_some(), blend_tex,
            model: cgmath::Matrix4::from_scale(1f32).into(),
        });
        let (program, params, uniform) = shader.use_shader(&args, None, Some(cache));
        match uniform {
            shader::UniformType::UiUniform(uniform) => {
                let mut surface = (self.get_surface)();
                surface.draw(&self.vbo, &self.ebo, program, &uniform, &params).unwrap();
                (self.clean_surface)(surface);
            },
            _ => panic!("Invalid uniform type returned for RenderTarget"),
        };
    }
}

impl<S : Surface, F : Fn() -> S, G : Fn(S)> TextureProcessor for UiCompositeProcessor<S, F, G> {
    fn process(&mut self, source: Option<Vec<&TextureType>>, shader: &shader::ShaderManager,
        c: &mut PipelineCache, _: Option<&shader::SceneData>) -> Option<TextureType>
    {
        let source = source.unwrap();
        if source.len() == 2 {
            match (source[0], source[1]) {
                (TextureType::Tex2d(diffuse), TextureType::Tex2d(blend)) => {
                    self.render(diffuse, c, Some(blend), shader);
                    None
                },
                _ => panic!("Invalid texture type passed to texture processor")
            }
        } else if source.len() == 1 {
            if let TextureType::Tex2d(diffuse) = source[0] {
                self.render(diffuse, c, None, shader);
                None
            } else {
                panic!("Invalid texture type passed to ui composer")
            }
        } else {
            panic!("Invalid number of source textures")
        }
    }
}

/// Texture processor which copies its input texture by performing a framebuffer blit
/// 
/// ### Inputs
/// Any texture
/// ### Outputs
/// An owned texture that is exactly the same as the input
pub struct CopyTextureProcessor<'a, F : backend::Facade> {
    facade: &'a F,
    width: u32,
    height: u32,
    tex_format: texture::UncompressedFloatFormat,
    mipmap: texture::MipmapsOption,
}

impl<'a, F : backend::Facade> CopyTextureProcessor<'a, F> {
    /// `fmt` - the output texture format or `None` for F16 RGBA
    /// 
    /// `mipmap` - the output texture mipmapping or `None` for No mipmaps
    pub fn new(width: u32, height: u32, fmt: Option<texture::UncompressedFloatFormat>, 
        mipmap: Option<texture::MipmapsOption>, facade: &F) -> CopyTextureProcessor<F> 
    {
        CopyTextureProcessor {width, height, facade, tex_format: fmt.unwrap_or(texture::UncompressedFloatFormat::F16F16F16F16),
        mipmap: mipmap.unwrap_or(texture::MipmapsOption::NoMipmap)}
    }

    fn blit_src_to_dst<'b, S : ToColorAttachment<'b>, D : ToColorAttachment<'b>>(&self, source: S, dst: D) {
        let out_fbo = framebuffer::SimpleFrameBuffer::new(self.facade, dst).unwrap();
        let in_fbo = framebuffer::SimpleFrameBuffer::new(self.facade, source).unwrap();
        let target = BlitTarget {
            left: 0,
            bottom: 0,
            width: self.height as i32,
            height: self.width as i32,
        };
        in_fbo.blit_whole_color_to(&out_fbo, &target, uniforms::MagnifySamplerFilter::Linear);
    }
}

impl<'a, F : backend::Facade> TextureProcessor for CopyTextureProcessor<'a, F> {
    fn process(&mut self, source: Option<Vec<&TextureType>>, _: &shader::ShaderManager, 
        _: &mut PipelineCache, _: Option<&shader::SceneData>) -> Option<TextureType>
    {
        if source.is_none() {
            return None
        }
        match source.unwrap()[0] {
            TextureType::Tex2d(Ref(x)) => {
                let out = texture::Texture2d::empty_with_format(self.facade,
                    self.tex_format, self.mipmap,
                    self.width, self.height).unwrap();
                self.blit_src_to_dst(*x, &out);
                Some(TextureType::Tex2d(Own(out)))
            },
            TextureType::TexCube(Ref(x)) => {
                use texture::CubeLayer::*;
                let out = texture::Cubemap::empty_with_format(self.facade,
                    self.tex_format, self.mipmap,
                    self.width).unwrap();
                let layers = [PositiveX, NegativeX, PositiveY, NegativeY, PositiveZ, NegativeZ];
                for layer in layers {
                    self.blit_src_to_dst(x.main_level().image(layer), 
                        out.main_level().image(layer));
                }
                Some(TextureType::TexCube(Own(out)))
            },
            _ => panic!("Not implemented copy type"),
        }
    }
}

/// Texture processor which generates a BRDF lookup texture
/// Can assume that this processor ignores its inputs
/// 
/// ### Inputs
/// None
/// ### Outputs
/// RGB_F16 Look up texture
pub struct GenLutProcessor<'a, F : backend::Facade> {
    vbo: VertexBuffer<Vertex>,
    ebo: IndexBuffer<u16>,
    width: u32, height: u32,
    facade: &'a F,
}

impl<'a, F : backend::Facade> GenLutProcessor<'a, F> {
    pub fn new(facade: &'a F, width: u32, height: u32) -> GenLutProcessor<'a, F> {
        let (vbo, ebo) = get_rect_vbo_ebo(facade);
        GenLutProcessor {
            ebo, vbo, width, height, facade
        }
    }
}

impl<'a, F : backend::Facade> TextureProcessor for GenLutProcessor<'a, F> {
    fn process(&mut self, _: Option<Vec<&TextureType>>, shader: &shader::ShaderManager, 
        pc: &mut PipelineCache, sd: Option<&shader::SceneData>) -> Option<TextureType>
    {
        let tex = texture::Texture2d::empty_with_format(self.facade,
            texture::UncompressedFloatFormat::F16F16, texture::MipmapsOption::NoMipmap,
            self.width, self.height).unwrap();
        let rbo = framebuffer::DepthRenderBuffer::new(self.facade, texture::DepthFormat::I24,
            self.width, self.height).unwrap();
        let mut fbo = framebuffer::SimpleFrameBuffer::with_depth_buffer(self.facade, &tex, &rbo).unwrap();
        fbo.clear_color_and_depth((0., 0., 0., 0.), 1.);
        let (program, params, uniform) = shader.use_shader(&shader::UniformInfo::GenLutInfo, sd, Some(pc));
        match uniform {
            shader::UniformType::BrdfLutUniform(uniform) => 
                fbo.draw(&self.vbo, &self.ebo, program, &uniform, params).unwrap(),
            _ => panic!("Gen lut got unexepected uniform type")
        };
        Some(TextureType::Tex2d(Own(tex)))
    }
}

/// Texture processor for culling lights from the input depth map
/// Results are stored in a shared shader storage buffer
/// 
/// ### Inputs
/// 2D Depth Texture
/// ### Outputs
/// None (results stored in SSBO owned by this processor)
/// ### Mutators
/// Saves the horizontal work group number to SceneData's tiles_x param
pub struct CullLightProcessor {
    work_groups_x: u32,
    work_groups_y: u32,
    visible_light_buffer: ssbo::SSBO<i32>,
    width: u32,
    height: u32,
}

impl CullLightProcessor {
    pub fn new(width: u32, height: u32, tile_size: u32) -> CullLightProcessor {
        let max_lights = 1024;
        let work_groups_x = (width + width % tile_size) / tile_size;
        let work_groups_y = (height + height % tile_size) / tile_size;
        CullLightProcessor {
            work_groups_x, work_groups_y,
            visible_light_buffer: ssbo::SSBO::<i32>::static_empty(work_groups_x * work_groups_y * max_lights),
            width, height,
        }
    }

    #[allow(dead_code)]
    pub fn get_groups_x(&self) -> u32 {
        self.work_groups_x
    }
}

impl TextureProcessor for CullLightProcessor {
    fn process(&mut self, input: Option<Vec<&TextureType>>, shader: &shader::ShaderManager, 
        cache: &mut PipelineCache, data: Option<&shader::SceneData>) -> Option<TextureType>
    {
        if let TextureType::Depth2d(depth) = input.unwrap()[0] {
            let depth_tex = depth.to_ref();
            let params = shader::UniformInfo::LightCullInfo(shader::LightCullData {
                depth_tex: depth_tex,
                scr_width: self.width,
                scr_height: self.height,
            });
            self.visible_light_buffer.bind(1);
            cache.tiles_x = Some(self.work_groups_x);
            shader.execute_compute(self.work_groups_x, self.work_groups_y, 1, params, data);
            None
        } else {
            panic!("Unexpected texture input!");
        }
    }
}
/// Texture processor that stores its inputs in PipelineCache to be used as
/// shader uniform inputs for subsequent stages
pub struct ToCacheProcessor<'a, F : backend::Facade> {
    facade: &'a F,
}

impl<'a, F : backend::Facade> ToCacheProcessor<'a, F> {
    pub fn new(facade: &'a F) -> ToCacheProcessor<'a, F> {
        ToCacheProcessor { facade }
    }
}

impl<'a, F : backend::Facade> TextureProcessor for ToCacheProcessor<'a, F> {
    fn process<'b>(&mut self, input: Option<Vec<&'b TextureType>>, _: &shader::ShaderManager, 
        cache: &mut PipelineCache<'b>, _: Option<&shader::SceneData>) -> Option<TextureType>
    {
        use std::mem::MaybeUninit;
        if input.is_none() { return None }
        else {
            let input = input.unwrap();
            assert_eq!(input.len(), 3);
            let mut depth_texs = Vec::<&'b glium::texture::DepthTexture2d>::new();
            let mut mats: [MaybeUninit<[[f32; 4]; 4]>; 5] = unsafe { MaybeUninit::uninit().assume_init() };
            let mut fars: [MaybeUninit<f32>; 4] = unsafe { MaybeUninit::uninit().assume_init() };
            for (tex, i) in input.into_iter().zip(0..3) {
                match tex {
                    TextureType::WithArg(b, StageArgs::CascadeArgs(mat, far)) => {
                        match &**b {
                            TextureType::Depth2d(tex) => {                            
                                //depth_texs[i].write(glium::texture::TextureHandle::new(tex, &sb));
                                depth_texs.push(tex.to_ref());
                                mats[i].write(*mat);
                                fars[i].write(*far);
                            },
                            _ => panic!("Unimplemented"),
                        }
                    },
                    _ => panic!("Unimplemented"),
                }
            }
            fars[3].write(1f32);
            mats[3].write(cgmath::Matrix4::<f32>::from_scale(1f32).into());
            mats[4].write(cgmath::Matrix4::<f32>::from_scale(1f32).into());
            unsafe {
                cache.cascade_ubo = glium::uniforms::UniformBuffer::persistent(self.facade, shader::CascadeUniform {
                    //depth_maps: std::mem::transmute::<_, [glium::texture::TextureHandle<'b>; 5]>(depth_texs),
                    far_planes: std::mem::transmute::<_, [f32; 4]>(fars),
                    viewproj_mats: std::mem::transmute::<_, [[[f32; 4]; 4]; 5]>(mats),

                }).ok();
                cache.cascade_maps = Some(depth_texs);
            }
        }
        None
    }
}