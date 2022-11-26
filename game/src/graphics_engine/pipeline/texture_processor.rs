use super::*;
use crate::cg_support::ssbo;
use glium::framebuffer::ToColorAttachment;
use std::pin::Pin;
use Vertex2D as Vertex;

/// Gets the vertex and index buffer for a rectangle
fn get_rect_vbo_ebo<F: glium::backend::Facade>(
    facade: &F,
) -> (VertexBuffer<Vertex>, IndexBuffer<u16>) {
    let verts: [Vertex; 4] = [
        Vertex {
            pos: [1.0, -1.0],
            tex_coords: [1., 0.],
        },
        Vertex {
            pos: [1.0, 1.0],
            tex_coords: [1., 1.],
        },
        Vertex {
            pos: [-1.0, 1.0],
            tex_coords: [0., 1.],
        },
        Vertex {
            pos: [-1.0, -1.0],
            tex_coords: [0., 0.],
        },
    ];
    let indices: [u16; 6] = [2, 3, 0, 2, 0, 1];

    (
        VertexBuffer::new(facade, &verts).unwrap(),
        IndexBuffer::new(facade, glium::index::PrimitiveType::TrianglesList, &indices).unwrap(),
    )
}

/// Texture processor which extracts bright parts of a texture for Bloom
///
/// ### Inputs
/// 2D texture
/// ### Outputs
/// 2D RGBA F16 texture
pub struct ExtractBrightProcessor {
    bright_color_tex: Pin<Box<glium::texture::Texture2d>>,
    bright_color_fbo: framebuffer::SimpleFrameBuffer<'static>,
    vbo: VertexBuffer<Vertex>,
    ebo: IndexBuffer<u16>,
}

impl ExtractBrightProcessor {
    pub fn new<F: backend::Facade>(facade: &F, width: u32, height: u32) -> ExtractBrightProcessor {
        let bright_color_tex = Box::pin(
            glium::texture::Texture2d::empty_with_format(
                facade,
                glium::texture::UncompressedFloatFormat::F16F16F16F16,
                glium::texture::MipmapsOption::NoMipmap,
                width,
                height,
            )
            .unwrap(),
        );
        let (vbo, ebo) = get_rect_vbo_ebo(facade);
        unsafe {
            let tex_ptr = &*bright_color_tex as *const texture::Texture2d;
            ExtractBrightProcessor {
                bright_color_tex,
                bright_color_fbo: glium::framebuffer::SimpleFrameBuffer::new(facade, &*tex_ptr)
                    .unwrap(),
                ebo,
                vbo,
            }
        }
    }
}

