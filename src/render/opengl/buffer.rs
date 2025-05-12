use std::ptr;
use std::mem;
use std::ffi::c_void;

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const NULL_BUFFER_HANDLE: gl::types::GLuint = 0;
pub const NULL_VERTEX_ARRAY_HANDLE: gl::types::GLuint = 0;

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum BufferUsageHint {
    StaticDraw  = gl::STATIC_DRAW,  // The user will set the data once.
    DynamicDraw = gl::DYNAMIC_DRAW, // The user will set the data occasionally.
    StreamDraw  = gl::STREAM_DRAW,  // The user will be changing the data after every use. Or almost every use.
}

// ----------------------------------------------
// VertexBuffer
// ----------------------------------------------

pub struct VertexBuffer {
    handle: gl::types::GLuint,
    count: u32,
    stride: u32,
    usage_hint: BufferUsageHint,
}

impl VertexBuffer {
    pub fn with_uninitialized_data(count: u32, stride: u32, usage_hint: BufferUsageHint) -> Self {
        Self::create_buffer(ptr::null(), count, stride, usage_hint)
    }

    pub fn with_data<T>(vertices: &[T], usage_hint: BufferUsageHint) -> Self {
        Self::create_buffer(
            vertices.as_ptr() as *const c_void,
            vertices.len() as u32,
            mem::size_of::<T>() as u32,
            usage_hint)
    }

    pub fn with_data_raw(vertices: *const c_void, count: u32, stride: u32, usage_hint: BufferUsageHint) -> Self {
        Self::create_buffer(vertices, count, stride, usage_hint)
    }

    pub fn is_valid(&self) -> bool {
        self.handle != NULL_BUFFER_HANDLE
    }

    pub fn handle(&self) -> gl::types::GLuint {
        self.handle
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn stride(&self) -> u32 {
        self.stride
    }

    pub fn usage_hint(&self) -> BufferUsageHint {
        self.usage_hint
    }

    pub fn set_data<T>(&self, vertices: &[T]) {
        debug_assert!(mem::size_of::<T>() == (self.stride as usize));
        Self::set_data_raw(
            self,
            vertices.as_ptr() as *const c_void,
            vertices.len() as u32);
    }

    pub fn set_data_raw(&self, vertices: *const c_void, count: u32) {
        debug_assert!(count != 0);
        debug_assert!(count <= self.count);
        debug_assert!(self.is_valid());

        unsafe {
            gl::BindBuffer(gl::ARRAY_BUFFER, self.handle);

            let buffer_size = count * self.stride;
            gl::BufferData(
                gl::ARRAY_BUFFER,
                buffer_size as gl::types::GLsizeiptr,
                vertices, // may be null
                self.usage_hint as gl::types::GLenum);

            gl::BindBuffer(gl::ARRAY_BUFFER, NULL_BUFFER_HANDLE);
        }
    }

    pub fn resize(&mut self, new_size: usize) {
        debug_assert!(new_size != 0);
        self.count = new_size as u32;
    }

    fn create_buffer(vertices: *const c_void,
                     count: u32,
                     stride: u32,
                     usage_hint: BufferUsageHint) -> Self {

        // `vertices` may be null.
        debug_assert!(count != 0);
        debug_assert!(stride != 0);

        let buffer_handle = unsafe {
            let mut buffer_handle = NULL_BUFFER_HANDLE;
            gl::GenBuffers(1, &mut buffer_handle);
            if buffer_handle == NULL_BUFFER_HANDLE {
                panic!("Failed to create vertex buffer handle!");
            }

            gl::BindBuffer(gl::ARRAY_BUFFER, buffer_handle);

            let buffer_size = count * stride;
            gl::BufferData(
                gl::ARRAY_BUFFER,
                buffer_size as gl::types::GLsizeiptr,
                vertices,
                usage_hint as gl::types::GLenum);

            gl::BindBuffer(gl::ARRAY_BUFFER, NULL_BUFFER_HANDLE);

            buffer_handle
        };

        Self {
            handle: buffer_handle,
            count: count,
            stride: stride,
            usage_hint: usage_hint,
        }
    }
}

impl Drop for VertexBuffer {
    fn drop(&mut self) {
        if self.handle != NULL_BUFFER_HANDLE {
            unsafe {
                gl::DeleteBuffers(1, &self.handle);
            }
            self.handle = NULL_BUFFER_HANDLE;
        }
    }
}

// ----------------------------------------------
// IndexType
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum IndexType {
    U16,
    U32,
}

impl IndexType {
    pub fn size_in_bytes(&self) -> usize {
        match self {
            IndexType::U16 => mem::size_of::<u16>(),
            IndexType::U32 => mem::size_of::<u32>(),
        }
    }

