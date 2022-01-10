use cgmath::*;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use super::object::Object;
extern crate arr_macro;


type ObjectList = Vec<Rc<RefCell<Object>>>;

pub struct ONode {
    center: Point3<f64>,
    h_width: f64, // dist to center to any axis-aligned side of AABB
    objects: ObjectList,
    children: Option<[Rc<RefCell<ONode>>; 8]>, // ith bit in children index is 1 if ith coordinate is > center
    parent: Weak<RefCell<ONode>>,
    self_ref: Weak<RefCell<ONode>>,
}

impl ONode {
    const MAX_OBJS_PER_LEAF : usize = 12;

    /// After creation, a self reference must be assigned
    pub fn new(c: Point3<f64>, h_width: f64) -> ONode {
        ONode {
            center: c, h_width,
            objects: Vec::new(),
            children: None,
            parent: Weak::new(),
            self_ref: Weak::new(),
        }
    }

    /// After creation, a self reference, center, and width must be assigned
    fn empty() -> ONode {
        ONode {
            center: point3(0., 0., 0.), h_width: 0.,
            objects: Vec::new(),
            children: None, parent: Weak::new(),
            self_ref: Weak::new(),
        }
    }

    fn create_children(parent: Weak<RefCell<ONode>>, 
        parent_c: &Point3<f64>, parent_h: f64) -> [Rc<RefCell<ONode>>; 8] 
    {
        let mut res = arr_macro::arr![Rc::new(RefCell::new(ONode::empty())); 8];
        let step = parent_h / 2.0;
        for (child, idx) in res.iter_mut().zip(0u32 .. 8) {
            let step_x = if idx & 1 == 1 { step } else { -step };
            let step_y = if idx & 2 == 1 { step } else { -step };
            let step_z = if idx & 4 == 1 { step } else { -step };
            child.borrow_mut().center = parent_c + vec3(step_x, step_y, step_z);
            child.borrow_mut().h_width = step;
            child.borrow_mut().parent = parent.clone();
            child.borrow_mut().self_ref = Rc::downgrade(&child);
        };
        res
    }

    /// Splits objects in `objects` into octree nodes of `children`
    /// 
    /// Returns the new object list for the current node
    fn split_into_children(children: &mut [Rc<RefCell<ONode>>; 8], 
        objects: &mut Vec<Rc<RefCell<Object>>>, center: &Point3<f64>, h_width: f64) -> Vec<Rc<RefCell<Object>>> {
        let mut new_objs = Vec::<Rc<RefCell<Object>>>::new();
        for obj in objects {
            match ONode::get_octant_index(center, h_width, &obj) {
                Some(idx) => {
                    //println!("{:?} has octant index: {}", obj, idx);
                    children[idx as usize].borrow_mut().insert(obj.clone())
                },
                None => new_objs.push(obj.clone())
            }
        };
        new_objs
    }

    /// Gets octant index or `None` if object is in multiple octants
    fn get_octant_index(center: &Point3<f64>, h_width: f64, obj: &Rc<RefCell<Object>>) -> Option<u8> {
        let o = obj.borrow().center() - center;
        let mut index = 0u8;
        for i in 0 .. 3 {
            if o[i].abs() < obj.borrow().radius() {
                return None
            } else if o[i] > 0. {
                index |= 1 << i;
            }
        };
        Some(index)
    }

    pub fn insert(&mut self, obj: Rc<RefCell<Object>>) {
        if self.children.is_none() && self.objects.len() + 1 < ONode::MAX_OBJS_PER_LEAF {
            obj.borrow_mut().octree_cell = self.self_ref.clone();
            self.objects.push(obj);
            return
        } else if self.children.is_none() {
            self.children = Some(ONode::create_children(self.self_ref.clone(),
                &self.center, self.h_width));
            self.objects = ONode::split_into_children(self.children.as_mut().unwrap(), 
                &mut self.objects, &self.center, self.h_width);
        }
        match ONode::get_octant_index(&self.center, self.h_width, &obj) {
            Some(idx) => self.children.as_mut().unwrap()[idx as usize]
                .borrow_mut().insert(obj.clone()),
            None => {
                obj.borrow_mut().octree_cell = self.self_ref.clone();
                self.objects.push(obj.clone())
            },
        }
    }

    /// Gets all objects that have overlapping bounding spheres as `test_obj` in `node` or children of `node`
    fn get_subtree_colliders(node: &Rc<RefCell<ONode>>, test_obj: &Rc<RefCell<Object>>) -> ObjectList {
        let mut v : ObjectList = Vec::new();
        for obj in node.borrow().objects.iter() {
            if Rc::ptr_eq(obj, test_obj) { continue; }
            let (o, other) = (obj.borrow(), test_obj.borrow());
            let dist = (other.center() - o.center()).dot(other.center() - o.center());
            if dist < (o.radius() + other.radius()).powi(2) {
                v.push(obj.clone())
            }
        }
        if let Some(children) = node.borrow().children.as_ref() {
            for c in children {
                v.append(&mut ONode::get_subtree_colliders(c, test_obj))
            }
        }
        v
    }

