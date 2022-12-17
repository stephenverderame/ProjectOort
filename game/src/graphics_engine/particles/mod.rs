mod particle;
mod system;
use super::instancing;
use super::shader;
use crate::cg_support::node;
use std::time::Duration;

pub use particle::{Particle, ParticleEmitter};
pub use system::ParticleSystem;

pub trait Emitter {
    /// Emits particles and moves them according to the change in time since
    /// last frame
    fn emit(&mut self, dt: Duration);

    /// Returns `true` if the emitter has finished emitting, and can be deleted
    /// (so all particles died)
    fn expired(&self) -> bool;

    /// Gets the lights emitted by this emitter or `None` if this emitter
    /// does not emit lights
    fn lights(&self) -> Option<Vec<shader::LightData>>;

    fn instance_data(&self) -> glium::vertex::VerticesSource;
}

/*
pub fn dust_emitter<F : glium::backend::Facade>(facade: &F, pos: cgmath::Point3<f64>) -> Box<dyn Emitter> {
    use std::time::Instant;
    use cgmath::*;
    use rand::Rng;
    Box::new(ParticleEmitter::new(node::Node::default().pos(pos), None, 100, facade,
    |origin| {
        let mut rnd = rand::thread_rng();
        Particle::new(origin.pos, node::Node::default().pos(origin.pos).u_scale(rnd.gen_range(0. .. 1f64)))
            .vel(vec3(rnd.gen_range(0. .. 30f64), rnd.gen_range(0. .. 30f64), rnd.gen_range(0. .. 30f64)))
            .lifetime(Duration::from_millis(rnd.gen_range(10 .. 1500)))
            .color(vec4(0.5, 0.5, 0.5, 0.8))
    },
    |particle| {
        Instant::now().duration_since(particle.birth) > particle.lifetime
    },
    |particle, dt| {
        particle.transform.pos += particle.vel * dt;
        particle.transform.orientation.s += 0.2;
    },
    |particle| {
        let pos = particle.transform.pos;
        let rot = particle.transform.orientation.s;
        instancing::BillboardAttributes {
            instance_color: particle.color.into(),
            instance_pos_rot: vec4(pos.x, pos.y, pos.z, rot).cast().unwrap().into(),
            instance_scale: [particle.transform.scale.x as f32, particle.transform.scale.y as f32],
        }
    },
    |_| None))
}*/

/// Constructs a simple particle emitting billboard particles
///
/// Each particle lives as long as its lifetime. Each frame it moves based on particle velocity and rotational velocity.
/// The scalar part of the quaternion represents rotation angle about camera z-axis
///
/// `path` - texture path for particle
///
/// `particle_num` - number of particles
///
/// `are_lights` - true if all particles are light sources
///
/// `particle_gen` - generator for new particles
///
/// `particle_step` - callback function each time particles are drawn
pub fn simple_emitter<G, S, F>(
    pos: cgmath::Point3<f64>,
    are_lights: bool,
    particle_num: u32,
    emitter_lifetime: Option<std::time::Duration>,
    facade: &F,
    particle_gen: G,
    mut particle_step: Option<S>,
) -> Box<dyn Emitter>
where
    G: Fn(&node::Node) -> Particle + 'static,
    S: FnMut(&mut Particle, f64) + 'static,
    F: glium::backend::Facade,
{
    use cgmath::*;
    use std::time::Instant;
    Box::new(ParticleEmitter::new(
        node::Node::default().pos(pos),
        emitter_lifetime,
        particle_num,
        facade,
        particle_gen,
        |particle| Instant::now().duration_since(particle.birth) > particle.lifetime,
        move |particle, dt| {
            if let Some(step) = particle_step.as_mut() {
                step(particle, dt);
            }
            particle.transform.translate(particle.vel * dt);
            particle.transform.rotate_world(particle.rot_vel);
        },
        |particle| {
            let pos = particle.transform.local_pos();
            let rot = particle.transform.local_rot().s;
            let scale = particle.transform.local_scale();
            instancing::BillboardAttributes {
                instance_color: particle.color.into(),
                instance_pos_rot: vec4(pos.x, pos.y, pos.z, rot).cast().unwrap().into(),
                instance_scale: [scale.x as f32, scale.y as f32],
            }
        },
        move |particle| {
            if !are_lights {
                return None;
            }
            let pt = particle.transform.mat().transform_point(point3(0., 0., 0.));
            let c = vec3(particle.color.x, particle.color.y, particle.color.z);
            Some(shader::LightData::point_light(
                pt.cast().unwrap(),
                c.magnitude() * 3.,
                c,
            ))
        },
    ))
}
use cgmath::*;

