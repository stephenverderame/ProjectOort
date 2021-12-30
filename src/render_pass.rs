use crate::render_target::*;
use crate::draw_traits::*;
use glium::*;
use crate::shader;
use std::collections::HashMap;
use std::collections::HashSet;

/// A pipeline is a connected DAG with start nodes. Pipeline stores the indices of
/// transformations in a RenderPass
pub struct Pipeline {
    pub starts: Vec<u16>,
    pub adj_list: HashMap<u16, Vec<u16>>,
}

impl Pipeline {
    /// Creates a new pipleline from a **connected DAG**.
    /// 
    /// **Requires**: pipeline node indexes are consecutive. That is to say that if there is an edge `(0, 10)`,
    /// there must be a node `10` and must have nodes `0 - 9`.
    /// ## Arguments
    /// `starts` - a vector of the start node id's
    /// 
    /// `edges` - a set of edges `(u, v)` that indicates a directed edge from `u` to `v`. Where
    /// `u` and `v` are indexes of nodes.
    pub fn new(starts: Vec<u16>, edges: Vec<(u16, u16)>) -> Pipeline {
        Pipeline {
            starts,
            adj_list: Pipeline::to_adj_list(edges),
        }
    }

    /// Creates an adjacency list for the graph defined by the edge set `edges`
    fn to_adj_list(edges: Vec<(u16, u16)>) -> HashMap<u16, Vec<u16>> {
        let mut adj_list = HashMap::<u16, Vec<u16>>::new();
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
                for ns in neighbors {
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

/// A RenderPass is a render target followed by a series of texture transformations.
/// A renderpass is rendered to and produces a texture result
pub struct RenderPass<'a> {
    targets: Vec<&'a mut dyn RenderTarget>,
    processes: Vec<&'a mut dyn TextureProcessor>,
    topo_order: Vec<u16>,
    pipeline: Pipeline,
}

macro_rules! get_inputs {
    ($node:expr) => {
        registers.get($node).map(|input_indices| {
            input_indices.iter().map(|idx| { &saved_textures[*idx] }).collect::<Vec<&TextureType>>()
        });
    };
}

impl<'a> RenderPass<'a> {
    /// Creates a new RenderPass
    /// 
    /// The `0`th node id in the pipeline refers to the render target and the id `1` refers to index `0` in `processes`.
    /// The Pipeline DAG must contain nodes ids in `[0, processes.len()]`
    pub fn new(targets: Vec<&'a mut dyn RenderTarget>, processes: Vec<&'a mut dyn TextureProcessor>, pipeline: Pipeline) -> RenderPass<'a> {
        RenderPass { targets, processes, topo_order: pipeline.topo_order(), pipeline }
    }

    /// Stores the index of the texture result `v`, in the stored results for the node `k`.
    /// The stored results are in `registers`
    fn add_to_reg(registers: &mut HashMap<u16, Vec<usize>>, k: u16, v: usize) {
        match registers.get_mut(&k) {
            Some(vals) => vals.push(v),
            None => {
                registers.insert(k, vec![v]);
            }
        }
    }
    /// Saves the texture result saved at index `val` into the registers of all
    /// adjacent nodes of `node`. That is, saves `val` after it has been produced
    /// by `node`. Returns `val` if `node` has no outgoing edges, otherwise `None`.
    /// 
    /// `pipeline` - the pipeline DAG
    /// 
    /// `registers` - the pipeline registers
    fn save_stage_out(registers: &mut HashMap<u16, Vec<usize>>, 
        val: usize, node: u16, pipeline: &Pipeline) -> Option<usize>
    {
       match pipeline.adj_list.get(&node) {
           Some(neighbors) => {
               neighbors.iter().for_each(
                   |v| RenderPass::add_to_reg(registers, *v, val));
               None
           },
           None => Some(val),
       }

    }

    fn get_inputs<'b>(registers: &HashMap<u16, Vec<usize>>, saved_textures: &'b Vec<TextureType>, node: u16) 
        -> Option<Vec<&'b TextureType<'b>>>
    {
        registers.get(&node).map(|input_indices| {
            input_indices.iter().map(|idx| { &saved_textures[*idx] }).collect::<Vec<&TextureType>>()
        })
    }

    /// Calls the render function, saving the results to the render target
    /// Then runs the render target through the process pipeline until it procudes a texture
    pub fn run_pass(&mut self, viewer: &dyn Viewer, shader: &shader::ShaderManager, sdata: &shader::SceneData,
        render_func: &dyn Fn(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer, RenderTargetType, &Option<Vec<&TextureType>>)) -> TextureType 
    {
        let mut saved_textures = Vec::<TextureType>::new();
        let mut registers = HashMap::<u16, Vec<usize>>::new();
        let mut final_out : Option<usize> = None;
        let targets_len = self.targets.len();
        for node in &self.topo_order {
            let unode = *node as usize;
            if unode < targets_len {
                let index = unode;
                let inputs = RenderPass::get_inputs(&registers, &saved_textures, *node);
                let idx_ptr = self.targets.as_mut_ptr();
                unsafe {
                    let elem = idx_ptr.add(index);
                    saved_textures.push((*elem).draw(viewer, inputs, render_func));
                }
                final_out = RenderPass::save_stage_out(
                    &mut registers, saved_textures.len() - 1, *node, &self.pipeline);
            } else {
                let index = unode - targets_len;
                // -1 because index 0 is the render target
                match registers.get(&node) {
                    Some(data) => {
                        let process_input = RenderPass::get_inputs(&registers, &saved_textures, *node);
                        let idx_ptr = self.processes.as_mut_ptr();
                        unsafe {
                            // need to use pointers because compiler can't know that we're borrowing
                            // different elements of the vector
                            let elem = idx_ptr.add(index);
                            let tex = (*elem).process(process_input.unwrap(), shader, Some(sdata));
                            saved_textures.push(tex);
                        }
                        final_out = RenderPass::save_stage_out(&mut registers, 
                            saved_textures.len() - 1, *node, &self.pipeline);
                    },
                    None => {
                        let idx_ptr = self.processes.as_mut_ptr();
                        unsafe {
                            let elem = idx_ptr.add(index);
                            let tex = (*elem).process(Vec::<&TextureType>::new(), shader, Some(sdata));
                            saved_textures.push(tex);
                        }
                        final_out = RenderPass::save_stage_out(&mut registers, 
                            saved_textures.len() - 1, *node, &self.pipeline);
                    },
                }
            }
        }
        final_out.map(|tex_idx| saved_textures.swap_remove(tex_idx)).unwrap()
    }
}