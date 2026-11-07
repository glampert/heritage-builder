// What the minimap knows about the map, kept apart from how it's drawn: a grid of terrain and
// cover per cell, which the bake reads, and the features (buildings, fields, plazas) the shape
// layer and the lenses draw. Both are kept up to date by Minimap::place_tile and clear_tile.

use std::collections::BTreeMap;

use serde::Deserialize;
use strum::{Display, EnumCount, EnumIter, EnumString};

use common::{Color, Size, coords::Cell};
use minimap_bake::{CellRect, Cover, Grid, Terrain};

use super::style::HexColor;
use crate::tile::{
    TileFlags,
    TileKind,
    road,
    sets::{TileDef, TileSector},
};

// ----------------------------------------------
// Tile data: the `minimap` block on tile defs
// ----------------------------------------------

// How a tile shows on the minimap. Buildings are drawn as blocks unless their tile def says
// otherwise.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Deserialize, Display, EnumIter, EnumString)]
pub enum MinimapClass {
    #[default]
    Building, // A block in its sector's colour, with a shadow and an outline.
    Field,    // Paddies and farms: a striped patch, flat.
    Plaza,    // Paved squares: flat, no shadow.
    Decor,    // Gardens and statues: no outline.
    None,     // Not drawn.
}

// Buildings that get an icon on the minimap. The seat (palace or governor's house) is where the
// Home tool goes.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Deserialize, Display, EnumCount, EnumIter, EnumString)]
pub enum LandmarkKind {
    Seat,
    Temple,
    Market,
    Monument,
    Port,
    Fort,
}

impl LandmarkKind {
    pub fn file_stem(self) -> &'static str {
        match self {
            Self::Seat => "seat",
            Self::Temple => "temple",
            Self::Market => "market",
            Self::Monument => "monument",
            Self::Port => "port",
            Self::Fort => "fort",
        }
    }
}

// The optional `minimap` block of a tile def, e.g. `"minimap": { "class": "Field" }`. Everything
// in it is optional: without it a building is a Building with no landmark, in its sector's colour.
#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinimapTileDef {
    #[serde(default)]
    pub class: Option<MinimapClass>,
    #[serde(default)]
    pub landmark: Option<LandmarkKind>,
    #[serde(default)]
    pub color: Option<HexColor>,
}

pub fn tile_class(tile_def: &TileDef) -> MinimapClass {
    match tile_def.minimap.as_ref().and_then(|minimap| minimap.class) {
        Some(class) => class,
        None if tile_def.is(TileKind::Building) => MinimapClass::Building,
        None => MinimapClass::None,
    }
}

pub fn tile_landmark(tile_def: &TileDef) -> Option<LandmarkKind> {
    tile_def.minimap.as_ref().and_then(|minimap| minimap.landmark)
}

pub fn tile_color(tile_def: &TileDef) -> Option<Color> {
    tile_def.minimap.as_ref().and_then(|minimap| minimap.color).map(HexColor::color)
}

// ----------------------------------------------
// MinimapGrid
// ----------------------------------------------

#[repr(u8)]
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum MinimapRoad {
    #[default]
    None,
    Dirt,
    Paved,
}

// One entry per cell, row-major (index = x + y * width).
#[derive(Default)]
pub struct MinimapGrid {
    size: Size,
    terrain: Vec<Terrain>,
    cover: Vec<Cover>,
    roads: Vec<MinimapRoad>,
    dirty: CellRect,    // Cells the bake must redo: union of terrain and cover edits since the last bake.
    roads_version: u64, // Goes up on every road edit, for the shape cache.
}

impl MinimapGrid {
    pub fn reset(&mut self, size: Size, fill: Option<&'static TileDef>) {
        let count = (size.width.max(0) * size.height.max(0)) as usize;
        let (terrain, road) = fill.map_or((Terrain::None, MinimapRoad::None), terrain_of);
        self.size = size;
        self.terrain = vec![terrain; count];
        self.cover = vec![Cover::None; count];
        self.roads = vec![road; count];
        self.dirty = CellRect::map(size.width, size.height);
        self.roads_version += 1;
    }

