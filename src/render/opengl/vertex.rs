use crate::utils::{Color, Vec2};
use super::buffer::{VertexTrait, VertexElementDef};

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
            VertexElementDef { count: 2, kind: gl::FLOAT },
            // vec2 in_tex_coords
            VertexElementDef { count: 2, kind: gl::FLOAT },
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
            VertexElementDef { count: 2, kind: gl::FLOAT },
            // vec4 in_color
            VertexElementDef { count: 4, kind: gl::FLOAT },
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
            VertexElementDef { count: 2, kind: gl::FLOAT },
            // vec4 in_color
            VertexElementDef { count: 4, kind: gl::FLOAT },
            // float in_point_size
            VertexElementDef { count: 1, kind: gl::FLOAT },
        ]
    }

    fn stride() -> usize { std::mem::size_of::<Self>() }
}
