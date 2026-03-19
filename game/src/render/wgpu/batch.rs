use std::fmt::Debug;
use std::convert::{Into, TryFrom};

use crate::{
    render::TextureHandle,
    utils::Color,
};

// ----------------------------------------------
// DrawBatchEntry
// ----------------------------------------------

pub struct DrawBatchEntry {
    pub first_index: u32,
    pub index_count: u32,
    pub texture:     TextureHandle,
}

// ----------------------------------------------
// DrawBatch
// ----------------------------------------------

pub struct DrawBatch<V: Copy, I: Copy> {
    pub vertices: Vec<V>,
    pub indices:  Vec<I>,
    pub entries:  Vec<DrawBatchEntry>,
}

impl<V, I> DrawBatch<V, I>
where
    V: Copy + bytemuck::Pod,
    I: Copy + bytemuck::Pod + TryFrom<usize> + Into<usize>,
{
    pub fn new(vertices_cap: usize, indices_cap: usize, entries_cap: usize) -> Self {
        Self {
            vertices: Vec::with_capacity(vertices_cap),
            indices:  Vec::with_capacity(indices_cap),
            entries:  Vec::with_capacity(entries_cap),
        }
    }

    pub fn add_entry(&mut self, vertices: &[V], indices: &[I], texture: TextureHandle, _color: Color)
    where
        <I as TryFrom<usize>>::Error: Debug,
    {
        let first_index = self.add_fast(vertices, indices);
        self.entries.push(DrawBatchEntry {
            first_index: first_index as u32,
            index_count: indices.len() as u32,
            texture,
        });
    }

    pub fn add_fast(&mut self, vertices: &[V], indices: &[I]) -> usize
    where
        <I as TryFrom<usize>>::Error: Debug,
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
    pub vertices: Vec<u8>, // Raw imgui::DrawVert bytes.
    pub indices:  Vec<u8>, // Raw imgui::DrawIdx bytes.
}

impl UiDrawBatch {
    pub fn new() -> Self {
        Self {
            vertices: Vec::with_capacity(1024 * std::mem::size_of::<imgui::DrawVert>()),
            indices:  Vec::with_capacity(1024 * std::mem::size_of::<imgui::DrawIdx>()),
        }
    }

    /// Appends a draw list's vertex/index data to the batch.
    /// Returns (base_vertex, index_offset) for use with draw_indexed.
    ///  - base_vertex: the vertex offset (in vertices, not bytes) to pass as base_vertex.
    ///  - index_offset: the index offset (in indices, not bytes) to add to first_index.
    pub fn append_data(&mut self, vtx_buffer: &[imgui::DrawVert], idx_buffer: &[imgui::DrawIdx])
        -> (i32, u32)
    {
        let base_vertex  = (self.vertices.len() / std::mem::size_of::<imgui::DrawVert>()) as i32;
        let index_offset = (self.indices.len()  / std::mem::size_of::<imgui::DrawIdx>()) as u32;

        // imgui::DrawVert doesn't implement bytemuck::Pod, so we reinterpret as raw bytes.
        // SAFETY: DrawVert is a repr(C) struct of f32s and u32 — no padding, no drop, all
        //         bit-patterns valid. Same for DrawIdx (u16 or u32).
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

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
    }
}
