#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 2],
    tex_coords: [f32; 2],
}

glium::implement_vertex!(Vertex, pos, tex_coords);

use glium::Surface;
use crate::shader;
use crate::draw_traits::*;
use glium::*;
use glium::framebuffer::ToDepthAttachment;
use framebuffer::ToColorAttachment;

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

pub enum Ownership<'a, T> {
    Own(T),
    Ref(&'a T),
}

impl<'a, T> Ownership<'a, T> {
    pub fn to_ref(&self) -> &T {
        match &self {
            Own(s) => &s,
            Ref(s) => s,
        }
    }
}

use Ownership::*;

pub enum TextureType<'a> {
    Tex2d(Ownership<'a, texture::Texture2d>),
    TexCube(Ownership<'a, texture::Cubemap>),
    ToDefaultFbo,
}

/// A RenderTarget is something that can be rendered to and produces a texture
pub trait RenderTarget {
    /// Draws to the render target by passing a framebuffer to `func`. Must be called before `read()`.
    /// 
    /// `viewer` - the viewer for this render. May or may not be passed verbatim to `func`
    /// 
    /// Returns the texture output of rendering to this render target
    fn draw(&mut self, viewer: &dyn Viewer, 
        func: &dyn Fn(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer)) -> TextureType;
}

/// A TextureProcessor transforms input textures into an output texture. It is basically
/// a function on textures
pub trait TextureProcessor {
    fn process(&mut self, source: Vec<&TextureType>, shader: &shader::ShaderManager) -> TextureType;
}

/// RenderTarget which renders to an MSAA color and depth buffer
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
    fn draw(&mut self, viewer: &dyn Viewer, func: &dyn Fn(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer)) -> TextureType {
        func(&mut self.fbo, viewer);
        let dst_target = glium::BlitTarget {
            left: 0,
            bottom: 0,
            width: self.width as i32,
            height: self.height as i32,
        };
        self.fbo.blit_whole_color_to(&self.out_fbo, 
            &dst_target, glium::uniforms::MagnifySamplerFilter::Linear);
        TextureType::Tex2d(Ref(&self.out_tex))
    }
}
/// Helper struct for render targets rendering to a cubemap with perspective
struct CubemapRenderBase {
    view_dist: f32,
    view_pos: cgmath::Point3<f32>,
}

impl CubemapRenderBase {
    fn new<F : backend::Facade>(view_dist: f32, view_pos: cgmath::Point3<f32>, size: u32,
        facade: &F) -> CubemapRenderBase
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
        use crate::node;
        use cgmath::*;
        let mut cam = PerspectiveCamera {
            cam: node::Node::new(Some(self.view_pos), None, None, None),
            aspect: 1f32,
            fov_deg: 90f32,
            target: cgmath::point3(0., 0., 0.),
            near: 0.1,
            far: self.view_dist,
            up: cgmath::vec3(0., 1., 0.),
        };
        let target_faces = Self::get_target_face_up();
        for (target, face, up) in target_faces {
            let target : (f32, f32, f32) = (target.to_vec() + cam.cam.pos.to_vec()).into();
            cam.target = std::convert::From::from(target);
            cam.up = up;
            func(face, &cam);
        }
    }
}

/// RenderTarget which renders to a cubemap with perspective. Can assume that `draw()` ignores its viewer argument
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
            cubemap: CubemapRenderBase::new(view_dist, view_pos, size, facade),
            depth_buffer: glium::framebuffer::DepthRenderBuffer::new(facade, 
                glium::texture::DepthFormat::F32, size, size).unwrap(),
            facade,
            cbo_tex: texture::Cubemap::empty_with_format(facade, texture::UncompressedFloatFormat::F16F16F16,
                texture::MipmapsOption::NoMipmap, size).unwrap(),
        }
    }
}

impl<'a, F : backend::Facade> RenderTarget for CubemapRenderTarget<'a, F> {
    fn draw(&mut self, _: &dyn Viewer, func: &dyn Fn(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer)) -> TextureType {
        self.cubemap.draw(&|face, cam| {
            let mut fbo = glium::framebuffer::SimpleFrameBuffer::with_depth_buffer(self.facade, 
                self.cbo_tex.main_level().image(face), self.depth_buffer.to_depth_attachment()).unwrap();
            fbo.clear_color_and_depth((0., 0., 0., 1.), 1.);
            func(&mut fbo, cam);
        });
        TextureType::TexCube(Ref(&self.cbo_tex))
    }

}

/// RenderTarget which renders to a cubemap with perspective. Can assume that `draw()` ignores its viewer argument
pub struct MipCubemapRenderTarget<'a, F : backend::Facade> {
    cubemap: CubemapRenderBase,
    mip_levels: u32,
    facade: &'a F,
    size: u32,
    cbo_tex: Option<texture::Cubemap>,
}

