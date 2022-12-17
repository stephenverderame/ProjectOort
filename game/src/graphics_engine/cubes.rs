use super::drawable::*;
use super::shader;
use VertexPos as Vertex;

const CUBE_VERTS: [Vertex; 8] = [
    Vertex {
        pos: [-1.0, -1.0, 1.0],
    },
    Vertex {
        pos: [1.0, -1.0, 1.0],
    },
    Vertex {
        pos: [1.0, 1.0, 1.0],
    },
    Vertex {
        pos: [-1.0, 1.0, 1.0],
    },
    Vertex {
        pos: [-1.0, -1.0, -1.0],
    },
    Vertex {
        pos: [1.0, -1.0, -1.0],
    },
    Vertex {
        pos: [1.0, 1.0, -1.0],
    },
    Vertex {
        pos: [-1.0, 1.0, -1.0],
    },
];

const CUBE_INDICES: [u16; 36] = [
    0, 1, 2, 2, 3, 0, 1, 5, 6, 6, 2, 1, 7, 6, 5, 5, 4, 7, 4, 0, 3, 3, 7, 4, 4,
    5, 1, 1, 0, 4, 3, 2, 6, 6, 7, 3,
];

/// The type of texture for the skybox. Either a cubemap or a 2d texture
/// storing an equirectangular spherical image
pub enum SkyboxTex {
    Cube(glium::texture::Cubemap),
    Sphere(glium::texture::Texture2d), //equirectangular spherical texture
}

/// A cube textured by a cubemap or equirectangular texture that is
/// always centered around the camera
pub struct Skybox {
    vbo: glium::VertexBuffer<Vertex>,
    ebo: glium::IndexBuffer<u16>,
    tex: SkyboxTex,
    mip_progress: Option<f32>,
}

impl Skybox {
    pub fn new<F>(tex: SkyboxTex, facade: &F) -> Self
    where
        F: glium::backend::Facade,
    {
        Self {
            vbo: glium::VertexBuffer::new(facade, &CUBE_VERTS).unwrap(),
            ebo: glium::IndexBuffer::new(
                facade,
                glium::index::PrimitiveType::TrianglesList,
                &CUBE_INDICES,
            )
            .unwrap(),
            tex,
            mip_progress: None,
        }
    }

    /// Creates a skybox from a equirectangular texture
    #[allow(dead_code)]
    pub fn from_sphere<F: glium::backend::Facade>(
        path: &str,
        facade: &F,
    ) -> Self {
        use super::textures;
        let t = SkyboxTex::Sphere(textures::load_texture_2d(path, facade));
        Self::new(t, facade)
    }

    /// Creates a skybox from a cubemap which is generated from
    /// an equirectangular texture specified by `path`
    pub fn cvt_from_sphere<F: glium::backend::Facade>(
        path: &str,
        cubemap_size: u32,
        shader_manager: &shader::ShaderManager,
        facade: &F,
    ) -> Self {
        let t = SkyboxTex::Cube(gen_cubemap_from_sphere(
            path,
            cubemap_size,
            shader_manager,
            facade,
        ));
        Self::new(t, facade)
    }

    /// Sets the progress value of the mipmap progress
    /// If this function is used, skybox will render to a filtered skybox shader
    /// which takes this parameter as an argument to control different outputs based on the
    /// mip level of the render target. This does nothing if using a spherical equirectangular texture
    ///
    /// If `progress` is none, disables mipping. Otherwise `0 <= progress <= 1`
    pub fn set_mip_progress(&mut self, progress: Option<f32>) {
        self.mip_progress = progress;
    }

    pub fn into_entity(self) -> super::entity::Entity {
        use super::entity;
        use std::cell::RefCell;
        use std::rc::Rc;
        entity::Entity {
            geometry: Box::new(self),
            render_passes: vec![
                shader::RenderPassType::Visual,
                shader::RenderPassType::transparent_tag(),
                shader::RenderPassType::LayeredVisual,
            ],
            locations: vec![Rc::new(RefCell::new(
                cgmath::Matrix4::from_scale(1f64),
            ))],
            order: entity::RenderOrder::First,
        }
    }
}

impl Drawable for Skybox {
    fn render_args<'a>(
        &'a mut self,
        _: &[[[f32; 4]; 4]],
    ) -> Vec<(
        shader::UniformInfo,
        VertexHolder<'a>,
        glium::index::IndicesSource<'a>,
    )> {
        let args = match (&self.tex, self.mip_progress) {
            (SkyboxTex::Sphere(map), _) => {
                shader::UniformInfo::EquiRect(shader::EqRectData {
                    env_map: map,
                })
            }
            (SkyboxTex::Cube(map), None) => {
                shader::UniformInfo::Skybox(shader::SkyboxData { env_map: map })
            }
            (SkyboxTex::Cube(map), Some(progress)) => {
                shader::UniformInfo::PrefilterHdrEnv(
                    shader::PrefilterHdrEnvData {
                        env_map: map,
                        roughness: progress,
                    },
                )
            }
        };
        vec![(
            args,
            VertexHolder::new(VertexSourceData::Single(From::from(&self.vbo))),
            From::from(&self.ebo),
        )]
    }

    fn transparency(&self) -> Option<f32> {
        None
    }
}

