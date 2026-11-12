// The Custom Game dialog's map preview: the minimap's bake (crates/minimap_bake) of a map that
// isn't loaded, drawn in the world's isometric projection (a diamond, north up and to the right) on
// the sea, with its roads and buildings painted over it cell by cell, in a realm's minimap style.
//
// A blank map's grid is one terrain, and a preset's comes from its layout (preset_maps::preset_layout).
// A save's map would need the save's tile map read, which for the sample saves (up to 50 MB of JSON)
// takes seconds: the dialog shows the realm's emblem for those instead.
// TODO(realm-ui): preview a save's map, e.g. from a small preview image written beside the save, or
// its tile map read on a worker thread.

use minimap_bake::{Cover, Grid, Scratch, Terrain};

use common::Size;

use crate::{debug::preset_maps::{self, PresetGround}, tile::{minimap::style::MinimapStyle, sets::PresetTiles}};

// The preview image's size in pixels at the mockup's shape (its well is 470 × 330 points). The
// dialog bakes it at its well's shape (preview_pixels), so the sea fills the well.
pub const PREVIEW_PIXELS: (u32, u32) = (564, 396);

// The image size for a well `aspect` (height over width) high: PREVIEW_PIXELS's width, and a
// height from 16 pixels to twice the width.
pub fn preview_pixels(aspect: f32) -> (u32, u32) {
    let width = PREVIEW_PIXELS.0;
    let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { PREVIEW_PIXELS.1 as f32 / width as f32 };
    (width, ((width as f32 * aspect).round() as u32).clamp(16, width * 2))
}

// How much of the image's width or height the map's diamond takes at most.
const DIAMOND_FILL: f32 = 0.86;

// Largest side of the bake, in pixels: enough for the preview, which is small.
const MAX_BAKE_SIDE: u32 = 512;

// What a map holds, cell by cell (row-major: x + y * width), as far as the preview draws it.
#[derive(Clone, PartialEq, Debug)]
pub struct PreviewGrid {
    pub size: Size,
    pub terrain: Vec<Terrain>,
    pub cover: Vec<Cover>,
    pub roads: Vec<bool>,
    pub buildings: Vec<Option<[f32; 3]>>, // A building's colour (0..1), from its sector.
}

// What the preview's details say of a map: shares of its cells, in percent.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct GridStats {
    pub water_percent: u32,
    pub woodland_percent: u32,
}

impl PreviewGrid {
    fn new(size: Size, terrain: Terrain) -> Self {
        let count = (size.width.max(0) * size.height.max(0)) as usize;
        Self { size, terrain: vec![terrain; count], cover: vec![Cover::None; count], roads: vec![false; count], buildings: vec![None; count] }
    }

    // A blank map: one terrain all over.
    pub fn blank(width: i32, height: i32, terrain: PresetTiles) -> Self {
        let ground = if terrain == PresetTiles::Water { Terrain::Water } else { Terrain::Land };
        Self::new(Size::new(width, height), ground)
    }

