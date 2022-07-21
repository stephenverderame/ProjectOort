use std::cell::Cell;
#[derive(PartialEq, Eq)]
enum SSBOMode {
    /// Automatic resizing with implicit size member in the buffer
    Dynamic, 
    /// Size and data does not change (doing either creates a new buffer)
    Static,
    /// Size does not change but the data can (new data must be less than original size)
    StaticAllocDynamic,
}

/// # Modes
/// ## Dynamic
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
/// ## Static
/// A shader storage buffer holding an array of `T`. Memory is allocated once at initializiation
/// and data updates are not supported.
/// 
/// ## StaticAllocDynamic
/// A shader storage buffer holding an array of `T`. Memory is allocated once at initialization
/// and data updates must not use more memory than what was allocated.
pub struct SSBO<T : Copy> {
    buffer: gl::types::GLuint,
    buffer_count: Cell<u32>,
    mode: SSBOMode,
    phantom: std::marker::PhantomData<T>,
    // PhantomData which takes no space so compiler thinks this 
    // owns a generic type. Prevents from accidentally passing in a different type
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
         {
            let error = gl::GetError();
            if error != gl::NO_ERROR {
                panic!("GL error: '{}' at {}:{}::{}", error, std::file!(), std::line!(), std::column!());
            }
        }
    }
}