pub fn gen_cubemap_from_sphere<F: glium::backend::Facade>(
    tex_path: &str,
    cubemap_size: u32,
    shader_manager: &shader::ShaderManager,
    facade: &F,
) -> glium::texture::Cubemap {
    use super::{camera, drawable, pipeline, textures};
    use pipeline::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    let mut sky = Skybox::new(
        SkyboxTex::Sphere(textures::load_tex_2d_or_hdr(tex_path, facade)),
        facade,
    );
    let cam = camera::PerspectiveCamera::default(1.);
    let gen_sky = Box::new(render_target::CubemapRenderTarget::new(
        cubemap_size,
        10.,
        Box::new(|| cgmath::point3(0., 0., 0.)),
        facade,
    ));
    let cp = Box::new(texture_processor::CopyTextureProcessor::new(
        cubemap_size,
        cubemap_size,
        None,
        None,
    ));
    let mut gen_sky_pass = RenderPass::new(
        vec![gen_sky],
        vec![cp],
        Pipeline::new(vec![0], vec![(0, (1, 0))]),
    );
    let sd = Rc::new(RefCell::new(shader::SceneData {
        viewer: viewer_data_from(&cam),
        pass_type: shader::RenderPassType::LayeredVisual,
        lights: None,
        light_pos: None,
        ibl_maps: None,
    }));
    let cbo = gen_sky_pass.run_pass(
        &cam,
        shader_manager,
        &sd,
        &mut |fbo, viewer, _, cache, _, _| {
            {
                sd.borrow_mut().viewer = viewer_data_from(viewer);
            }
            drawable::render_drawable(
                &mut sky,
                None,
                fbo,
                &*sd.borrow(),
                cache,
                shader_manager,
            );
        },
    );
    if let TextureType::TexCube(pipeline::Ownership::Own(x)) = cbo.unwrap() {
        x
    } else {
        panic!("Unexpected final texture")
    }
}

/// A cube drawn in wireframe mode as a single color
pub struct DebugCube {
    vbo: glium::VertexBuffer<Vertex>,
    ebo: glium::IndexBuffer<u16>,
}

impl DebugCube {
    #[allow(dead_code)]
    pub fn new<F: glium::backend::Facade>(facade: &F) -> Self {
        Self {
            vbo: glium::VertexBuffer::new(facade, &CUBE_VERTS).unwrap(),
            ebo: glium::IndexBuffer::new(
                facade,
                glium::index::PrimitiveType::TrianglesList,
                &CUBE_INDICES,
            )
            .unwrap(),
        }
    }
}

impl Drawable for DebugCube {
    fn render_args<'a>(
        &'a mut self,
        models: &[[[f32; 4]; 4]],
    ) -> Vec<(
        shader::UniformInfo,
        VertexHolder<'a>,
        glium::index::IndicesSource<'a>,
    )> {
        let mut v = Vec::new();
        for m in models.iter() {
            let args = shader::UniformInfo::CollisionDebug(*m);
            v.push((
                args,
                VertexHolder::new(VertexSourceData::Single(From::from(
                    &self.vbo,
                ))),
                From::from(&self.ebo),
            ));
        }
        v
    }

    fn transparency(&self) -> Option<f32> {
        None
    }
}

pub struct Volumetric {
    vbo: glium::VertexBuffer<Vertex>,
    ebo: glium::IndexBuffer<u16>,
    vol: glium::texture::Texture3d,
}

impl Volumetric {
    /// Constructs a new cloud volume
    pub fn cloud<F: glium::backend::Facade>(tex_size: u32, facade: &F) -> Self {
        use super::textures;
        Self {
            vbo: glium::VertexBuffer::new(facade, &CUBE_VERTS).unwrap(),
            ebo: glium::IndexBuffer::new(
                facade,
                glium::index::PrimitiveType::TrianglesList,
                &CUBE_INDICES,
            )
            .unwrap(),
            vol: textures::gen_cloud_noise_vol(
                tex_size, tex_size, tex_size, facade,
            ),
        }
    }
}

impl Drawable for Volumetric {
    fn render_args<'a>(
        &'a mut self,
        pos: &[[[f32; 4]; 4]],
    ) -> Vec<(
        shader::UniformInfo,
        VertexHolder<'a>,
        glium::index::IndicesSource<'a>,
    )> {
        let mut out = Vec::new();
        for p in pos {
            let arg = shader::UniformInfo::Cloud(shader::CloudData {
                model: *p,
                volume: &self.vol,
            });
            out.push((
                arg,
                VertexHolder::new(VertexSourceData::Single(From::from(
                    &self.vbo,
                ))),
                From::from(&self.ebo),
            ));
        }
        out
    }

    fn transparency(&self) -> Option<f32> {
        None
    }
}