    pub fn get_possible_colliders(obj: &Rc<RefCell<Object>>) -> ObjectList {
        if let Some(cell) = obj.borrow().octree_cell.upgrade() {
            ONode::get_subtree_colliders(&cell, obj)
        } else { Vec::new() }
    }

    /// Indicates that `obj` has changed and should be re-evaluated for placement in the octree
    /// 
    /// If `obj` no longer fits in the octree, it remains in the root node
    pub fn update(&mut self, obj: &Rc<RefCell<Object>>) {
        if let Some(parent) = self.parent.upgrade() {
            let delta = obj.borrow().center() - self.center;
            let radius = obj.borrow().radius();
            for i in 0 .. 3 {
                if delta[i].abs() + radius > self.h_width {
                    self.objects.retain(|e| !Rc::ptr_eq(&e, &obj));
                    return parent.borrow_mut().insert(obj.clone());
                }
            }
        } 
        if let Some(child_idx) = ONode::get_octant_index(&self.center, self.h_width, &obj) {
            self.children.as_mut().map(|c| {
                obj.borrow_mut().octree_cell = Rc::downgrade(&c[child_idx as usize]);
                c[child_idx as usize].borrow_mut().insert(obj.clone())
            });
        } 
    }
}

pub struct Octree {
    root: Rc<RefCell<ONode>>,
}

impl Octree {
    /// Inserts a node in the tree. If the node doesn't fit,
    /// it stays at the root node
    #[inline(always)]
    pub fn insert(&mut self, obj: Rc<RefCell<Object>>) {
        self.root.borrow_mut().insert(obj)
    }

    /// Creates a new octree centered at `center` with a half width (width of child) as
    /// `half_side_len`
    pub fn new(center: Point3<f64>, half_side_len: f64) -> Octree {
        let root = Rc::new(RefCell::new(ONode::new(center, half_side_len)));
        root.borrow_mut().self_ref = Rc::downgrade(&root);
        Octree {
            root
        }
    }

    /// Get's all objects that have overlapping bounding spheres with `obj`
    pub fn get_colliders(&self, obj: &Rc<RefCell<Object>>) -> ObjectList {
        ONode::get_possible_colliders(obj)
    }

    pub fn remove(&mut self, obj: &Rc<RefCell<Object>>) {
        if let Some(node) = obj.borrow().octree_cell.upgrade() {
            node.borrow_mut().objects.retain(|e| !Rc::ptr_eq(e, obj));
            if node.borrow().objects.is_empty() {
                if let Some(parent) = node.borrow().parent.upgrade() {
                    Octree::maybe_make_leaf(&parent, &node);
                }
            }
        }
        obj.borrow_mut().octree_cell = Weak::default();
    }

    /// Checks the children of `node`. If they are empty, makes `node` a leaf.
    /// 
    /// `initiator` - the child of `node` who just became empty and initiated the 
    /// leaf check for `node`
    fn maybe_make_leaf(node: &Rc<RefCell<ONode>>, initiator: &Rc<RefCell<ONode>>) {
        for c in node.borrow().children.as_ref().unwrap() {
            if !Rc::ptr_eq(c, initiator) {
                if !(c.borrow().objects.is_empty() && c.borrow().children.is_none()) {
                    return
                }
            }
        }
        node.borrow_mut().children = None;
    }

