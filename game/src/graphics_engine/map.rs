use std::rc::Rc;
use std::cell::RefCell;
use super::entity::*;
use super::shader::*;
use crate::physics;

/// A Map contains the entities and lighting information of a scene
pub trait Map<T> {
    /// Gets the entities in this map
    fn entities(&self) -> Vec<Rc<RefCell<dyn AbstractEntity>>>;

    /// Gets the lights in this map
    fn lights(&self) -> Vec<LightData>;

    /// Gets the rigid bodies in this map
    fn iter_bodies<'a>(&self, 
        func: Box<dyn FnMut(&mut dyn Iterator<Item = &mut physics::RigidBody<T>>) + 'a>);

    /// Gets the map's IBL maps and directional light direction
    fn lighting_info(&self) -> (PbrMaps, cgmath::Vector3<f32>);
}