

/// A shader storage buffer which holds a uint size parameter and then a variable sized
/// `T` array.
/// The GPU buffer is resized in multiples of 2 from the initial size
/// 
/// Expects the buffer in GLSL to have the following structure:
/// 
/// ```text
/// buffer BufferName {
///     uint size;
///     T data[];
/// }
/// ```
pub struct SSBO<T : Copy> {
    buffer: gl::types::GLuint,
    buffer_count: u32,
    phantom: std::marker::PhantomData<T>,
}

macro_rules! assert_no_error {
    ($msg:expr) => {
        unsafe {
            let error = gl::GetError();
            if error != gl::NO_ERROR {
                panic!("GL error: '{}' at {}:{}::{}\n'{}'", error, std::file!(), std::line!(), std::column!(), $msg);
            }
        }
    };
    () => {
        unsafe {
            let error = gl::GetError();
            if error != gl::NO_ERROR {
                panic!("GL error: '{}' at {}:{}::{}", error, std::file!(), std::line!(), std::column!());
            }
        }
    }
}


impl<T : Copy> SSBO<T> {
    pub fn new(data: Option<&Vec<T>>) -> SSBO<T> {
        let mut buffer = 0 as gl::types::GLuint;
        let length = data.map(|x| x.len()).unwrap_or(0);
        let size_w_padding = [length as u32, 0u32, 0u32, 0u32];
        // glsl pads to 16 bytes
        unsafe {
            gl::GenBuffers(1, &mut buffer as *mut gl::types::GLuint);
            assert_no_error!();
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, buffer);
            gl::BufferData(gl::SHADER_STORAGE_BUFFER, (16 + std::mem::size_of::<T>() * length) as isize, 
                0 as *const std::ffi::c_void, gl::DYNAMIC_COPY);
            assert_no_error!();
            gl::BufferSubData(gl::SHADER_STORAGE_BUFFER, 0, 16, size_w_padding.as_ptr() as *const std::ffi::c_void);
            assert_no_error!();
            if let Some(data) = data {
                gl::BufferSubData(gl::SHADER_STORAGE_BUFFER, 16, (std::mem::size_of::<T>() * data.len()) as isize,
                    data.as_ptr() as *const std::ffi::c_void);
                assert_no_error!();
            }
        }
        SSBO {
            buffer, buffer_count: length as u32,
            phantom: std::marker::PhantomData,
        }
    }
    /// Resizes the buffer to fit `data_size` elements
    /// `data_size` is not the new size, but rather the new data size
    /// 
    /// Assumes that `data_size` elements cannot fit
    /// 
    /// Resizes the buffer to `2 * data_size` elements
    unsafe fn resize(&mut self, data_size: usize) {
        self.buffer_count = data_size as u32 * 2;
        self.del_buffer();
        gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, self.buffer);
        gl::BufferData(gl::SHADER_STORAGE_BUFFER, (16 + std::mem::size_of::<T>() as u32 * self.buffer_count) as isize, 
                0 as *const std::ffi::c_void, gl::DYNAMIC_COPY);
        assert_no_error!();
    }

    /// Updates the data of the SSBO, resizing if necessary
    pub fn update(&mut self, data: &Vec<T>) {
        unsafe {
            if data.len() as u32 >= self.buffer_count {
                self.resize(data.len());
            }
            let size_w_padding = [data.len() as u32, 0u32, 0u32, 0u32];
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, self.buffer);
            gl::BufferSubData(gl::SHADER_STORAGE_BUFFER, 0, 16, size_w_padding.as_ptr() as *const std::ffi::c_void);
            gl::BufferSubData(gl::SHADER_STORAGE_BUFFER, 16, (std::mem::size_of::<T>() * data.len()) as isize,
                data.as_ptr() as *const std::ffi::c_void);
            assert_no_error!();
        }
    }

    /// Binds the buffer to index `index`
    pub fn bind(&self, index: u32) {
        unsafe {
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, self.buffer);
            gl::BindBufferBase(gl::SHADER_STORAGE_BUFFER, index, self.buffer);
        }

    }

    fn del_buffer(&self) {
        unsafe { gl::DeleteBuffers(1, &self.buffer as *const gl::types::GLuint); }
    }
}

impl<T : Copy> Drop for SSBO<T> {
    fn drop(&mut self) {
        unsafe { gl::DeleteBuffers(1, &self.buffer as *const gl::types::GLuint); }
    }
}