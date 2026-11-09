// 9-slice panels: a sprite cut into a 3 × 3 grid by its borders, so it can cover a rect of any
// size. The corners keep their size, the edges stretch (or repeat) along their length and the
// centre fills the middle, e.g. a dialog's frame from a small sprite.
//
// This file is the geometry only, with no egui: the quads to draw and their texture coordinates,
// a pure function of the rect and the slice. UiPainter::nine_slice (immediate.rs) draws them as one
// mesh, and so do the retained widgets that take a panel: UiMenu::with_panel,
// UiMessageBox::with_panel, UiTooltipText::with_panel and UiTextButton::with_frame. Load one with
// UiWidgetContext::load_nine_slice, which knows the texture's size.

use smallvec::SmallVec;

use common::{Rect, Vec2};

use crate::render::texture::TextureHandle;

// ----------------------------------------------
// UiNineSliceCenter
// ----------------------------------------------

// How the middle of a 9-slice fills its rect.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum UiNineSliceCenter {
    // The centre stretched over the middle, and the edges along their length.
    #[default]
    Stretch,

    // The centre repeated at its own size, and the edges along their length, from the top left;
    // the last tile of a row or column is cut where the middle ends. For paper or stone grain.
    // Each repeat shows its part from half a texel in, so linear filtering doesn't blend in the
    // border's color at the seams when the art is drawn larger than its pixels; the art should
    // repeat cleanly at its edges.
    Tile,

    // Like Tile, but each tile is scaled a little, up or down, so a whole number of them fills
    // the middle and the edges along their length, as CSS's `border-image-repeat: round` does: no
    // tile is cut, so a motif (petals, gilt) or the grain ends at the borders as it starts. A
    // tile changes size by at most half its own. For art drawn to repeat.
    Round,
}

impl UiNineSliceCenter {
    // Whether the middle and the edges repeat (Tile or Round).
    #[inline]
    pub fn repeats(self) -> bool {
        !matches!(self, Self::Stretch)
    }
}

// ----------------------------------------------
// UiNineSlice
// ----------------------------------------------

// A sprite and the borders that cut it into nine parts.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct UiNineSlice {
    pub texture: TextureHandle,
    pub texture_size: Vec2,       // In pixels.
    pub slice_px: [f32; 4],       // The borders in pixels: left, top, right, bottom.
    pub scale: f32,               // Points per pixel: 0.5 draws 2× art at its intended size.
    pub center: UiNineSliceCenter,
}

// One part of a 9-slice as drawn: a rect in points and the part of the texture it shows.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct UiNineSliceQuad {
    pub rect: Rect,
    pub uv: Rect, // (0, 0) is the texture's top left, (1, 1) its bottom right.
}

pub type UiNineSliceQuads = SmallVec<[UiNineSliceQuad; 9]>;

impl UiNineSlice {
    // A tiled middle repeats at most this many times across or down, larger tiles beyond that,
    // so a panel never turns into thousands of quads.
    pub const MAX_TILES_PER_AXIS: usize = 64;

    // `slice_px`: the borders in pixels, left, top, right, bottom. At one point per pixel, with
    // its middle stretched.
    pub fn new(texture: TextureHandle, texture_size: Vec2, slice_px: [f32; 4]) -> Self {
        Self { texture, texture_size, slice_px, scale: 1.0, center: UiNineSliceCenter::Stretch }
    }

    // Points per pixel, e.g. 0.5 for art drawn at 2×.
    pub fn with_scale(mut self, scale: f32) -> Self {
        debug_assert!(scale > 0.0);
        self.scale = scale;
        self
    }

    pub fn with_center(mut self, center: UiNineSliceCenter) -> Self {
        self.center = center;
        self
    }

    // The borders in points on a rect large enough for them: left, top, right, bottom.
    pub fn border_points(&self) -> [f32; 4] {
        self.clamped_slice_px().map(|border| border * self.points_per_pixel())
    }

