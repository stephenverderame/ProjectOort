use super::{to_m4, Lerp};
use assimp::*;
use cgmath::Quaternion;
use cgmath::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
pub struct Bone {
    pub id: i32,
    /// Matrix to transform a vector into bone space
    pub offset_matrix: Matrix4<f64>,
}

/// Stores the sequence of keyframes for a particular bone
struct BoneAnim {
    /// vector of pos keyframes and tick tuples
    positions: Vec<(Vector3<f64>, f64)>,
    rotations: Vec<(Quaternion<f64>, f64)>,
    scales: Vec<(Vector3<f64>, f64)>,
    //id: i32,
    last_pos: RefCell<usize>, //interior mutability
    last_rot: RefCell<usize>,
    last_scale: RefCell<usize>,
}

impl BoneAnim {
    /// `id` - the id of the given Bone this represents
    fn new(anim: &scene::NodeAnim) -> BoneAnim {
        let mut positions = Vec::<(Vector3<f64>, f64)>::new();
        let mut scales = Vec::<(Vector3<f64>, f64)>::new();
        let mut rotations = Vec::<(Quaternion<f64>, f64)>::new();
        for pos_idx in 0..(*anim).num_position_keys {
            let key = anim.get_position_key(pos_idx as usize).unwrap();
            positions.push((
                vec3(key.value.x, key.value.y, key.value.z).cast().unwrap(),
                key.time,
            ));
        }
        for rot_idx in 0..(*anim).num_rotation_keys {
            let key = anim.get_rotation_key(rot_idx as usize).unwrap();
            rotations.push((
                Quaternion::new(key.value.w, key.value.x, key.value.y, key.value.z)
                    .cast()
                    .unwrap(),
                key.time,
            ));
        }
        for scale_idx in 0..(*anim).num_scaling_keys {
            let key = anim.get_scaling_key(scale_idx as usize).unwrap();
            scales.push((
                vec3(key.value.x, key.value.y, key.value.z).cast().unwrap(),
                key.time,
            ));
        }
        BoneAnim {
            positions,
            rotations,
            scales, //id,
            last_pos: RefCell::new(0),
            last_rot: RefCell::new(0),
            last_scale: RefCell::new(0),
        }
    }

    /// Gets the last keyframe, next keyframe, `0 - 1` factor to lerp between the two, and index of last keyframe
    /// Requires that `anim_time >= vec[last_idx].1` and that `vec.len() >= 2`
    ///
    /// `last_idx` - the last used keyframe in `vec`. Will start searching for the next keyframe in `vec` from
    /// `last_idx`. Panics if there is no next keyframe
    fn get_last_next_lerp<T: Clone>(
        vec: &Vec<(T, f64)>,
        anim_time: f64,
        last_idx: usize,
    ) -> (T, T, f64, usize) {
        for idx in last_idx..vec.len() - 1 {
            if anim_time < vec[idx + 1].1 {
                let lerp_fac = (anim_time - vec[idx].1) / (vec[idx + 1].1 - vec[idx].1);
                return (vec[idx].0.clone(), vec[idx + 1].0.clone(), lerp_fac, idx);
            }
        }
        panic!("Animation out of bounds!")
    }

    /// Interpolates between the previous and next keyframe in `keyframes`, updating `last_idx` so that if
    /// `anim_ticks < keyframes[last_idx].1`, then `last_idx = 0` as this indicates that the animation looped.
    ///
    /// The animation does not interpolate between the last keyframe and the first if the animation loops
    fn get_cur<T: Lerp<Numeric = f64> + Clone>(
        anim_ticks: f64,
        keyframes: &Vec<(T, f64)>,
        last_idx: &mut usize,
    ) -> T {
        if keyframes.len() == 1 {
            return keyframes[0].0.clone();
        }
        if anim_ticks < keyframes[*last_idx].1 || *last_idx == keyframes.len() - 1 {
            *last_idx = 0;
        }
        let (last, next, lerp_fac, new_last) =
            BoneAnim::get_last_next_lerp(keyframes, anim_ticks, *last_idx);
        *last_idx = new_last;
        Lerp::lerp(last, next, lerp_fac)
    }

