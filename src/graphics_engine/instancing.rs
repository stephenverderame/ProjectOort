#[derive(Copy, Clone)]
pub struct InstanceAttribute {
    //instance_model: [[f32; 4]; 4],
    pub instance_model_col0: [f32; 4],
    pub instance_model_col1: [f32; 4],
    pub instance_model_col2: [f32; 4],
    pub instance_model_col3: [f32; 4],
    pub instance_color: [f32; 3],
}

glium::implement_vertex!(InstanceAttribute, instance_model_col0, instance_model_col1, instance_model_col2, 
    instance_model_col3, instance_color);

/// A dynamically resizing buffer of per-instance information
/// If the amount of instances change, the buffer is resized
pub struct InstanceBuffer {
    instance_data: Option<glium::VertexBuffer<InstanceAttribute>>,
    buffer_count: usize,
    active_count: usize,
}

impl InstanceBuffer {
    pub fn new() -> InstanceBuffer {
        InstanceBuffer {
            instance_data: None,
            buffer_count: 0,
            active_count: 0,
        }
    }

    fn resize_buffer<F : glium::backend::Facade>(instances: &[[[f32; 4]; 4]], facade: &F) 
    -> (glium::VertexBuffer<InstanceAttribute>, usize)
    {
        let new_size = instances.len();
        let data : Vec<InstanceAttribute> = instances.iter().map(|data| {
            InstanceAttribute {
                instance_model_col0: data[0],
                instance_model_col1: data[1],
                instance_model_col2: data[2],
                instance_model_col3: data[3],
                instance_color: [0.5451, 0f32, 0.5451],
            }
        }).collect();
        (glium::VertexBuffer::dynamic(facade, &data).unwrap(), new_size)
    }

    pub fn update_buffer<F : glium::backend::Facade>(&mut self, data: &[[[f32; 4]; 4]], facade: &F)
    {
        if data.len() != self.buffer_count {
            let (buffer, size) = InstanceBuffer::resize_buffer(data, facade);
            self.instance_data = Some(buffer);
            self.buffer_count = size;
        } else if let Some(buffer) = &mut self.instance_data {
           let mut mapping = buffer.map();
           for (dst, src) in mapping.iter_mut().zip(data.iter()) {
               dst.instance_model_col0 = src[0];
               dst.instance_model_col1 = src[1];
               dst.instance_model_col2 = src[2];
               dst.instance_model_col3 = src[3];
           }
        }
        self.active_count = data.len();
    }

    pub fn get_stored_buffer(&self) -> &glium::vertex::VertexBuffer<InstanceAttribute>
    {
        self.instance_data.as_ref().unwrap()
    }

}