    // The quads that draw the slice over `rect`, in drawing order (rows from the top). Corners
    // keep their size and shape. On a rect smaller than its borders, all four borders shrink by
    // the same factor until they fit, so they meet instead of overlapping and the corners aren't
    // squashed. Parts with no area are left out.
    pub fn quads(&self, rect: Rect) -> UiNineSliceQuads {
        let mut quads = UiNineSliceQuads::new();

        let size = rect.size();
        let texture_size = self.texture_size;
        if !(size.x > 0.0 && size.y > 0.0 && texture_size.x > 0.0 && texture_size.y > 0.0) {
            return quads;
        }

        let [left, top, right, bottom] = self.clamped_slice_px();
        let mut points_per_pixel = self.points_per_pixel();

        // Shrink the borders to fit the rect, keeping the corners' shape.
        let fit = |border_points: f32, available: f32| if border_points > available { available / border_points } else { 1.0 };
        let shrink = fit((left + right) * points_per_pixel, size.x).min(fit((top + bottom) * points_per_pixel, size.y));
        points_per_pixel *= shrink;

        // The edges of the three columns and rows, in points and in texture coordinates.
        let xs = edges(rect.min.x, rect.max.x, left * points_per_pixel, right * points_per_pixel);
        let ys = edges(rect.min.y, rect.max.y, top * points_per_pixel, bottom * points_per_pixel);
        let us = [0.0, left / texture_size.x, (texture_size.x - right) / texture_size.x, 1.0];
        let vs = [0.0, top / texture_size.y, (texture_size.y - bottom) / texture_size.y, 1.0];

        // A tiled middle repeats at the centre's own size in points, half a texel in; a round one
        // at the nearest size that fits a whole number of tiles.
        let tile = self.center.repeats();
        let round = self.center == UiNineSliceCenter::Round;
        let tile_width = (texture_size.x - left - right) * points_per_pixel;
        let tile_height = (texture_size.y - top - bottom) * points_per_pixel;
        let row_tile = Tiling { size: tile_height, inset: 0.5 / texture_size.y, round };
        let column_tile = Tiling { size: tile_width, inset: 0.5 / texture_size.x, round };

        for row in 0..3 {
            let row_spans = spans(ys[row], ys[row + 1], vs[row], vs[row + 1], (tile && row == 1).then_some(row_tile));

            for column in 0..3 {
                let column_spans =
                    spans(xs[column], xs[column + 1], us[column], us[column + 1], (tile && column == 1).then_some(column_tile));

                for &(y0, y1, v0, v1) in &row_spans {
                    for &(x0, x1, u0, u1) in &column_spans {
                        quads.push(UiNineSliceQuad {
                            rect: Rect { min: Vec2::new(x0, y0), max: Vec2::new(x1, y1) },
                            uv: Rect { min: Vec2::new(u0, v0), max: Vec2::new(u1, v1) },
                        });
                    }
                }
            }
        }

        quads
    }

    // ----------------------
    // Internal:
    // ----------------------

    #[inline]
    fn points_per_pixel(&self) -> f32 {
        if self.scale > 0.0 { self.scale } else { 1.0 }
    }

    // The borders, none negative, and each pair no wider than the texture (shrunk alike if they
    // are), so the parts' texture coordinates never cross.
    fn clamped_slice_px(&self) -> [f32; 4] {
        let [left, top, right, bottom] = self.slice_px.map(|border| if border.is_finite() { border.max(0.0) } else { 0.0 });
        let fit = |a: f32, b: f32, size: f32| {
            let size = size.max(0.0);
            if a + b > size && a + b > 0.0 { (a * size / (a + b), b * size / (a + b)) } else { (a, b) }
        };
        let (left, right) = fit(left, right, self.texture_size.x);
        let (top, bottom) = fit(top, bottom, self.texture_size.y);
        [left, top, right, bottom]
    }
}

// The four edges of a row or column of three parts between `min` and `max`, with borders `start`
// and `end` points wide. The middle is never negative.
fn edges(min: f32, max: f32, start: f32, end: f32) -> [f32; 4] {
    let first = (min + start).min(max);
    let last = (max - end).max(first);
    [min, first, last, max]
}

// How a part repeats along an axis: its tiles' length in points, how far in from the part's
// edges, in texture coordinates, each tile's texture starts and ends (half a texel), and whether
// the tiles are resized so a whole number of them fits (UiNineSliceCenter::Round).
#[derive(Copy, Clone)]
struct Tiling {
    size: f32,
    inset: f32,
    round: bool,
}

