use glium::Surface;
use crate::draw_traits;
use crate::shader;
use crate::camera;
use crate::node;

use cgmath::*;
use glium::framebuffer::ToDepthAttachment;

pub struct Scene {}

impl Scene {
    pub fn render<S : glium::Surface, F : FnOnce(&mut S, &shader::Matrices)>
    (&self, frame: &mut S, viewer: &dyn draw_traits::Viewer, aspect: f32, func: F)
    {
        let view = viewer.view_mat();
        let proj = viewer.proj_mat(aspect);
        let mats = shader::Matrices {
            viewproj: (proj * view).into(),
            view: view.into(),
            proj: proj.into(),
            cam_pos: viewer.cam_pos().into(),
        };
        func(frame, &mats);
    }

    pub fn render_to_cubemap<F : glium::backend::Facade, 
        Cb : Fn(&mut glium::framebuffer::SimpleFrameBuffer, &shader::Matrices)>
        (&self, cam_pos: cgmath::Point3<f32>, facade: &F, func: Cb) -> glium::texture::Cubemap 
    {
        let mut cam = camera::PerspectiveCamera {
            cam: node::Node::new(Some(cam_pos), None, None, None),
            aspect: 1f32,
            fov_deg: 90f32,
            target: cgmath::point3(0., 0., 0.),
            near: 0.1,
            far: 10.,
            up: vec3(0., 1., 0.),
        };
        use glium::texture::CubeLayer::*;
        let target_faces : [(cgmath::Point3<f32>, glium::texture::CubeLayer, cgmath::Vector3<f32>); 6] = 
            [(point3(1., 0., 0.), PositiveX, vec3(0., -1., 0.)), (point3(-1., 0., 0.), NegativeX, vec3(0., -1., 0.)),
            (point3(0., 1., 0.), PositiveY, vec3(0., 0., 1.)), (point3(0., -1., 0.), NegativeY, vec3(0., 0., -1.)),
            (point3(0., 0., 1.), PositiveZ, vec3(0., -1., 0.)), (point3(0., 0., -1.), NegativeZ, vec3(0., -1., 0.))];
        let im_size = 1024u32;
        /*let cubemap = glium::texture::Cubemap::empty_with_format(facade, glium::texture::UncompressedFloatFormat::F16,
            glium::texture::MipmapsOption::NoMipmap, im_size).unwrap();*/
        let cubemap = glium::texture::Cubemap::empty(facade, im_size).unwrap();
        for (target, face, up) in target_faces {
            let target : (f32, f32, f32) = (target.to_vec() + cam.cam.pos.to_vec()).into();
            cam.target = std::convert::From::from(target);
            cam.up = up;
            let rbo = glium::framebuffer::DepthRenderBuffer::new(facade, 
                glium::texture::DepthFormat::F32, im_size, im_size).unwrap();
            let mut fbo = glium::framebuffer::SimpleFrameBuffer::with_depth_buffer(facade, 
                cubemap.main_level().image(face), rbo.to_depth_attachment()).unwrap();
            fbo.clear_color_and_depth((0., 0., 0., 1.), 0.);
            self.render(&mut fbo, &cam, 1., &func);
        }
        cubemap
    }
}