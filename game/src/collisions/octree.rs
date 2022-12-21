use super::object::Object;
use cgmath::*;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
extern crate arr_macro;

type ObjectList = Vec<Weak<RefCell<Object>>>;

/// A node in an octree holding weak references to objects that fit in it and
/// (optionally) 8 child nodes
///
/// Each node is a `2 * h_width` x `2 * h_width` x `2 * h_width` box centered around `center`
pub struct ONode {
    center: Point3<f64>,
    h_width: f64, // dist to center to any axis-aligned side of AABB
    objects: ObjectList,
    children: Option<[Rc<RefCell<ONode>>; 8]>, // ith bit in children index is 1 if ith coordinate is > center
    parent: Weak<RefCell<ONode>>,
    self_ref: Weak<RefCell<ONode>>,
    self_index: u8,
}

impl ONode {
    const MAX_OBJS_PER_LEAF: usize = 12;

    /// After creation, a self reference must be assigned
    pub fn new(c: Point3<f64>, h_width: f64) -> Self {
        Self {
            center: c,
            h_width,
            objects: Vec::new(),
            children: None,
            parent: Weak::new(),
            self_ref: Weak::new(),
            self_index: 0,
        }
    }

    /// After creation, a self reference, center, and width must be assigned
    fn empty() -> Self {
        Self {
            center: point3(0., 0., 0.),
            h_width: 0.,
            objects: Vec::new(),
            children: None,
            parent: Weak::new(),
            self_ref: Weak::new(),
            self_index: 0,
        }
    }

    fn create_children(
        parent: &Weak<RefCell<Self>>,
        parent_c: &Point3<f64>,
        parent_h: f64,
    ) -> [Rc<RefCell<Self>>; 8] {
        let mut res = arr_macro::arr![Rc::new(RefCell::new(ONode::empty())); 8];
        let step = parent_h / 2.0;
        for (child, idx) in res.iter_mut().zip(0u32..8) {
            let step_x = if idx & 1 == 1 { step } else { -step };
            let step_y = if idx & 2 > 0 { step } else { -step };
            let step_z = if idx & 4 > 0 { step } else { -step };
            child.borrow_mut().center = parent_c + vec3(step_x, step_y, step_z);
            child.borrow_mut().h_width = step;
            child.borrow_mut().parent = parent.clone();
            child.borrow_mut().self_ref = Rc::downgrade(child);
            child.borrow_mut().self_index = idx as u8;
        }
        res
    }

    /// Splits objects in `objects` into octree nodes of `children`
    ///
    /// Returns the new object list for the current node
    fn split_into_children(
        children: &mut [Rc<RefCell<Self>>; 8],
        objects: &mut [Weak<RefCell<Object>>],
        center: &Point3<f64>,
        h_width: f64,
    ) -> Vec<Weak<RefCell<Object>>> {
        let mut new_objs = Vec::new();
        for obj in objects
            .iter()
            .filter(|x| x.strong_count() > 0)
            .map(|x| x.upgrade().unwrap())
        {
            match Self::get_octant_index(center, h_width, &obj) {
                Some(idx) => {
                    //println!("{:?} has octant index: {}", obj, idx);
                    children[idx as usize].borrow_mut().insert(&obj);
                }
                None => new_objs.push(Rc::downgrade(&obj)),
            }
        }
        new_objs
    }

    /// Gets octant index or `None` if object is in multiple octants
    fn get_octant_index(
        center: &Point3<f64>,
        h_width: f64,
        obj: &Rc<RefCell<Object>>,
    ) -> Option<u8> {
        let o = obj.borrow().center() - center;
        let mut index = 0u8;
        for i in 0..3 {
            if o[i].abs() < obj.borrow().radius()
                || o[i].abs() + obj.borrow().radius() > h_width
            {
                return None;
            } else if o[i] > 0. {
                index |= 1 << i;
            }
        }
        Some(index)
    }

    pub fn insert(&mut self, obj: &Rc<RefCell<Object>>) {
        if self.children.is_none()
            && self.objects.len() + 1 < Self::MAX_OBJS_PER_LEAF
        {
            obj.borrow_mut().octree_cell = self.self_ref.clone();
            self.objects.push(Rc::downgrade(obj));
            return;
        } else if self.children.is_none() {
            self.children = Some(Self::create_children(
                &self.self_ref,
                &self.center,
                self.h_width,
            ));
            self.objects = Self::split_into_children(
                self.children.as_mut().unwrap(),
                &mut self.objects,
                &self.center,
                self.h_width,
            );
        }
        match Self::get_octant_index(&self.center, self.h_width, obj) {
            Some(idx) => self.children.as_mut().unwrap()[idx as usize]
                .borrow_mut()
                .insert(obj),
            None => {
                obj.borrow_mut().octree_cell = self.self_ref.clone();
                self.objects.push(Rc::downgrade(obj));
            }
        }
    }

