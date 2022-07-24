
use super::*;
use cgmath::*;
use rand;
use std::ops::Range;
use crate::node::{Node, to_remote_object};
use std::collections::HashMap;

pub struct GameStats {

}

pub struct PlayerStats {

}

pub trait GameController {
    fn get_game_objects(&self) -> &[RemoteObject];

    fn get_game_time(&self) -> std::time::Duration;

    fn get_game_stats(&self) -> &GameStats;

    fn get_player_stats(&self) -> &PlayerStats;

    fn set_objects(&mut self, objects: &[RemoteObject]);

    fn update_objects(&mut self, updates: &[RemoteObjectUpdate]);

    fn remove_objects(&mut self, ids: &[u32]);

    /// Get `n` ids to use for objects
    /// Returns the first and last id in the range, note that the last
    /// id may be less than the first if the ids wrap around
    fn get_n_ids(&mut self, n: u32) -> (u32, u32);
}

pub trait Map {
    fn initial_objects(&self) -> Vec<RemoteObject>;
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
    fn randomize_spherical<F>(center: Point3<f64>, radius: Range<f64>, 
        theta: Range<f64>, phi: Range<f64>, scale: Range<f64>, 
        count: usize, mut func : F)
        where F : FnMut(Node)
    {
        use rand::distributions::*;
        let radius_distrib = Uniform::from(radius);
        let theta_distrib = Uniform::from(theta);
        let phi_distrib = Uniform::from(phi);
        let scale_distrib = Uniform::from(scale);
        let axis_distrib = Uniform::from(-1.0 .. 1.0);
        let angle_distrib = Uniform::from(0.0 .. 360.0);
        let (mut rng_r, mut rng_t, mut rng_p, mut rng_s, mut rng_a) = 
            (rand::thread_rng(), rand::thread_rng(), 
            rand::thread_rng(), rand::thread_rng(), rand::thread_rng());
        for (((radius, theta), phi), scale) in radius_distrib.sample_iter(&mut rng_r)
            .zip(theta_distrib.sample_iter(&mut rng_t))
            .zip(phi_distrib.sample_iter(&mut rng_p))
            .zip(scale_distrib.sample_iter(&mut rng_s))
            .take(count)
        {
            let x = vec3(phi.sin() * theta.cos(), phi.sin() * theta.sin(),
                phi.cos()) * radius;
            let pos : [f64; 3] = (center.to_vec() + x).into();
            let axis = vec3(axis_distrib.sample(&mut rng_a), 
                axis_distrib.sample(&mut rng_a), 
                axis_distrib.sample(&mut rng_a)).normalize();
            let rot = Quaternion::<f64>::from_axis_angle(axis, 
                Deg::<f64>(angle_distrib.sample(&mut rng_a))).normalize();
            let n = Node::default().pos(pos.into()).u_scale(scale)
                .rot(rot);
            func(n);
        }
    }
}

impl Map for AsteroidMap {
    fn initial_objects(&self) -> Vec<RemoteObject> {
        let mut vec = Vec::new();
        use std::f64::consts::PI;
        let mut ids = 0;
        Self::randomize_spherical(point3(0., 0., 0.), 120. .. 600., 0. .. 2. * PI, 
            0. .. PI, 0.002 .. 0.8, 100, 
            |t| { 
                vec.push(to_remote_object(&t, &vec3(0., 0., 0.), 
                    &vec3(0., 0., 0.), ObjectType::Asteroid, ids));
                ids += 1;
            });
        vec.push(to_remote_object(&Node::default().u_scale(10.), 
            &vec3(0., 0., 0.), &vec3(0., 0., 0.), ObjectType::Planet, ids));
        vec
    }
}

pub struct LocalGameController{
    last_id: u32,
    objects: Vec<RemoteObject>,
    indices: HashMap<u32, usize>,
    start_time: std::time::Instant,
}

impl LocalGameController {
    pub fn new<M : Map>(map: &M) -> LocalGameController {
        let objs = map.initial_objects();
        let indices = (0 .. objs.len()).map(|i| (objs[i].id, i)).collect();
        LocalGameController {
            last_id: objs.last().map(|o| o.id).unwrap_or(0),
            objects: objs,
            start_time: std::time::Instant::now(),
            indices,
        }
    }
}

impl GameController for LocalGameController {
    fn get_game_objects(&self) -> &[RemoteObject] {
        &self.objects
    }

    fn get_game_time(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    fn get_game_stats(&self) -> &GameStats {
        &GameStats {}
    }

    fn get_player_stats(&self) -> &PlayerStats {
        &PlayerStats {}
    }

    fn set_objects(&mut self, objects: &[RemoteObject]) {
        for obj in objects {
            if let Some(i) = self.indices.get(&obj.id) {
                self.objects[*i] = *obj;
            } else {
                self.indices.insert(obj.id, self.objects.len());
                self.objects.push(*obj);
            }
        }
    }

    fn update_objects(&mut self, updates: &[RemoteObjectUpdate]) {
        for update in updates {
            if let Some(idx) = self.indices.get(&update.id) {
                let (node, mut vel, mut rot, typ, id) = 
                    node::from_remote_object(&self.objects[*idx]);
                vel += From::from(update.delta_vel);
                rot += From::from(update.delta_rot);

                // TODO: update position here?

                self.objects[*idx] = node::to_remote_object(&node, &vel, &rot, typ, id);
            }
        }
    }

    fn remove_objects(&mut self, ids: &[u32]) {
        for id in ids {
            if let Some(index) = self.indices.remove(id) {
                if self.objects.len() > 1 {
                    let last_index = self.objects.len() - 1;
                    let last_id = self.objects[last_index].id;
                    self.indices.insert(last_id, index);
                }
                self.objects.swap_remove(index);

            }
        }
    }

    fn get_n_ids(&mut self, n: u32) -> (u32, u32) {
        let id = self.last_id;
        self.last_id += n;
        (id, self.last_id)
    }
}