impl<'a, F : backend::Facade> MipCubemapRenderTarget<'a, F> {
    /// Creates a new CubemapRenderTarget. The cubemap is a F16 RGB texture with no mipmapping
    /// `view_dist` - the view distance for the viewer when rendering to a cubemap
    /// 
    /// `size` - the square side length of each texture face in the cubemap
    /// 
    /// `view_pos` - the position in the scene the cubemap is rendered from
    /// 
    /// `mip_levels` - the amount of mipmaps
    pub fn new(size: u32, mip_levels: u32, view_dist: f32, view_pos: cgmath::Point3<f32>, facade: &'a F) -> MipCubemapRenderTarget<'a, F> {
        MipCubemapRenderTarget {
            mip_levels, facade, size,
            cubemap: CubemapRenderBase::new(view_dist, view_pos, size, facade),
            cbo_tex: None,
        }
    }
}

impl<'a, F : backend::Facade> RenderTarget for MipCubemapRenderTarget<'a, F> {
    fn draw(&mut self, _: &dyn Viewer, func: &dyn Fn(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer)) -> TextureType {
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
                func(&mut fbo, cam);
            });
        }
        TextureType::TexCube(Own(cbo_tex))
    }

}

/// Texture processor which extracts bright parts of a texture for Bloom
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
    fn process(&mut self, source: Vec<&TextureType>, shader: &shader::ShaderManager) -> TextureType {
        if let TextureType::Tex2d(source) = source[0] {
            let source = source.to_ref();
            let data = shader::UniformInfo::ExtractBrightInfo(shader::ExtractBrightData {
                tex: source
            });
            let (program, params, uniform) = shader.use_shader(&data);
            match uniform {
                shader::UniformType::ExtractBrightUniform(uniform) => {
                    let fbo = &mut self.bright_color_fbo;
                    fbo.clear_color(0., 0., 0., 1.);
                    fbo.draw(&self.vbo, &self.ebo, program, &uniform, &params).unwrap()
                },
                _ => panic!("Invalid uniform type returned for RenderTarget"),
            };
            TextureType::Tex2d(Ref(&self.bright_color_tex))
        } else {
            panic!("Invalid texture source for extract bright");
        }
    }
}

/// Texture processor which performs a separable convolution
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
        let (program, params, uniform) = shaders.use_shader(&data);
        match uniform {
            shader::UniformType::SepConvUniform(uniform) => {
                dst.draw(vbo, ebo, program, &uniform, params).unwrap();
            },
            _ => panic!("Invalid uniform type returned for RenderTarget"),
        }
    }
}

impl<'a> TextureProcessor for SepConvProcessor<'a> {
    fn process(&mut self, source: Vec<&TextureType>, shader: &shader::ShaderManager) -> TextureType {
        if let TextureType::Tex2d(source) = source[0] {
            let source = source.to_ref();
            SepConvProcessor::pass(&mut self.ping_pong_fbo[0], source, &self.vbo, &self.ebo, 0, shader);
            for i in 1 .. self.iterations {
                let tex = &*self.ping_pong_tex[(i - 1) % 2];
                let dst = &mut self.ping_pong_fbo[i % 2];
                SepConvProcessor::pass(dst, tex, &self.vbo, &self.ebo, i, shader);
            }
            TextureType::Tex2d(Ref(&*self.ping_pong_tex[(self.iterations - 1) % 2]))
        } else {
            panic!("Invalid source type for separable convolution");
        }
    }
}

/// A processor which additively blends together textures and renders them to a surface
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
}

impl<S : Surface, F : Fn() -> S, G : Fn(S)> TextureProcessor for UiCompositeProcessor<S, F, G> {
    fn process(&mut self, source: Vec<&TextureType>, shader: &shader::ShaderManager) -> TextureType {
        if source.len() == 2 {
            match (source[0], source[1]) {
                (TextureType::Tex2d(diffuse), TextureType::Tex2d(blend)) => {
                    let (diffuse, blend) = (diffuse.to_ref(), blend.to_ref());
                    let args = shader::UniformInfo::UiInfo(shader::UiData {
                        diffuse,
                        do_blend: true,
                        blend_tex: Some(blend),
                        model: cgmath::Matrix4::from_scale(1f32).into(),
                    });
                    let (program, params, uniform) = shader.use_shader(&args);
                    match uniform {
                        shader::UniformType::UiUniform(uniform) => {
                            let mut surface = (self.get_surface)();
                            surface.draw(&self.vbo, &self.ebo, program, &uniform, &params).unwrap();
                            (self.clean_surface)(surface);
                        },
                        _ => panic!("Invalid uniform type returned for RenderTarget"),
                    };
                    TextureType::ToDefaultFbo
                },
                _ => panic!("Invalid texture type passed to texture processor")
            }
        } else {
            panic!("Invalid number of source textures")
        }
    }
}

/// Texture processor which copies its input texture by performing a framebuffer blit
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
    fn process(&mut self, source: Vec<&TextureType>, _: &shader::ShaderManager) -> TextureType {
        match source[0] {
            TextureType::Tex2d(Own(_)) => panic!("Not implemented copy"),//TextureType::Tex2d(Own(x)),
            TextureType::TexCube(Own(_)) => panic!("Not implemented copy"),//TextureType::TexCube(Own(x)),
            TextureType::ToDefaultFbo => TextureType::ToDefaultFbo,
            TextureType::Tex2d(Ref(x)) => {
                let out = texture::Texture2d::empty_with_format(self.facade,
                    self.tex_format, self.mipmap,
                    self.width, self.height).unwrap();
                self.blit_src_to_dst(*x, &out);
                TextureType::Tex2d(Own(out))
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
                TextureType::TexCube(Own(out))
            }
        }
    }
}