    /// Gets all objects that have overlapping bounding spheres as `test_obj` in `node` or children of `node`
    ///
    /// `node` - the containing octree cell of `test_obj`
    fn get_subtree_colliders(
        node: &Rc<RefCell<Self>>,
        test_obj: &Rc<RefCell<Object>>,
    ) -> Vec<Rc<RefCell<Object>>> {
        let mut v = Vec::new();
        node.borrow_mut().objects.retain(|x| x.strong_count() > 0);
        for obj in node
            .borrow()
            .objects
            .iter()
            .map(|x| x.upgrade().unwrap())
            .filter(|x| !Rc::ptr_eq(x, test_obj))
        {
            if obj.borrow().bounding_sphere_collide(&*test_obj.borrow()) {
                v.push(obj);
            }
        }
        if let Some(children) = node.borrow().children.as_ref() {
            for c in children {
                v.append(&mut Self::get_subtree_colliders(c, test_obj));
            }
        }
        v
    }

    /// Gets all objects that have overlappring bounding spheres as `test` object that is a parent of `test_obj`
    ///
    /// `node` - the containing octree cell of `test_obj`
    fn get_parent_colliders(
        node: &Rc<RefCell<Self>>,
        test_obj: &Rc<RefCell<Object>>,
    ) -> Vec<Rc<RefCell<Object>>> {
        let mut n = node.borrow().parent.clone();
        let mut v = Vec::new();
        while let Some(parent) = n.upgrade() {
            parent.borrow_mut().objects.retain(|x| x.strong_count() > 0);
            for obj in
                parent.borrow().objects.iter().map(|x| x.upgrade().unwrap())
            {
                if obj.borrow().bounding_sphere_collide(&*test_obj.borrow()) {
                    v.push(obj);
                }
            }
            n = parent.borrow().parent.clone();
        }
        v
    }

    /// Gets objects that might collide with `obj`
    ///
    /// As the tree is traversed, references to freed objects are removed from object lists
    pub fn get_possible_colliders(
        obj: &Rc<RefCell<Object>>,
    ) -> Vec<Rc<RefCell<Object>>> {
        obj.borrow()
            .octree_cell
            .upgrade()
            .map_or_else(Vec::new, |cell| {
                let mut v = Self::get_subtree_colliders(&cell, obj);
                v.append(&mut Self::get_parent_colliders(&cell, obj));
                v
            })
    }

    /// Tests for collisions with an object that is not in the octree
    pub fn test_for_collisions(
        node: &Rc<RefCell<Self>>,
        center: Point3<f64>,
        radius: f64,
    ) -> Vec<Rc<RefCell<Object>>> {
        use crate::node;
        let test_obj = Rc::new(RefCell::new(Object {
            model: Rc::new(RefCell::new(node::Node::default().pos(center))),
            local_radius: radius,
            octree_cell: Weak::new(),
            mesh: Weak::new(),
        }));
        Self::get_subtree_colliders(node, &test_obj)
    }

    /// Indicates that `obj` has changed and should be re-evaluated for placement in the octree
    ///
    /// If `obj` no longer fits in the octree, it remains in the root node
    pub fn update(&mut self, obj: &Rc<RefCell<Object>>) {
        if let Some(parent) = self.parent.upgrade() {
            if Self::get_octant_index(
                &parent.borrow().center,
                parent.borrow().h_width,
                obj,
            ) != Some(self.self_index)
            {
                self.objects.retain(|o| {
                    o.strong_count() > 0
                        && !Rc::ptr_eq(&o.upgrade().unwrap(), obj)
                });
                return parent.borrow_mut().insert(obj);
            }
        }
        if let Some(child_idx) =
            Self::get_octant_index(&self.center, self.h_width, obj)
        {
            if let Some(children) = self.children.as_mut() {
                self.objects.retain(|o| {
                    o.strong_count() > 0
                        && !Rc::ptr_eq(&o.upgrade().unwrap(), obj)
                });
                children[child_idx as usize].borrow_mut().insert(obj);
            }
        }
    }

