use crate::utils::{Color, Size2D, RectTexCoords};
use super::texture::TextureHandle;

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
            logical_size: Size2D::zero(),
            draw_size: Size2D::zero(),
            tex_info: TileTexInfo::default(),
            color: Color::white(),
            name: String::new(),
        }
    }

    pub fn empty() -> &'static Self {
        static EMPTY_TILE: TileDef = TileDef::new(TileKind::Empty);
        &EMPTY_TILE
    }

    pub fn building_blocker() -> &'static Self {
        static BUILDING_BLOCKER_TILE: TileDef = TileDef::new(TileKind::BuildingBlocker);
        &BUILDING_BLOCKER_TILE
    }

    pub fn is_valid(&self) -> bool {
        self.kind != TileKind::Empty
        && self.logical_size.is_valid()
        && self.draw_size.is_valid()
        && self.tex_info.is_valid()
    }

    pub fn is_empty(&self) -> bool {
        self.kind == TileKind::Empty
    }

    pub fn is_terrain(&self) -> bool {
        self.kind == TileKind::Terrain
    }

    pub fn is_building(&self) -> bool {
        self.kind == TileKind::Building
    }

    pub fn is_building_blocker(&self) -> bool {
        self.kind == TileKind::BuildingBlocker
    }

    pub fn is_unit(&self) -> bool {
        self.kind == TileKind::Unit
    }

    pub fn size_in_tiles(&self) -> Size2D {
        // `logical_size` is assumed to be a multiple of the base tile size.
        Size2D::new(
            self.logical_size.width / BASE_TILE_SIZE.width,
            self.logical_size.height / BASE_TILE_SIZE.height)
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
    pub const fn default() -> Self {
        Self {
            texture: TextureHandle::invalid(),
            coords: RectTexCoords::default(),
        }
    }

    pub fn with_texture(texture: TextureHandle) -> Self {
        Self {
            texture: texture,
            coords: RectTexCoords::default(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.texture.is_valid()
    }
}