    #[inline]
    pub fn size(&self) -> Size {
        self.size
    }

    #[inline]
    pub fn is_cell_within_bounds(&self, cell: Cell) -> bool {
        cell.x >= 0 && cell.y >= 0 && cell.x < self.size.width && cell.y < self.size.height
    }

    #[inline]
    fn index(&self, cell: Cell) -> usize {
        (cell.x + cell.y * self.size.width) as usize
    }

    #[inline]
    pub fn terrain(&self, cell: Cell) -> Terrain {
        self.terrain[self.index(cell)]
    }

    #[inline]
    pub fn cover(&self, cell: Cell) -> Cover {
        self.cover[self.index(cell)]
    }

    #[inline]
    pub fn road(&self, cell: Cell) -> MinimapRoad {
        if self.is_cell_within_bounds(cell) { self.roads[self.index(cell)] } else { MinimapRoad::None }
    }

    #[inline]
    pub fn roads_version(&self) -> u64 {
        self.roads_version
    }

    pub fn set_terrain(&mut self, cell: Cell, terrain: Terrain, road: MinimapRoad) {
        if !self.is_cell_within_bounds(cell) {
            return;
        }
        let index = self.index(cell);
        if self.terrain[index] != terrain {
            self.terrain[index] = terrain;
            self.mark_dirty(cell);
        }
        if self.roads[index] != road {
            self.roads[index] = road;
            self.roads_version += 1;
        }
    }

    pub fn set_cover(&mut self, cell: Cell, cover: Cover) {
        if !self.is_cell_within_bounds(cell) {
            return;
        }
        let index = self.index(cell);
        if self.cover[index] != cover {
            let was_baked = is_baked_cover(self.cover[index]);
            self.cover[index] = cover;
            if was_baked || is_baked_cover(cover) {
                self.mark_dirty(cell);
            }
        }
    }

    fn mark_dirty(&mut self, cell: Cell) {
        self.dirty = self.dirty.union(&CellRect::cell(cell.x, cell.y));
    }

    #[inline]
    pub fn dirty(&self) -> CellRect {
        self.dirty
    }

    pub fn set_dirty(&mut self, dirty: CellRect) {
        self.dirty = dirty;
    }

    pub fn mark_all_dirty(&mut self) {
        self.dirty = CellRect::map(self.size.width, self.size.height);
    }

    // The grid as the bake reads it.
    pub fn bake_grid(&self) -> Grid<'_> {
        Grid { width: self.size.width, height: self.size.height, terrain: &self.terrain, cover: &self.cover }
    }

    pub fn memory_usage_estimate(&self) -> usize {
        self.terrain.capacity() * size_of::<Terrain>() + self.cover.capacity() * size_of::<Cover>() + self.roads.capacity()
    }
}

fn is_baked_cover(cover: Cover) -> bool {
    matches!(cover, Cover::Vegetation | Cover::Rocks)
}

// The ground a terrain tile gives a cell.
pub fn terrain_of(tile_def: &'static TileDef) -> (Terrain, MinimapRoad) {
    let path_kind = tile_def.path_kind;
    if path_kind.is_water() {
        (Terrain::Water, MinimapRoad::None)
    } else if path_kind.is_road() {
        let road = match road::kind(tile_def) {
            road::RoadKind::Dirt => MinimapRoad::Dirt,
            road::RoadKind::Paved => MinimapRoad::Paved,
        };
        (Terrain::Land, road)
    } else if path_kind.is_vacant_lot() {
        (Terrain::VacantLot, MinimapRoad::None)
    } else {
        (Terrain::Land, MinimapRoad::None)
    }
}

// What an object tile puts on its cells, or None for tiles the minimap ignores (units).
pub fn cover_of(tile_def: &TileDef) -> Option<Cover> {
    if tile_def.is(TileKind::Building) {
        Some(Cover::Building)
    } else if tile_def.path_kind.is_vegetation() || tile_def.is(TileKind::Vegetation) {
        Some(Cover::Vegetation)
    } else if tile_def.path_kind.is_rocks() || tile_def.is(TileKind::Rocks) {
        Some(Cover::Rocks)
    } else {
        None
    }
}