    /// Gets all objects in the tree
    pub fn get_all_objects(&self) -> Vec<Rc<RefCell<Object>>> {
        let mut v = Vec::new();
        for obj in self.objects.iter().filter_map(std::rc::Weak::upgrade) {
            v.push(obj);
        }
        if let Some(children) = self.children.as_ref() {
            for c in children {
                v.append(&mut c.borrow().get_all_objects());
            }
        }
        v
    }
}

pub struct Octree {
    root: Rc<RefCell<ONode>>,
}

impl Octree {
    /// Inserts a node in the tree. If the node doesn't fit,
    /// ~~it stays at the root node~~ panics
    #[inline]
    pub fn insert(&mut self, obj: &Rc<RefCell<Object>>) {
        if obj.borrow().radius() > self.root.borrow().h_width * 2. {
            panic!("Cannot fit into tree");
        }
        self.root.borrow_mut().insert(obj);
    }

    /// Creates a new octree centered at `center` with a half width (width of child) as
    /// `half_side_len`
    pub fn new(center: Point3<f64>, half_side_len: f64) -> Self {
        let root = Rc::new(RefCell::new(ONode::new(center, half_side_len)));
        root.borrow_mut().self_ref = Rc::downgrade(&root);
        Self { root }
    }

    /// Get's all objects that have overlapping bounding spheres with `obj`
    pub fn get_colliders(
        obj: &Rc<RefCell<Object>>,
    ) -> Vec<Rc<RefCell<Object>>> {
        ONode::get_possible_colliders(obj)
    }

    pub fn remove(obj: &Rc<RefCell<Object>>) {
        if let Some(node) = obj.borrow().octree_cell.upgrade() {
            node.borrow_mut().objects.retain(|e| {
                e.strong_count() > 0 && !Rc::ptr_eq(&e.upgrade().unwrap(), obj)
            });
            if node.borrow().objects.is_empty() {
                if let Some(parent) = node.borrow().parent.upgrade() {
                    Self::maybe_make_leaf(&parent, &node);
                }
            }
        }
        obj.borrow_mut().octree_cell = Weak::default();
    }

