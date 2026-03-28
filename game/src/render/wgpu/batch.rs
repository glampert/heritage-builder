use std::{
    fmt::Debug,
    borrow::Cow,
    convert::{Into, TryFrom},
};

use crate::{
    render,
    utils::{Rect, fixed_string::format_fixed_string},
};

// ----------------------------------------------
// DrawBatchEntry
// ----------------------------------------------

pub struct DrawBatchEntry {
    pub first_index: u32,
    pub index_count: u32,
    pub texture: render::texture::TextureHandle,
}

// ----------------------------------------------
// DrawBatch
// ----------------------------------------------

pub struct DrawBatch<V: Copy, I: Copy> {
    vertices: Vec<V>,
    indices:  Vec<I>,
    entries:  Vec<DrawBatchEntry>,
}

impl<V, I> DrawBatch<V, I>
where
    V: Copy + bytemuck::Pod,
    I: Copy + bytemuck::Pod + TryFrom<usize> + Into<usize>,
{
    pub fn new(vertices_capacity: usize, indices_capacity: usize, entries_capacity: usize) -> Self {
        Self {
            vertices: Vec::with_capacity(vertices_capacity),
            indices:  Vec::with_capacity(indices_capacity),
            entries:  Vec::with_capacity(entries_capacity),
        }
    }

    pub fn add_entry(&mut self, vertices: &[V], indices: &[I], texture: render::texture::TextureHandle)
        where <I as TryFrom<usize>>::Error: Debug
    {
        let first_index = self.add_fast(vertices, indices);
        self.entries.push(DrawBatchEntry {
            first_index: first_index as u32,
            index_count: indices.len() as u32,
            texture,
        });
    }

    pub fn add_fast(&mut self, vertices: &[V], indices: &[I]) -> usize
        where <I as TryFrom<usize>>::Error: Debug
    {
        let ib_start = self.indices.len();
        let vb_base  = self.vertices.len();

        for &i in indices {
            let idx: usize = i.into() + vb_base;
            let narrowed: I = idx.try_into()
                .expect("INTEGER OVERFLOW! Value does not fit into index type.");

            self.indices.push(narrowed);
        }

        self.vertices.extend_from_slice(vertices);
        ib_start
    }

    #[inline]
    pub fn vertices(&self) -> &[V] {
        &self.vertices
    }

    #[inline]
    pub fn indices(&self) -> &[I] {
        &self.indices
    }

    #[inline]
    pub fn entries(&self) -> &[DrawBatchEntry] {
        &self.entries
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.entries.clear();
    }
}

// ----------------------------------------------
// UiDrawBatch
// ----------------------------------------------

pub struct UiDrawBatch {
    vertices: Vec<u8>, // Raw render::UiDrawVertex bytes.
    indices:  Vec<u8>, // Raw render::UiDrawIndex bytes.
}

impl UiDrawBatch {
    pub fn new() -> Self {
        Self {
            vertices: Vec::with_capacity(1024 * std::mem::size_of::<render::UiDrawVertex>()),
            indices:  Vec::with_capacity(1024 * std::mem::size_of::<render::UiDrawIndex>()),
        }
    }

    // Appends a draw list's vertex/index data to the batch.
    // Returns (base_vertex, index_offset) for use with draw_indexed.
    //  - base_vertex: the vertex offset (in vertices, not bytes) to pass as base_vertex.
    //  - index_offset: the index offset (in indices, not bytes) to add to first_index.
    pub fn append_data(&mut self,
                       vtx_buffer: &[render::UiDrawVertex],
                       idx_buffer: &[render::UiDrawIndex])
                       -> (i32, u32)
    {
        let base_vertex  = (self.vertices.len() / std::mem::size_of::<render::UiDrawVertex>()) as i32;
        let index_offset = (self.indices.len()  / std::mem::size_of::<render::UiDrawIndex>())  as u32;

        // render::UiDrawVertex doesn't implement bytemuck::Pod, so we reinterpret as raw bytes.
        // SAFETY: UiDrawVertex is a repr(C) struct of f32s and u8 — no padding, no drop, all
        //         bit-patterns valid. Same for UiDrawIndex (u16 or u32).
        self.vertices.extend_from_slice(unsafe {
            std::slice::from_raw_parts(
                vtx_buffer.as_ptr() as *const u8,
                std::mem::size_of_val(vtx_buffer),
            )
        });

        self.indices.extend_from_slice(unsafe {
            std::slice::from_raw_parts(
                idx_buffer.as_ptr() as *const u8,
                std::mem::size_of_val(idx_buffer),
            )
        });

        (base_vertex, index_offset)
    }