    /// Indictaes `obj` position data has changed.
    /// If `obj` no longer fits in the tree, it stays at the root node
    pub fn update(obj: &Rc<RefCell<Object>>) {
        let res = obj.borrow().octree_cell.upgrade();
        if let Some(n) = res {
            n.borrow_mut().update(obj);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::node;
    
    fn new_obj(center: Point3<f64>, radius: f64) -> Rc<RefCell<Object>> {
        Rc::new(RefCell::new(Object::new(Rc::new(RefCell::new(node::Node::new(None, None, None, None))), 
            center, radius)))
    }

    #[test]
    fn root_no_split() {
        let mut ot = Octree::new(point3(0., 0., 0.), 25.);
        let o1 = new_obj(point3(10., 1., 1.), 4.);
        let o2 = new_obj(point3(-5., 2., 2.), 3.);
        ot.insert(o1.clone());
        ot.insert(o2.clone());
        assert_eq!(ot.get_colliders(&o1).len(), 0);
        assert_eq!(ot.get_colliders(&o2).len(), 0);
        assert_eq!(o1.borrow().octree_cell.ptr_eq(&Rc::downgrade(&ot.root)), true);
    }

    #[test]
    fn collide_no_split() {
        let mut ot = Octree::new(point3(0., 0., 0.), 25.);
        let o1 = new_obj(point3(10., 1., 1.), 4.);
        let o2 = new_obj(point3(8., 2., 2.), 3.);
        ot.insert(o1.clone());
        ot.insert(o2.clone());
        assert_eq!(Rc::ptr_eq(&ot.get_colliders(&o1)[0], &o2), true);
        assert_eq!(Rc::ptr_eq(&ot.get_colliders(&o2)[0], &o1), true);
    }

    #[test]
    fn split() {
        let mut ot = Octree::new(point3(0., 0., 0.), 25.);
        let obj = [
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(-3., -3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 2.),
            new_obj(point3(-10., -3., 3.), 2.),
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(4., 3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 20.),
            new_obj(point3(0., -3., 3.), 2.),
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(-3., -3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 2.),
            new_obj(point3(-10., -3., 3.), 2.),
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(4., 3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 20.),
            new_obj(point3(0., -3., 3.), 2.)
        ];
        for o in &obj {
            ot.insert(o.clone());
        }
        assert_eq!(obj[0].borrow().octree_cell.ptr_eq(&Rc::downgrade(&ot.root)), false);
        assert_eq!(obj[6].borrow().octree_cell.ptr_eq(&Rc::downgrade(&ot.root)), true);
        assert_eq!(obj[3].borrow().octree_cell.ptr_eq(&Rc::downgrade(&ot.root.borrow().children.as_ref().unwrap()[4])), true);
        assert_eq!(obj[2].borrow().octree_cell.ptr_eq(&Rc::downgrade(&ot.root.borrow().children.as_ref().unwrap()[6])), true);
        assert_eq!(obj[4].borrow().octree_cell.ptr_eq(&Rc::downgrade(&ot.root.borrow().children.as_ref().unwrap()[7])), true);
        assert_eq!(obj[7].borrow().octree_cell.ptr_eq(&Rc::downgrade(&ot.root)), true);
        assert_eq!(ONode::get_octant_index(&point3(0., 0., 0.), 25., &obj[1]), Some(0));

        let root = &ot.root.borrow();
        for o in &obj {
            let oct = ONode::get_octant_index(&point3(0., 0., 0.), 25., o);
            assert_eq!(o.borrow().octree_cell.ptr_eq(
                &Rc::downgrade(&oct.map(|id| &root.children.as_ref().unwrap()[id as usize])
                .unwrap_or(&ot.root))
            ), true);
        }
    }

    #[test]
    fn remove_tst() {
        let mut ot = Octree::new(point3(0., 0., 0.), 25.);
        let obj = [
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(-3., -3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 2.),
            new_obj(point3(-10., -3., 3.), 2.),
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(4., 3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 20.),
            new_obj(point3(0., -3., 3.), 2.),
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(-3., -3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 2.),
            new_obj(point3(-10., -3., 3.), 2.),
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(4., 3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 20.),
            new_obj(point3(0., -3., 3.), 2.)
        ];
        for o in &obj {
            ot.insert(o.clone());
        }
        ot.remove(&obj[0]);
        assert_eq!(ot.get_colliders(&obj[0]).len(), 0);
        ot.remove(&obj[10]);
        assert_eq!(obj[10].borrow().octree_cell.as_ptr(), Weak::default().as_ptr());

        for o in &obj {
            ot.remove(o);
        }

        assert_eq!(ot.root.borrow().children.is_none(), true);
        assert_eq!(ot.root.borrow().objects.len(), 0);
    }

    #[test]
    fn update_test() {
        let mut ot = Octree::new(point3(0., 0., 0.), 25.);
        let obj = [
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(-3., -3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 2.),
            new_obj(point3(-10., -3., 3.), 2.),
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(4., 3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 20.),
            new_obj(point3(0., -3., 3.), 2.),
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(-3., -3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 2.),
            new_obj(point3(-10., -3., 3.), 2.),
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(4., 3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 20.),
            new_obj(point3(0., -3., 3.), 2.)
        ];
        for o in &obj {
            ot.insert(o.clone());
        }
        obj[0].borrow_mut().model.borrow_mut().pos = point3(-6., 3., 3.);
        Octree::update(&obj[0]);
        assert_eq!(obj[0].borrow().octree_cell.ptr_eq(&Rc::downgrade(&ot.root.borrow().children.as_ref().unwrap()[6])), true);
        let local_origin = obj[14].borrow().local_center;
        obj[14].borrow_mut().model.borrow_mut().scale = vec3(0.1, 0.1, 0.1);
        obj[14].borrow_mut().model.borrow_mut().anchor = local_origin;
        Octree::update(&obj[14]);
        assert_eq!(obj[14].borrow().octree_cell.ptr_eq(&Rc::downgrade(&ot.root.borrow().children.as_ref().unwrap()[6])), true);
        let local_origin = obj[14].borrow().local_center;
        obj[1].borrow_mut().model.borrow_mut().scale = vec3(5., 5., 5.);
        obj[1].borrow().model.borrow_mut().anchor = local_origin;
        Octree::update(&obj[1]);
        assert_eq!(obj[1].borrow().octree_cell.ptr_eq(&Rc::downgrade(&ot.root)), true);
    }
}