extern crate gl;

pub struct SSBO {
    buffer: gl::types::GLuint,
    buffer_count: u32,
}

impl SSBO {
    pub fn new() {
        let mut buffer = 0 as gl::types::GLuint;
        unsafe {
            gl::GenBuffers(1, &mut buffer as *mut gl::types::GLuint);
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, buffer);
            gl::BufferData()
        }
    }
}

impl Drop for SSBO {
    fn drop(&mut self) {
        unsafe { gl::DeleteBuffers(1, &self.buffer as *const gl::types::GLuint); }
    }
}