// What the UI drew in a frame, in the form the render backends draw it.
//
// The UI system fills a UiDrawData at the end of each frame (ui/renderer.rs), and
// RenderSystem::end_frame draws it over the scene: one upload of every vertex and index, then
// one draw call per command, each with its own texture and scissor rect.
//
// UI colors use premultiplied alpha, the way egui produces them, and the UI pipelines blend with
// (ONE, ONE_MINUS_SRC_ALPHA). Textures are stored with straight alpha, like every other texture,
// and the UI shaders premultiply each texel before applying the vertex color.

use common::{Size, Vec2};

use super::texture::TextureHandle;

// ----------------------------------------------
// UiVertex / UiIndex
// ----------------------------------------------

// Laid out like egui's epaint::Vertex, so its meshes can be copied as they are.
#[repr(C)]
#[derive(Copy, Clone, Default, PartialEq, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex {
    pub pos: [f32; 2],  // In window points, from the top-left corner.
    pub uv: [f32; 2],   // (0, 0) is the texture's top-left corner.
    pub color: [u8; 4], // RGBA with premultiplied alpha. Multiplies the (premultiplied) texel.
}

const _: () = {
    assert!(std::mem::size_of::<UiVertex>() == 20);
    assert!(std::mem::offset_of!(UiVertex, pos) == 0);
    assert!(std::mem::offset_of!(UiVertex, uv) == 8);
    assert!(std::mem::offset_of!(UiVertex, color) == 16);
};

// Indices count from the first vertex of their mesh (see UiDrawCommand::base_vertex).
pub type UiIndex = u32;

// ----------------------------------------------
// UiClipRect
// ----------------------------------------------

// A scissor rect in framebuffer pixels, from the top-left corner.
// Never empty, and never reaches outside the framebuffer.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct UiClipRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl UiClipRect {
    // From the corners of a clip rect in window points: scaled to pixels, rounded to the nearest
    // pixel and clamped to the framebuffer. None if nothing of it is left.
    pub fn from_points(min: Vec2, max: Vec2, pixels_per_point: Vec2, framebuffer_size: Size) -> Option<Self> {
        let fb_width = framebuffer_size.width.max(0) as f32;
        let fb_height = framebuffer_size.height.max(0) as f32;

        let min_x = (min.x * pixels_per_point.x).round().clamp(0.0, fb_width);
        let min_y = (min.y * pixels_per_point.y).round().clamp(0.0, fb_height);
        let max_x = (max.x * pixels_per_point.x).round().clamp(0.0, fb_width);
        let max_y = (max.y * pixels_per_point.y).round().clamp(0.0, fb_height);

        if !(max_x > min_x && max_y > min_y) {
            return None; // Also rejects NaNs.
        }

        Some(Self {
            x: min_x as u32,
            y: min_y as u32,
            width: (max_x - min_x) as u32,
            height: (max_y - min_y) as u32,
        })
    }
}

// ----------------------------------------------
// UiDrawCommand
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct UiDrawCommand {
    pub first_index: u32, // Into UiDrawData::indices().
    pub index_count: u32, // Triangles, so a multiple of 3.
    pub base_vertex: u32, // Added to each index: the first vertex of the command's mesh.
    pub texture: TextureHandle,
    pub clip_rect: UiClipRect,
}

// Where a mesh added with UiDrawData::add_mesh starts.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct UiMeshStart {
    pub base_vertex: u32,
    pub first_index: u32,
}

// ----------------------------------------------
// UiDrawData
// ----------------------------------------------

// Everything the UI drew in a frame, in drawing order. Its buffers keep their capacity from one
// frame to the next.
#[derive(Default)]
pub struct UiDrawData {
    vertices: Vec<UiVertex>,
    indices: Vec<UiIndex>,
    commands: Vec<UiDrawCommand>,
}

impl UiDrawData {
    #[inline]
    pub fn vertices(&self) -> &[UiVertex] {
        &self.vertices
    }

    #[inline]
    pub fn indices(&self) -> &[UiIndex] {
        &self.indices
    }

    #[inline]
    pub fn commands(&self) -> &[UiDrawCommand] {
        &self.commands
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn triangle_count(&self) -> u32 {
        self.commands.iter().map(|command| command.index_count / 3).sum()
    }

    #[inline]
    pub(crate) fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.commands.clear();
    }

