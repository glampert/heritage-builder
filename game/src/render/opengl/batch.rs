use std::fmt::Debug;
use std::convert::{Into, TryFrom};

use crate::{
    utils::Color,
    render::TextureHandle
};

use super::{
    buffer::*,
    shader::ShaderProgram,
    context::{RenderContext, PrimitiveTopology}
};

// ----------------------------------------------
// DrawBatch
// ----------------------------------------------

pub struct DrawBatchEntry {
    slice: IndexBufferSlice,
    pub texture: TextureHandle,
    pub color: Color,
}

pub struct DrawBatch<V: VertexTrait + Copy, I: IndexTrait + Copy + TryFrom<usize> + Into<usize>> {
    vertices: Vec<V>,
    indices: Vec<I>,
    entries: Vec<DrawBatchEntry>,
    vertex_array: VertexArray,
    primitive_topology: PrimitiveTopology,
    needs_sync: bool,
}

impl<V: VertexTrait + Copy, I: IndexTrait + Copy + TryFrom<usize> + Into<usize>> DrawBatch<V, I> {
    pub fn new(vertices_capacity: u32,
               indices_capacity: u32,
               entries_capacity: u32,
               primitive_topology: PrimitiveTopology) -> Self {

        let vertex_layout = V::layout();
        let vertex_stride = V::stride();

        let vertex_buffer = VertexBuffer::with_uninitialized_data(
            vertices_capacity,
            vertex_stride as u32,
            BufferUsageHint::DynamicDraw);

        let index_buffer = IndexBuffer::with_uninitialized_data(
            indices_capacity,
            I::index_type(),
            BufferUsageHint::DynamicDraw);

        Self {
            vertices: if vertices_capacity != 0 { Vec::with_capacity(vertices_capacity as usize) } else { Vec::new() },
            indices:  if indices_capacity  != 0 { Vec::with_capacity(indices_capacity as usize)  } else { Vec::new() },
            entries:  if entries_capacity  != 0 { Vec::with_capacity(entries_capacity as usize)  } else { Vec::new() },
            vertex_array: VertexArray::new(
                vertex_buffer,
                index_buffer,
                &vertex_layout,
                vertex_stride),
            primitive_topology,
            needs_sync: false,
        }
    }

    pub fn add_entry(&mut self,
                     vertices: &[V],
                     indices: &[I],
                     texture: TextureHandle,
                     color: Color)
                     where <I as TryFrom<usize>>::Error: Debug {

        let ib_slice_start = self.add_fast(vertices, indices);

        self.entries.push(DrawBatchEntry {
            slice: IndexBufferSlice {
                start: ib_slice_start as u32,
                count: indices.len() as u32,
            },
            texture,
            color,
        });
    }

    pub fn add_fast(&mut self, vertices: &[V], indices: &[I]) -> usize
                    where <I as TryFrom<usize>>::Error: Debug {

        let new_vb_size = self.vertices.len() + vertices.len();
        if new_vb_size > self.vertex_array.vertex_buffer().count() as usize {
            self.vertices.reserve(vertices.len());
            self.vertex_array.vertex_buffer_mut().resize(new_vb_size);
        }

        let new_ib_size = self.indices.len() + indices.len();
        if new_ib_size > self.vertex_array.index_buffer().count() as usize {
            self.indices.reserve(indices.len());
            self.vertex_array.index_buffer_mut().resize(new_ib_size);
        }

        let ib_slice_start = self.indices.len();
        let vb_base_vertex = self.vertices.len();

        // Add the base vertex offset to each index:
        for &i in indices {
            let index_as_usize: usize = i.into() + vb_base_vertex;

            // Narrow cast with overflow check (e.g. to u32 or u16):
            let index_as_i: I = index_as_usize
                .try_into()
                .expect("INTEGER OVERFLOW! Value does not fit into index type.");

            self.indices.push(index_as_i);
        }

        self.vertices.extend_from_slice(vertices);
        self.needs_sync = true;

        ib_slice_start
    }

    pub fn draw_entries<F>(&self,
                           render_context: &mut RenderContext,
                           shader_program: &ShaderProgram,
                           mut set_shader_vars_fn: F)
                           where F: FnMut(&mut RenderContext, &DrawBatchEntry) {

        if self.entries.is_empty() {
            return;
        }

        debug_assert!(!self.needs_sync); // call sync() first!

        render_context.set_primitive_topology(self.primitive_topology);
        render_context.set_shader_program(shader_program);
        render_context.set_vertex_array(&self.vertex_array);

        for entry in &self.entries {
            set_shader_vars_fn(render_context, entry);
            render_context.draw_indexed(entry.slice.start, entry.slice.count);
        }

        render_context.unset_vertex_array();
    }

    // Draw whole vertex buffer in a single draw-call, ignoring entry textures/colors.
    // Useful for lines and points.
    pub fn draw_fast(&self,
                     render_context: &mut RenderContext,
                     shader_program: &ShaderProgram) {

        if self.vertices.is_empty() {
            return;
        }

        debug_assert!(!self.needs_sync); // call sync() first!

        render_context.set_primitive_topology(self.primitive_topology);
        render_context.set_shader_program(shader_program);
        render_context.set_vertex_array(&self.vertex_array);

        render_context.draw_indexed(0, self.indices.len() as u32);

        render_context.unset_vertex_array();
    }

    pub fn sync(&mut self) {
        if self.vertices.is_empty() || !self.needs_sync {
            return;
        }

        self.vertex_array.vertex_buffer().set_data(&self.vertices);
        self.vertex_array.index_buffer().set_data(&self.indices);

        self.needs_sync = false;
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.entries.clear();
        self.needs_sync = false;
    }
}

// For DrawBatch::draw_entries() if no shader vars need to be set.
#[inline]
pub fn noop_set_shader_vars() -> impl FnMut(&mut RenderContext, &DrawBatchEntry) {
    |_: &mut RenderContext, _: &DrawBatchEntry| {}
}
