use smallvec::SmallVec;

use crate::utils::{Cell2D, Color, RectTexCoords, Size2D};
use crate::render::TextureHandle;

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const BASE_TILE_SIZE: Size2D = Size2D{ width: 64, height: 32 };

// ----------------------------------------------
// TileKind
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum TileKind {
    Empty, // No tile, draws nothing.
    Terrain,
    Building,
    BuildingBlocker, // Draws nothing; for multi-tile buildings.
    Unit,
}

// ----------------------------------------------
// TileDef
// ----------------------------------------------

pub type TileFootprintList = SmallVec<[Cell2D; 16]>;

#[derive(Clone)]
pub struct TileDef {
    pub kind: TileKind,
    pub logical_size: Size2D, // Logical size for the tile map. Always a multiple of the base tile size.
    pub draw_size: Size2D,    // Draw size for tile rendering. Can be any size ratio.
    pub tex_info: TileTexInfo,
    pub color: Color,
    pub name: String, // Debug name.
}

impl TileDef {
    pub const fn new(tile_kind: TileKind) -> Self {
        Self {
            kind: tile_kind,
            logical_size: BASE_TILE_SIZE,
            draw_size: BASE_TILE_SIZE,
            tex_info: TileTexInfo::default(),
            color: Color::white(),
            name: String::new(),
        }
    }

    pub const fn empty() -> &'static Self {
        static EMPTY_TILE: TileDef = TileDef::new(TileKind::Empty);
        &EMPTY_TILE
    }

    pub const fn building_blocker() -> &'static Self {
        static BUILDING_BLOCKER_TILE: TileDef = TileDef::new(TileKind::BuildingBlocker);
        &BUILDING_BLOCKER_TILE
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.logical_size.is_valid()
        && self.draw_size.is_valid()
        && self.tex_info.is_valid()
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
    pub fn is_building_blocker(&self) -> bool {
        self.kind == TileKind::BuildingBlocker
    }

    #[inline]
    pub fn is_unit(&self) -> bool {
        self.kind == TileKind::Unit
    }

    #[inline]
    pub fn size_in_tiles(&self) -> Size2D {
        // `logical_size` is assumed to be a multiple of the base tile size.
        Size2D::new(
            self.logical_size.width / BASE_TILE_SIZE.width,
            self.logical_size.height / BASE_TILE_SIZE.height)
    }

    pub fn calc_footprint_cells(&self, base_cell: Cell2D) -> TileFootprintList {
        let mut footprint = TileFootprintList::new();

        if !self.is_empty() {
            let size = self.size_in_tiles();
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
            debug_assert!((*footprint.last().unwrap()) == base_cell);
        } else {
            // Empty tiles always occupy one cell.
            footprint.push(base_cell);
        }

        footprint
    }
}

// ----------------------------------------------
// TileTexInfo
// ----------------------------------------------

#[derive(Clone)]
pub struct TileTexInfo {
    pub texture: TextureHandle,
    pub coords: RectTexCoords,
}

impl TileTexInfo {
    // NOTE: This needs to be const for static declarations, so we don't derive from Default.
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
