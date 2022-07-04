use super::super::drawable::*;
use glium::*;
use super::shader;
use std::collections::HashMap;
use shader::PipelineCache;
use super::*;

/// A RenderPass is a render target followed by a series of texture transformations.
/// A renderpass is rendered to and produces a texture result
pub struct RenderPass {
    targets: Vec<Box<dyn RenderTarget>>,
    processes: Vec<Box<dyn TextureProcessor>>,
    topo_order: Vec<u16>,
    pipeline: Pipeline,
    active_func: Option<Box<dyn Fn(u16) -> bool>>,
}

impl RenderPass {
    /// Creates a new RenderPass
    /// 
    /// The first `[0, targets.len())` ids refer to render targets. Then the next `[targets.len(), processes.len())` ids refer
    /// to processes. Therefore, the pipeline must contain nodes from `0` to `targets.len() + processes.len()` with the upper bound being exclusive
    pub fn new(targets: Vec<Box<dyn RenderTarget>>, processes: Vec<Box<dyn TextureProcessor>>, pipeline: Pipeline) -> RenderPass {
        RenderPass { targets, processes, topo_order: pipeline.topo_order(), pipeline, active_func: None }
    }

    /// Sets the conditional stage predicate to the render pass.
    /// If no predicate is supplied for the Render Pass, all stages are used
    /// 
    /// `pred` must return `true` if the stage id passed to it should be run
    pub fn with_active_pred(mut self, pred: Box<dyn Fn(u16) -> bool>) -> Self {
        self.active_func = Some(pred);
        self
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
        render_func: &mut dyn FnMut(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer, 
            shader::RenderPassType, &PipelineCache, TargetType, &Option<Vec<&TextureType>>)) -> Option<TextureType>
    {
        let mut saved_textures = Vec::<TextureType>::new();
        let mut registers = HashMap::<u16, Vec<usize>>::new();
        let mut final_out : Option<usize> = None;
        let targets_len = self.targets.len();
        let mut cache = PipelineCache::default();
        let mut tex_count : usize = 0;
        let tex_buf_ptr = &mut saved_textures as *mut Vec<TextureType>;
        for node in &self.topo_order {
            if let Some(pred) = &self.active_func {
                if !pred(*node) { continue; }
            }
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
                // - targets_len because the first 0 .. targets_len indexes are render targets
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