    // Appends a mesh: its vertices, and indices that count from its first vertex. Commands then
    // draw ranges of its indices, starting from the returned position.
    pub(crate) fn add_mesh(&mut self, vertices: &[UiVertex], indices: &[UiIndex]) -> UiMeshStart {
        let start = UiMeshStart {
            base_vertex: self.vertices.len().try_into().expect("Too many UI vertices!"),
            first_index: self.indices.len().try_into().expect("Too many UI indices!"),
        };

        self.vertices.extend_from_slice(vertices);
        self.indices.extend_from_slice(indices);
        start
    }

    #[inline]
    pub(crate) fn add_command(&mut self, command: UiDrawCommand) {
        debug_assert!(command.index_count != 0 && command.index_count.is_multiple_of(3));
        debug_assert!((command.first_index + command.index_count) as usize <= self.indices.len());
        debug_assert!((command.base_vertex as usize) < self.vertices.len());

        self.commands.push(command);
    }
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const FRAMEBUFFER: Size = Size::new(200, 100);

    fn clip(min: (f32, f32), max: (f32, f32), scale: f32) -> Option<UiClipRect> {
        UiClipRect::from_points(Vec2::new(min.0, min.1), Vec2::new(max.0, max.1), Vec2::new(scale, scale), FRAMEBUFFER)
    }

    #[test]
    fn clip_rect_scales_rounds_and_clamps() {
        assert_eq!(clip((10.0, 5.0), (20.0, 15.0), 2.0), Some(UiClipRect { x: 20, y: 10, width: 20, height: 20 }));
        assert_eq!(clip((10.4, 5.6), (20.6, 15.4), 1.0), Some(UiClipRect { x: 10, y: 6, width: 11, height: 9 }));
        assert_eq!(clip((-50.0, -50.0), (500.0, 500.0), 1.0), Some(UiClipRect { x: 0, y: 0, width: 200, height: 100 }));
        assert_eq!(
            clip((f32::NEG_INFINITY, f32::NEG_INFINITY), (f32::INFINITY, f32::INFINITY), 2.0),
            Some(UiClipRect { x: 0, y: 0, width: 200, height: 100 })
        );
    }

    #[test]
    fn clip_rect_with_nothing_left_is_none() {
        assert_eq!(clip((10.0, 10.0), (10.0, 20.0), 1.0), None); // Zero width.
        assert_eq!(clip((10.0, 10.0), (20.0, 10.2), 1.0), None); // Less than half a pixel high.
        assert_eq!(clip((300.0, 10.0), (400.0, 20.0), 1.0), None); // Right of the framebuffer.
        assert_eq!(clip((20.0, 10.0), (10.0, 20.0), 1.0), None); // Inverted.
        assert_eq!(clip((f32::NAN, 0.0), (10.0, 10.0), 1.0), None);
    }

    #[test]
    fn meshes_and_commands_keep_their_offsets() {
        let vertex = UiVertex::default();
        let mut draw_data = UiDrawData::default();

        let first = draw_data.add_mesh(&[vertex; 4], &[0, 1, 2, 2, 3, 0]);
        let second = draw_data.add_mesh(&[vertex; 3], &[0, 1, 2]);

        assert_eq!(first, UiMeshStart { base_vertex: 0, first_index: 0 });
        assert_eq!(second, UiMeshStart { base_vertex: 4, first_index: 6 });
        assert_eq!(draw_data.vertices().len(), 7);
        assert_eq!(draw_data.indices(), &[0, 1, 2, 2, 3, 0, 0, 1, 2]);

        let clip_rect = UiClipRect { x: 0, y: 0, width: 10, height: 10 };
        draw_data.add_command(UiDrawCommand {
            first_index: first.first_index + 3,
            index_count: 3,
            base_vertex: first.base_vertex,
            texture: TextureHandle::White,
            clip_rect,
        });
        draw_data.add_command(UiDrawCommand {
            first_index: second.first_index,
            index_count: 3,
            base_vertex: second.base_vertex,
            texture: TextureHandle::Index(7),
            clip_rect,
        });

        assert_eq!(draw_data.commands().len(), 2);
        assert_eq!(draw_data.triangle_count(), 2);
        assert!(!draw_data.is_empty());

        draw_data.clear();
        assert!(draw_data.is_empty() && draw_data.vertices().is_empty() && draw_data.indices().is_empty());
    }
}
