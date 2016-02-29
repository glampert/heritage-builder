
// ================================================================================================
// File: tile.rs
// Author: Guilherme R. Lampert
// Created on: 29/02/16
// Brief: Tile and Tile Map utilities.
//
// This source code is released under the MIT license.
// See the accompanying LICENSE file for details.
// ================================================================================================

use citysim::common::{Rect2d, Color};
use citysim::texcache::{TexId, TEX_ID_NONE};

// ----------------------------------------------
// TileGeometry
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct TileGeometry {
    pub rect:       Rect2d,
    pub color:      Color,
    pub tex_coords: [f32; 8],
}

impl TileGeometry {
    pub fn new() -> TileGeometry {
        TileGeometry{
            rect:       Rect2d::new(),
            color:      Color::white(),
            tex_coords: TileGeometry::default_tex_coords(),
        }
    }

    pub fn with_bounds(x_min: i32, y_min: i32, x_max: i32, y_max: i32) -> TileGeometry {
        TileGeometry{
            rect:       Rect2d::with_bounds(x_min, y_min, x_max, y_max),
            color:      Color::white(),
            tex_coords: TileGeometry::default_tex_coords(),
        }
    }

    pub fn default_tex_coords() -> [f32; 8] {
        [ 0.0, 0.0,
          0.0, 1.0,
          1.0, 1.0,
          1.0, 0.0 ]
    }
}

// ----------------------------------------------
// Tile
// ----------------------------------------------

pub struct Tile {
    pub tex_id:   TexId,
    pub geometry: TileGeometry,
}

impl Tile {
    pub fn new() -> Tile {
        Tile{ tex_id: TEX_ID_NONE, geometry: TileGeometry::new() }
    }
}
