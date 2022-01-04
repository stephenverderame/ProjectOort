use crate::draw_traits;
use cgmath::*;
use draw_traits::Viewer;

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

    pub fn get_cascade(&self, light_dir: Vector3<f32>, near: f32, far: f32, map_size: u32) -> Box<dyn Viewer> {
        println!();
        println!();

        let mut f = self.clone();
        f.near = near;
        f.far = far;

        let (frustum, center) = get_frustum_world(&f);
        //println!("Near: TL: {:?}, TR: {:?}, BR: {:?}, BL: {:?}", frustum[0], frustum[1], frustum[2], frustum[3]);
        //println!("Far: TL: {:?}, TR: {:?}, BR: {:?}, BL: {:?}", frustum[4], frustum[5], frustum[6], frustum[7]);
        println!("Frustum World center: {:?}", center);

        let frustum_radius = (frustum[0] - frustum[6]).magnitude() / 2.0; // half distance between opposite corners
        //println!("Frustum radius: {}", frustum_radius);
        let texels_per_unit = map_size as f32 / frustum_radius * 2.0;

        let view = Matrix4::look_at_rh(center + light_dir, center, vec3(0., 1., 0.)); //right-handed system, positive facing towards the camera
        //println!("Center {:?}\n Light-view Center: {:?}", center, lc / lc.w);

        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        let mut min_z = f32::MAX;
        let mut max_z = f32::MIN;
        for pt in &frustum {
            let pt = view * vec4(pt.x, pt.y, pt.z, 1.0);
            min_x = min_x.min(pt.x);
            max_x = max_x.max(pt.x);
            min_y = min_y.min(pt.y);
            max_y = max_y.max(pt.y);
            min_z = min_z.min(-pt.z);
            max_z = max_z.max(-pt.z);
        }
        println!("{} {} {} {} {} {}", min_x, max_x, min_y, max_y, min_z, max_z);
        //println!("View space center: {:?}", view.transform_point(center));

        /*let lookat = Matrix4::from_scale(texels_per_unit) *
            Matrix4::look_at_rh(point3(0f32, 0., 0.), light_dir_pt, self.up.cast::<f32>().unwrap());
        let lookat_inv = lookat.invert().unwrap();

        center = lookat.transform_point(center);
        center.x = center.x.floor(); // keep frustum center to texel_sized increments
        center.y = center.y.floor();
        center = lookat_inv.transform_point(center);

        let eye = center.to_vec() - (light_dir * frustum_radius * 2.0);
        let eye_pt : Point3<f32> = From::from(Into::<(f32, f32, f32)>::into(eye));*/

        let near = if min_z < 0f32 {
            min_z * 6f32
        } else {
            min_z / 6f32
        };
        let far = if max_z < 0f32 {
            max_z / 6f32
        } else {
            max_z * 6f32
        };
        /*Box::new(OrthoCamera { 
            cam_pos: eye_pt, target: center,
            left: min_x, right: max_x,
            btm: min_y, top: max_y,
            near, // use radius to ensure the split has a consistant size and includes things that cast shadows into the frustum
            far,
            up: vec3(0., 1., 0.),
        });*/
        //center = view.transform_point(center);
        //let tl = view.transform_point(frustum[0]);
        //let br = view.transform_point(frustum[6]);
        //let frustum_radius = (tl - br).magnitude() / 2.0;
        let c = Box::new(StaticCamera {
            view,
            near: f.near,
            far: f.far,
            proj: ortho(min_x, max_x, min_y, max_y, min_z, max_z),
            cam_pos: center + light_dir,
        });
        //let (f, ce) = get_frustum_world(&*c);
       // println!("Output world center: {:?}", ce);
        c
    }

    pub fn get_cascade_2(&self, near: f32, far: f32, light_dir: Vector3<f32>) {
        let mut cam = self.clone();
        cam.near = near;
        cam.far = far;

        let proj = cam.proj_mat();

        let scale_x = proj[0][0];
        let scale_y = proj[1][1];

    }

    /// Gets the cameras for cascade splits of this frustum
    /// 
    /// `splits` - a vector of `(far_plane, tex_square_size)` tuples for each cascade
    /// Each subsequenct cascade has a near plane of the previous cascade's far plane
    /// Requires `splits` to be ordered closest to farthest cascade
    /// 
    /// Returns the cameras specified from the first split to the last one
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
    //let inv = viewer.view_mat().invert().unwrap();
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

pub fn get_frustum_view(viewer: &dyn Viewer) -> [Point3<f32>; 8] {
    let proj = viewer.proj_mat();
    let (near, far) = viewer.view_dist();
    let scale_x_inv = 1.0 / proj[0][0];
    let scale_y_inv = 1.0 / proj[1][1];
    let near_x = scale_x_inv * near;
    let near_y = scale_y_inv * near;
    let far_x = scale_x_inv * far;
    let far_y = scale_y_inv * far;
    [
        point3(-near_x, near_y, near),
        point3(near_x, near_y, near),
        point3(near_x, -near_y, near),
        point3(-near_x, -near_y, near),
        point3(-far_x, far_y, far),
        point3(far_x, far_y, far),
        point3(far_x, -far_y, far),
        point3(-far_x, -far_y, far),
    ]
}

pub struct StaticCamera {
    pub view: Matrix4<f32>,
    pub proj: Matrix4<f32>,
    pub cam_pos: Point3<f32>,
    pub near: f32,
    pub far: f32,
}

impl Viewer for StaticCamera {
    fn proj_mat(&self) -> Matrix4<f32> { self.proj }
    fn view_mat(&self) -> Matrix4<f32> { self.view }
    fn cam_pos(&self) -> Point3<f32> { self.cam_pos }
    fn view_dist(&self) -> (f32, f32) { (self.near, self.far) }
}
