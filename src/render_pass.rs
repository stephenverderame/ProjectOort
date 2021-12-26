use crate::render_target::*;
use crate::draw_traits::*;
use glium::*;
use crate::shader;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::collections::HashSet;

pub struct Pipeline {
    pub starts: Vec<u16>,
    pub adj_list: HashMap<u16, Vec<u16>>,
}

impl Pipeline {
    pub fn new(starts: Vec<u16>, edges: Vec<(u16, u16)>) -> Pipeline {
        Pipeline {
            starts,
            adj_list: Pipeline::to_adj_list(edges),
        }
    }

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


    fn topo_order(&self) -> Vec<u16> {
        let mut order = Vec::<u16>::new();
        let mut discovered = HashSet::<u16>::new();
        for start in &self.starts {
            self.topo_sort(*start, &mut order, &mut discovered);
        }     
        order.iter().rev().map(|x| *x).collect()

    }
}

pub struct RenderPass<'a> {
    target: &'a mut dyn RenderTarget,
    processes: Vec<&'a mut dyn TextureProcessor>,
    topo_order: Vec<u16>,
    pipeline: Pipeline,
}

impl<'a> RenderPass<'a> {
    pub fn new(target: &'a mut dyn RenderTarget, processes: Vec<&'a mut dyn TextureProcessor>, pipeline: Pipeline) -> RenderPass<'a> {
        RenderPass { target, processes, topo_order: pipeline.topo_order(), pipeline }
    }
    /*fn add_to_reg(&mut self, node: u16, tex: u16) {
        match self.registers.get_mut(&node) {
            Some(data) => data.push(tex),
            None => {
                self.registers.insert(node, vec![tex]);
            }
        }
    }

    fn save_stage_out(&mut self, node: u16, tex: TextureType<'a>) -> Option<TextureType<'a>> {
        let ns = self.pipeline.adj_list.get(&node);
        match ns {
            Some(neighbors) => {
                self.saved_textures.push(tex);
                for ns in neighbors {
                    self.add_to_reg(*ns, (self.saved_textures.len() - 1) as u16);
                }
                None
            },
            None => Some(tex),
        }
    }*/
    fn add_to_reg(registers: &mut HashMap<u16, Vec<usize>>, k: u16, v: usize) {
        match registers.get_mut(&k) {
            Some(vals) => vals.push(v),
            None => {
                registers.insert(k, vec![v]);
            }
        }
    }
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

    //
    pub fn run_pass(&mut self, viewer: &dyn Viewer, shader: &shader::ShaderManager, 
        render_func: &dyn Fn(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer)) -> TextureType 
    {
        let mut saved_textures = Vec::<TextureType>::new();
        let mut registers = HashMap::<u16, Vec<usize>>::new();
        let mut final_out : Option<usize>;
        self.target.draw(viewer, render_func);
        saved_textures.push(self.target.read());
        final_out = RenderPass::save_stage_out(
            &mut registers, saved_textures.len() - 1, 0, &self.pipeline);
        for node in self.topo_order.clone() {
            if node == 0 {
                continue;
            } else {
                let index = (node as usize) - 1;
                match registers.get(&node) {
                    Some(data) => {
                        let process_input = 
                            data.iter().map(|idx| { &saved_textures[*idx] }).collect();
                        let idx_ptr = self.processes.as_mut_ptr();
                        unsafe {
                            // need to use pointers because compiler can't know that we're borrowing
                            // different elements of the vector
                            let elem = idx_ptr.add(index);
                            let tex = (*elem).process(process_input, shader);
                            saved_textures.push(tex);
                        }
                        final_out = RenderPass::save_stage_out(&mut registers, 
                            saved_textures.len() - 1, node, &self.pipeline);
                    },
                    _ => panic!("No saved data for pipeline node"),
                }
            }
        }
        final_out.map(|tex_idx| saved_textures.swap_remove(tex_idx)).unwrap()
    }
}