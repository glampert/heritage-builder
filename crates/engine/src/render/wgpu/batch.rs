use std::{
    convert::{Into, TryFrom},
    fmt::Debug,
};

use common::format_fixed_string;

use crate::render;

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
    indices: Vec<I>,
    entries: Vec<DrawBatchEntry>,
}

impl<V, I> DrawBatch<V, I>
where
    V: Copy + bytemuck::Pod,
    I: Copy + bytemuck::Pod + TryFrom<usize> + Into<usize>,
{
    pub fn new(vertices_capacity: usize, indices_capacity: usize, entries_capacity: usize) -> Self {
        Self {
            vertices: Vec::with_capacity(vertices_capacity),
            indices: Vec::with_capacity(indices_capacity),
            entries: Vec::with_capacity(entries_capacity),
        }
    }

    pub fn add_entry(&mut self, vertices: &[V], indices: &[I], texture: render::texture::TextureHandle)
    where
        <I as TryFrom<usize>>::Error: Debug,
    {
        let first_index = self.add_fast(vertices, indices);
        self.entries.push(DrawBatchEntry { first_index: first_index as u32, index_count: indices.len() as u32, texture });
    }

    pub fn add_fast(&mut self, vertices: &[V], indices: &[I]) -> usize
    where
        <I as TryFrom<usize>>::Error: Debug,
    {
        let ib_start = self.indices.len();
        let vb_base = self.vertices.len();

        for &i in indices {
            let idx: usize = i.into() + vb_base;
            let narrowed: I = idx.try_into().expect("INTEGER OVERFLOW! Value does not fit into index type.");

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
// GpuVertexIndexBuffers
// ----------------------------------------------

// Vertex + Index GPU buffers that grow lazily to accommodate frame data.
pub struct GpuVertexIndexBuffers {
    label: &'static str,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    vertex_capacity: usize, // In bytes.
    index_capacity: usize,  // In bytes.
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

        Self { label, vertex_buffer, index_buffer, vertex_capacity: vb_bytes, index_capacity: ib_bytes }
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
    pub fn upload<V: bytemuck::Pod, I: bytemuck::Pod>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[V],
        indices: &[I],
    ) {
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

    // Bind vertex and index buffers to a render pass.
    pub fn bind_to_render_pass(&self, pass: &mut wgpu::RenderPass, index_format: wgpu::IndexFormat) {
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), index_format);
    }
}