// ----------------------------------------------
// MinimapFeatures
// ----------------------------------------------

#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Display)]
pub enum FeatureState {
    #[default]
    Normal,
    UnderConstruction,
    Renovating,
}

impl FeatureState {
    pub fn from_tile_flags(flags: TileFlags) -> Self {
        if flags.intersects(TileFlags::UnderConstruction) {
            Self::UnderConstruction
        } else if flags.intersects(TileFlags::Renovating) {
            Self::Renovating
        } else {
            Self::Normal
        }
    }
}

// A building, field or plaza: one per object tile the minimap draws, keyed by its base cell.
#[derive(Copy, Clone)]
pub struct MinimapFeature {
    pub base_cell: Cell,
    pub size: Size, // Footprint in cells.
    pub class: MinimapClass,
    pub sector: TileSector,
    pub landmark: Option<LandmarkKind>,
    pub color: Option<Color>, // The tile def's override of its sector colour.
    pub state: FeatureState,
    pub tile_def: &'static TileDef,
}

impl MinimapFeature {
    pub fn new(base_cell: Cell, tile_def: &'static TileDef) -> Self {
        Self {
            base_cell,
            size: tile_def.size_in_cells(),
            class: tile_class(tile_def),
            sector: tile_def.sector,
            landmark: tile_landmark(tile_def),
            color: tile_color(tile_def),
            state: FeatureState::Normal,
            tile_def,
        }
    }

    // The footprint's centre in cell coordinates (cell centres at whole numbers).
    pub fn center(&self) -> (f32, f32) {
        (self.base_cell.x as f32 + (self.size.width - 1) as f32 * 0.5, self.base_cell.y as f32 + (self.size.height - 1) as f32 * 0.5)
    }

    pub fn contains(&self, cell: Cell) -> bool {
        cell.x >= self.base_cell.x
            && cell.y >= self.base_cell.y
            && cell.x < self.base_cell.x + self.size.width
            && cell.y < self.base_cell.y + self.size.height
    }
}

#[derive(Default)]
pub struct MinimapFeatures {
    features: BTreeMap<(i32, i32), MinimapFeature>, // By base cell, (y, x), for a stable order.
    version: u64,                                    // Goes up on every change, for the shape cache.
}

impl MinimapFeatures {
    pub fn clear(&mut self) {
        self.features.clear();
        self.version += 1;
    }

    pub fn insert(&mut self, feature: MinimapFeature) {
        self.features.insert(key(feature.base_cell), feature);
        self.version += 1;
    }

    pub fn remove(&mut self, base_cell: Cell) -> Option<MinimapFeature> {
        let removed = self.features.remove(&key(base_cell));
        if removed.is_some() {
            self.version += 1;
        }
        removed
    }

    pub fn get(&self, base_cell: Cell) -> Option<&MinimapFeature> {
        self.features.get(&key(base_cell))
    }

    pub fn set_state(&mut self, base_cell: Cell, state: FeatureState) {
        if let Some(feature) = self.features.get_mut(&key(base_cell))
            && feature.state != state
        {
            feature.state = state;
            self.version += 1;
        }
    }

    #[inline]
    pub fn version(&self) -> u64 {
        self.version
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.features.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &MinimapFeature> {
        self.features.values()
    }

    // The feature covering `cell`, if any. Footprints are at most a few cells, so it looks at the
    // base cells up to that far back.
    pub fn at(&self, cell: Cell) -> Option<&MinimapFeature> {
        const MAX_FOOTPRINT: i32 = 6;
        for dy in 0..MAX_FOOTPRINT {
            for dx in 0..MAX_FOOTPRINT {
                if let Some(feature) = self.get(Cell::new(cell.x - dx, cell.y - dy))
                    && feature.contains(cell)
                {
                    return Some(feature);
                }
            }
        }
        None
    }

    pub fn find_landmark(&self, landmark: LandmarkKind) -> Option<&MinimapFeature> {
        self.features.values().find(|feature| feature.landmark == Some(landmark))
    }
}

#[inline]
fn key(cell: Cell) -> (i32, i32) {
    (cell.y, cell.x)
}