// The number of whole tiles of `tile` points that best fills `length` points, at least one: the
// nearest count, so each tile is resized by at most half its size.
fn round_tile_count(length: f32, tile: f32) -> usize {
    ((length / tile).round() as usize).max(1)
}

// The spans of one part along an axis, from `p0` to `p1` in points and `t0` to `t1` in texture
// coordinates: the whole part, or tiles, the last one cut short. None if the part has no size.
fn spans(p0: f32, p1: f32, t0: f32, t1: f32, tiling: Option<Tiling>) -> SmallVec<[(f32, f32, f32, f32); 1]> {
    const EPSILON: f32 = 1e-4;

    let mut spans = SmallVec::new();
    let length = p1 - p0;
    if length <= EPSILON {
        return spans;
    }

    let (tile, inset, round) = match tiling {
        Some(tiling) if tiling.size > EPSILON => (tiling.size, tiling.inset, tiling.round),
        _ => {
            spans.push((p0, p1, t0, t1));
            return spans;
        }
    };

    // Each tile's texture from half a texel in, so its seams don't sample the borders; a part
    // one texel wide shows that texel's centre.
    let inset = inset.clamp(0.0, (t1 - t0).max(0.0) * 0.5);
    let (t0, t1) = (t0 + inset, t1 - inset);

    // Larger tiles past the cap: exactly MAX_TILES_PER_AXIS of them, none cut. Round: the
    // nearest whole number of tiles, resized to fill the part exactly.
    const MAX_TILES: usize = UiNineSlice::MAX_TILES_PER_AXIS;
    let tiles = length / tile;
    let (tile, count) = if tiles > MAX_TILES as f32 {
        (length / MAX_TILES as f32, MAX_TILES)
    } else if round {
        let count = round_tile_count(length, tile);
        (length / count as f32, count)
    } else {
        (tile, ((tiles - 1e-3).ceil() as usize).max(1))
    };

    for index in 0..count {
        let start = p0 + tile * index as f32;
        let end = if index + 1 == count { p1 } else { start + tile };
        let fraction = ((end - start) / tile).min(1.0);
        spans.push((start, end, t0, t0 + (t1 - t0) * fraction));
    }

    spans
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn slice(texture_size: (f32, f32), slice_px: [f32; 4]) -> UiNineSlice {
        UiNineSlice::new(TextureHandle::White, Vec2::new(texture_size.0, texture_size.1), slice_px)
    }

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect { min: Vec2::new(x0, y0), max: Vec2::new(x1, y1) }
    }

    fn assert_near(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-4, "{a} != {b}");
    }

    fn assert_rect(actual: Rect, expected: Rect) {
        assert_near(actual.min.x, expected.min.x);
        assert_near(actual.min.y, expected.min.y);
        assert_near(actual.max.x, expected.max.x);
        assert_near(actual.max.y, expected.max.y);
    }

    #[test]
    fn stretched_slice_has_nine_quads_with_fixed_borders() {
        let slice = slice((100.0, 60.0), [10.0, 8.0, 20.0, 12.0]);
        let quads = slice.quads(rect(10.0, 20.0, 210.0, 120.0));
        assert_eq!(quads.len(), 9);

        // Columns at 10, 20, 190, 210 and rows at 20, 28, 108, 120.
        let xs = [10.0, 20.0, 190.0, 210.0];
        let ys = [20.0, 28.0, 108.0, 120.0];
        let us = [0.0, 0.1, 0.8, 1.0];
        let vs = [0.0, 8.0 / 60.0, 48.0 / 60.0, 1.0];

        for row in 0..3 {
            for column in 0..3 {
                let quad = quads[row * 3 + column];
                assert_rect(quad.rect, rect(xs[column], ys[row], xs[column + 1], ys[row + 1]));
                assert_rect(quad.uv, rect(us[column], vs[row], us[column + 1], vs[row + 1]));
            }
        }

        // The corners keep their size in pixels.
        assert_eq!(quads[0].rect.size(), Vec2::new(10.0, 8.0));
        assert_eq!(quads[8].rect.size(), Vec2::new(20.0, 12.0));
        assert_eq!(slice.border_points(), [10.0, 8.0, 20.0, 12.0]);
    }

    #[test]
    fn scale_sizes_borders_in_points_not_uvs() {
        let slice = slice((64.0, 64.0), [16.0, 16.0, 16.0, 16.0]).with_scale(0.5);
        let quads = slice.quads(rect(0.0, 0.0, 100.0, 50.0));
        assert_eq!(quads.len(), 9);
        assert_rect(quads[0].rect, rect(0.0, 0.0, 8.0, 8.0));
        assert_rect(quads[0].uv, rect(0.0, 0.0, 0.25, 0.25));
        assert_rect(quads[4].rect, rect(8.0, 8.0, 92.0, 42.0));
        assert_rect(quads[4].uv, rect(0.25, 0.25, 0.75, 0.75));
        assert_rect(quads[8].rect, rect(92.0, 42.0, 100.0, 50.0));
        assert_eq!(slice.border_points(), [8.0, 8.0, 8.0, 8.0]);
    }

    #[test]
    fn borders_shrink_on_a_small_rect_without_overlapping() {
        // 20 points of borders across, on a rect 10 points wide: all borders shrink to half.
        let slice = slice((40.0, 40.0), [10.0, 10.0, 10.0, 10.0]);
        let quads = slice.quads(rect(0.0, 0.0, 10.0, 40.0));

        // No middle column: two columns of three rows.
        assert_eq!(quads.len(), 6);
        assert_rect(quads[0].rect, rect(0.0, 0.0, 5.0, 5.0)); // Top left corner, still square.
        assert_rect(quads[0].uv, rect(0.0, 0.0, 0.25, 0.25)); // The whole corner, smaller.
        assert_rect(quads[1].rect, rect(5.0, 0.0, 10.0, 5.0));
        assert_rect(quads[1].uv, rect(0.75, 0.0, 1.0, 0.25));
        assert_rect(quads[2].rect, rect(0.0, 5.0, 5.0, 35.0)); // Left edge.
        assert_rect(quads[5].rect, rect(5.0, 35.0, 10.0, 40.0)); // Bottom right corner.

        for quad in &quads {
            assert!(quad.rect.min.x >= 0.0 && quad.rect.max.x <= 10.0);
            assert!(quad.rect.width() > 0.0 && quad.rect.height() > 0.0);
        }

        // Smaller than its borders both ways: shrunk by the smaller factor (to a fifth here), so
        // the corners stay square and meet top to bottom, with the top and bottom edges between.
        let quads = slice.quads(rect(0.0, 0.0, 8.0, 4.0));
        assert_eq!(quads.len(), 6);
        assert_rect(quads[0].rect, rect(0.0, 0.0, 2.0, 2.0));
        assert_rect(quads[1].rect, rect(2.0, 0.0, 6.0, 2.0));
        assert_rect(quads[1].uv, rect(0.25, 0.0, 0.75, 0.25));
        assert_rect(quads[5].rect, rect(6.0, 2.0, 8.0, 4.0));
    }

    #[test]
    fn tiled_center_repeats_and_cuts_the_last_tile() {
        // A 20 × 20 pixel centre in an 80 × 40 middle: 4 × 2 tiles.
        let slice = slice((40.0, 40.0), [10.0, 10.0, 10.0, 10.0]).with_center(UiNineSliceCenter::Tile);
        let quads = slice.quads(rect(0.0, 0.0, 100.0, 60.0));

        // Each repeat's texture starts and ends half a texel (1/80) inside the centre's.
        const HALF_TEXEL: f32 = 0.5 / 40.0;
        let (c0, c1) = (0.25 + HALF_TEXEL, 0.75 - HALF_TEXEL);

        // Corners 4, top and bottom edges 4 each, left and right edges 2 each, centre 8.
        assert_eq!(quads.len(), 4 + 8 + 4 + 8);
        let center: Vec<_> = quads.iter().filter(|quad| quad.rect.min.x >= 10.0 && quad.rect.max.x <= 90.0 && quad.rect.min.y >= 10.0 && quad.rect.max.y <= 50.0).collect();
        assert_eq!(center.len(), 8);
        for quad in &center {
            assert_eq!(quad.rect.size(), Vec2::new(20.0, 20.0));
            assert_rect(quad.uv, rect(c0, c0, c1, c1));
        }

        // The corners aren't repeated, so their texture runs to the borders.
        assert_rect(quads[0].uv, rect(0.0, 0.0, 0.25, 0.25));

        // A 70 point middle: tiles of 20, 20, 20 and a last one of 10 showing half the centre.
        // The top edge repeats across only, so its texture is inset across only.
        let quads = slice.quads(rect(0.0, 0.0, 90.0, 40.0));
        let top_edge: Vec<_> = quads.iter().filter(|quad| quad.rect.min.y == 0.0 && quad.rect.min.x >= 10.0 && quad.rect.max.x <= 80.0).collect();
        assert_eq!(top_edge.len(), 4);
        assert_rect(top_edge[0].uv, rect(c0, 0.0, c1, 0.25));
        assert_rect(top_edge[3].rect, rect(70.0, 0.0, 80.0, 10.0));
        assert_rect(top_edge[3].uv, rect(c0, 0.0, c0 + (c1 - c0) * 0.5, 0.25));
        // The left edge spans 20 points: one whole tile, inset down only.
        let left_edge: Vec<_> = quads.iter().filter(|quad| quad.rect.min.x == 0.0 && quad.rect.min.y >= 10.0 && quad.rect.max.y <= 30.0).collect();
        assert_eq!(left_edge.len(), 1);
        assert_rect(left_edge[0].uv, rect(0.0, c0, 0.25, c1));
    }

    #[test]
    fn round_center_fits_whole_tiles() {
        // A 20 × 20 pixel centre. A 70 point middle: 4 tiles of 17.5 (3.5 rounds to 4), none cut.
        let slice = slice((40.0, 40.0), [10.0, 10.0, 10.0, 10.0]).with_center(UiNineSliceCenter::Round);
        let quads = slice.quads(rect(0.0, 0.0, 90.0, 40.0));

        const HALF_TEXEL: f32 = 0.5 / 40.0;
        let (c0, c1) = (0.25 + HALF_TEXEL, 0.75 - HALF_TEXEL);

        let top_edge: Vec<_> = quads.iter().filter(|quad| quad.rect.min.y == 0.0 && quad.rect.min.x >= 10.0 && quad.rect.max.x <= 80.0).collect();
        assert_eq!(top_edge.len(), 4);
        for (index, quad) in top_edge.iter().enumerate() {
            let x0 = 10.0 + 17.5 * index as f32;
            assert_rect(quad.rect, rect(x0, 0.0, x0 + 17.5, 10.0));
            assert_rect(quad.uv, rect(c0, 0.0, c1, 0.25)); // Every tile whole.
        }

        // 20 points down: one whole tile, as Tile would draw it.
        let left_edge: Vec<_> = quads.iter().filter(|quad| quad.rect.min.x == 0.0 && quad.rect.min.y >= 10.0 && quad.rect.max.y <= 30.0).collect();
        assert_eq!(left_edge.len(), 1);
        assert_rect(left_edge[0].rect, rect(0.0, 10.0, 10.0, 30.0));
        assert_rect(left_edge[0].uv, rect(0.0, c0, 0.25, c1));

        // The centre: 4 × 1 whole tiles.
        let center: Vec<_> = quads.iter().filter(|quad| quad.rect.min.x >= 10.0 && quad.rect.max.x <= 80.0 && quad.rect.min.y >= 10.0 && quad.rect.max.y <= 30.0).collect();
        assert_eq!(center.len(), 4);
        for quad in &center {
            assert_rect(quad.uv, rect(c0, c0, c1, c1));
            assert_near(quad.rect.width(), 17.5);
            assert_near(quad.rect.height(), 20.0);
        }
    }

    #[test]
    fn round_center_shrinks_or_grows_by_at_most_half_a_tile() {
        // 2.4 tiles round down to 2 larger ones; 2.6 up to 3 smaller ones; less than one half
        // still draws one tile, stretched or squeezed to fit.
        assert_eq!(round_tile_count(48.0, 20.0), 2);
        assert_eq!(round_tile_count(52.0, 20.0), 3);
        assert_eq!(round_tile_count(5.0, 20.0), 1);
        assert_eq!(round_tile_count(60.0, 20.0), 3);

        let slice = slice((40.0, 40.0), [10.0, 10.0, 10.0, 10.0]).with_center(UiNineSliceCenter::Round);
        for width in [25.0, 47.0, 68.0, 71.0, 133.3, 500.0] {
            let quads = slice.quads(rect(0.0, 0.0, width + 20.0, 40.0));
            let top_edge: Vec<_> = quads.iter().filter(|quad| quad.rect.min.y == 0.0 && quad.rect.min.x >= 10.0 && quad.rect.max.x <= width + 10.0).collect();
            let count = top_edge.len();
            assert_eq!(count, round_tile_count(width, 20.0), "{width}");
            for quad in &top_edge {
                // Same size, the whole tile's texture, between 2/3 and 2 times the tile's size.
                assert_near(quad.rect.width(), width / count as f32);
                assert!(quad.rect.width() >= 20.0 * 2.0 / 3.0 - 1e-3 && quad.rect.width() <= 20.0 * 2.0 + 1e-3, "{width}");
                assert_near(quad.uv.max.x - quad.uv.min.x, 0.5 - 2.0 * (0.5 / 40.0));
            }
            // They fill the edge exactly.
            assert_near(top_edge.last().unwrap().rect.max.x, width + 10.0);
        }

        // The cap holds for round tiles too.
        let tiny = self::slice((3.0, 3.0), [1.0, 1.0, 1.0, 1.0]).with_center(UiNineSliceCenter::Round);
        let quads = tiny.quads(rect(0.0, 0.0, 1002.0, 3.0));
        let middle = quads.iter().filter(|quad| quad.rect.min.x >= 1.0 && quad.rect.max.x <= 1001.0 && quad.rect.min.y == 1.0).count();
        assert_eq!(middle, UiNineSlice::MAX_TILES_PER_AXIS);
    }

    #[test]
    fn a_centre_one_texel_wide_tiles_its_texel_centre() {
        let slice = slice((3.0, 3.0), [1.0, 1.0, 1.0, 1.0]).with_center(UiNineSliceCenter::Tile);
        let quads = slice.quads(rect(0.0, 0.0, 6.0, 3.0));
        let center: Vec<_> = quads.iter().filter(|quad| quad.rect.min.x >= 1.0 && quad.rect.max.x <= 5.0 && quad.rect.min.y == 1.0).collect();
        assert_eq!(center.len(), 4);
        for quad in &center {
            assert_rect(quad.uv, rect(0.5, 0.5, 0.5, 0.5));
        }
    }

    #[test]
    fn tiles_are_capped() {
        let slice = slice((3.0, 3.0), [1.0, 1.0, 1.0, 1.0]).with_center(UiNineSliceCenter::Tile);
        let quads = slice.quads(rect(0.0, 0.0, 1002.0, 3.0)); // A 1000 point middle of 1 point tiles.
        let middle = quads.iter().filter(|quad| quad.rect.min.x >= 1.0 && quad.rect.max.x <= 1001.0 && quad.rect.min.y == 1.0).count();
        assert_eq!(middle, UiNineSlice::MAX_TILES_PER_AXIS);
    }

    #[test]
    fn borders_wider_than_the_texture_are_clamped() {
        let slice = slice((10.0, 10.0), [8.0, -3.0, 12.0, f32::NAN]);
        assert_eq!(slice.clamped_slice_px(), [4.0, 0.0, 6.0, 0.0]);

        let quads = slice.quads(rect(0.0, 0.0, 100.0, 100.0));
        for quad in &quads {
            assert!(quad.uv.min.x <= quad.uv.max.x && quad.uv.min.y <= quad.uv.max.y);
            assert!(quad.uv.min.x >= 0.0 && quad.uv.max.x <= 1.0);
        }
    }

    #[test]
    fn empty_rect_or_texture_draws_nothing() {
        let slice = slice((10.0, 10.0), [2.0, 2.0, 2.0, 2.0]);
        assert!(slice.quads(rect(5.0, 5.0, 5.0, 20.0)).is_empty());
        assert!(slice.quads(rect(5.0, 5.0, 4.0, 4.0)).is_empty());
        assert!(self::slice((0.0, 10.0), [2.0, 2.0, 2.0, 2.0]).quads(rect(0.0, 0.0, 10.0, 10.0)).is_empty());
    }
}
