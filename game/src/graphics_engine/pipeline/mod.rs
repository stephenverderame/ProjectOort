mod render_pass;
pub mod render_target;
pub mod texture_processor;
use glium::*;
use super::shader;
use shader::PipelineCache;
use shader::RenderPassType;
use super::drawable::*;
use std::collections::HashMap;
use std::collections::HashSet;

pub use render_pass::*;

/// Either a `T` or `&T`
pub enum Ownership<'a, T> {
    Own(T),
    Ref(&'a T),
}

impl<'a, T> Ownership<'a, T> {
    /// Gets a reference of the data, regardless of the onwership type
    pub fn to_ref(&self) -> &T {
        match &self {
            Own(s) => s,
            Ref(s) => s,
        }
    }
}

use Ownership::*;

pub enum StageArgs {
    Cascade([[f32; 4]; 4], f32),
    Object(u32),
    Compositor([[f32; 3]; 3]),
}

/// The type of texture returned by a pipeline stage
pub enum TextureType<'a> {
    Tex2d(Ownership<'a, texture::Texture2d>),
    Depth2d(Ownership<'a, texture::DepthTexture2d>),
    TexCube(Ownership<'a, texture::Cubemap>),
    #[allow(dead_code)]
    Bindless(texture::ResidentTexture),
    WithArg(Box<TextureType<'a>>, StageArgs),
    Multi(Vec<TextureType<'a>>),
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum TargetType {
    Msaa,
    Cubemap,
    Mipcube,
    Depth,
}

/// A RenderTarget is something that can be rendered to and produces a texture
pub trait RenderTarget {
    /// Draws to the render target by passing a framebuffer to `func`. Must be called before `read()`.
    /// 
    /// `viewer` - the viewer for this render. May or may not be passed verbatim to `func`
    /// 
    /// `pipeline_inputs` - any texture inputs to this render target from the pipeline
    /// 
    /// `func` - the function called to render to the render target. Passed the render target
    /// framebuffer, viewer, type of the render target, and any pipeline inputs to this render target
    /// 
    /// Returns the texture output of rendering to this render target
    fn draw(&mut self, viewer: &dyn Viewer, pipeline_inputs: Option<Vec<&TextureType>>,
        cache: &mut PipelineCache,
        func: &mut dyn FnMut(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer, RenderPassType,
        &PipelineCache, TargetType, &Option<Vec<&TextureType>>)) -> Option<TextureType>;

    /// Gets the type of this RenderTarget
    fn type_of(&self) -> TargetType;
}

/// A TextureProcessor transforms input textures into an output texture. It is basically
/// a function on textures
pub trait TextureProcessor {
    /// `source` - input textures for the processor
    /// 
    /// `shader` - shader manager
    /// 
    /// `data` - the scene data for the processor or `None`
    fn process<'a>(&mut self, source: Option<Vec<&'a TextureType>>, shader: &shader::ShaderManager,
        cache: &mut PipelineCache<'a>, data: Option<&shader::SceneData>) -> Option<TextureType>;
}

/// A pipeline is a connected DAG with start nodes. Pipeline stores the indices of
/// transformations in a RenderPass
pub struct Pipeline {
    pub starts: Vec<u16>,
    pub adj_list: HashMap<u16, Vec<(u16, usize)>>,
}

impl Pipeline {
    /// Creates a new pipleline from a **connected DAG**.
    /// 
    /// **Requires**: pipeline node indexes are consecutive. That is to say that if there is an edge `(0, 10)`,
    /// there must be a node `10` and must have nodes `0 - 9`.
    /// ## Arguments
    /// `starts` - a vector of the start node id's
    /// 
    /// `edges` - a set of edges `(u, (v, idx))` that indicates a directed edge from `u` to `v`. Where
    /// `u` and `v` are indexes of nodes. `idx` is the index of `v`s input list that the output from `u` will
    /// be sent to. Requires that all consecutive inputs are used. 
    pub fn new(starts: Vec<u16>, edges: Vec<(u16, (u16, usize))>) -> Pipeline {
        Pipeline {
            starts,
            adj_list: Pipeline::to_adj_list(edges),
        }
    }

    /// Creates an adjacency list for the graph defined by the edge set `edges`
    fn to_adj_list(edges: Vec<(u16, (u16, usize))>) -> HashMap<u16, Vec<(u16, usize)>> {
        let mut adj_list = HashMap::<u16, Vec<(u16, usize)>>::new();
        for (u, v) in edges {
            match adj_list.get_mut(&u) {
                Some(lst) => lst.push(v),
                None => {
                    adj_list.insert(u, vec![v]);
                },
            }
        }
        adj_list
    }
    /// Topologically sorts the graph starting from `node`
    /// # Arguments
    /// `node` - starting node
    /// 
    /// `order` - the reverse topological order. Results are stored here
    /// 
    /// `discovered` - the set of all nodes that have been discovered
    fn topo_sort(&self, node: u16, order: &mut Vec<u16>, discovered: &mut HashSet<u16>) {
        match self.adj_list.get(&node) {
            Some(neighbors) => {
                for (ns, _) in neighbors {
                    if discovered.get(ns).is_none() {
                        discovered.insert(*ns);
                        self.topo_sort(*ns, order, discovered);
                    }
                }
            },
            _ => ()
        };
        order.push(node);
    }

    /// Gets the topological order of the pipeline
    fn topo_order(&self) -> Vec<u16> {
        let mut order = Vec::<u16>::new();
        let mut discovered = HashSet::<u16>::new();
        for start in &self.starts {
            self.topo_sort(*start, &mut order, &mut discovered);
        }     
        order.iter().rev().map(|x| *x).collect()

    }
}

macro_rules! pipeline_map_and_list {
    ([$($target:ident),+], [$($process:ident),*], $($stage_a:ident -> $stage_b:ident.$b_in:expr),*) => {{
        let mut map = std::collections::HashMap::<String, u16>::new();
        let mut _id : u16 = 0;
        $(
            map.insert(stringify!($target).to_string(), _id);
            _id += 1;
        )*
        $(
            map.insert(stringify!($process).to_string(), _id);
            _id += 1;
        )*
        let mut adj_list : Vec<(u16, (u16, usize))> = Vec::new();
        $(
            adj_list.push((map[stringify!($stage_a)], 
            (map[stringify!($stage_b)], $b_in)));
        )*
        (map, adj_list)
    }};
}

/// Constructs a pipeline using a DSL
/// 
/// Expects `[<render_targets>], [<texture_processors>], <stage_a> -> <stage_b>.<b_in>`
/// where `b_in` is the number of the input to put the output of `stage_a` into `stage_b`
/// 
/// The first render target will automatically be the starting stage
/// 
/// The stage flow block can be followed by `{}`, which can contain expressions for conditional stages
/// For example: `{ stage_a | stage_b if 1 > var }` which will execute `stage_a` and `stage_b`
/// only if `1 > var`. Any variables used in the if expression will be moved into the underlying active
/// predicate closure
macro_rules! pipeline {
    ([$($target:ident),+], [$($process:ident),*], $($stage_a:ident -> $stage_b:ident.$b_in:expr),*) => {{
            let (_, adj_list) = pipeline_map_and_list!([$($target),+], [$($process),*], $($stage_a -> $stage_b.$b_in),*);
            RenderPass::new(vec![$($target),*], vec![$($process),*], 
                Pipeline::new(vec![0], adj_list))
    }};
    ([$($target:ident),+], [$($process:ident),*], $($stage_a:ident -> $stage_b:ident.$b_in:expr),*,
        {$($($conditional_stage:ident)|+ if $condition:expr),*}) => {{
        let (map, adj_list) = pipeline_map_and_list!([$($target),+], [$($process),*], $($stage_a -> $stage_b.$b_in),*);
        RenderPass::new(vec![$($target),*], vec![$($process),*], 
            Pipeline::new(vec![0], adj_list)).with_active_pred(Box::new(move |stage| {
                match stage {
                    $(
                        x if ($(x == map[stringify!($conditional_stage)] ||)* false) && $condition => true,
                        x if ($(x == map[stringify!($conditional_stage)] ||)* false) => false,
                    )*
                    _ => true,
                }
            }))
    }};
}