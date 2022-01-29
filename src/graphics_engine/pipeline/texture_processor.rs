use super::*;
use Vertex2D as Vertex;
use glium::framebuffer::ToColorAttachment;
use crate::cg_support::ssbo;

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

/// Texture processor which extracts bright parts of a texture for Bloom
/// 
/// ### Inputs
/// 2D texture
/// ### Outputs
/// 2D RGBA F16 texture
pub struct ExtractBrightProcessor {
    bright_color_tex: Box<glium::texture::Texture2d>,
    bright_color_fbo: framebuffer::SimpleFrameBuffer<'static>,
    vbo: VertexBuffer<Vertex>,
    ebo: IndexBuffer<u16>,
}

impl ExtractBrightProcessor {
    pub fn new<F : backend::Facade>(facade: &F, width: u32, height: u32) -> ExtractBrightProcessor {
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

impl TextureProcessor for ExtractBrightProcessor {
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
pub struct SepConvProcessor {
    ping_pong_tex: [Box<texture::Texture2d>; 2],
    ping_pong_fbo: [framebuffer::SimpleFrameBuffer<'static>; 2],
    iterations: usize,
    ebo: IndexBuffer<u16>,
    vbo: VertexBuffer<Vertex>,
}

impl SepConvProcessor {
    /// Requires `iterations >= 2` because a single convolution is broken up into two passes. So an odd number
    /// for `iterations` performs a multiple of `1/2` convolutions
    pub fn new<F : backend::Facade>(width: u32, height: u32, iterations: usize, facade: &F) -> SepConvProcessor {
        use std::mem::MaybeUninit;
        let mut ping_pong_tex: [MaybeUninit<Box<texture::Texture2d>>; 2] = unsafe { MaybeUninit::uninit().assume_init() };
        let mut ping_pong_fbo: [MaybeUninit<framebuffer::SimpleFrameBuffer<'static>>; 2] = unsafe { MaybeUninit::uninit().assume_init() };
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
                iterations, ping_pong_fbo: std::mem::transmute::<_, [framebuffer::SimpleFrameBuffer<'static>; 2]>(ping_pong_fbo), 
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
                dst.draw(vbo, ebo, program, &uniform, &params).unwrap();
            },
            _ => panic!("Invalid uniform type returned for RenderTarget"),
        }
    }
}

impl<'a> TextureProcessor for SepConvProcessor {
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
pub struct UiCompositeProcessor<S : Surface, 
    D : std::ops::DerefMut<Target = S>, F : Fn() -> D, 
    G : Fn(D)> 
{
    vbo: VertexBuffer<Vertex>,
    ebo: IndexBuffer<u16>,
    get_surface: F,
    clean_surface: G,
}

impl<S : Surface, D : std::ops::DerefMut<Target = S>, 
    F : Fn() -> D, G : Fn(D)> UiCompositeProcessor<S, D, F, G> 
{
    /// `get_surface` - callable that returns the surface to render to. The surface is **not** cleared
    /// 
    /// `clean_surface` - callable that accepts the returned surface and performs any necessary cleanup
    /// after drawing is finished
    pub fn new<Fac: backend::Facade>(facade: &Fac, get_surface: F, clean_surface: G) -> UiCompositeProcessor<S, D, F, G> {
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
                let mut surface_holder = (self.get_surface)();
                {
                    let surface = &mut *surface_holder;
                    surface.clear_color_and_depth((0., 0., 0., 1.), 1.);
                    surface.draw(&self.vbo, &self.ebo, program, &uniform, &params).unwrap();
                }
                (self.clean_surface)(surface_holder);
            },
            _ => panic!("Invalid uniform type returned for RenderTarget"),
        };
    }
}

impl<S : Surface, D : std::ops::DerefMut<Target = S>, F : Fn() -> D, G : Fn(D)> 
    TextureProcessor for UiCompositeProcessor<S, D, F, G> 
{
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
pub struct CopyTextureProcessor {
    width: u32,
    height: u32,
    tex_format: texture::UncompressedFloatFormat,
    mipmap: texture::MipmapsOption,
}

impl CopyTextureProcessor {
    /// `fmt` - the output texture format or `None` for F16 RGBA
    /// 
    /// `mipmap` - the output texture mipmapping or `None` for No mipmaps
    pub fn new(width: u32, height: u32, fmt: Option<texture::UncompressedFloatFormat>, 
        mipmap: Option<texture::MipmapsOption>) -> CopyTextureProcessor
    {
        CopyTextureProcessor {width, height, tex_format: fmt.unwrap_or(texture::UncompressedFloatFormat::F16F16F16F16),
        mipmap: mipmap.unwrap_or(texture::MipmapsOption::NoMipmap)}
    }

    fn blit_src_to_dst<'b, S : ToColorAttachment<'b>, 
        D : ToColorAttachment<'b>, F : glium::backend::Facade>(&self, source: S, dst: D, facade: &F) 
    {
        let out_fbo = framebuffer::SimpleFrameBuffer::new(facade, dst).unwrap();
        let in_fbo = framebuffer::SimpleFrameBuffer::new(facade, source).unwrap();
        let target = BlitTarget {
            left: 0,
            bottom: 0,
            width: self.height as i32,
            height: self.width as i32,
        };
        in_fbo.blit_whole_color_to(&out_fbo, &target, uniforms::MagnifySamplerFilter::Linear);
    }
}

impl TextureProcessor for CopyTextureProcessor {
    fn process(&mut self, source: Option<Vec<&TextureType>>, _: &shader::ShaderManager, 
        _: &mut PipelineCache, _: Option<&shader::SceneData>) -> Option<TextureType>
    {
        if source.is_none() {
            return None
        }
        let ctx = super::super::get_active_ctx();
        match source.unwrap()[0] {
            TextureType::Tex2d(Ref(x)) => {
                let out = texture::Texture2d::empty_with_format(&*ctx.ctx.borrow(),
                    self.tex_format, self.mipmap,
                    self.width, self.height).unwrap();
                self.blit_src_to_dst(*x, &out, &*ctx.ctx.borrow());
                Some(TextureType::Tex2d(Own(out)))
            },
            TextureType::TexCube(Ref(x)) => {
                use texture::CubeLayer::*;
                let out = texture::Cubemap::empty_with_format(&*ctx.ctx.borrow(),
                    self.tex_format, self.mipmap,
                    self.width).unwrap();
                let layers = [PositiveX, NegativeX, PositiveY, NegativeY, PositiveZ, NegativeZ];
                for layer in layers {
                    self.blit_src_to_dst(x.main_level().image(layer), 
                        out.main_level().image(layer), &*ctx.ctx.borrow());
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
pub struct GenLutProcessor {
    vbo: VertexBuffer<Vertex>,
    ebo: IndexBuffer<u16>,
    width: u32, height: u32,
}

impl GenLutProcessor {
    pub fn new<F : glium::backend::Facade>(width: u32, height: u32, facade: &F) -> GenLutProcessor {
        let (vbo, ebo) = get_rect_vbo_ebo(facade);
        GenLutProcessor {
            ebo, vbo, width, height
        }
    }
}

impl TextureProcessor for GenLutProcessor {
    fn process(&mut self, _: Option<Vec<&TextureType>>, shader: &shader::ShaderManager, 
        pc: &mut PipelineCache, sd: Option<&shader::SceneData>) -> Option<TextureType>
    {
        let ctx = super::super::get_active_ctx();
        let tex = texture::Texture2d::empty_with_format(&*ctx.ctx.borrow(),
            texture::UncompressedFloatFormat::F16F16, texture::MipmapsOption::NoMipmap,
            self.width, self.height).unwrap();
        let rbo = framebuffer::DepthRenderBuffer::new(&*ctx.ctx.borrow(), texture::DepthFormat::I24,
            self.width, self.height).unwrap();
        let mut fbo = framebuffer::SimpleFrameBuffer::with_depth_buffer(&*ctx.ctx.borrow(), &tex, &rbo).unwrap();
        fbo.clear_color_and_depth((0., 0., 0., 0.), 1.);
        let (program, params, uniform) = shader.use_shader(&shader::UniformInfo::GenLutInfo, sd, Some(pc));
        match uniform {
            shader::UniformType::BrdfLutUniform(uniform) => 
                fbo.draw(&self.vbo, &self.ebo, program, &uniform, &params).unwrap(),
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
pub struct ToCacheProcessor {}

impl ToCacheProcessor {
    pub fn new() -> ToCacheProcessor {
        ToCacheProcessor { }
    }

    fn cascade_maps_to_cache<'b>(input: Vec<&'b TextureType>, cache: &mut PipelineCache<'b>) {
        use std::mem::MaybeUninit;
        let mut depth_texs = Vec::<&'b glium::texture::DepthTexture2d>::new();
        let mut trans_depths = Vec::new();
        let mut mats: [MaybeUninit<[[f32; 4]; 4]>; 5] = unsafe { MaybeUninit::uninit().assume_init() };
        let mut fars: [MaybeUninit<f32>; 4] = unsafe { MaybeUninit::uninit().assume_init() };
        for (tex, i) in input.into_iter().zip(0..3) {
            match tex {
                TextureType::WithArg(b, StageArgs::CascadeArgs(mat, far)) => {
                    mats[i].write(*mat);
                    fars[i].write(*far);
                    match &**b {
                        TextureType::Depth2d(tex) => {                            
                            //depth_texs[i].write(glium::texture::TextureHandle::new(tex, &sb));
                            depth_texs.push(tex.to_ref());
                        },
                        TextureType::Multi(texes) if texes.len() == 3 => {
                            match (&*texes[0], &*texes[1], &*texes[2]) {
                                (TextureType::Depth2d(opaque_depth), TextureType::Depth2d(trans_depth), TextureType::Tex2d(trans_fac)) => {
                                    depth_texs.push(opaque_depth.to_ref());
                                    trans_depths.push((trans_depth.to_ref(), trans_fac.to_ref()));
                                },
                                _ => panic!("Unexpected input"),
                            }
                        }
                        _ => panic!("Unimplemented"),
                    }
                },
                _ => panic!("Unimplemented"),
            }
        }
        fars[3].write(1f32);
        mats[3].write(cgmath::Matrix4::<f32>::from_scale(1f32).into());
        mats[4].write(cgmath::Matrix4::<f32>::from_scale(1f32).into());
        cache.cascade_maps = Some(depth_texs);
        if !trans_depths.is_empty() { 
            cache.trans_cascade_maps = Some(trans_depths);
        }
        unsafe {
            let ctx = super::super::get_active_ctx();
            cache.cascade_ubo = glium::uniforms::UniformBuffer::persistent(&*ctx.ctx.borrow(), shader::CascadeUniform {
                //depth_maps: std::mem::transmute::<_, [glium::texture::TextureHandle<'b>; 5]>(depth_texs),
                far_planes: std::mem::transmute::<_, [f32; 4]>(fars),
                viewproj_mats: std::mem::transmute::<_, [[[f32; 4]; 4]; 5]>(mats),

            }).ok();
        }
    }
}

impl TextureProcessor for ToCacheProcessor {
    fn process<'b>(&mut self, input: Option<Vec<&'b TextureType>>, _: &shader::ShaderManager, 
        cache: &mut PipelineCache<'b>, _: Option<&shader::SceneData>) -> Option<TextureType>
    {
        if input.is_none() { return None }
        else {
            let input = input.unwrap();
            if input.len() == 1 {
                if let TextureType::WithArg(b, StageArgs::ObjectArgs(i)) = input[0] {
                    if let TextureType::TexCube(cbo) = &**b {
                        cache.obj_cubemaps.insert(*i, cbo.to_ref());
                        return None
                    }
                }
            } else {
                let mut is_cascade = true;
                for i in &input {
                    if let TextureType::WithArg(b, StageArgs::CascadeArgs(..)) = i {
                        if let TextureType::Depth2d(_) = &**b {}
                        else if let TextureType::Multi(_) = &** b {}
                        else { is_cascade = false; break; }
                    } else { is_cascade = false; break; }
                }
                if is_cascade {
                    ToCacheProcessor::cascade_maps_to_cache(input, cache);
                    return None
                }
            }
        }
        panic!("Unrecognized cache input")
    }
}