    // A preset map, from its layout; None if there's no such preset. `style` colours its buildings.
    pub fn preset(preset_number: usize, style: &MinimapStyle) -> Option<Self> {
        let layout = preset_maps::preset_layout(preset_number)?;
        let mut grid = Self::new(layout.size, Terrain::Land);
        for (index, ground) in layout.ground.iter().enumerate() {
            match ground {
                PresetGround::Land => {}
                PresetGround::Road => grid.roads[index] = true,
                PresetGround::VacantLot => grid.terrain[index] = Terrain::VacantLot,
            }
        }
        for tree in layout.trees {
            if let Some(index) = grid.index(tree.x, tree.y) {
                grid.cover[index] = Cover::Vegetation;
            }
        }
        for (base_cell, tile_def) in layout.buildings {
            let color = style.sector_colors.color(tile_def.sector);
            for cell in &tile_def.cell_range(base_cell) {
                if let Some(index) = grid.index(cell.x, cell.y) {
                    grid.cover[index] = Cover::Building;
                    grid.buildings[index] = Some([color.r, color.g, color.b]);
                }
            }
        }
        Some(grid)
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.size.width && y < self.size.height).then(|| (x + y * self.size.width) as usize)
    }

    pub fn stats(&self) -> GridStats {
        let count = self.terrain.len().max(1) as f32;
        let percent = |n: usize| ((n as f32 / count) * 100.0).round() as u32;
        GridStats {
            water_percent: percent(self.terrain.iter().filter(|terrain| **terrain == Terrain::Water).count()),
            woodland_percent: percent(self.cover.iter().filter(|cover| **cover == Cover::Vegetation).count()),
        }
    }

    // The preview image, `pixels` (width, height) large, as RGBA8 rows from the top: the map's
    // diamond baked in `style`, on its deep water colour.
    pub fn render(&self, style: &MinimapStyle, pixels: (u32, u32)) -> Vec<u8> {
        let (image_width, image_height) = pixels;
        let mut image = vec![0u8; (image_width * image_height * 4) as usize];
        let palette = &style.palette;
        let sea = to_rgba(palette.water_deep.rgb255());
        for pixel in image.chunks_exact_mut(4) {
            pixel.copy_from_slice(&sea);
        }

        let (width, height) = (self.size.width, self.size.height);
        if width <= 0 || height <= 0 {
            return image;
        }

        // The bake, `px` pixels per cell, rows from cell y = 0.
        let bake_style = style.bake_style();
        let px = minimap_bake::px_per_cell(width, height, MAX_BAKE_SIDE.min(style.max_texture_side.max(64)));
        let bake_width = width as usize * px as usize;
        let mut bake = vec![0u8; bake_width * height as usize * px as usize * 4];
        let grid = Grid { width, height, terrain: &self.terrain, cover: &self.cover };
        minimap_bake::bake_all(&grid, &bake_style, px, &mut Scratch::default(), &mut bake);

        let road = to_rgba(palette.road.rgb255());

        // The diamond: in units of half a cell's width, the map spans (width + height) across and
        // half that down. a = x - y runs to the right, b = x + y up.
        let span = (width + height) as f32;
        let scale = (image_width as f32 * DIAMOND_FILL / span).min(image_height as f32 * DIAMOND_FILL / (span * 0.5));
        let (center_x, center_y) = (image_width as f32 * 0.5, image_height as f32 * 0.5);
        let (a_mid, b_mid) = ((width - height) as f32 * 0.5, span * 0.5);

        for row in 0..image_height {
            for column in 0..image_width {
                let u = (column as f32 + 0.5 - center_x) / scale;
                let v = (row as f32 + 0.5 - center_y) / scale;
                let a = u + a_mid;
                let b = b_mid - v * 2.0;
                let (x, y) = ((a + b) * 0.5, (b - a) * 0.5);
                if x < 0.0 || y < 0.0 || x >= width as f32 || y >= height as f32 {
                    continue;
                }
                let cell = (x as i32 + y as i32 * width) as usize;
                let color = if let Some([r, g, b]) = self.buildings[cell] {
                    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, 255]
                } else if self.roads[cell] {
                    road
                } else {
                    let source = ((y * px as f32) as usize * bake_width + (x * px as f32) as usize) * 4;
                    [bake[source], bake[source + 1], bake[source + 2], 255]
                };
                let target = ((row * image_width + column) * 4) as usize;
                image[target..target + 4].copy_from_slice(&color);
            }
        }
        image
    }
}

fn to_rgba(rgb: [f32; 3]) -> [u8; 4] {
    [rgb[0].clamp(0.0, 255.0) as u8, rgb[1].clamp(0.0, 255.0) as u8, rgb[2].clamp(0.0, 255.0) as u8, 255]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_grids_and_stats() {
        let grass = PreviewGrid::blank(64, 96, PresetTiles::Grass);
        assert_eq!(grass.terrain.len(), 64 * 96);
        assert_eq!(grass.stats(), GridStats { water_percent: 0, woodland_percent: 0 });
        let water = PreviewGrid::blank(64, 64, PresetTiles::Water);
        assert_eq!(water.stats().water_percent, 100);
    }

    #[test]
    fn previews_draw_the_diamond_on_the_sea() {
        let style = MinimapStyle::default();
        let image = PreviewGrid::blank(64, 64, PresetTiles::Grass).render(&style, PREVIEW_PIXELS);
        let (width, height) = PREVIEW_PIXELS;
        assert_eq!(image.len(), (width * height * 4) as usize);
        let pixel = |x: u32, y: u32| {
            let index = ((y * width + x) * 4) as usize;
            [image[index], image[index + 1], image[index + 2], image[index + 3]]
        };
        let sea = to_rgba(style.palette.water_deep.rgb255());
        assert_eq!(pixel(0, 0), sea); // A corner is past the diamond.
        assert_ne!(pixel(width / 2, height / 2), sea); // Its middle is land.
        assert_eq!(pixel(width / 2, height / 2)[3], 255);

        // At another shape too: a wide, low well.
        let pixels = preview_pixels(0.25);
        assert_eq!(pixels, (564, 141));
        assert_eq!(PreviewGrid::blank(64, 64, PresetTiles::Grass).render(&style, pixels).len(), (564 * 141 * 4) as usize);
        assert_eq!(preview_pixels(0.0), PREVIEW_PIXELS);
        assert_eq!(preview_pixels(100.0).1, 564 * 2);
    }
}
