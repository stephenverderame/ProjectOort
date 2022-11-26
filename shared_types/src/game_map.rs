use super::*;
use crate::node::{to_remote_object, Node};
use cgmath::*;
use rand;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub struct GlobalLightingInfo {
    pub skybox: String,
    pub hdr: String,
    pub dir_light: Vector3<f32>,
}

impl Eq for GlobalLightingInfo {}

pub trait Map {
    fn initial_objects(&self) -> Vec<RemoteObject>;

    fn lighting_info(&self) -> GlobalLightingInfo;
}

pub struct AsteroidMap {}

impl AsteroidMap {
    /// Generates `count` number of transforms all arranged around `center` in
    /// a sphere. Calls `func`, passing in each transform
    ///
    /// `radius` - the range of radii to place objects
    ///
    /// `theta` - the range of radians to place objects around (horizontally)
    ///
    /// `phi` - the range of radians to place objects from the zenith
    ///
    /// `scale` - the range of uniform scale factors
    fn randomize_spherical<F>(
        center: Point3<f64>,
        radius: Range<f64>,
        theta: Range<f64>,
        phi: Range<f64>,
        scale: Range<f64>,
        count: usize,
        mut func: F,
    ) where
        F: FnMut(Node),
    {
        use rand::distributions::*;
        let radius_distrib = Uniform::from(radius);
        let theta_distrib = Uniform::from(theta);
        let phi_distrib = Uniform::from(phi);
        let scale_distrib = Uniform::from(scale);
        let axis_distrib = Uniform::from(-1.0..1.0);
        let angle_distrib = Uniform::from(0.0..360.0);
        let (mut rng_r, mut rng_t, mut rng_p, mut rng_s, mut rng_a) = (
            rand::thread_rng(),
            rand::thread_rng(),
            rand::thread_rng(),
            rand::thread_rng(),
            rand::thread_rng(),
        );
        for (((radius, theta), phi), scale) in radius_distrib
            .sample_iter(&mut rng_r)
            .zip(theta_distrib.sample_iter(&mut rng_t))
            .zip(phi_distrib.sample_iter(&mut rng_p))
            .zip(scale_distrib.sample_iter(&mut rng_s))
            .take(count)
        {
            let x = vec3(phi.sin() * theta.cos(), phi.sin() * theta.sin(), phi.cos()) * radius;
            let pos: [f64; 3] = (center.to_vec() + x).into();
            let axis = vec3(
                axis_distrib.sample(&mut rng_a),
                axis_distrib.sample(&mut rng_a),
                axis_distrib.sample(&mut rng_a),
            )
            .normalize();
            let rot = Quaternion::<f64>::from_axis_angle(
                axis,
                Deg::<f64>(angle_distrib.sample(&mut rng_a)),
            )
            .normalize();
            let n = Node::default().pos(pos.into()).u_scale(scale).rot(rot);
            func(n);
        }
    }
}

impl Map for AsteroidMap {
    fn initial_objects(&self) -> Vec<RemoteObject> {
        let mut vec = Vec::new();
        use std::f64::consts::PI;
        let mut ids = ObjectId::default();
        Self::randomize_spherical(
            point3(0., 0., 0.),
            120. ..600.,
            0. ..2. * PI,
            0. ..PI,
            0.002..0.8,
            100,
            |t| {
                vec.push(to_remote_object(
                    &t,
                    &vec3(0., 0., 0.),
                    &vec3(0., 0., 0.),
                    ObjectType::Asteroid,
                    ids,
                ));
                ids = ids.incr(1);
            },
        );
        vec.push(to_remote_object(
            &Node::default().u_scale(10.),
            &vec3(0., 0., 0.),
            &vec3(0., 0., 0.),
            ObjectType::Planet,
            ids,
        ));
        vec
    }

    fn lighting_info(&self) -> GlobalLightingInfo {
        GlobalLightingInfo {
            skybox: String::from("assets/Milkyway/Milkyway_BG.jpg"),
            hdr: String::from("assets/Milkyway/Milkyway_Light.hdr"),
            dir_light: vec3(-2_396.839_8, -1_668.553, 3_637.501).normalize(),
        }
    }
}
