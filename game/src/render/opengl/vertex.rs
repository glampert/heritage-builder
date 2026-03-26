use super::buffer::{VertexElementDef, VertexTrait};
use crate::{render, utils::{Color, Vec2}};

// ----------------------------------------------
// Sprite Vertex
// ----------------------------------------------

pub type SpriteIndex2D = u16;

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct SpriteVertex2D {
    pub position: Vec2,   // X,Y
    pub tex_coords: Vec2, // U,V
}

impl VertexTrait for SpriteVertex2D {
    fn layout() -> Vec<VertexElementDef> {
        vec![
            // vec2 in_position
            VertexElementDef { count: 2, kind: gl::FLOAT, normalized: gl::FALSE },
            // vec2 in_tex_coords
            VertexElementDef { count: 2, kind: gl::FLOAT, normalized: gl::FALSE },
        ]
    }

    fn stride() -> usize { std::mem::size_of::<Self>() }
}

// ----------------------------------------------
// Line Vertex
// ----------------------------------------------

pub type LineIndex2D = u16;

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct LineVertex2D {
    pub position: Vec2, // X,Y
    pub color: Color,   // R,G,B,A
}

impl VertexTrait for LineVertex2D {
    fn layout() -> Vec<VertexElementDef> {
        vec![
            // vec2 in_position
            VertexElementDef { count: 2, kind: gl::FLOAT, normalized: gl::FALSE },
            // vec4 in_color
            VertexElementDef { count: 4, kind: gl::FLOAT, normalized: gl::FALSE },
        ]
    }

    fn stride() -> usize { std::mem::size_of::<Self>() }
}

// ----------------------------------------------
// Point Vertex
// ----------------------------------------------

pub type PointIndex2D = u16;

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct PointVertex2D {
    pub position: Vec2, // X,Y
    pub color: Color,   // R,G,B,A
    pub size: f32,      // gl_PointSize
}

impl VertexTrait for PointVertex2D {
    fn layout() -> Vec<VertexElementDef> {
        vec![
            // vec2 in_position
            VertexElementDef { count: 2, kind: gl::FLOAT, normalized: gl::FALSE },
            // vec4 in_color
            VertexElementDef { count: 4, kind: gl::FLOAT, normalized: gl::FALSE },
            // float in_point_size
            VertexElementDef { count: 1, kind: gl::FLOAT, normalized: gl::FALSE },
        ]
    }

    fn stride() -> usize { std::mem::size_of::<Self>() }
}

// ----------------------------------------------
// ImGui UI Vertex
// ----------------------------------------------

impl VertexTrait for render::UiDrawVertex {
    fn layout() -> Vec<VertexElementDef> {
        vec![
            // vec2 in_position
            VertexElementDef { count: 2, kind: gl::FLOAT, normalized: gl::FALSE },
            // vec2 in_tex_coords
            VertexElementDef { count: 2, kind: gl::FLOAT, normalized: gl::FALSE },
            // vec2 in_vert_color
            VertexElementDef { count: 4, kind: gl::UNSIGNED_BYTE, normalized: gl::TRUE },
        ]
    }

    fn stride() -> usize { std::mem::size_of::<Self>() }
}