impl<T : Copy> SSBO<T> {
    /// Creates a dynamic shader storage buffer.
    /// Resizing is handled by this wrapper
    pub fn dynamic(data: Option<&[T]>) -> SSBO<T> {
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
            buffer, buffer_count: Cell::new(length as u32),
            phantom: std::marker::PhantomData,
            mode: SSBOMode::Dynamic,
        }
    }
    /// Allocates a new SSBO with the specified size an optional data
    /// This allocation function does not allocate extra members, so the glsl
    /// buffer should be a variable sized `T` array
    /// 
    /// `data` - optional initial data. If specified, data length must be equal to `buffer_size`
    fn new_static(buffer_size: usize, data: Option<&[T]>, mode: SSBOMode) -> SSBO<T> {
        let mut buffer = 0 as gl::types::GLuint;
        unsafe {
            gl::GenBuffers(1, &mut buffer as *mut gl::types::GLuint);
            assert_no_error!();
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, buffer);
            let cpy_mode = if mode == SSBOMode::Static { gl::STATIC_COPY } else { gl::DYNAMIC_COPY };
            gl::BufferData(gl::SHADER_STORAGE_BUFFER, (buffer_size * std::mem::size_of::<T>()) as isize, 
                data.map(|x| x.as_ptr()).unwrap_or(0 as *const T) as *const std::ffi::c_void, cpy_mode);
            assert_no_error!();
        }
        SSBO {
            buffer, buffer_count: Cell::new(buffer_size as u32),
            phantom: std::marker::PhantomData,
            mode,
        }
    }
    /// Creates a SSBO that cannot be easily resized
    #[inline(always)]
    pub fn static_empty(count: u32) -> SSBO<T> {
        SSBO::new_static(count as usize, None, SSBOMode::Static)
    }

    /// Creates a SSBO that cannot be easily resized
    #[inline(always)]
    pub fn create_static(data: &[T]) -> SSBO<T> {
        SSBO::new_static(data.len(), Some(data), SSBOMode::Static)
    }

    /// Creates a new SSBO that cannot be resized but whose data can change
    #[inline(always)]
    pub fn static_alloc_dyn(buffer_size: usize, data: Option<&[T]>) -> SSBO<T> {
        SSBO::new_static(buffer_size, data, SSBOMode::StaticAllocDynamic)
    }
    /// Resizes the buffer to fit `data_size` elements.
    /// Requires to be operating in dynamic mode.
    /// `data_size` is not the new size, but rather the new data size
    /// 
    /// Assumes that `data_size` elements cannot fit
    /// 
    /// Resizes the buffer to `2 * data_size` elements
    unsafe fn dynamic_resize(&self, data_size: usize) {
        self.buffer_count.set(data_size as u32 * 2);
        //self.del_buffer();
        gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, self.buffer);
        gl::BufferData(gl::SHADER_STORAGE_BUFFER, (16 + std::mem::size_of::<T>() as u32 * self.buffer_count.get()) as isize, 
                0 as *const std::ffi::c_void, gl::DYNAMIC_COPY);
        assert_no_error!();
    }

    fn update_dynamic(&self, data: &[T]) {
        unsafe {
            if data.len() as u32 >= self.buffer_count.get() {
                self.dynamic_resize(data.len());
            }
            let size_w_padding = [data.len() as u32, 0u32, 0u32, 0u32];
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, self.buffer);
            gl::BufferSubData(gl::SHADER_STORAGE_BUFFER, 0, 16, size_w_padding.as_ptr() as *const std::ffi::c_void);
            gl::BufferSubData(gl::SHADER_STORAGE_BUFFER, 16, (std::mem::size_of::<T>() * data.len()) as isize,
                data.as_ptr() as *const std::ffi::c_void);
            assert_no_error!();
        }
    }

    fn update_static_alloc(&self, data: &[T]) {
        if data.len() > self.buffer_count.get() as usize {
            panic!("Cannot allocate more memory in static alloc mode!");
        }
        unsafe {
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, self.buffer);
            gl::BufferSubData(gl::SHADER_STORAGE_BUFFER, 0, (std::mem::size_of::<T>() * data.len()) as isize,
                data.as_ptr() as *const std::ffi::c_void);
            assert_no_error!();
        }
    }
    /// Sets the entire memory of the ssbo to 0s
    /// Stes the data byte-wise
    pub fn zero_bytes(&self) {
        unsafe {
            let val = 0u8;
            gl::ClearNamedBufferData(self.buffer, gl::R8, gl::RED, gl::UNSIGNED_BYTE, 
                &val as *const u8 as *const std::ffi::c_void);
            assert_no_error!();
        }
    }

    /// Updates the data of the SSBO, resizing if necessary (for dynamic mode)
    pub fn update(&self, data: &[T]) {
        match self.mode {
            SSBOMode::Dynamic => self.update_dynamic(data),
            SSBOMode::StaticAllocDynamic => self.update_static_alloc(data),
            SSBOMode::Static => panic!("Cannot mutate data in static mode"),
        }
    }

    /// Binds the buffer to index `index`
    pub fn bind(&self, index: u32) {
        unsafe {
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, self.buffer);
            gl::BindBufferBase(gl::SHADER_STORAGE_BUFFER, index, self.buffer);
        }

    }


    /// Copies the data from the buffer
    #[allow(dead_code)]
    pub fn get_data(&self) -> Vec<T> {
        let mut v = Vec::<T>::new();
        v.resize(self.buffer_count.get() as usize, unsafe { std::mem::zeroed() });
        unsafe {
            //println!("Read size {}", (std::mem::size_of::<T>() * self.buffer_count as usize) as isize);
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, self.buffer);
            gl::GetBufferSubData(gl::SHADER_STORAGE_BUFFER, 0,
                (std::mem::size_of::<T>() * self.buffer_count.get() as usize) as isize, 
                v.as_mut_ptr() as *mut std::ffi::c_void);
            assert_no_error!();
        }
        v
    }

    /// Maps the shader storage buffer onto the CPU memory for reading only
    pub fn map_read(&self) -> MappedBuffer<T> {
        unsafe {
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, self.buffer);
            let buf = gl::MapBuffer(gl::SHADER_STORAGE_BUFFER, gl::READ_ONLY) as *const T;
            assert_no_error!();
            if buf == 0 as *const T {
                assert_no_error!();
                assert!(false);
            }
            MappedBuffer {
                gpu_buf: self.buffer,
                size: self.buffer_count.get(),
                buf,
            }
        }
    }

    /// Maps the shader storage buffer onto the CPU memory for writing only
    pub fn map_write(&self) -> MutMappedBuffer<T> {
        unsafe {
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, self.buffer);
            let buf = gl::MapBuffer(gl::SHADER_STORAGE_BUFFER, gl::WRITE_ONLY) as *mut T;
            assert_no_error!();
            if buf == 0 as *mut T {
                assert_no_error!();
                assert!(false);
            }
            MutMappedBuffer {
                gpu_buf: self.buffer,
                size: self.buffer_count.get(),
                buf,
            }
        }
    }
}

impl<T : Copy> Drop for SSBO<T> {
    fn drop(&mut self) {
        unsafe { gl::DeleteBuffers(1, &self.buffer as *const gl::types::GLuint); }
    }
}

/// RAII for the pointer returned from a call to glMapBuffer
pub struct MappedBuffer<T : Copy> {
    gpu_buf: gl::types::GLuint,
    buf: *const T,
    size: u32,
}