    /// Interpolates to get current position for `anim_ticks`. Updates
    /// `self.last_pos`, looping if necessary
    #[inline(always)]
    fn get_cur_pos(&self, anim_ticks: f64) -> Vector3<f64> {
        BoneAnim::get_cur(
            anim_ticks,
            &self.positions,
            &mut *self.last_pos.borrow_mut(),
        )
    }

    /// Interpolates to get current scale for `anim_ticks`. Updates
    /// `self.last_scale`, looping if necessary
    #[inline(always)]
    fn get_cur_scale(&self, anim_ticks: f64) -> Vector3<f64> {
        BoneAnim::get_cur(anim_ticks, &self.scales, &mut *self.last_scale.borrow_mut())
    }

    /// Interpolates to get current rotation for `anim_ticks`. Updates
    /// `self.last_rot`, looping if necessary
    #[inline(always)]
    fn get_cur_rot(&self, anim_ticks: f64) -> Quaternion<f64> {
        BoneAnim::get_cur(
            anim_ticks,
            &self.rotations,
            &mut *self.last_rot.borrow_mut(),
        )
    }

    /// Gets the current transformation matrix for the bone at the time
    /// `anim_ticks`
    pub fn get_bone_matrix(&self, anim_ticks: f64) -> Matrix4<f64> {
        let scale = self.get_cur_scale(anim_ticks);
        Matrix4::from_translation(self.get_cur_pos(anim_ticks))
            * Matrix4::from(self.get_cur_rot(anim_ticks))
            * Matrix4::from_nonuniform_scale(scale.x, scale.y, scale.z)
    }
}

/// Encapsulates transformation information of an AiNode.
/// Essentially represents a node in the scene graph for the model
pub struct AssimpNode {
    transformation: Matrix4<f64>,
    name: String,
    children: Vec<AssimpNode>,
}

impl AssimpNode {
    /// Creates a new scene heirarchy tree from a scene graph node and all its descendants
    pub fn new(node: &assimp::Node) -> AssimpNode {
        AssimpNode {
            name: node.name().to_owned(),
            transformation: to_m4(*node.transformation()),
            children: node.child_iter().map(|c| AssimpNode::new(&c)).collect(),
        }
    }
}

/// Stores a single animation
struct Animation {
    ticks_per_sec: f64,
    duration: f64,
    root_node: Rc<AssimpNode>,
    bone_map: Rc<HashMap<String, Bone>>,
    name: String,
    anim_bones: HashMap<String, BoneAnim>,
    root_inverse: Matrix4<f64>,
}

impl Animation {
    /// Requires there are no missing bones from `bone_map`
    pub fn new(
        anim: &assimp::Animation,
        root_node: Rc<AssimpNode>,
        bone_map: Rc<HashMap<String, Bone>>,
    ) -> Animation {
        let mut used_bones = HashMap::<String, BoneAnim>::new();
        println!("New animation named: `{}`", anim.name.as_ref());
        for i in 0..anim.num_channels as usize {
            let node = anim.get_node_anim(i).unwrap();
            //let bone_info = bone_map.get((*node).node_name.as_ref()).unwrap();
            used_bones.insert((*node).node_name.as_ref().to_owned(), BoneAnim::new(&node));
        }
        Animation {
            root_inverse: root_node.transformation.invert().unwrap(),
            ticks_per_sec: if anim.ticks_per_second == 0. {
                25.
            } else {
                anim.ticks_per_second
            },
            duration: anim.duration,
            name: anim.name.as_ref().to_owned(),
            root_node,
            bone_map,
            anim_bones: used_bones,
        }
    }

    /// Plays the animations and gets the bone matrices
    ///
    /// `dt` - seconds since animations has begun. If `dt > duration`
    /// animation loops to beginning
    fn play(&self, dt: f64) -> Vec<Matrix4<f32>> {
        let ticks = self.ticks_per_sec * dt;
        let iterations = (ticks / self.duration).floor() as i32;
        let ticks = ticks - iterations as f64 * self.duration;

        let mut final_mats = Vec::<Matrix4<f32>>::new();
        final_mats.resize(self.bone_map.len(), Matrix4::from_scale(1.));
        let identity = Matrix4::from_scale(1f64);
        self.get_bone_transforms(ticks, &self.root_node, &identity, &mut final_mats);
        final_mats
    }

