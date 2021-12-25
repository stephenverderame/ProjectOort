use crate::render_target::*;
use crate::draw_traits::*;
use glium::*;
use crate::shader;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::collections::HashSet;

#[derive(PartialEq, Eq, std::hash::Hash, Copy, Clone)]
pub enum PipelineNode {
    Target(u16),
    Processor(u16),
}

impl std::convert::From<&PipelineNode> for u32{
    fn from(val: &PipelineNode) -> Self {
        use PipelineNode::*;
        match val {
            Target(x) => *x as u32 | 1 << 31,
            Processor(x) => *x as u32,
        }
    }
}

impl std::cmp::Ord for PipelineNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let a : u32 = self.into();
        let b : u32 = other.into();
        a.cmp(&b)
    }
}

impl std::cmp::PartialOrd for PipelineNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}



pub struct Pipeline {
    pub starts: Vec<u16>,
    pub adj_list: HashMap<u16, Vec<u16>>,
}

impl Pipeline {
    fn new(starts: Vec<u16>, edges: Vec<(u16, u16)>) -> Pipeline {
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


    fn topo_order(&self) -> Vec<u16> {
        let mut order = Vec::<u16>::new();
        let mut q = VecDeque::<u16>::new();
        let mut discovered = HashSet::<u16>::new();
        for start in self.starts {
            q.push_back(start);
        }
        while !q.is_empty() {
            let n = q.pop_front().unwrap();
            match self.adj_list.get(&n) {
                Some(neighbors) => {
                    for ns in neighbors {
                        if discovered.get(ns).is_none() {
                            q.push_back(*ns);
                            discovered.insert(*ns);
                        }
                    }
                },
                _ => ()
            }
        }
        order

    }
}

pub struct RenderPass<'a> {
    target: &'a dyn RenderTarget,
    processes: Vec<&'a dyn TextureProcessor>,
    topo_order: Vec<u16>,
    pipeline: Pipeline,
    registers: HashMap<u16, Vec<TextureType<'a>>>,
}

impl<'a> RenderPass<'a> {
    pub fn new(target: &'a dyn RenderTarget, processes: Vec<&'a dyn TextureProcessor>, pipeline: Pipeline) -> RenderPass<'a> {
        RenderPass { target, processes, topo_order: pipeline.topo_order(), pipeline,
            registers: HashMap::<u16, Vec<TextureType>>::new(),
        }
    }
    fn add_to_reg(&mut self, node: u16, tex: TextureType<'a>) {
        match self.registers.get_mut(&node) {
            Some(data) => data.push(tex),
            None => {
                self.registers.insert(node, vec![tex]);
            }
        }
    }

    fn save_stage_out(&mut self, node: u16, tex: TextureType<'a>) {
        self.pipeline.adj_list.get(&node).map(|neighbors| {
            for ns in neighbors {
                self.add_to_reg(*ns, tex);
            }
            Some(0)
        });
    }

    //
    pub fn run_pass(&mut self, viewer: &dyn Viewer, shader: &shader::ShaderManager, 
        render_func: &dyn Fn(&mut framebuffer::SimpleFrameBuffer, &dyn Viewer)) -> TextureType 
    {
        self.registers.clear();
        let mut final_out : TextureType;
        self.target.draw(viewer, render_func);
        for node in self.topo_order {
            if node == 0 {
                let tex = self.target.read();
                self.save_stage_out(node, &tex);
                final_out = tex;
            } else {
                let index = (node as usize) - 1;
                match self.registers.get(&node) {
                    Some(data) => {
                        let tex = self.processes[index].process(data, shader);
                        self.save_stage_out(node, &tex);
                        final_out = tex;
                    },
                    _ => panic!("No saved data for pipeline node"),
                }
            }
        }
        final_out
    }
}