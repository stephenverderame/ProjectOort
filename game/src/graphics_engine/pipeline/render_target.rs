use glium::Surface;
use super::shader;
use super::super::drawable::*;
use glium::*;
use shader::RenderPassType;
use shader::PipelineCache;
use super::*;
use crate::cg_support::ssbo;
use super::super::camera;
use std::pin::*;

/// RenderTarget which renders to an MSAA color and depth buffer
/// 
/// ### Output
/// 2D RGBA F16 texture with multisampling already resolved
pub struct MsaaRenderTarget {
    fbo: framebuffer::SimpleFrameBuffer<'static>,
    _tex: Pin<Box<texture::Texture2dMultisample>>,
    _depth_tex: Pin<Box<texture::DepthTexture2dMultisample>>,
    out_fbo: framebuffer::SimpleFrameBuffer<'static>,
    out_tex: Pin<Box<texture::Texture2d>>,
    width: u32,
    height: u32,
}

impl MsaaRenderTarget {
    pub fn new<F : glium::backend::Facade>(samples: u32, width: u32, height: u32, facade: &F) -> MsaaRenderTarget {
        let _depth_tex = Box::pin(glium::texture::DepthTexture2dMultisample::empty(facade, width, height, samples).unwrap());
        let _tex = Box::pin(glium::texture::Texture2dMultisample::empty_with_format(facade, glium::texture::UncompressedFloatFormat::F16F16F16F16,
                    glium::texture::MipmapsOption::NoMipmap, width, height, samples).unwrap());
        let out_tex = Box::pin(glium::texture::Texture2d::empty_with_format(facade, glium::texture::UncompressedFloatFormat::F16F16F16F16,
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

impl RenderTarget for MsaaRenderTarget {
    fn draw(&mut self, viewer: &dyn Viewer, pipeline_inputs: Option<Vec<&TextureType>>, cache: &mut PipelineCache,
        func: &mut dyn FnMut(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer, RenderPassType, &PipelineCache, TargetType, &Option<Vec<&TextureType>>)) 
        -> Option<TextureType>
    {
        func(&mut self.fbo, viewer, RenderPassType::Visual, cache, TargetType::MsaaTarget, &pipeline_inputs);
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

    fn type_of(&self) -> TargetType {
        TargetType::MsaaTarget
    }
}

/// RenderTarget which renders to Depth buffer
/// 
/// ### Output
/// F32 2D DepthTexture
/// 
/// If a custom view getter is specified, then returns the depth texture with 
/// the used viewer's viewproj matrix
pub struct DepthRenderTarget {
    depth_tex: Pin<Box<texture::DepthTexture2d>>,
    main_fbo: framebuffer::SimpleFrameBuffer<'static>,
    trans_fbo: Option<(framebuffer::SimpleFrameBuffer<'static>,
        Pin<Box<texture::DepthTexture2d>>, Pin<Box<texture::Texture2d>>)>,
    render_cascades: bool,
}

impl DepthRenderTarget {

    /// `width` - width of depth texture
    /// 
    /// `height` - height of depth texture
    /// 
    /// `transparency` - `true` to also render transparent objects (to a separate texture)
    /// 
    /// `render_cascades` - `true` if this depth target is part of a cascade
    fn make(width: u32, height: u32, transparency: bool, render_cascades: bool) -> Self {
        let (main_fbo, depth_tex) = DepthRenderTarget::get_fbo_rbo(width, height);
        let trans_fbo = if transparency {
            Some(DepthRenderTarget::get_tex_fbo_rbo(width, height))
        } else { None };
        DepthRenderTarget {
            main_fbo, depth_tex,
            trans_fbo, render_cascades
        }
    }
    /// `width` - width of depth texture
    /// 
    /// `height` - height of depth texture
    /// 
    /// `transparency` - `true` to also render transparent objects (to a separate texture)
    #[inline]
    pub fn new(width: u32, height: u32, transparency: bool) -> DepthRenderTarget
    {
        Self::make(width, height, transparency, false)
    }

    #[inline]
    pub fn new_cascade(width: u32, height: u32, transparency: bool) -> DepthRenderTarget
    {
        Self::make(width, height, transparency, true)
    }

    fn get_fbo_rbo<'b>(width: u32, height: u32) -> (framebuffer::SimpleFrameBuffer<'b>, Pin<Box<texture::DepthTexture2d>>) {
        let ctx = super::super::get_active_ctx();
        let rbo = Box::pin(texture::DepthTexture2d::empty_with_format(&*ctx.ctx.borrow(), texture::DepthFormat::F32, 
            texture::MipmapsOption::NoMipmap, width, height).unwrap());
        let rbo_ptr = &*rbo as *const texture::DepthTexture2d;
        unsafe {
            let ctx = ctx.ctx.borrow();
            (glium::framebuffer::SimpleFrameBuffer::depth_only(&*ctx, &*rbo_ptr).unwrap(), rbo)
        }
    }

    fn get_tex_fbo_rbo<'b>(width: u32, height: u32) -> (framebuffer::SimpleFrameBuffer<'b>, 
        Pin<Box<texture::DepthTexture2d>>, Pin<Box<texture::Texture2d>>)
    {
        let ctx = super::super::get_active_ctx();
        let rbo = Box::pin(texture::DepthTexture2d::empty_with_format(&*ctx.ctx.borrow(), texture::DepthFormat::F32, 
            texture::MipmapsOption::NoMipmap, width, height).unwrap());
        let tex = Box::pin(texture::Texture2d::empty_with_format(&*ctx.ctx.borrow(), 
            texture::UncompressedFloatFormat::F16,
            texture::MipmapsOption::NoMipmap, width, height).unwrap());
        let tex_ptr = &*tex as *const texture::Texture2d;
        let rbo_ptr = &*rbo as *const texture::DepthTexture2d;
        unsafe {
            let ctx = ctx.ctx.borrow();
            (glium::framebuffer::SimpleFrameBuffer::with_depth_buffer(&*ctx, 
                &*tex_ptr, &*rbo_ptr).unwrap(), rbo, tex)
        }
    }
  
}

impl RenderTarget for DepthRenderTarget {
    fn draw(&mut self, viewer: &dyn Viewer, pipeline_inputs: Option<Vec<&TextureType>>, cache: &mut PipelineCache,
        func: &mut dyn FnMut(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer, RenderPassType, &PipelineCache, 
            TargetType, &Option<Vec<&TextureType>>)) 
        -> Option<TextureType> 
    {
        let vp : [[f32; 4]; 4] = (viewer.proj_mat() * viewer.view_mat()).into();
        func(&mut self.main_fbo, viewer, shader::RenderPassType::Depth, cache, TargetType::DepthTarget, &pipeline_inputs);
        let mut tex = TextureType::Depth2d(Ref(&*self.depth_tex));
        if let Some((ref mut fbo, depth, facs)) = self.trans_fbo.as_mut() {
            func(fbo, viewer, shader::RenderPassType::TransparentDepth, cache, TargetType::DepthTarget, &pipeline_inputs);
            tex = TextureType::Multi(vec![Box::new(tex), 
                Box::new(TextureType::Depth2d(Ref(&*depth))), 
                Box::new(TextureType::Tex2d(Ref(&*facs)))]);
        }
        if self.render_cascades {
            Some(TextureType::WithArg(Box::new(tex), StageArgs::CascadeArgs(vp, viewer.view_dist().1)))
        } else {
            Some(tex)
        }
       
    }

    fn type_of(&self) -> TargetType {
        TargetType::DepthTarget
    }
}

/// Helper struct for render targets rendering to a cubemap with perspective
struct CubemapRenderBase {
    view_dist: f32,
    get_view_pos: Box<dyn Fn() -> cgmath::Point3<f32>>,
    view_matrices: ssbo::SSBO<[[f32; 4]; 4]>,
}

impl CubemapRenderBase {
    fn new(view_dist: f32, get_view_pos: Box<dyn Fn() -> cgmath::Point3<f32>>) -> CubemapRenderBase
    {
        CubemapRenderBase {
            view_dist, get_view_pos,
            view_matrices: ssbo::SSBO::static_alloc_dyn(6, None),
        }
    }

    /// Gets an array of tuples of view target direction, CubeFace, and up vector
    fn get_target_up() 
        -> [(cgmath::Point3<f32>, cgmath::Vector3<f32>); 6]
    {
        use cgmath::*;
        [(point3(1., 0., 0.), vec3(0., -1., 0.)), (point3(-1., 0., 0.), vec3(0., -1., 0.)),
            (point3(0., 1., 0.), vec3(0., 0., 1.)), (point3(0., -1., 0.), vec3(0., 0., -1.)),
            (point3(0., 0., 1.), vec3(0., -1., 0.)), (point3(0., 0., -1.), vec3(0., -1., 0.))]
    }

    /// Repeatedly calls `func` for each face of the cubemap
    /// 
    /// `func` - callable to render a single face of a cubemap. Passed a cube face and camera
    fn bind_views(&self) -> camera::StaticCamera {
        use super::super::camera::*;
        use cgmath::*;
        let mut cam = PerspectiveCamera {
            cam: (self.get_view_pos)(),
            aspect: 1f32,
            fov_deg: 90f32,
            target: cgmath::point3(0., 0., 0.),
            near: 0.1,
            far: self.view_dist,
            up: cgmath::vec3(0., 1., 0.),
        };
        let target_faces = Self::get_target_up();
        for ((target, up), mat_dst) in target_faces.iter()
            .zip(self.view_matrices.map_write().as_slice().iter_mut()) 
        {
            let target : (f32, f32, f32) = (target.to_vec() + cam.cam.to_vec()).into();
            cam.target = std::convert::From::from(target);
            cam.up = *up;
            *mat_dst = (cam.proj_mat() * cam.view_mat()).into();
        }
        self.view_matrices.bind(5);
        //println!("{:?}", cam.proj_mat() * cam.view_mat());
        StaticCamera::from(&cam)
    }
}

/// RenderTarget which renders to a cubemap with perspective. Can assume that `draw()` ignores its viewer argument
/// and that its called once per face
/// 
/// ### Output
/// F16 RGB cubemap
pub struct CubemapRenderTarget {
    cubemap: CubemapRenderBase,
    cbo_tex: Pin<Box<texture::Cubemap>>,
    _depth_buffer: Pin<Box<texture::DepthCubemap>>,
    _size: u32,
    pass_type: RenderPassType,
    get_trans_id: Option<Box<dyn Fn() -> u32>>,
    fbo: framebuffer::SimpleFrameBuffer<'static>,
}

impl CubemapRenderTarget {
    /// Creates a new CubemapRenderTarget. The cubemap is a F16 RGB texture with no mipmapping
    /// `view_dist` - the view distance for the viewer when rendering to a cubemap
    /// 
    /// `size` - the square side length of each texture face in the cubemap
    /// 
    /// `view_pos` - the position in the scene the cubemap is rendered from
    pub fn new<F : glium::backend::Facade>(size: u32, view_dist: f32, 
        get_view_pos: Box<dyn Fn() -> cgmath::Point3<f32>>, facade: &F) 
        -> CubemapRenderTarget 
    {
        let _depth_buffer = Box::pin(texture::DepthCubemap::empty_with_format(facade,
            texture::DepthFormat::I24, texture::MipmapsOption::NoMipmap,
            size).unwrap());
        let cbo_tex = Box::pin(texture::Cubemap::empty_with_format(facade, 
            texture::UncompressedFloatFormat::F16F16F16,
            texture::MipmapsOption::NoMipmap, size).unwrap());
        let color_ptr = &*cbo_tex as *const texture::Cubemap;
        let depth_ptr = &*_depth_buffer as *const texture::DepthCubemap;
        let fbo = unsafe {
            framebuffer::SimpleFrameBuffer::with_depth_buffer(facade, 
                (*color_ptr).main_level(), (*depth_ptr).main_level())
            .unwrap()
        };
        CubemapRenderTarget {
            _size: size, 
            cubemap: CubemapRenderBase::new(view_dist, get_view_pos),
            cbo_tex, _depth_buffer,
            pass_type: RenderPassType::LayeredVisual,
            get_trans_id: None,
            fbo
        }
    }

    /// Sets the render pass type of this Render Target
    pub fn with_pass(mut self, pass: RenderPassType) -> Self {
        self.pass_type = pass;
        self
    }

    /// If this target is producing a texture for a specific object, specify a function
    /// to get the object's graphical object id
    /// 
    /// This rendered texture will be returned as `WithArgs` along with this id
    pub fn with_trans_getter(mut self, get_id: Box<dyn Fn() -> u32>) -> Self {
        self.get_trans_id = Some(get_id);
        self
    }
}

impl RenderTarget for CubemapRenderTarget {
    fn draw(&mut self, _: &dyn Viewer, pipeline_inputs: Option<Vec<&TextureType>>, cache: &mut PipelineCache,
        func: &mut dyn FnMut(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer, 
            RenderPassType, &PipelineCache, TargetType, &Option<Vec<&TextureType>>)) 
        -> Option<TextureType>
    {
        let cam_base = self.cubemap.bind_views();
        self.fbo.clear_color_and_depth((0., 0., 0., 1.), 1.);
        func(&mut self.fbo, &cam_base, self.pass_type, cache, TargetType::CubemapTarget, &pipeline_inputs);
        let tex = TextureType::TexCube(Ref(&self.cbo_tex));
        if let Some(get_id) = &self.get_trans_id {
            Some(TextureType::WithArg(
                Box::new(tex), StageArgs::ObjectArgs(get_id())))
        } else { Some(tex) }
    }

    fn type_of(&self) -> TargetType {
        TargetType::CubemapTarget
    }

}

/// RenderTarget which renders to a cubemap with perspective. Can assume that `draw()` ignores its viewer argument
/// and that it is called once per face, per mipmap level, starting at level 0.
/// 
/// ### Output
/// RGB F16 Cubemap texture with mipmapping
pub struct MipCubemapRenderTarget {
    cubemap: CubemapRenderBase,
    mip_levels: u32,
    size: u32,
}

impl MipCubemapRenderTarget {
    /// Creates a new CubemapRenderTarget. The cubemap is a F16 RGB texture with no mipmapping
    /// `view_dist` - the view distance for the viewer when rendering to a cubemap
    /// 
    /// `size` - the square side length of each texture face in the cubemap at the highest detail mipmap (level 0)
    /// Each successive mipmap level has half the previous size
    /// 
    /// `view_pos` - the position in the scene the cubemap is rendered from
    /// 
    /// `mip_levels` - the amount of mipmaps
    pub fn new(size: u32, mip_levels: u32, view_dist: f32, 
        get_view_pos: Box<dyn Fn() -> cgmath::Point3<f32>>) -> MipCubemapRenderTarget{
        MipCubemapRenderTarget {
            mip_levels, size,
            cubemap: CubemapRenderBase::new(view_dist, get_view_pos),
        }
    }
}

impl RenderTarget for MipCubemapRenderTarget {
    fn draw(&mut self, _: &dyn Viewer, pipeline_inputs: Option<Vec<&TextureType>>, cache: &mut PipelineCache,
        func: &mut dyn FnMut(&mut framebuffer::SimpleFrameBuffer, 
            &dyn Viewer, RenderPassType, &PipelineCache, TargetType, &Option<Vec<&TextureType>>)) 
        -> Option<TextureType>
    {
        let ctx = super::super::get_active_ctx();
        let cbo_tex = texture::Cubemap::empty_with_format(&*ctx.ctx.borrow(), texture::UncompressedFloatFormat::F16F16F16,
            texture::MipmapsOption::AutoGeneratedMipmapsMax(self.mip_levels - 1), self.size).unwrap();
        let depth_tex = texture::DepthCubemap::empty_with_format(&*ctx.ctx.borrow(),
            texture::DepthFormat::I24, 
            texture::MipmapsOption::AutoGeneratedMipmapsMax(self.mip_levels - 1), self.size).unwrap();
        let cam_base = self.cubemap.bind_views();
        for mip_level in 0 .. self.mip_levels {
            //let mip_pow = 0.5f32.powi(mip_level as i32);
            //let mipped_size = ((self.size as f32) * mip_pow) as u32;
            let mut fbo = framebuffer::SimpleFrameBuffer::with_depth_buffer(&*ctx.ctx.borrow(), 
                cbo_tex.mipmap(mip_level).unwrap(), depth_tex.mipmap(mip_level).unwrap()).unwrap();
            func(&mut fbo, &cam_base, RenderPassType::LayeredVisual, cache, TargetType::MipcubeTarget, &pipeline_inputs);
        }
        Some(TextureType::TexCube(Own(cbo_tex)))
    }

    fn type_of(&self) -> TargetType {
        TargetType::MipcubeTarget
    }

}

/// A RenderTarget decorator wich supplies the target with an arbitrary view
/// on draw
pub struct CustomViewRenderTargetDecorator<
    V : Viewer,
    F : Fn(&dyn Viewer) -> V, 
    Rt : RenderTarget> 
{
    target: Rt,
    get_viewer: F
}

impl<
    V : Viewer,
    F : Fn(&dyn Viewer) -> V,
    Rt : RenderTarget> CustomViewRenderTargetDecorator<V, F, Rt>
{
    /// `target` - the render target to supply the view to
    /// 
    /// `get_viewer` - a function which computes the view to use
    pub fn new(target: Rt, get_viewer: F) -> Self {
        Self {
            target, get_viewer
        }
    }
}

impl<
    V : Viewer,
    F : Fn(&dyn Viewer) -> V,
    Rt : RenderTarget> 
    RenderTarget for CustomViewRenderTargetDecorator<V, F, Rt>
{
    fn draw(&mut self, v: &dyn Viewer, pipeline_inputs: Option<Vec<&TextureType>>, cache: &mut PipelineCache,
        func: &mut dyn FnMut(&mut framebuffer::SimpleFrameBuffer, 
            &dyn Viewer, RenderPassType, &PipelineCache, TargetType, &Option<Vec<&TextureType>>)) 
        -> Option<TextureType>
    {
       self.target.draw(&(self.get_viewer)(v), pipeline_inputs, cache, func)
    }

    fn type_of(&self) -> TargetType {
        self.target.type_of()
    }
}