impl TextureProcessor for ExtractBrightProcessor {
    fn process(
        &mut self,
        source: Option<Vec<&TextureType>>,
        shader: &shader::ShaderManager,
        pc: &mut PipelineCache,
        sd: Option<&shader::SceneData>,
    ) -> Option<TextureType> {
        if let TextureType::Tex2d(source) = source.unwrap()[0] {
            let source = source.to_ref();
            let data =
                shader::UniformInfo::ExtractBright(shader::ExtractBrightData { tex: source });
            let (program, params, uniform) = shader.use_shader(&data, sd, Some(pc));
            match uniform {
                shader::UniformType::ExtractBright(uniform) => {
                    let fbo = &mut self.bright_color_fbo;
                    fbo.clear_color(0., 0., 0., 1.);
                    fbo.draw(&self.vbo, &self.ebo, program, &uniform, &params)
                        .unwrap()
                }
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
    ping_pong_tex: [Pin<Box<texture::Texture2d>>; 2],
    ping_pong_fbo: [framebuffer::SimpleFrameBuffer<'static>; 2],
    iterations: usize,
    ebo: IndexBuffer<u16>,
    vbo: VertexBuffer<Vertex>,
}

impl SepConvProcessor {
    /// Requires `iterations >= 2` because a single convolution is broken up into two passes. So an odd number
    /// for `iterations` performs a multiple of `1/2` convolutions
    pub fn new<F: backend::Facade>(
        width: u32,
        height: u32,
        iterations: usize,
        facade: &F,
    ) -> SepConvProcessor {
        use std::mem::MaybeUninit;
        const UNINIT_TEX: MaybeUninit<Pin<Box<texture::Texture2d>>> = MaybeUninit::uninit();
        const UNINIT_FBO: MaybeUninit<framebuffer::SimpleFrameBuffer<'static>> =
            MaybeUninit::uninit();
        let mut ping_pong_tex = [UNINIT_TEX; 2];
        let mut ping_pong_fbo = [UNINIT_FBO; 2];
        let (vbo, ebo) = get_rect_vbo_ebo(facade);
        for i in 0..2 {
            let tex_box = Box::pin(
                glium::texture::Texture2d::empty_with_format(
                    facade,
                    glium::texture::UncompressedFloatFormat::F16F16F16F16,
                    glium::texture::MipmapsOption::NoMipmap,
                    width,
                    height,
                )
                .unwrap(),
            );
            let tex_ptr = &*tex_box as *const texture::Texture2d;
            ping_pong_tex[i].write(tex_box);
            unsafe {
                ping_pong_fbo[i]
                    .write(glium::framebuffer::SimpleFrameBuffer::new(facade, &*tex_ptr).unwrap());
            }
        }
        unsafe {
            SepConvProcessor {
                iterations,
                ping_pong_fbo: std::mem::transmute::<_, [framebuffer::SimpleFrameBuffer<'static>; 2]>(
                    ping_pong_fbo,
                ),
                ping_pong_tex: std::mem::transmute::<_, [Pin<Box<texture::Texture2d>>; 2]>(
                    ping_pong_tex,
                ),
                vbo,
                ebo,
            }
        }
    }

    fn pass(
        dst: &mut framebuffer::SimpleFrameBuffer,
        source: &texture::Texture2d,
        vbo: &VertexBuffer<Vertex>,
        ebo: &IndexBuffer<u16>,
        iteration: usize,
        shaders: &shader::ShaderManager,
    ) {
        let data = shader::UniformInfo::SepConv(shader::SepConvData {
            horizontal_pass: iteration % 2 == 0,
            tex: source,
        });
        let (program, params, uniform) = shaders.use_shader(&data, None, None);
        match uniform {
            shader::UniformType::SepConv(uniform) => {
                dst.draw(vbo, ebo, program, &uniform, &params).unwrap();
            }
            _ => panic!("Invalid uniform type returned for RenderTarget"),
        }
    }
}

impl TextureProcessor for SepConvProcessor {
    fn process(
        &mut self,
        source: Option<Vec<&TextureType>>,
        shader: &shader::ShaderManager,
        _: &mut PipelineCache,
        _: Option<&shader::SceneData>,
    ) -> Option<TextureType> {
        if let TextureType::Tex2d(source) = source.unwrap()[0] {
            let source = source.to_ref();
            SepConvProcessor::pass(
                &mut self.ping_pong_fbo[0],
                source,
                &self.vbo,
                &self.ebo,
                0,
                shader,
            );
            for i in 1..self.iterations {
                let tex = &*self.ping_pong_tex[(i - 1) % 2];
                let dst = &mut self.ping_pong_fbo[i % 2];
                SepConvProcessor::pass(dst, tex, &self.vbo, &self.ebo, i, shader);
            }
            Some(TextureType::Tex2d(Ref(
                &*self.ping_pong_tex[(self.iterations - 1) % 2]
            )))
        } else {
            panic!("Invalid source type for separable convolution");
        }
    }
}

/// A processor which additively blends together textures and renders them to a surface
///
/// ### Inputs
/// 2D Main texture
/// 2D additive texture(s)
/// ### Outputs
/// Additively blends all input 2D textures
pub struct CompositorProcessor {
    vbo: VertexBuffer<Vertex>,
    ebo: IndexBuffer<u16>,
    tex: Pin<Box<texture::Texture2d>>,
    fbo: framebuffer::SimpleFrameBuffer<'static>,
    mode: shader::BlendFn,
}

impl CompositorProcessor {
    /// `width` - width of input and output textures
    ///
    /// `height` - height of input and output textures
    pub fn new<Fac: backend::Facade>(
        width: u32,
        height: u32,
        mode: shader::BlendFn,
        facade: &Fac,
    ) -> CompositorProcessor {
        let (vbo, ebo) = get_rect_vbo_ebo(facade);
        let tex = Box::pin(
            glium::texture::Texture2d::empty_with_format(
                facade,
                glium::texture::UncompressedFloatFormat::F16F16F16F16,
                glium::texture::MipmapsOption::NoMipmap,
                width,
                height,
            )
            .unwrap(),
        );

        unsafe {
            let tex_ptr = &*tex as *const texture::Texture2d;
            CompositorProcessor {
                vbo,
                ebo,
                fbo: glium::framebuffer::SimpleFrameBuffer::new(facade, &*tex_ptr).unwrap(),
                tex,
                mode,
            }
        }
    }

    fn render<'a>(
        &mut self,
        textures: Vec<&'a texture::Texture2d>,
        transforms: Vec<[[f32; 3]; 3]>,
        cache: &PipelineCache,
        shader: &shader::ShaderManager,
    ) -> Option<TextureType> {
        let args = shader::UniformInfo::Composite(shader::CompositeData {
            textures,
            transforms,
            model: cgmath::Matrix4::from_scale(1f32).into(),
            blend_function: (self.mode, glium::program::ShaderStage::Fragment),
        });
        let (program, params, uniform) = shader.use_shader(&args, None, Some(cache));
        match uniform {
            shader::UniformType::Composite(uniform) => {
                self.fbo.clear_color_and_depth((0., 0., 0., 1.0), 1.0);
                self.fbo
                    .draw(&self.vbo, &self.ebo, program, &uniform, &params)
                    .expect("Error drawing to fbo in compositor processor");
            }
            _ => panic!("Invalid uniform type returned for RenderTarget"),
        };
        Some(TextureType::Tex2d(Ref(&self.tex)))
    }
}

impl TextureProcessor for CompositorProcessor {
    fn process(
        &mut self,
        source: Option<Vec<&TextureType>>,
        shader: &shader::ShaderManager,
        cache: &mut PipelineCache,
        _: Option<&shader::SceneData>,
    ) -> Option<TextureType> {
        let source = source.unwrap();
        let (textures, transforms): (Vec<_>, Vec<_>) = source
            .iter()
            .filter_map(|tt| match tt {
                TextureType::Tex2d(tex) => Some((
                    tex.to_ref(),
                    [[1f32, 0., 0.], [0f32, 1., 0.], [0f32, 0., 1.]],
                )),
                TextureType::WithArg(tex, StageArgs::Compositor(transform)) => {
                    if let TextureType::Tex2d(tex) = tex.as_ref() {
                        Some((tex.to_ref(), *transform))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .unzip();
        if textures.is_empty() {
            panic!("Not enough 2d textures input to compositor")
        } else {
            self.render(textures, transforms, cache, shader)
        }
    }
}

/// A processor which blits a texture onto another surface
///
/// ### Inputs
/// A single 2d texture
/// ### Outputs
/// None (texture blitted to surface returned by getter function)
///
/// ### Template Params
/// `S` - Surface type
///
/// `SHolder` - something that dereferences to `S`
///
/// `GetSHolder` - function that returns surface and rectangle on it to blit
///
/// `CleanSHolder` - function that cleans the surface
pub struct BlitTextureProcessor<
    S: Surface,
    SHolder: std::ops::DerefMut<Target = S>,
    GetSHolder: Fn() -> (SHolder, BlitTarget),
    CleanSHolder: Fn(SHolder),
> {
    get_surface: GetSHolder,
    clean_surface: CleanSHolder,
}

impl<
        S: Surface,
        SHolder: std::ops::DerefMut<Target = S>,
        GetSHolder: Fn() -> (SHolder, BlitTarget),
        CleanSHolder: Fn(SHolder),
    > BlitTextureProcessor<S, SHolder, GetSHolder, CleanSHolder>
{
    /// `get_surface` - callable that returns the surface to render to and a
    /// destination rectangle on that surface that we will blit to.
    /// The surface is **not** cleared
    ///
    /// `clean_surface` - callable that accepts the returned surface and performs
    /// any necessary cleanup after drawing is finished
    ///
    pub fn new(
        get_surface: GetSHolder,
        clean_surface: CleanSHolder,
    ) -> BlitTextureProcessor<S, SHolder, GetSHolder, CleanSHolder> {
        BlitTextureProcessor {
            get_surface,
            clean_surface,
        }
    }

    fn render<'a>(
        &self,
        texture: &'a texture::Texture2d,
        _: &PipelineCache,
        _: &shader::ShaderManager,
    ) {
        let (mut surface_holder, dst_rect) = (self.get_surface)();
        {
            let surface = &mut *surface_holder;
            surface.clear_color_and_depth((0., 0., 0., 1.), 1.);
            texture.as_surface().blit_whole_color_to(
                surface,
                &dst_rect,
                uniforms::MagnifySamplerFilter::Linear,
            );
        }
        (self.clean_surface)(surface_holder);
    }
}

impl<
        S: Surface,
        SHolder: std::ops::DerefMut<Target = S>,
        GetSHolder: Fn() -> (SHolder, BlitTarget),
        CleanSHolder: Fn(SHolder),
    > TextureProcessor for BlitTextureProcessor<S, SHolder, GetSHolder, CleanSHolder>
{
    fn process(
        &mut self,
        source: Option<Vec<&TextureType>>,
        shader: &shader::ShaderManager,
        cache: &mut PipelineCache,
        _: Option<&shader::SceneData>,
    ) -> Option<TextureType> {
        let source = source.unwrap();
        let v: Vec<_> = source
            .iter()
            .filter_map(|tt| match tt {
                TextureType::Tex2d(tex) => Some(tex.to_ref()),
                _ => None,
            })
            .collect();
        if v.len() != 1 {
            panic!("Invalid number of 2d textures to input to compositor")
        } else {
            self.render(v[0], cache, shader);
            None
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
    pub fn new(
        width: u32,
        height: u32,
        fmt: Option<texture::UncompressedFloatFormat>,
        mipmap: Option<texture::MipmapsOption>,
    ) -> CopyTextureProcessor {
        CopyTextureProcessor {
            width,
            height,
            tex_format: fmt.unwrap_or(texture::UncompressedFloatFormat::F16F16F16F16),
            mipmap: mipmap.unwrap_or(texture::MipmapsOption::NoMipmap),
        }
    }

    fn blit_src_to_dst<
        'b,
        S: ToColorAttachment<'b>,
        D: ToColorAttachment<'b>,
        F: glium::backend::Facade,
    >(
        &self,
        source: S,
        dst: D,
        facade: &F,
    ) {
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
    fn process(
        &mut self,
        source: Option<Vec<&TextureType>>,
        _: &shader::ShaderManager,
        _: &mut PipelineCache,
        _: Option<&shader::SceneData>,
    ) -> Option<TextureType> {
        source.as_ref()?;
        let ctx = super::super::get_active_ctx();
        match source.unwrap()[0] {
            TextureType::Tex2d(Ref(x)) => {
                let out = texture::Texture2d::empty_with_format(
                    &*ctx.ctx.borrow(),
                    self.tex_format,
                    self.mipmap,
                    self.width,
                    self.height,
                )
                .unwrap();
                self.blit_src_to_dst(*x, &out, &*ctx.ctx.borrow());
                Some(TextureType::Tex2d(Own(out)))
            }
            TextureType::TexCube(Ref(x)) => {
                use texture::CubeLayer::*;
                let out = texture::Cubemap::empty_with_format(
                    &*ctx.ctx.borrow(),
                    self.tex_format,
                    self.mipmap,
                    self.width,
                )
                .unwrap();
                let layers = [
                    PositiveX, NegativeX, PositiveY, NegativeY, PositiveZ, NegativeZ,
                ];
                for layer in layers {
                    self.blit_src_to_dst(
                        x.main_level().image(layer),
                        out.main_level().image(layer),
                        &*ctx.ctx.borrow(),
                    );
                }
                Some(TextureType::TexCube(Own(out)))
            }
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
    width: u32,
    height: u32,
}

impl GenLutProcessor {
    pub fn new<F: glium::backend::Facade>(width: u32, height: u32, facade: &F) -> GenLutProcessor {
        let (vbo, ebo) = get_rect_vbo_ebo(facade);
        GenLutProcessor {
            ebo,
            vbo,
            width,
            height,
        }
    }
}

impl TextureProcessor for GenLutProcessor {
    fn process(
        &mut self,
        _: Option<Vec<&TextureType>>,
        shader: &shader::ShaderManager,
        pc: &mut PipelineCache,
        sd: Option<&shader::SceneData>,
    ) -> Option<TextureType> {
        let ctx = super::super::get_active_ctx();
        let tex = texture::Texture2d::empty_with_format(
            &*ctx.ctx.borrow(),
            texture::UncompressedFloatFormat::F16F16,
            texture::MipmapsOption::NoMipmap,
            self.width,
            self.height,
        )
        .unwrap();
        let rbo = framebuffer::DepthRenderBuffer::new(
            &*ctx.ctx.borrow(),
            texture::DepthFormat::I24,
            self.width,
            self.height,
        )
        .unwrap();
        let mut fbo =
            framebuffer::SimpleFrameBuffer::with_depth_buffer(&*ctx.ctx.borrow(), &tex, &rbo)
                .unwrap();
        fbo.clear_color_and_depth((0., 0., 0., 0.), 1.);
        let (program, params, uniform) =
            shader.use_shader(&shader::UniformInfo::GenLut, sd, Some(pc));
        match uniform {
            shader::UniformType::BrdfLut(uniform) => fbo
                .draw(&self.vbo, &self.ebo, program, &uniform, &params)
                .unwrap(),
            _ => panic!("Gen lut got unexepected uniform type"),
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
    visible_light_buffer: ssbo::Ssbo<i32>,
    width: u32,
    height: u32,
}

impl CullLightProcessor {
    pub fn new(width: u32, height: u32, tile_size: u32) -> CullLightProcessor {
        let max_lights = 1024;
        let work_groups_x = (width + width % tile_size) / tile_size;
        let work_groups_y = (height + height % tile_size) / tile_size;
        CullLightProcessor {
            work_groups_x,
            work_groups_y,
            visible_light_buffer: ssbo::Ssbo::<i32>::static_empty(
                work_groups_x * work_groups_y * max_lights,
            ),
            width,
            height,
        }
    }

    #[allow(dead_code)]
    pub fn get_groups_x(&self) -> u32 {
        self.work_groups_x
    }
}

impl TextureProcessor for CullLightProcessor {
    fn process(
        &mut self,
        input: Option<Vec<&TextureType>>,
        shader: &shader::ShaderManager,
        cache: &mut PipelineCache,
        data: Option<&shader::SceneData>,
    ) -> Option<TextureType> {
        if let TextureType::Depth2d(depth) = input.unwrap()[0] {
            let depth_tex = depth.to_ref();
            let params = shader::UniformInfo::LightCull(shader::LightCullData {
                depth_tex,
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
        ToCacheProcessor {}
    }

    fn cascade_maps_to_cache<'b>(input: Vec<&'b TextureType>, cache: &mut PipelineCache<'b>) {
        use std::mem::MaybeUninit;
        let mut depth_texs = Vec::<&'b glium::texture::DepthTexture2d>::new();
        let mut trans_depths = Vec::new();
        let mut mats: [MaybeUninit<[[f32; 4]; 4]>; 5] =
            unsafe { MaybeUninit::uninit().assume_init() };
        let mut fars: [MaybeUninit<f32>; 4] = unsafe { MaybeUninit::uninit().assume_init() };
        for (tex, i) in input.into_iter().zip(0..3) {
            match tex {
                TextureType::WithArg(b, StageArgs::Cascade(mat, far)) => {
                    mats[i].write(*mat);
                    fars[i].write(*far);
                    match &**b {
                        TextureType::Depth2d(tex) => {
                            //depth_texs[i].write(glium::texture::TextureHandle::new(tex, &sb));
                            depth_texs.push(tex.to_ref());
                        }
                        TextureType::Multi(texes) if texes.len() == 3 => {
                            match (&texes[0], &texes[1], &texes[2]) {
                                (
                                    TextureType::Depth2d(opaque_depth),
                                    TextureType::Depth2d(trans_depth),
                                    TextureType::Tex2d(trans_fac),
                                ) => {
                                    depth_texs.push(opaque_depth.to_ref());
                                    trans_depths.push((trans_depth.to_ref(), trans_fac.to_ref()));
                                }
                                _ => panic!("Unexpected input"),
                            }
                        }
                        _ => panic!("Unimplemented"),
                    }
                }
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
            cache.cascade_ubo = glium::uniforms::UniformBuffer::persistent(
                &*ctx.ctx.borrow(),
                shader::CascadeUniform {
                    //depth_maps: std::mem::transmute::<_, [glium::texture::TextureHandle<'b>; 5]>(depth_texs),
                    far_planes: std::mem::transmute::<_, [f32; 4]>(fars),
                    viewproj_mats: std::mem::transmute::<_, [[[f32; 4]; 4]; 5]>(mats),
                },
            )
            .ok();
        }
    }
}

impl TextureProcessor for ToCacheProcessor {
    fn process<'b>(
        &mut self,
        input: Option<Vec<&'b TextureType>>,
        _: &shader::ShaderManager,
        cache: &mut PipelineCache<'b>,
        _: Option<&shader::SceneData>,
    ) -> Option<TextureType> {
        if let Some(input) = input {
            if input.len() == 1 {
                match input[0] {
                    TextureType::WithArg(b, StageArgs::Object(i)) => {
                        if let TextureType::TexCube(cbo) = &**b {
                            cache.obj_cubemaps.insert(*i, cbo.to_ref());
                        }
                    }
                    TextureType::Depth2d(tex) => cache.cam_depth = Some(tex.to_ref()),
                    _ => (),
                }
                None
            } else {
                let mut is_cascade = true;
                for i in &input {
                    if let TextureType::WithArg(b, StageArgs::Cascade(..)) = i {
                        if let TextureType::Depth2d(_) = &**b {
                        } else if let TextureType::Multi(_) = &**b {
                        } else {
                            is_cascade = false;
                            break;
                        }
                    } else {
                        is_cascade = false;
                        break;
                    }
                }
                if is_cascade {
                    ToCacheProcessor::cascade_maps_to_cache(input, cache);
                    None
                } else {
                    panic!("Unrecognized cache input")
                }
            }
        } else {
            None
        }
    }
}
