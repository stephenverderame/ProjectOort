use super::drawable;
use cgmath::*;
use drawable::Viewer;

#[derive(Clone)]
pub struct PerspectiveCamera {
    pub cam: cgmath::Point3<f32>,
    pub aspect: f32,
    pub fov_deg: f32,
    pub target: cgmath::Point3<f32>,
    pub near: f32,
    pub far: f32,
    pub up: cgmath::Vector3<f32>,

}

impl PerspectiveCamera {
    pub fn default(aspect: f32) -> PerspectiveCamera {
        PerspectiveCamera {
            cam: point3(0., 0., 0.),
            aspect, fov_deg: 60., target: point3(0., 0., 1.),
            near: 0.1, far: 100., up: vec3(0., 1., 0.),
        }
    }

    /// Gets a viewer for a cascade that spans the perspective frustum from `near` to `far` along the view space z axis
    /// 
    /// `light_dir` - direction of light/angle to view the cascade at
    /// 
    /// `map_size` - shadow map size for texel snapping
    pub fn get_cascade(&self, light_dir: Vector3<f32>, near: f32, far: f32, map_size: u32) -> Box<dyn Viewer> {

        let mut f = self.clone();
        f.near = near;
        f.far = far;

        let (frustum, mut center) = get_frustum_world(&f);
        let mut radius = f32::MIN;
        for pt in &frustum {
            radius = radius.max((pt - center).magnitude());
        }

        let texels_per_unit = map_size as f32 / (radius * 2.0);
        let lookat = Matrix4::look_at_rh(point3(light_dir.x, light_dir.y, light_dir.z), point3(0., 0., 0.), vec3(0., 1., 0.))
            * Matrix4::from_scale(texels_per_unit);
        let lookat_inv = lookat.invert().unwrap();
        center = lookat.transform_point(center);
        center.x = center.x.floor();
        center.y = center.y.floor(); // move the center in texel-sized increments
        center.z = center.z.floor();
        center = lookat_inv.transform_point(center);

        let view = Matrix4::look_at_rh(center + light_dir, center, vec3(0., 1., 0.)); 
        //right-handed system, positive z facing towards the camera (ortho expects positize z facing away)

        let z_factor = 6f32; // expand in the z-direction to include objects that might cast a shadow into the map
        Box::new(StaticCamera {
            view,
            near: f.near,
            far: f.far,
            proj: ortho(-radius, radius, -radius, radius, -radius * z_factor, radius * z_factor),
            cam_pos: center + light_dir,
        })
    }

    /// Gets the cameras for cascade splits of this frustum
    /// 
    /// `splits` - a vector of `(far_plane, tex_square_size)` tuples for each cascade
    /// Each subsequenct cascade has a near plane of the previous cascade's far plane
    /// Requires `splits` to be ordered closest to farthest cascade
    /// 
    /// Returns the cameras specified from the first split to the last one
    #[allow(dead_code)]
    pub fn get_cascades(&self, splits: Vec<(f32, u32)>, light_dir: Vector3<f32>) -> Vec<Box<dyn Viewer>> {
        let mut last_depth = self.near;
        let mut cams = Vec::<Box<dyn Viewer>>::new();
        for (split, map_size) in splits {
            cams.push(self.get_cascade(light_dir, last_depth, split, map_size));
            last_depth = split;
        }
        cams
    }
}

impl Viewer for PerspectiveCamera {
    fn proj_mat(&self) -> cgmath::Matrix4<f32> {
        cgmath::perspective(cgmath::Deg::<f32>(self.fov_deg), self.aspect, self.near, self.far)
    }

    fn cam_pos(&self) -> cgmath::Point3<f32> {
        self.cam
    }

    fn view_mat(&self) -> Matrix4<f32> {
        let cam_pos = self.cam_pos();
        Matrix4::look_at_rh(cam_pos, self.target.cast::<f32>().unwrap(), 
            self.up.cast::<f32>().unwrap())
    }

    fn view_dist(&self) -> (f32, f32) {
        (self.near, self.far)
    }
}

#[derive(Clone)]
pub struct OrthoCamera {
    pub left: f32,
    pub right: f32,
    pub near: f32,
    pub far: f32,
    pub top: f32,
    pub btm: f32,
    pub target: cgmath::Point3<f32>,
    pub cam_pos: cgmath::Point3<f32>,
    pub up: cgmath::Vector3<f32>,
}