    /// Computes the bone transforms recursively done the node tree and stores them in `out_bone_matrices`
    ///
    /// `parent_transform` - the matrix to transfrom from parent space to world space
    ///
    /// `out_bone_matrices` - the vector storing final bone transformation matrices. Required to have size equal
    /// to the number of bones
    ///
    /// `anim_time` - the duration the animation has been running. Required to be between `0` and `duration`
    fn get_bone_transforms(
        &self,
        anim_time: f64,
        ai_node: &AssimpNode,
        parent_transform: &Matrix4<f64>,
        out_bone_matrices: &mut Vec<Matrix4<f32>>,
    ) {
        let bone_transform = match self.anim_bones.get(&ai_node.name) {
            Some(bone) => bone.get_bone_matrix(anim_time),
            _ => ai_node.transformation,
        };
        let to_world_space = parent_transform * bone_transform;

        match self.bone_map.get(&ai_node.name) {
            Some(bone_info) => {
                out_bone_matrices[bone_info.id as usize] =
                    (self.root_inverse * to_world_space * bone_info.offset_matrix)
                        .cast()
                        .unwrap();
            }
            None => (),
        }

        for child in ai_node.children.iter().map(|c| &*c) {
            self.get_bone_transforms(anim_time, child, &to_world_space, out_bone_matrices);
        }
    }

    /// True if `anim_sec` exceeds the duration of a single play of the animation
    #[inline(always)]
    fn is_finished(&self, anim_sec: f64) -> bool {
        anim_sec * self.ticks_per_sec > self.duration
    }
}

/// An animator holds all the animations of a single model and handles playing and stopping them.
/// Only one animation can play at a time
pub struct Animator {
    animations: Vec<Animation>,
    cur_anim: Option<usize>,
    anim_start: std::time::Instant,
    play_loop: bool,
}

impl Animator {
    pub fn new(
        anims: assimp::scene::AnimationIter,
        bone_map: Rc<HashMap<String, Bone>>,
        root_node: Rc<AssimpNode>,
    ) -> Animator {
        let total_anims: Vec<Animation> = anims
            .map(|x| Animation::new(&x, root_node.clone(), bone_map.clone()))
            .collect();
        Animator {
            cur_anim: None,
            animations: total_anims,
            anim_start: std::time::Instant::now(),
            play_loop: true,
        }
    }

    /// Plays the current animation, if any, and gets the bone matrices
    /// If no animations is playing, returns `None`
    pub fn animate(&self, frame_time: std::time::Instant) -> Option<Vec<[[f32; 4]; 4]>> {
        match self.cur_anim {
            Some(cur_anim) => {
                let anim_sec = frame_time.duration_since(self.anim_start).as_secs_f64();
                if self.play_loop || !self.animations[cur_anim].is_finished(anim_sec) {
                    Some(
                        self.animations[cur_anim]
                            .play(anim_sec)
                            .into_iter()
                            .map(|x| x.into())
                            .collect(),
                    )
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Starts an animation with the name `anim_name`. Panics if no animation with that name is found.
    /// Will interrupt itself if it is already playing and any other animation currently being played
    #[allow(dead_code)]
    pub fn start(&mut self, anim_name: &str, do_loop: bool) {
        for (anim, idx) in self.animations.iter().zip(0..self.animations.len()) {
            if anim.name == anim_name {
                self.play_loop = do_loop;
                self.cur_anim = Some(idx);
                self.anim_start = std::time::Instant::now();
                return;
            }
        }
        panic!("Animation '{}' not found", anim_name);
    }

    /// Stops any animation playing, resetting model position
    #[inline]
    #[allow(dead_code)]
    pub fn stop(&mut self) {
        self.cur_anim = None;
    }
}