    pub fn to_gl_enum(&self) -> gl::types::GLenum {
        match self {
            IndexType::U16 => gl::UNSIGNED_SHORT,
            IndexType::U32 => gl::UNSIGNED_INT,
        }
    }
}

// Map type (u16, u32) to enum (IndexType::U16, IndexType::U32).
pub trait IndexTrait {
    fn index_type() -> IndexType;
}

impl IndexTrait for u16 {
    fn index_type() -> IndexType {
        IndexType::U16
    }
}

impl IndexTrait for u32 {
    fn index_type() -> IndexType {
        IndexType::U32
    }
}

// ----------------------------------------------
// IndexBuffer
// ----------------------------------------------

pub struct IndexBuffer {
    handle: gl::types::GLuint,
    count: u32,
    usage_hint: BufferUsageHint,
    index_type: IndexType,
}

impl IndexBuffer {
    pub fn with_uninitialized_data(count: u32, index_type: IndexType, usage_hint: BufferUsageHint) -> Self {
        Self::create_buffer(ptr::null(), count, index_type, usage_hint)
    }

    pub fn with_data<T: IndexTrait>(indices: &[T], usage_hint: BufferUsageHint) -> IndexBuffer {
        debug_assert!(mem::size_of::<T>() == T::index_type().size_in_bytes());
        Self::create_buffer(
            indices.as_ptr() as *const c_void,
            indices.len() as u32,
            T::index_type(),
            usage_hint)
    }

    pub fn with_data_raw(indices: *const c_void, count: u32, index_type: IndexType, usage_hint: BufferUsageHint) -> Self {
        Self::create_buffer(indices, count, index_type, usage_hint)
    }

    pub fn is_valid(&self) -> bool {
        self.handle != NULL_BUFFER_HANDLE
    }

    pub fn handle(&self) -> gl::types::GLuint {
        self.handle
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn usage_hint(&self) -> BufferUsageHint {
        self.usage_hint
    }

    pub fn index_type(&self) -> IndexType {
        self.index_type
    }

    pub fn set_data<T>(&self, indices: &[T]) {
        debug_assert!(mem::size_of::<T>() == self.index_type.size_in_bytes());
        Self::set_data_raw(
            self,
            indices.as_ptr() as *const c_void,
            indices.len() as u32);
    }

    pub fn set_data_raw(&self, indices: *const c_void, count: u32) {
        debug_assert!(count != 0);
        debug_assert!(count <= self.count);
        debug_assert!(self.is_valid());

        unsafe {
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, self.handle);

            let buffer_size = (count as usize) * self.index_type.size_in_bytes();
            gl::BufferData(
                gl::ELEMENT_ARRAY_BUFFER,
                buffer_size as gl::types::GLsizeiptr,
                indices, // may be null
                self.usage_hint as gl::types::GLenum);

            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, NULL_BUFFER_HANDLE);
        }
    }

    pub fn resize(&mut self, new_size: usize) {
        debug_assert!(new_size != 0);
        self.count = new_size as u32;
    }

    fn create_buffer(indices: *const c_void,
                     count: u32,
                     index_type: IndexType,
                     usage_hint: BufferUsageHint) -> Self {

        // `indices` may be null.
        debug_assert!(count != 0);

        let buffer_handle = unsafe {
            let mut buffer_handle = NULL_BUFFER_HANDLE;
            gl::GenBuffers(1, &mut buffer_handle);
            if buffer_handle == NULL_BUFFER_HANDLE {
                panic!("Failed to create index buffer handle!");
            }

            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, buffer_handle);

            let buffer_size = (count as usize) * index_type.size_in_bytes();
            gl::BufferData(
                gl::ELEMENT_ARRAY_BUFFER,
                buffer_size as gl::types::GLsizeiptr,
                indices,
                usage_hint as gl::types::GLenum);

            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, NULL_BUFFER_HANDLE);

            buffer_handle
        };

        Self {
            handle: buffer_handle,
            count: count,
            usage_hint: usage_hint,
            index_type: index_type,
        }
    }
}

impl Drop for IndexBuffer {
    fn drop(&mut self) {
        if self.handle != NULL_BUFFER_HANDLE {
            unsafe {
                gl::DeleteBuffers(1, &self.handle);
            }
            self.handle = NULL_BUFFER_HANDLE;
        }
    }
}