    #[inline]
    pub fn vertex_bytes(&self) -> &[u8] {
        &self.vertices
    }

    #[inline]
    pub fn index_bytes(&self) -> &[u8] {
        &self.indices
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
    }
}

// ----------------------------------------------
// UiDrawCommand
// ----------------------------------------------

// Recorded during the frame; replayed in the UI render pass.
pub struct UiDrawCommand {
    pub first_index: u32,
    pub index_count: u32,
    pub base_vertex: i32,
    pub texture:     render::texture::TextureHandle,
    pub clip_rect:   Rect,
}

// ----------------------------------------------
// GpuVertexIndexBuffers
// ----------------------------------------------

// Vertex + Index GPU buffers that grow lazily to accommodate frame data.
pub struct GpuVertexIndexBuffers {
    label:           &'static str,
    vertex_buffer:   wgpu::Buffer,
    index_buffer:    wgpu::Buffer,
    vertex_capacity: usize, // In bytes.
    index_capacity:  usize, // In bytes.
}

impl GpuVertexIndexBuffers {
    pub fn new(device: &wgpu::Device, label: &'static str, vb_bytes: usize, ib_bytes: usize) -> Self {
        debug_assert!(!label.is_empty());

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format_fixed_string!(64, "{label}_vb")),
            size: vb_bytes as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format_fixed_string!(64, "{label}_ib")),
            size: ib_bytes as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            label,
            vertex_buffer,
            index_buffer,
            vertex_capacity: vb_bytes,
            index_capacity: ib_bytes,
        }
    }

    pub fn ensure_capacity(&mut self, device: &wgpu::Device, vb_bytes: usize, ib_bytes: usize) {
        if vb_bytes > self.vertex_capacity {
            let new_cap = vb_bytes.next_power_of_two();
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format_fixed_string!(64, "{}_vb", self.label)),
                size: new_cap as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.vertex_capacity = new_cap;
        }

        if ib_bytes > self.index_capacity {
            let new_cap = ib_bytes.next_power_of_two();
            self.index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format_fixed_string!(64, "{}_ib", self.label)),
                size: new_cap as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.index_capacity = new_cap;
        }
    }

    // Upload a typed batch (Pod vertices + indices) the GPU.
    pub fn upload<V: bytemuck::Pod, I: bytemuck::Pod>(&mut self,
                                                      device: &wgpu::Device,
                                                      queue: &wgpu::Queue,
                                                      vertices: &[V],
                                                      indices: &[I])
    {
        let vb_bytes = std::mem::size_of_val(vertices);
        let ib_bytes = std::mem::size_of_val(indices);
        self.ensure_capacity(device, vb_bytes, ib_bytes);

        if !vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(vertices));
        }

        if !indices.is_empty() {
            queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(indices));
        }
    }

    // Upload raw bytes (for UI batch data that isn't bytemuck::Pod).
    // Handles the 4-byte alignment padding required by wgpu's write_buffer.
    pub fn upload_bytes(&mut self,
                        device: &wgpu::Device,
                        queue: &wgpu::Queue,
                        vertex_bytes: &[u8],
                        index_bytes: &[u8])
    {
        let vb_bytes = align_to_4(vertex_bytes.len());
        let ib_bytes = align_to_4(index_bytes.len());
        self.ensure_capacity(device, vb_bytes, ib_bytes);

        if !vertex_bytes.is_empty() {
            let data = pad_to_alignment::<4>(vertex_bytes);
            queue.write_buffer(&self.vertex_buffer, 0, &data);
        }

        if !index_bytes.is_empty() {
            let data = pad_to_alignment::<4>(index_bytes);
            queue.write_buffer(&self.index_buffer, 0, &data);
        }
    }

    // Bind vertex and index buffers to a render pass.
    pub fn bind_to_render_pass(&self, pass: &mut wgpu::RenderPass, index_format: wgpu::IndexFormat) {
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), index_format);
    }
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

// Round up to the next multiple of 4.
#[inline]
pub fn align_to_4(n: usize) -> usize {
    (n + 3) & !3
}

// Pad a byte slice to ALIGN-byte boundary. Returns borrowed data if already aligned.
#[inline]
pub fn pad_to_alignment<const ALIGN: usize>(data: &[u8]) -> Cow<'_, [u8]> {
    let remainder = data.len() % ALIGN;
    if remainder == 0 {
        Cow::Borrowed(data)
    } else {
        let mut padded = data.to_vec();
        padded.resize(data.len() + ALIGN - remainder, 0);
        Cow::Owned(padded)
    }
}