/// RAII for the slice from the mapped buffer pointer
pub struct MappedBufferSlice<'a, 'b, T : Copy> {
    pub slice: &'a [T],
    _owner: &'b MappedBuffer<T>, // reference to owner so we can't outlive it
}

impl<T : Copy> MappedBuffer<T> {
    pub fn as_slice<'a, 'b>(&'b self) -> MappedBufferSlice<'a, 'b, T> {
        unsafe {
            MappedBufferSlice {
                slice: std::slice::from_raw_parts(self.buf as *const T, self.size as usize),
                _owner: self,
            }
        }
    }
}

impl<T : Copy> Drop for MappedBuffer<T> {
    fn drop(&mut self) {
        unsafe {
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, self.gpu_buf);
            if gl::UnmapBuffer(gl::SHADER_STORAGE_BUFFER) == gl::FALSE {
                assert_no_error!();
                assert!(false);
            }
        }
    }
}

impl<'a, 'b, T : Copy> std::ops::Deref for MappedBufferSlice<'a, 'b, T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        self.slice
    }
}

/// RAII for the pointer returned from a call to glMapBuffer
pub struct MutMappedBuffer<T : Copy> {
    gpu_buf: gl::types::GLuint,
    buf: *mut T,
    size: u32,
}

/// RAII for the slice from the mapped buffer pointer
pub struct MutMappedBufferSlice<'a, 'b, T : Copy> {
    pub slice: &'a mut [T],
    _owner: &'b MutMappedBuffer<T>, // reference to owner so we can't outlive it
}

impl<T : Copy> MutMappedBuffer<T> {
    pub fn as_slice<'a, 'b>(&'b self) -> MutMappedBufferSlice<'a, 'b, T> {
        unsafe {
            MutMappedBufferSlice {
                slice: std::slice::from_raw_parts_mut(self.buf as *mut T, self.size as usize),
                _owner: self,
            }
        }
    }
}

impl<T : Copy> Drop for MutMappedBuffer<T> {
    fn drop(&mut self) {
        unsafe {
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, self.gpu_buf);
            if gl::UnmapBuffer(gl::SHADER_STORAGE_BUFFER) == gl::FALSE {
                assert_no_error!();
                assert!(false);
            }
        }
    }
}

impl<'a, 'b, T : Copy> std::ops::Deref for MutMappedBufferSlice<'a, 'b, T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        self.slice
    }
}

impl<'a, 'b, T : Copy> std::ops::DerefMut for MutMappedBufferSlice<'a, 'b, T> {

    fn deref_mut(&mut self) -> &mut [T] {
        self.slice
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serial_test::*;
    use crate::shader;

    #[cfg(unix)]
    fn get_event_loop() -> glutin::event_loop::EventLoop<()> {
        use glutin::platform::unix::EventLoopExtUnix;
        glutin::event_loop::EventLoop::new_any_thread()
    }

    #[cfg(windows)]
    fn get_event_loop() -> glutin::event_loop::EventLoop<()> {
        use glutin::platform::windows::EventLoopExtWindows;
        glutin::event_loop::EventLoop::new_any_thread()
    }

    fn init() -> (shader::ShaderManager, glium::Display) {
        use glium::*;
        use glutin::window::{WindowBuilder};
        use glutin::ContextBuilder;
        let e_loop = get_event_loop();
        let window_builder = WindowBuilder::new().with_visible(false).with_inner_size(glium::glutin::dpi::PhysicalSize::<u32>{
            width: 128, height: 128,
        });
        let wnd_ctx = ContextBuilder::new();//.build_headless(&e_loop, glutin::dpi::PhysicalSize::from((128, 128)));
        let wnd_ctx = Display::new(window_builder, wnd_ctx, &e_loop).unwrap();
        gl::load_with(|s| wnd_ctx.gl_window().get_proc_address(s));
        (shader::ShaderManager::init(&wnd_ctx), wnd_ctx)
    }

    #[test]
    #[serial]
    fn map_test() {
        use cgmath::*;
        let (_a, _b) = init();
        unsafe { assert_no_error!() };
        let ssbo = SSBO::<[[f32; 4]; 4]>::static_alloc_dyn(10, None);

        for (e, i) in ssbo.map_write().as_slice().iter_mut().zip(0 .. 10) {
            *e = Matrix4::from_scale(i as f32).into();
        }

        for (e, i) in ssbo.map_read().as_slice().iter().zip(0 .. 10) {
            let m : Matrix4<f32> = From::from(*e);
            assert_relative_eq!(m, Matrix4::from_scale(i as f32));
        }
    }
}