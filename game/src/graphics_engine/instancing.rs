#[derive(Copy, Clone)]
pub struct InstancePosition {
    //instance_model: [[f32; 4]; 4],
    pub instance_model_col0: [f32; 4],
    pub instance_model_col1: [f32; 4],
    pub instance_model_col2: [f32; 4],
    pub instance_model_col3: [f32; 4],
}

glium::implement_vertex!(
    InstancePosition,
    instance_model_col0,
    instance_model_col1,
    instance_model_col2,
    instance_model_col3
);

#[derive(Copy, Clone)]
pub struct BillboardAttributes {
    pub instance_pos_rot: [f32; 4],
    pub instance_scale: [f32; 2],
    pub instance_color: [f32; 4],
}

glium::implement_vertex!(
    BillboardAttributes,
    instance_pos_rot,
    instance_scale,
    instance_color
);

#[derive(Copy, Clone)]
pub struct LineAttributes {
    pub start_pos: [f32; 4],
    pub end_pos: [f32; 4],
    pub color: [f32; 4],
}

glium::implement_vertex!(LineAttributes, start_pos, end_pos, color);

#[derive(Copy, Clone)]
pub struct TextAttributes {
    pub x_y_width_height: [i32; 4],
    pub color: [f32; 4],
}

glium::implement_vertex!(TextAttributes, x_y_width_height, color);

#[derive(Copy, Clone)]
pub struct ParticleAttributes {
    pub color: [f32; 4],
    pub tex_idx: u32,
}

glium::implement_vertex!(ParticleAttributes, color, tex_idx);

/// A dynamically resizing buffer of per-instance information
/// If the amount of instances change, the buffer is resized
pub struct InstanceBuffer<T: Copy + glium::Vertex> {
    instance_data: Option<glium::VertexBuffer<T>>,
    buffer_count: usize,
}

impl<T: Copy + glium::Vertex> InstanceBuffer<T> {
    pub const fn new() -> Self {
        Self {
            instance_data: None,
            buffer_count: 0,
        }
    }

    pub fn new_sized<F: glium::backend::Facade>(
        num: usize,
        facade: &F,
    ) -> Self {
        Self {
            instance_data: Some(
                glium::VertexBuffer::empty_dynamic(facade, num).unwrap(),
            ),
            buffer_count: num,
        }
    }

    fn resize_buffer<F: glium::backend::Facade>(
        instances: &[T],
        facade: &F,
    ) -> (glium::VertexBuffer<T>, usize) {
        let new_size = instances.len();
        (
            glium::VertexBuffer::dynamic(facade, instances).unwrap(),
            new_size,
        )
    }

    /// Updates the buffer with `data`, resizing the buffer if its length is not `data.len()`
    pub fn update_buffer<F: glium::backend::Facade>(
        &mut self,
        data: &[T],
        facade: &F,
    ) {
        if data.len() != self.buffer_count {
            let (buffer, size) = Self::resize_buffer(data, facade);
            self.instance_data = Some(buffer);
            self.buffer_count = size;
        } else if let Some(buffer) = &mut self.instance_data {
            let mut mapping = buffer.map();
            for (dst, src) in mapping.iter_mut().zip(data.iter()) {
                *dst = *src;
            }
        }
    }

    /// Updates the buffer with `data` which must be less than the allocated buffer
    ///
    /// `data` is assigned to the first `data.len()` elements of the buffer,
    /// and `empty` is assigned to the rest
    pub fn update_no_grow(&mut self, data: &[T], empty: T) {
        if self.buffer_count < data.len() || self.instance_data.is_none() {
            panic!("Cannot grow data");
        }

        if let Some(buf) = &mut self.instance_data {
            let mut mapping = buf.map();
            for (dst, idx) in mapping.iter_mut().zip(0..self.buffer_count) {
                if idx < data.len() {
                    *dst = data[idx];
                } else {
                    *dst = empty;
                }
            }
        }
    }

    /// Gets the stored instance buffer or `None` if there has been no instances stored
    /// in the buffer
    pub const fn get_stored_buffer(
        &self,
    ) -> Option<&glium::vertex::VertexBuffer<T>> {
        self.instance_data.as_ref()
    }
}

pub fn model_mats_to_vertex(data: &[[[f32; 4]; 4]]) -> Vec<InstancePosition> {
    data.iter()
        .map(|x| InstancePosition {
            instance_model_col0: x[0],
            instance_model_col1: x[1],
            instance_model_col2: x[2],
            instance_model_col3: x[3],
        })
        .collect()
}

pub fn mat_to_instance_pos<T: cgmath::BaseFloat>(
    mat: &cgmath::Matrix4<T>,
) -> InstancePosition {
    let mat: cgmath::Matrix4<f32> = mat.cast().unwrap();
    InstancePosition {
        instance_model_col0: mat.x.into(),
        instance_model_col1: mat.y.into(),
        instance_model_col2: mat.z.into(),
        instance_model_col3: mat.w.into(),
    }
}