impl OrthoCamera {
    #[allow(dead_code)]
    pub fn new(width: f32, height: f32, near: f32, far: f32, pos: cgmath::Point3<f32>, 
        target: Option<cgmath::Point3<f32>>, up: Option<cgmath::Vector3<f32>>) -> OrthoCamera
    {
        let x = width / 2.0;
        let y = height / 2.0;
        OrthoCamera {
            left: -x, right: x, top: y, btm: -y, near, far,
            cam_pos: pos,
            target: target.unwrap_or_else(|| cgmath::point3(0., 0., 0.)),
            up: up.unwrap_or_else(|| cgmath::vec3(0., 1., 0.)),
        }
    }
}

impl Viewer for OrthoCamera {
    fn proj_mat(&self) -> cgmath::Matrix4<f32> {
        cgmath::ortho(self.left, self.right, self.btm, self.top, self.near, self.far)
    }

    fn cam_pos(&self) -> cgmath::Point3<f32> {
        self.cam_pos
    }

    fn view_mat(&self) -> Matrix4<f32> {
        let cam_pos = self.cam_pos();
        Matrix4::look_at_rh(cam_pos, self.target, self.up)
    }

    fn view_dist(&self) -> (f32, f32) {
        (self.near, self.far)
    }
}

impl std::default::Default for OrthoCamera {
    fn default() -> Self {
        OrthoCamera {
            left: -10., right: 10., near: 0.1,
            far: 10., top: 10., btm: -10.,
            target: point3(0., 0., 0.),
            cam_pos: point3(0., 0., -1.),
            up: vec3(0., 1., 0.),
        }
    }
}

/// Gets the world coordinates of a viewer's frustum, and the center point of that frustum
/// Points are ordered top left, top right, bottom right, bottom left, near plane then far plane
pub fn get_frustum_world(viewer: &dyn Viewer) -> (Vec<Point3<f32>>, Point3<f32>) {
    //v_ndc = M_proj * M_view * v_world
    let cube = [
        point3(-1f32, 1., -1.),
        point3(1., 1., -1.),
        point3(1., -1., -1.),
        point3(-1., -1., -1.),
        point3(-1., 1., 1.),
        point3(1., 1., 1.),
        point3(1., -1., 1.),
        point3(-1., -1., 1.),
    ];
    let inv = (viewer.proj_mat() * viewer.view_mat()).invert().unwrap();
    let mut out = Vec::<Point3<f32>>::new();
    let mut center = vec3(0f32, 0., 0.);
   for pt in cube {
        let mut r = inv * vec4(pt.x, pt.y, pt.z, 1.0);
        r /= r.w;
        center += vec3(r.x, r.y, r.z);
        out.push(point3(r.x, r.y, r.z));
    }
    center /= out.len() as f32;
    (out, point3(center.x, center.y, center.z))
    
}

/// A Camera that isn't easy to move as it just stores the prebuilt view and project matrices
pub struct StaticCamera {
    pub view: Matrix4<f32>,
    pub proj: Matrix4<f32>,
    pub cam_pos: Point3<f32>,
    pub near: f32,
    pub far: f32,
}

impl StaticCamera {
    pub fn from(cam: &dyn Viewer) -> Self {
        Self {
            view: cam.view_mat(),
            proj: cam.proj_mat(),
            cam_pos: cam.cam_pos(),
            near: cam.view_dist().0,
            far: cam.view_dist().1,
        }
    }
}

impl Viewer for StaticCamera {
    fn proj_mat(&self) -> Matrix4<f32> { self.proj }
    fn view_mat(&self) -> Matrix4<f32> { self.view }
    fn cam_pos(&self) -> Point3<f32> { self.cam_pos }
    fn view_dist(&self) -> (f32, f32) { (self.near, self.far) }
}

pub struct Camera2D {
    pub proj: Matrix4<f32>
}

impl Camera2D {
    pub fn new(width: u32, height: u32) -> Camera2D {
        Camera2D {
            proj: ortho(0f32, width as f32, height as f32, 0., 0., 1.)
        }
    }
}

impl Viewer for Camera2D {
    fn proj_mat(&self) -> Matrix4<f32> { self.proj }
    fn view_mat(&self) -> Matrix4<f32> { Matrix4::from_scale(1.) }
    fn cam_pos(&self) -> Point3<f32> { point3(0., 0., 0.) }
    fn view_dist(&self) -> (f32, f32) { (0., 1.) }
}
