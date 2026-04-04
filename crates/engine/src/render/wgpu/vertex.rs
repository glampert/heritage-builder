use common::{Color, Vec2};
use crate::render;

// ----------------------------------------------
// Compile time index size to wgpu::IndexFormat
// ----------------------------------------------

pub const fn size_to_index_format<T>() -> wgpu::IndexFormat {
    match std::mem::size_of::<T>() {
        2 => wgpu::IndexFormat::Uint16,
        4 => wgpu::IndexFormat::Uint32,
        _ => panic!("Unsupported DrawIndex format!"),
    }
}

// ----------------------------------------------
// Sprite Vertex
// ----------------------------------------------

// Sprite vertex: position + tex_coords + color (tint baked per-vertex).
#[repr(C)]
#[derive(Copy, Clone, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpriteVertex2D {
    pub position:   [f32; 2],
    pub tex_coords: [f32; 2],
    pub color:      [f32; 4],
}

pub type SpriteIndex2D = u16;

impl SpriteVertex2D {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x2, // position
            1 => Float32x2, // tex_coords
            2 => Float32x4, // color
        ],
    };

    #[inline]
    pub fn new(position: Vec2, tex_coords: Vec2, color: Color) -> Self {
        Self {
            position:   [position.x, position.y],
            tex_coords: [tex_coords.x, tex_coords.y],
            color:      [color.r, color.g, color.b, color.a],
        }
    }
}

// ----------------------------------------------
// Line Vertex
// ----------------------------------------------

// Line / colored-geometry vertex: position + color.
#[repr(C)]
#[derive(Copy, Clone, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineVertex2D {
    pub position: [f32; 2],
    pub color:    [f32; 4],
}

pub type LineIndex2D = u16;

impl LineVertex2D {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x2, // position
            1 => Float32x4, // color
        ],
    };

    #[inline]
    pub fn new(position: Vec2, color: Color) -> Self {
        Self {
            position: [position.x, position.y],
            color:    [color.r, color.g, color.b, color.a],
        }
    }
}

// ----------------------------------------------
// Point Vertex (Reuses LineVertex2D)
// ----------------------------------------------

pub type PointVertex2D = LineVertex2D;
pub type PointIndex2D  = u16;

// ----------------------------------------------
// ImGui UI Vertex
// ----------------------------------------------

// ImGui vertex: position + tex_coords + color (u8x4 normalized).
// Must match render::UiDrawVertex layout exactly.
#[repr(C)]
#[derive(Copy, Clone, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex2D {
    pub position:   [f32; 2],
    pub tex_coords: [f32; 2],
    pub color:      [u8; 4],
}

impl UiVertex2D {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x2, // position
            1 => Float32x2, // tex_coords
            2 => Unorm8x4,  // color (RGBA u8 normalized to 0..1)
        ],
    };
}

// Compile-time check that UiVertex2D matches render::UiDrawVertex in size.
const _: () = assert!(
    std::mem::size_of::<UiVertex2D>() == std::mem::size_of::<render::UiDrawVertex>(),
    "UiVertex2D size must match render::UiDrawVertex"
);
