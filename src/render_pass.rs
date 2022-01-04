use crate::render_target::*;
use crate::draw_traits::*;
use glium::*;
use crate::shader;
use std::collections::HashMap;
use std::collections::HashSet;
use shader::PipelineCache;

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

/// A RenderPass is a render target followed by a series of texture transformations.
/// A renderpass is rendered to and produces a texture result
pub struct RenderPass<'a> {
    targets: Vec<&'a mut dyn RenderTarget>,
    processes: Vec<&'a mut dyn TextureProcessor>,
    topo_order: Vec<u16>,
    pipeline: Pipeline,
}

impl<'a> RenderPass<'a> {
    /// Creates a new RenderPass
    /// 
    /// The first `[0, targets.len())` ids refer to render targets. Then the next `[targets.len(), processes.len())` ids refer
    /// to processes. Therefore, the pipeline must contain nodes from `0` to `targets.len() + processes.len()` with the upper bound being exclusive
    pub fn new(targets: Vec<&'a mut dyn RenderTarget>, processes: Vec<&'a mut dyn TextureProcessor>, pipeline: Pipeline) -> RenderPass<'a> {
        RenderPass { targets, processes, topo_order: pipeline.topo_order(), pipeline }
    }

    /// Stores the index of the texture result `v`, in the stored results for the node `k`.
    /// The stored results are in `registers`
    /// 
    /// `k` the `(node, parameter index)` pair that will store `v`. So `v` will be the `k.1`th input argument
    /// to `k.0`
    fn add_to_reg(registers: &mut HashMap<u16, Vec<usize>>, k: (u16, usize), v: usize) {
        match registers.get_mut(&k.0) {
            Some(vals) => {
                if vals.len() > k.1 {
                    assert_eq!(vals[k.1], !(1 as usize));
                    vals[k.1] = v;
                } else {
                    while vals.len() < k.1 {
                        vals.push(!(1 as usize));
                    }
                    vals.push(v)
                }
            },
            None => {
                let mut vc = Vec::<usize>::new();
                while vc.len() < k.1 {
                    vc.push(!(1 as usize));
                }
                vc.push(v);
                registers.insert(k.0, vc);
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

    /// Gets the inputs for `node`, or `None` if there are none saved
    fn get_inputs<'b>(registers: &HashMap<u16, Vec<usize>>, saved_textures: &'b Vec<TextureType<'b>>, node: u16) 
        -> Option<Vec<&'b TextureType<'b>>>
    {
        registers.get(&node).map(|input_indices| {
            input_indices.iter().map(|idx| { &saved_textures[*idx] }).collect::<Vec<&TextureType>>()
        })
    }

    /// Calls the render function, saving the results to the render target
    /// Then runs the render target through the process pipeline until it procudes a texture
    pub fn run_pass(&mut self, viewer: &dyn Viewer, shader: &shader::ShaderManager, sdata: std::rc::Rc<std::cell::RefCell<shader::SceneData>>,
        render_func: &dyn Fn(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer, 
            shader::RenderPassType, &PipelineCache, &Option<Vec<&TextureType>>)) -> Option<TextureType>
    {
        let mut saved_textures = Vec::<TextureType>::new();
        let mut registers = HashMap::<u16, Vec<usize>>::new();
        let mut final_out : Option<usize> = None;
        let targets_len = self.targets.len();
        let mut cache = PipelineCache::default();
        let mut tex_count : usize = 0;
        let tex_buf_ptr = &mut saved_textures as *mut Vec<TextureType>;
        for node in &self.topo_order {
            let unode = *node as usize;
            #[allow(unused_assignments)]
            let mut stage_out_tex : Option<TextureType> = None;
            if unode < targets_len {
                let index = unode;
                let inputs = RenderPass::get_inputs(&registers, &saved_textures, *node);
                let idx_ptr = self.targets.as_mut_ptr();
                unsafe {
                    let elem = idx_ptr.add(index);
                    stage_out_tex = (*elem).draw(viewer, inputs, &mut cache, render_func);              
                }
            } else {
                let index = unode - targets_len;
                // -1 because index 0 is the render target
                let process_input = RenderPass::get_inputs(&registers, &saved_textures, *node);
                let idx_ptr = self.processes.as_mut_ptr();
                let sd = sdata.borrow();
                unsafe {
                    // need to use pointers because compiler can't know that we're borrowing
                    // different elements of the vector
                    let elem = idx_ptr.add(index);
                    stage_out_tex = (*elem).process(process_input, shader, &mut cache, Some(&*sd));
                }
            }
            if stage_out_tex.is_some() {
                unsafe { (*tex_buf_ptr).push(stage_out_tex.unwrap()); }
                final_out = RenderPass::save_stage_out(
                    &mut registers, tex_count, *node, &self.pipeline);
                tex_count += 1;
            }    
        }
        final_out.map(|tex_idx| unsafe { (*tex_buf_ptr).swap_remove(tex_idx) })
    }
}