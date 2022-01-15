use super::draw_traits::*;
use crate::cg_support::Transformation;
use std::rc::Rc;
use std::cell::RefCell;
pub struct Entity {
    pub geometry: Box<dyn Drawable>,
    pub locations: Vec<Rc<RefCell<dyn Transformation>>>,
}

impl std::ops::Deref for Entity {
    type Target = dyn Drawable;

    fn deref(&self) -> &Self::Target {
        &*self.geometry
    }
}