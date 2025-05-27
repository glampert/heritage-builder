use smallvec::SmallVec;
use strum::EnumCount;
use strum_macros::{Display, EnumCount, EnumIter};
use serde::Deserialize;

use crate::{
    render::TextureHandle,
    utils::{self, Cell2D, Color, RectTexCoords, Size2D, Point2D, WorldToScreenTransform}
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const BASE_TILE_SIZE: Size2D = Size2D{ width: 64, height: 32 };

// ----------------------------------------------
// TileKind
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Debug, Display, EnumCount, EnumIter, Deserialize)]
pub enum TileKind {
    Empty,   // No tile, draws nothing.
    Blocker, // Draws nothing; blocker for multi-tile buildings, placed in the Buildings layer.
    Terrain,
    Building,
    Unit,
}

pub const TILE_KIND_COUNT: usize = TileKind::COUNT;

// ----------------------------------------------
// TileSprite
// ----------------------------------------------

#[derive(Clone, Deserialize)]
pub struct TileSprite {
    // Name of the tile texture. Resolved into a TextureHandle post load.
    pub name: String,

    // Not stored in serialized data.
    #[serde(skip)]
    pub tex_info: TileTexInfo,
}

// ----------------------------------------------
// TileAnimSet
// ----------------------------------------------

#[derive(Clone, Deserialize)]
pub struct TileAnimSet {
    #[serde(default)]
    pub name: String,

    // Duration of the whole anim in seconds.
    // Optional, can be zero if there's only a single frame.
    #[serde(default)]
    pub duration: f32,

    // True if the animation will loop, false for play only once.
    // Ignored when there's only one frame.
    #[serde(default)]
    pub looping: bool,

    // Textures for each animation frame. Texture handles are resolved after loading.
    // SmallVec optimizes for Terrain (single frame anim).
    pub frames: SmallVec<[TileSprite; 1]>,
}

// ----------------------------------------------
// TileVariation
// ----------------------------------------------

#[derive(Clone, Deserialize)]
pub struct TileVariation {
    // Variation name is optional for Terrain and Units.
    #[serde(default)]
    pub name: String,

    // AnimSet may contain one or more animation frames.
    pub anim_sets: SmallVec<[TileAnimSet; 1]>,
}

// ----------------------------------------------
// TileDef
// ----------------------------------------------

#[derive(Clone, Deserialize)]
pub struct TileDef {
    // Friendly display name.
    pub name: String,

    // Tile kind, also defines which layer the tile can be placed on.
    #[serde(default = "default_tile_kind")]
    pub kind: TileKind,

    // Internal runtime index into TileCategory.
    #[serde(skip)]
    pub category_tile_index: i32,

    // Internal runtime index into TileSet.
    #[serde(skip)]
    pub tileset_category_index: i32,

    // Logical size for the tile map. Always a multiple of the base tile size.
    // Optional for Terrain tiles (always = BASE_TILE_SIZE), required otherwise.
    #[serde(default = "default_tile_size")]
    pub logical_size: Size2D,

    // Draw size for tile rendering. Can be any size ratio.
    // Optional in serialized data. Defaults to the value of `logical_size` if missing.
    #[serde(default)]
    pub draw_size: Size2D,

    // Tint color is optional in serialized data. Default to white if missing.
    #[serde(default)]
    pub color: Color,

    // Tile variations for buildings.
    // SmallVec optimizes for Terrain/Units with single variation.
    pub variations: SmallVec<[TileVariation; 1]>,
}

impl TileDef {
    const fn new(tile_kind: TileKind) -> Self {
        Self {
            name: String::new(),
            kind: tile_kind,
            category_tile_index: -1,
            tileset_category_index: -1,
            logical_size: BASE_TILE_SIZE,
            draw_size: BASE_TILE_SIZE,
            color: Color::white(),
            variations: SmallVec::new_const(),
        }
    }