    /// Checks the children of `node`. If they are empty, makes `node` a leaf.
    ///
    /// `initiator` - the child of `node` who just became empty and initiated the
    /// leaf check for `node`
    fn maybe_make_leaf(
        node: &Rc<RefCell<ONode>>,
        initiator: &Rc<RefCell<ONode>>,
    ) {
        for c in node.borrow().children.as_ref().unwrap() {
            if !(Rc::ptr_eq(c, initiator)
                || c.borrow().objects.is_empty()
                    && c.borrow().children.is_none())
            {
                return;
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

    /// Returns all objects that have overlapping bounding spheres with the given
    /// sphere
    #[inline]
    pub fn test_for_collisions(
        &self,
        center: Point3<f64>,
        radius: f64,
    ) -> Vec<Rc<RefCell<Object>>> {
        ONode::test_for_collisions(&self.root, center, radius)
    }

    /// Returns all objects in the tree
    #[inline]
    pub fn get_all_objects(&self) -> Vec<Rc<RefCell<Object>>> {
        self.root.borrow().get_all_objects()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::cg_support::node;
    use assertables::*;

    fn new_obj(center: Point3<f64>, radius: f64) -> Rc<RefCell<Object>> {
        Rc::new(RefCell::new(Object::new(
            Rc::new(RefCell::new(node::Node::new(
                Some(center),
                None,
                None,
                None,
            ))),
            radius,
        )))
    }

    fn random_obj(
        tree_center: Point3<f64>,
        tree_width: f64,
    ) -> Rc<RefCell<Object>> {
        use rand::Rng;
        let mut rnd = rand::thread_rng();
        let c = point3(
            tree_center.x + rnd.gen_range(-tree_width..tree_width),
            tree_center.y + rnd.gen_range(-tree_width..tree_width),
            tree_center.z + rnd.gen_range(-tree_width..tree_width),
        );
        new_obj(c, rnd.gen_range(0. ..tree_width))
    }

    fn random_pt(tree_center: Point3<f64>, tree_width: f64) -> Point3<f64> {
        use rand::Rng;
        let mut rnd = rand::thread_rng();
        point3(
            tree_center.x + rnd.gen_range(-tree_width..tree_width),
            tree_center.y + rnd.gen_range(-tree_width..tree_width),
            tree_center.z + rnd.gen_range(-tree_width..tree_width),
        )
    }

    fn random_radius(r: f64) -> f64 {
        use rand::Rng;
        let mut rnd = rand::thread_rng();
        rnd.gen_range(0. ..r)
    }

    #[test]
    fn root_no_split() {
        let mut ot = Octree::new(point3(0., 0., 0.), 25.);
        let o1 = new_obj(point3(10., 1., 1.), 4.);
        let o2 = new_obj(point3(-5., 2., 2.), 3.);
        ot.insert(&o1);
        ot.insert(&o2);
        assert_eq!(Octree::get_colliders(&o1).len(), 0);
        assert_eq!(Octree::get_colliders(&o2).len(), 0);
        assert!(o1.borrow().octree_cell.ptr_eq(&Rc::downgrade(&ot.root)));
    }

    #[test]
    fn collide_no_split() {
        let mut ot = Octree::new(point3(0., 0., 0.), 25.);
        let o1 = new_obj(point3(10., 1., 1.), 4.);
        let o2 = new_obj(point3(8., 2., 2.), 3.);
        ot.insert(&o1);
        ot.insert(&o2);
        assert!(Rc::ptr_eq(&Octree::get_colliders(&o1)[0], &o2));
        assert!(Rc::ptr_eq(&Octree::get_colliders(&o2)[0], &o1));
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
            new_obj(point3(0., -3., 3.), 2.),
        ];
        for o in &obj {
            ot.insert(o);
        }
        assert!(!obj[0].borrow().octree_cell.ptr_eq(&Rc::downgrade(&ot.root)));
        assert!(obj[6].borrow().octree_cell.ptr_eq(&Rc::downgrade(&ot.root)));
        assert!(obj[3].borrow().octree_cell.ptr_eq(&Rc::downgrade(
            &ot.root.borrow().children.as_ref().unwrap()[4]
        )));
        assert!(obj[2].borrow().octree_cell.ptr_eq(&Rc::downgrade(
            &ot.root.borrow().children.as_ref().unwrap()[6]
        )));
        assert!(obj[4].borrow().octree_cell.ptr_eq(&Rc::downgrade(
            &ot.root.borrow().children.as_ref().unwrap()[7]
        )));
        assert!(obj[7].borrow().octree_cell.ptr_eq(&Rc::downgrade(&ot.root)));
        assert_eq!(
            ONode::get_octant_index(&point3(0., 0., 0.), 25., &obj[1]),
            Some(0)
        );

        let root = &ot.root.borrow();
        for o in &obj {
            let oct = ONode::get_octant_index(&point3(0., 0., 0.), 25., o);
            assert!(o.borrow().octree_cell.ptr_eq(&Rc::downgrade(
                oct.map_or(&ot.root, |id| &root.children.as_ref().unwrap()
                    [id as usize])
            )));
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
            new_obj(point3(0., -3., 3.), 2.),
        ];
        for o in &obj {
            ot.insert(o);
        }
        Octree::remove(&obj[0]);
        assert_eq!(Octree::get_colliders(&obj[0]).len(), 0);
        Octree::remove(&obj[10]);
        assert_eq!(
            obj[10].borrow().octree_cell.as_ptr(),
            Weak::default().as_ptr()
        );

        for o in &obj {
            Octree::remove(o);
        }

        assert!(ot.root.borrow().children.is_none());
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
            new_obj(point3(0., -3., 3.), 2.),
        ];
        for o in &obj {
            ot.insert(o);
        }
        obj[0]
            .borrow_mut()
            .model
            .borrow_mut()
            .set_pos(point3(-6., 3., 3.));
        Octree::update(&obj[0]);
        assert!(obj[0].borrow().octree_cell.ptr_eq(&Rc::downgrade(
            &ot.root.borrow().children.as_ref().unwrap()[6]
        )));
        //let local_origin = obj[14].borrow().local_center;
        obj[14]
            .borrow_mut()
            .model
            .borrow_mut()
            .set_scale(vec3(0.1, 0.1, 0.1));
        //obj[14].borrow_mut().model.borrow_mut().anchor = local_origin;
        Octree::update(&obj[14]);
        assert!(obj[14].borrow().octree_cell.ptr_eq(&Rc::downgrade(
            &ot.root.borrow().children.as_ref().unwrap()[6]
        )));
        //let local_origin = obj[14].borrow().local_center;
        obj[1]
            .borrow_mut()
            .model
            .borrow_mut()
            .set_scale(vec3(5., 5., 5.));
        //obj[1].borrow().model.borrow_mut().anchor = local_origin;
        Octree::update(&obj[1]);
        assert!(obj[1].borrow().octree_cell.ptr_eq(&Rc::downgrade(&ot.root)));
    }

    #[test]
    fn upgrade_o_index_test() {
        let trans = Rc::new(RefCell::new(node::Node::new(
            Some(point3(-3., -3., -3.)),
            None,
            None,
            None,
        )));
        let obj = Object::new(trans.clone(), 1.);
        let obj = Rc::new(RefCell::new(obj));
        assert_eq!(
            ONode::get_octant_index(&point3(0., 0., 0.), 10., &obj),
            Some(0)
        );
        assert_eq!(
            ONode::get_octant_index(&point3(-5., -5., -5.), 5., &obj),
            Some(7)
        );
        trans.borrow_mut().set_rot(From::from(Euler::new(
            Deg(10f64),
            Deg(0.),
            Deg(30f64),
        )));
        trans.borrow_mut().set_scale(vec3(10., 3., 1.));
        trans.borrow_mut().set_pos(point3(-20., -20., -20.));
        assert_eq!(
            ONode::get_octant_index(&point3(0., 0., 0.), 10., &obj),
            None
        );
        trans.borrow_mut().set_scale(vec3(10., 3., 1.));
        trans.borrow_mut().set_pos(point3(-20., -20., -20.));
    }

    #[test]
    fn parent_colliders_test() {
        let mut ot = Octree::new(point3(0., 0., 0.), 25.);
        let parent_collider = new_obj(point3(0., 0., 0.), 20.);
        let obj = [
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(-3., -3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 2.),
            new_obj(point3(-10., -3., 3.), 2.),
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(4., 3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 2.),
            new_obj(point3(0., -3., 3.), 2.),
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(-3., -3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 2.),
            new_obj(point3(-10., -3., 3.), 2.),
            new_obj(point3(3., 3., 3.), 2.),
            new_obj(point3(4., 3., -3.), 2.),
            new_obj(point3(-5., 3., 3.), 2.),
            new_obj(point3(0., -3., 3.), 2.),
            parent_collider.clone(),
        ];
        for o in &obj {
            ot.insert(o);
        }
        assert!(Octree::get_colliders(&obj[0])
            .iter()
            .any(|x| Rc::ptr_eq(x, &parent_collider)));
    }

    #[test]
    fn randomized_test() {
        use rand::seq::SliceRandom;
        use rand::thread_rng;
        let mut tree = Octree::new(point3(0., 0., 0.), 200.);
        let mut objs: Vec<Rc<RefCell<Object>>> = (0..300)
            .map(|_| {
                let o = random_obj(point3(0., 0., 0.), 200.);
                tree.insert(&o);
                o
            })
            .collect();
        for o in &objs {
            let colliders: Vec<*const Object> = objs
                .iter()
                .filter(|e| {
                    !Rc::ptr_eq(o, e)
                        && e.borrow().bounding_sphere_collide(&*o.borrow())
                })
                .map(|x| x.as_ptr() as *const Object)
                .collect();
            let tree_colliders: Vec<*const Object> = Octree::get_colliders(o)
                .iter()
                .map(|x| x.as_ptr() as *const Object)
                .collect();
            assert_bag_eq!(colliders, tree_colliders);
        }
        objs.shuffle(&mut thread_rng());
        for i in objs.iter().take(100) {
            i.borrow()
                .model
                .borrow_mut()
                .set_pos(random_pt(point3(0., 0., 0.), 200.));
            i.borrow_mut().local_radius = random_radius(200.);
            Octree::update(i);
        }
        for o in &objs {
            let colliders: Vec<*const Object> = objs
                .iter()
                .filter(|e| {
                    !Rc::ptr_eq(o, e)
                        && e.borrow().bounding_sphere_collide(&*o.borrow())
                })
                .map(|x| x.as_ptr() as *const Object)
                .collect();
            let tree_colliders: Vec<*const Object> = Octree::get_colliders(o)
                .iter()
                .map(|x| x.as_ptr() as *const Object)
                .collect();
            assert_bag_eq!(colliders, tree_colliders);
        }
    }
}