#[derive(Copy, Clone)]
pub struct IndexBufferSlice {
    pub start: u32,
    pub count: u32,
}

// ----------------------------------------------
// VertexElementDef
// ----------------------------------------------

pub struct VertexElementDef {
    pub count: u32,
    pub kind: gl::types::GLenum,
}

impl VertexElementDef {
    pub fn size_in_bytes(&self) -> usize {
        match self.kind {
            gl::FLOAT => mem::size_of::<f32>(),

            gl::BYTE => mem::size_of::<i8>(),
            gl::UNSIGNED_BYTE => mem::size_of::<u8>(),

            gl::SHORT => mem::size_of::<i16>(),
            gl::UNSIGNED_SHORT => mem::size_of::<u16>(),

            gl::INT => mem::size_of::<i32>(),
            gl::UNSIGNED_INT => mem::size_of::<u32>(),

            _ => panic!("Unhandled VertexElementDef type!"),
        }
    }
}

pub trait VertexTrait {
    fn layout() -> Vec<VertexElementDef>;
    fn stride() -> usize;
}

// ----------------------------------------------
// VertexArray
// ----------------------------------------------

pub struct VertexArray {
    handle: gl::types::GLuint,
    vertex_buffer: VertexBuffer,
    index_buffer: IndexBuffer,
}

impl VertexArray {
    pub fn new(vertex_buffer: VertexBuffer,
               index_buffer: IndexBuffer,
               vertex_layout: &[VertexElementDef],
               vertex_stride: usize) -> Self {

        debug_assert!(vertex_buffer.is_valid());
        debug_assert!(index_buffer.is_valid());
        debug_assert!(vertex_stride != 0);

        let array_handle = unsafe {
            let mut array_handle = NULL_VERTEX_ARRAY_HANDLE;
            gl::GenVertexArrays(1, &mut array_handle);
            if array_handle == NULL_BUFFER_HANDLE {
                panic!("Failed to create vertex array handle!");
            }

            gl::BindVertexArray(array_handle);
            gl::BindBuffer(gl::ARRAY_BUFFER, vertex_buffer.handle);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, index_buffer.handle);

            // Set vertex layout:
            let mut offset: usize = 0;
            let mut index: u32 = 0;
            for vertex_element in vertex_layout {
                gl::EnableVertexAttribArray(index);
                gl::VertexAttribPointer(
                    index,
                    vertex_element.count as gl::types::GLint,
                    vertex_element.kind,
                    gl::FALSE,
                    vertex_stride as gl::types::GLsizei,
                    offset as *const c_void);

                offset += (vertex_element.count as usize) * vertex_element.size_in_bytes();
                index += 1;
            }

            // Unbind all:
            gl::BindVertexArray(NULL_VERTEX_ARRAY_HANDLE);
            gl::BindBuffer(gl::ARRAY_BUFFER, NULL_BUFFER_HANDLE);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, NULL_BUFFER_HANDLE);

            array_handle
        };

        Self {
            handle: array_handle,
            vertex_buffer: vertex_buffer,
            index_buffer: index_buffer,
        }
    }

    pub fn handle(&self) -> gl::types::GLuint {
        self.handle
    }

    pub fn index_type(&self) -> IndexType {
        self.index_buffer.index_type
    }

    pub fn index_buffer(&self) -> &IndexBuffer {
        &self.index_buffer
    }

    pub fn vertex_buffer(&self) -> &VertexBuffer {
        &self.vertex_buffer
    }

    pub fn index_buffer_mut(&mut self) -> &mut IndexBuffer {
        &mut self.index_buffer
    }

    pub fn vertex_buffer_mut(&mut self) -> &mut VertexBuffer {
        &mut self.vertex_buffer
    }

    pub fn is_valid(&self) -> bool {
        (self.handle != NULL_VERTEX_ARRAY_HANDLE)
        && self.vertex_buffer.is_valid()
        && self.index_buffer.is_valid()
    }

    pub fn index_count(&self) -> u32 {
        self.index_buffer.count
    }

    pub fn vertex_count(&self) -> u32 {
        self.vertex_buffer.count
    }
}

impl Drop for VertexArray {
    fn drop(&mut self) {
        if self.handle != NULL_VERTEX_ARRAY_HANDLE {
            unsafe {
                gl::DeleteVertexArrays(1, &self.handle);
            }
            self.handle = NULL_VERTEX_ARRAY_HANDLE;
        }
    }
}