    pub const fn empty() -> &'static Self {
        static EMPTY_TILE: TileDef = TileDef::new(TileKind::Empty);
        &EMPTY_TILE
    }

    pub const fn blocker() -> &'static Self {
        static BLOCKER_TILE: TileDef = TileDef::new(TileKind::Blocker);
        &BLOCKER_TILE
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.logical_size.is_valid() && self.draw_size.is_valid()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.kind == TileKind::Empty
    }

    #[inline]
    pub fn is_terrain(&self) -> bool {
        self.kind == TileKind::Terrain
    }

    #[inline]
    pub fn is_building(&self) -> bool {
        self.kind == TileKind::Building
    }

    #[inline]
    pub fn is_blocker(&self) -> bool {
        self.kind == TileKind::Blocker
    }

    #[inline]
    pub fn is_unit(&self) -> bool {
        self.kind == TileKind::Unit
    }

    #[inline]
    pub fn size_in_cells(&self) -> Size2D {
        // `logical_size` is assumed to be a multiple of the base tile size.
        Size2D::new(
            self.logical_size.width / BASE_TILE_SIZE.width,
            self.logical_size.height / BASE_TILE_SIZE.height)
    }

    #[inline]
    pub fn has_multi_cell_footprint(&self) -> bool {
        let size = self.size_in_cells();
        size.width > 1 || size.height > 1 // Multi-tile building?
    }

    pub fn calc_footprint_cells(&self, base_cell: Cell2D) -> TileFootprintList {
        let mut footprint = TileFootprintList::new();

        if !self.is_empty() {
            let size = self.size_in_cells();
            debug_assert!(size.is_valid());

            // Buildings can occupy multiple cells; Find which ones:
            let start_cell = base_cell;
            let end_cell = Cell2D::new(start_cell.x + size.width - 1, start_cell.y + size.height - 1);

            for y in (start_cell.y..=end_cell.y).rev() {
                for x in (start_cell.x..=end_cell.x).rev() {
                    footprint.push(Cell2D::new(x, y));
                }
            }

            // Last cell should be the original starting cell (selection relies on this).
            debug_assert!(*footprint.last().unwrap() == base_cell);
        } else {
            // Empty tiles always occupy one cell.
            footprint.push(base_cell);
        }

        footprint
    }

    #[inline]
    pub fn texture_by_index(&self,
                            variation_index: usize,
                            anim_set_index: usize,
                            frame_index: usize) -> TextureHandle {

        if variation_index >= self.variations.len() {
            return TextureHandle::invalid();
        }

        let var = &self.variations[variation_index];
        if anim_set_index >= var.anim_sets.len() {
            return TextureHandle::invalid();
        }

        let anim_set = &var.anim_sets[anim_set_index];
        if frame_index >= anim_set.frames.len() {
            return TextureHandle::invalid();
        }

        anim_set.frames[frame_index].tex_info.texture
    }

    #[inline]
    pub fn anim_frame_by_index(&self,
                               variation_index: usize,
                               anim_set_index: usize,
                               frame_index: usize) -> Option<&TileSprite> {

        if variation_index >= self.variations.len() {
            return None;
        }

        let var = &self.variations[variation_index];
        if anim_set_index >= var.anim_sets.len() {
            return None;
        }

        let anim_set = &var.anim_sets[anim_set_index];
        if frame_index >= anim_set.frames.len() {
            return None;
        }

        Some(&anim_set.frames[frame_index])
    }

    pub fn count_anim_sets(&self) -> usize {
        let mut count = 0;
        for var in &self.variations {
            count += var.anim_sets.len();
        }
        count
    }

    pub fn count_anim_frames(&self) -> usize {
        let mut count = 0;
        for var in &self.variations {
            for anim in &var.anim_sets {
                count += anim.frames.len();
            }
        }
        count
    }
}

// Deserialization defaults:

#[inline]
const fn default_tile_size() -> Size2D { BASE_TILE_SIZE }

#[inline]
const fn default_tile_kind() -> TileKind { TileKind::Empty }

// ----------------------------------------------
// TileTexInfo
// ----------------------------------------------

#[derive(Clone)]
pub struct TileTexInfo {
    pub texture: TextureHandle,
    pub coords: RectTexCoords,
}

impl Default for TileTexInfo {
    fn default() -> Self { Self::default() }
}

impl TileTexInfo {
    // NOTE: This needs to be const for static declarations, so we don't just derive from Default.
    pub const fn default() -> Self {
        Self {
            texture: TextureHandle::invalid(),
            coords: RectTexCoords::default(),
        }
    }

    pub fn new(texture: TextureHandle) -> Self {
        Self {
            texture: texture,
            coords: RectTexCoords::default(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.texture.is_valid()
    }
}

// ----------------------------------------------
// Helper functions
// ----------------------------------------------

// Can fit a 6x6 tile without allocating.
pub type TileFootprintList = SmallVec<[Cell2D; 36]>;

pub fn cells_overlap(lhs_cells: &TileFootprintList, rhs_cells: &TileFootprintList) -> bool {
    for lhs_cell in lhs_cells {
        for rhs_cell in rhs_cells {
            if lhs_cell == rhs_cell {
                return true;
            }
        }
    }
    false
}

// Creates an isometric-aligned diamond rectangle for the given tile size and cell location.
pub fn cell_to_screen_diamond_points(cell: Cell2D,
                                     tile_size: Size2D,
                                     transform: &WorldToScreenTransform) -> [Point2D; 4] {

    let iso_center = utils::cell_to_iso(cell, BASE_TILE_SIZE);
    let screen_center = utils::iso_to_screen_point(iso_center, transform, BASE_TILE_SIZE, false);

    let tile_width  = tile_size.width  * transform.scaling;
    let tile_height = tile_size.height * transform.scaling;
    let base_height = BASE_TILE_SIZE.height * transform.scaling;

    let half_tile_w = tile_width  / 2;
    let half_tile_h = tile_height / 2;
    let half_base_h = base_height / 2;

    // Build 4 corners of the tile:
    let top    = Point2D::new(screen_center.x, screen_center.y - tile_height + half_base_h);
    let bottom = Point2D::new(screen_center.x, screen_center.y + half_base_h);
    let right  = Point2D::new(screen_center.x + half_tile_w, screen_center.y - half_tile_h + half_base_h);
    let left   = Point2D::new(screen_center.x - half_tile_w, screen_center.y - half_tile_h + half_base_h);

    [ top, right, bottom, left ]
}

// Test precisely if the screen point is inside the isometric cell diamond.
pub fn is_screen_point_inside_cell(screen_point: Point2D,
                                   cell: Cell2D,
                                   tile_def: &TileDef,
                                   transform: &WorldToScreenTransform) -> bool {

    debug_assert!(transform.is_valid());

    let screen_points = cell_to_screen_diamond_points(
        cell,
        tile_def.logical_size,
        transform);

    utils::is_screen_point_inside_diamond(screen_point, &screen_points)
}