pub fn laser_hit_emitter<F: glium::backend::Facade>(
    body_pos: Point3<f64>,
    body_normal: Vector3<f64>,
    laser_velocity: Vector3<f64>,
    facade: &F,
) -> Box<dyn Emitter> {
    use rand::Rng;
    let d = body_normal.dot(body_pos.to_vec());
    let z = (d - body_normal.x) / body_normal.z; // z coord of point with x = 1, y = 0 on the plane
    let v = vec3(1., 0., z) - body_pos.to_vec(); // line on the plane
    let mut rnd = rand::thread_rng();
    simple_emitter(
        body_pos,
        true,
        rnd.gen_range(10..50),
        Some(std::time::Duration::from_millis(10)),
        facade,
        move |origin| {
            let mut rnd = rand::thread_rng();
            let mag = laser_velocity.magnitude();
            let phi = rnd.gen_range(0. ..std::f64::consts::PI / 5.);
            let theta = rnd.gen_range(0. ..2. * std::f64::consts::PI);
            let z = body_normal.cross(v);
            let vel = v.normalize() * f64::cos(theta) * f64::sin(phi)
                + body_normal.normalize() * f64::sin(theta) * f64::cos(phi)
                + z.normalize() * f64::cos(phi);
            // randomly select a vector on a unit sphere with the normal as the zenith
            let origin_pos = origin.transform_point(point3(0., 0., 0.));
            Particle::new(
                origin_pos,
                node::Node::default()
                    .pos(origin_pos)
                    .u_scale(rnd.gen_range(0.1..0.8)),
            )
            .color(vec4(0.5451, 0., 0.5451, 1.))
            .lifetime(Duration::from_millis(rnd.gen_range(60..300)))
            .vel(vel.normalize() * mag)
        },
        Some(|particle: &mut Particle, dt| {
            particle.vel.y -= 0.2 * dt;
        }),
    )
}

pub fn asteroid_hit_emitter<F: glium::backend::Facade>(
    body_pos: Point3<f64>,
    body_normal: Vector3<f64>,
    relative_velocity: Vector3<f64>,
    facade: &F,
) -> Box<dyn Emitter> {
    use rand::Rng;
    let d = body_normal.dot(body_pos.to_vec());
    let z = (d - body_normal.x) / body_normal.z; // z coord of point with x = 1, y = 0 on the plane
    let v = vec3(1., 0., z) - body_pos.to_vec(); // line on the plane
                                                 //let mut rnd = rand::thread_rng();
    simple_emitter(
        body_pos,
        false,
        20,
        Some(std::time::Duration::from_millis(8)),
        facade,
        move |origin| {
            let mut rnd = rand::thread_rng();
            let mag = relative_velocity.magnitude() / 10.;
            let phi = rnd.gen_range(0. ..std::f64::consts::PI / 5.);
            let theta = rnd.gen_range(0. ..2. * std::f64::consts::PI);
            let z = body_normal.cross(v);
            let vel = v.normalize() * f64::cos(theta) * f64::sin(phi)
                + body_normal.normalize() * f64::sin(theta) * f64::cos(phi)
                + z.normalize() * f64::cos(phi);
            // randomly select a vector on a unit sphere with the normal as the zenith
            let origin_pos = origin.transform_point(point3(0., 0., 0.));
            Particle::new(
                origin_pos,
                node::Node::default()
                    .pos(origin_pos)
                    .u_scale(rnd.gen_range(0.6..1.2)),
            )
            .color(vec4(0.421_875, 0.2265_625, 0.0468_75, 0.5))
            .lifetime(Duration::from_millis(rnd.gen_range(300..1000)))
            .vel(vel.normalize() * mag)
        },
        Some(|_: &mut Particle, _| ()),
    )
}
