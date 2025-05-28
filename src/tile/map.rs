use bitflags::bitflags;
use smallvec::smallvec;
use arrayvec::ArrayVec;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{Display, EnumCount, EnumIter};
use serde::Deserialize;

use crate::{
    utils::{self, Cell2D, Point2D, IsoPoint2D, Size2D, WorldToScreenTransform}
};

use super::{
    def::{self, TileDef, TileKind, TileFootprintList, BASE_TILE_SIZE},
    selection::TileSelection,
    placement::{self}
};

// ----------------------------------------------
// Tile / TileFlags
// ----------------------------------------------

bitflags! {
    #[derive(Copy, Clone, Default, PartialEq)]
    pub struct TileFlags: u32 {
        const Highlighted     = 1 << 1;
        const Invalidated     = 1 << 2;
        const Hidden          = 1 << 3;
        const OccludesTerrain = 1 << 4;
    
        // Debug flags:
        const DrawDebugInfo   = 1 << 5;
        const DrawDebugBounds = 1 << 6;
        const DrawBlockerInfo = 1 << 7;
    }
}

#[derive(Clone)]
pub struct Tile<'a> {
    pub cell: Cell2D,
    owner_cell: Cell2D, // For building blockers only.
    pub def: &'a TileDef,
    pub flags: TileFlags,
}

impl<'a> Tile<'a> {
    #[inline]
    const fn new(cell: Cell2D, owner_cell: Cell2D, def: &'a TileDef, flags: TileFlags) -> Self {
        Self {
            cell: cell,
            owner_cell: owner_cell,
            def: def,
            flags: flags,
        }
    }

    #[inline]
    pub const fn empty() -> &'static Self {
        static EMPTY_TILE: Tile = Tile::new(
            Cell2D::invalid(),
            Cell2D::invalid(),
            TileDef::empty(),
            TileFlags::empty());
        &EMPTY_TILE
    }

    #[inline]
    pub fn set_as_blocker(&mut self, owner_cell: Cell2D, owner_flags: TileFlags) {
        self.owner_cell = owner_cell;
        self.def = TileDef::blocker();
        self.flags = owner_flags;
    }

    #[inline]
    pub fn set_as_empty(&mut self) {
        self.owner_cell = Cell2D::invalid();
        self.def = TileDef::empty();
        self.flags = TileFlags::empty();
    }

    #[inline]
    pub fn set_def(&mut self, tile_def: &'a TileDef) {
        self.def = tile_def;
        self.flags = tile_def.tile_flags();
    }

    #[inline]
    pub fn kind(&self) -> TileKind {
        self.def.kind
    }

    #[inline]
    pub fn name(&self) -> &str {
        match self.kind() {
            TileKind::Empty => "<empty>",
            TileKind::Blocker => "<blocker>",
            _ => &self.def.name,
        }
    }

    #[inline]
    pub fn logical_size(&self) -> Size2D {
        self.def.logical_size
    }

    #[inline]
    pub fn draw_size(&self) -> Size2D {
        self.def.draw_size
    }

    #[inline]
    pub fn size_in_cells(&self) -> Size2D {
        self.def.size_in_cells()
    }

    #[inline]
    pub fn has_multi_cell_footprint(&self) -> bool {
        self.def.has_multi_cell_footprint()
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.cell.is_valid() && self.def.is_valid()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.def.is_empty()
    }

    #[inline]
    pub fn is_terrain(&self) -> bool {
        self.def.is_terrain()
    }

    #[inline]
    pub fn is_building(&self) -> bool {
        self.def.is_building()
    }

    #[inline]
    pub fn is_blocker(&self) -> bool {
        self.def.is_blocker()
    }

    #[inline]
    pub fn is_unit(&self) -> bool {
        self.def.is_unit()
    }

    #[inline]
    pub fn occludes_terrain(&self) -> bool {
        self.def.occludes_terrain || self.flags.contains(TileFlags::OccludesTerrain)
    }

    #[inline]
    pub fn calc_z_sort(&self) -> i32 {
        utils::cell_to_iso(self.cell, BASE_TILE_SIZE).y - self.def.logical_size.height
    }

    #[inline]
    pub fn calc_footprint_cells(&self) -> TileFootprintList {
        self.def.calc_footprint_cells(self.cell)
    }

    // Buildings may take up multiple cells.
    // If `base_cell` has a building blocker tile, backtracks to its owner and returns the whole building footprint.
    pub fn calc_exact_footprint_cells(base_cell: Cell2D, buildings_layer: &TileMapLayer) -> TileFootprintList {
        debug_assert!(buildings_layer.kind == TileMapLayerKind::Buildings);

        if let Some(building_tile) = buildings_layer.try_tile(base_cell) {
            // Buildings have an origin tile and zero or more associated blockers
            // if they occupy multiple tiles, so here we might need to back-track
            // to the origin of the building from a blocker tile.
            //
            /* For instance, a 2x2 house tile `H` will have the house at its origin
            cell, and 3 other blocker tiles `B` that back-reference the house tile.
            +---+---+
            | B | B |
            +---+---+
            | B | H | <-- origin tile, AKA base tile
            +---+---+ 
            */
            if building_tile.is_blocker() {
                let building_blocker = building_tile;
                debug_assert!(building_blocker.owner_cell.is_valid());

                let owning_building = buildings_layer.tile(building_blocker.owner_cell);
                owning_building.calc_footprint_cells()
            } else {
                // Regular building tile.
                building_tile.calc_footprint_cells()
            }
        } else {
            // Not a building.
            smallvec![base_cell]
        }
    }

    pub fn calc_adjusted_iso_coords(&self) -> IsoPoint2D {
        match self.kind() {
            TileKind::Terrain | TileKind::Empty | TileKind::Blocker => {
                // No position adjustments needed for terrain/empty/blocker tiles.
                utils::cell_to_iso(self.cell, BASE_TILE_SIZE)
            },
            TileKind::Building => {
                // Convert the anchor (bottom tile) to isometric screen position:
                let mut tile_iso_coords = utils::cell_to_iso(self.cell, BASE_TILE_SIZE);

                // Center the image horizontally:
                tile_iso_coords.x += (BASE_TILE_SIZE.width / 2) - (self.def.logical_size.width / 2);

                // Vertical offset: move up the full sprite height *minus* 1 tile's height
                // Since the anchor is the bottom tile, and cell_to_isometric gives us the *bottom*,
                // we must offset up by (image_height - one_tile_height).
                tile_iso_coords.y -= self.def.draw_size.height - BASE_TILE_SIZE.height;

                tile_iso_coords
            },
            TileKind::Unit => {
                // Convert the anchor tile into isometric screen coordinates:
                let mut tile_iso_coords = utils::cell_to_iso(self.cell, BASE_TILE_SIZE);

                // Adjust to center the unit sprite:
                tile_iso_coords.x += (BASE_TILE_SIZE.width / 2) - (self.def.draw_size.width / 2);
                tile_iso_coords.y -= self.def.draw_size.height  - (BASE_TILE_SIZE.height / 2);

                tile_iso_coords
            },
        }
    }
}

// ----------------------------------------------
// TileMapLayerKind / TileMapLayerRefs
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Debug, Display, EnumCount, EnumIter, Deserialize)]
pub enum TileMapLayerKind {
    Terrain,
    Buildings,
    Units,
}

pub const TILE_MAP_LAYER_COUNT: usize = TileMapLayerKind::COUNT;

#[inline]
pub fn tile_kind_to_layer(tile_kind: TileKind) -> TileMapLayerKind {
    match tile_kind {
        TileKind::Terrain => TileMapLayerKind::Terrain,
        TileKind::Building | TileKind::Blocker => TileMapLayerKind::Buildings,
        TileKind::Unit => TileMapLayerKind::Units,
        _ => panic!("Invalid tile kind!")
    }
}

#[inline]
pub fn layer_to_tile_kind(layer_kind: TileMapLayerKind) -> TileKind {
    match layer_kind {
        TileMapLayerKind::Terrain   => TileKind::Terrain,
        TileMapLayerKind::Buildings => TileKind::Building,
        TileMapLayerKind::Units     => TileKind::Unit,
    }
}

pub struct TileMapLayerRefs<'a> {
    pub terrain: &'a TileMapLayer<'a>,
    pub buildings: &'a TileMapLayer<'a>,
    pub units: &'a TileMapLayer<'a>,
}

pub struct TileMapLayerMutRefs<'a> {
    pub terrain: &'a mut TileMapLayer<'a>,
    pub buildings: &'a mut TileMapLayer<'a>,
    pub units: &'a mut TileMapLayer<'a>,
}

// ----------------------------------------------
// TileMapLayer
// ----------------------------------------------

pub struct TileMapLayer<'a> {
    kind: TileMapLayerKind,
    size_in_cells: Size2D,
    tiles: Vec<Tile<'a>>,
}

impl<'a> TileMapLayer<'a> {
    pub fn new(kind: TileMapLayerKind, size_in_cells: Size2D, fill_tile: &'a TileDef) -> Self {
        let tile_count = (size_in_cells.width * size_in_cells.height) as usize;

        let mut layer = Self {
            kind: kind,
            size_in_cells: size_in_cells,
            tiles: vec![Tile::new(Cell2D::invalid(), Cell2D::invalid(), fill_tile, TileFlags::empty()); tile_count]
        };

        // Update all cell indices:
        for y in (0..size_in_cells.height).rev() {
            for x in (0..size_in_cells.width).rev() {
                let cell = Cell2D::new(x, y);
                let index = layer.cell_to_index(cell);
                layer.tiles[index].cell = cell;
            }
        }

        layer
    }

    #[inline]
    pub fn size(&self) -> Size2D {
        self.size_in_cells
    }

    #[inline]
    pub fn kind(&self) -> TileMapLayerKind {
        self.kind
    } 

    #[inline]
    pub fn add_tile(&mut self, cell: Cell2D, tile_def: &'a TileDef) {
        if !tile_def.is_empty() {
            debug_assert!(tile_kind_to_layer(tile_def.kind) == self.kind);
        }

        let flags = tile_def.tile_flags();
        let tile_index = self.cell_to_index(cell);
        self.tiles[tile_index] = Tile::new(cell, Cell2D::invalid(), tile_def, flags);
    }

    #[inline]
    pub fn add_empty_tile(&mut self, cell: Cell2D) {
        let tile_index = self.cell_to_index(cell);
        self.tiles[tile_index] = Tile::new(
            cell,
            Cell2D::invalid(),
            TileDef::empty(),
            TileFlags::empty());
    }

    #[inline]
    pub fn add_blocker_tile(&mut self, cell: Cell2D, owner_cell: Cell2D) {
        let owner_tile = self.tile(owner_cell);
        let blocker_flags = owner_tile.flags;
        let blocker_index = self.cell_to_index(cell);
        self.tiles[blocker_index] = Tile::new(cell, owner_cell, TileDef::blocker(), blocker_flags);
    }

    #[inline]
    pub fn is_cell_within_bounds(&self, cell: Cell2D) -> bool {
         if (cell.x < 0 || cell.x >= self.size_in_cells.width) ||
            (cell.y < 0 || cell.y >= self.size_in_cells.height) {
            return false;
        }
        true
    }

    #[inline]
    pub fn tile(&self, cell: Cell2D) -> &Tile {
        let tile_index = self.cell_to_index(cell);
        let tile = &self.tiles[tile_index];

        if !tile.is_empty() {
            debug_assert!(tile_kind_to_layer(tile.kind()) == self.kind);
        }
        debug_assert!(tile.cell == cell);

        tile
    }

    #[inline]
    pub fn tile_mut(&mut self, cell: Cell2D) -> &mut Tile<'a> {
        let tile_index = self.cell_to_index(cell);
        let tile = &mut self.tiles[tile_index];

        if !tile.is_empty() {
            debug_assert!(tile_kind_to_layer(tile.kind()) == self.kind);
        }
        debug_assert!(tile.cell == cell);

        tile
    }

    // Fails with None if the cell indices are not within bounds.
    #[inline]
    pub fn try_tile(&self, cell: Cell2D) -> Option<&Tile> {
        if !self.is_cell_within_bounds(cell) {
            return None;
        }
        Some(self.tile(cell))
    }

    #[inline]
    pub fn try_tile_mut(&mut self, cell: Cell2D) -> Option<&mut Tile<'a>> {
        if !self.is_cell_within_bounds(cell) {
            return None;
        }
        Some(self.tile_mut(cell))
    }

    #[inline]
    pub fn has_tile(&self, cell: Cell2D, tile_kinds: &[TileKind]) -> bool {
        self.find_tile(cell, tile_kinds).is_some()
    }

    #[inline]
    pub fn find_tile(&self, cell: Cell2D, tile_kinds: &[TileKind]) -> Option<&Tile> {
        if let Some(current_tile) = self.try_tile(cell) {
            for &kind in tile_kinds {
                if current_tile.kind() == kind {
                    return Some(current_tile);
                }
            }
        }
        None
    }

    #[inline]
    pub fn find_tile_mut(&mut self, cell: Cell2D, tile_kinds: &[TileKind]) -> Option<&mut Tile<'a>> {
        if let Some(current_tile) = self.try_tile_mut(cell) {
            for &kind in tile_kinds {
                if current_tile.kind() == kind {
                    return Some(current_tile);
                }
            }
        }
        None
    }

    // 8 neighboring tiles plus self cell (optionally).
    pub fn tile_neighbors(&self, cell: Cell2D, include_self: bool) -> ArrayVec::<Option<&Tile>, 9> {
        let mut neighbors = ArrayVec::<Option<&Tile>, 9>::new();

        if include_self {
            neighbors.push(self.try_tile(cell));
        }

        // left/right
        neighbors.push(self.try_tile(Cell2D::new(cell.x, cell.y - 1)));
        neighbors.push(self.try_tile(Cell2D::new(cell.x, cell.y + 1)));

        // top
        neighbors.push(self.try_tile(Cell2D::new(cell.x + 1, cell.y)));
        neighbors.push(self.try_tile(Cell2D::new(cell.x + 1, cell.y + 1)));
        neighbors.push(self.try_tile(Cell2D::new(cell.x + 1, cell.y - 1)));

        // bottom
        neighbors.push(self.try_tile(Cell2D::new(cell.x - 1, cell.y)));
        neighbors.push(self.try_tile(Cell2D::new(cell.x - 1, cell.y + 1)));
        neighbors.push(self.try_tile(Cell2D::new(cell.x - 1, cell.y - 1)));

        neighbors
    }

    pub fn tile_neighbors_mut(&mut self, cell: Cell2D, include_self: bool) -> ArrayVec::<Option<&mut Tile<'a>>, 9> {
        let mut neighbors: ArrayVec<Option<*mut Tile<'a>>, 9> = ArrayVec::new();

        // Helper closure to get a raw pointer from try_tile_mut().
        let mut raw_tile_ptr = |c: Cell2D| {
            self.try_tile_mut(c)
                .map(|tile| tile as *mut Tile<'a>) // Convert to raw pointer
        };

        if include_self {
            neighbors.push(raw_tile_ptr(cell));
        }

        neighbors.push(raw_tile_ptr(Cell2D::new(cell.x, cell.y - 1)));
        neighbors.push(raw_tile_ptr(Cell2D::new(cell.x, cell.y + 1)));

        neighbors.push(raw_tile_ptr(Cell2D::new(cell.x + 1, cell.y)));
        neighbors.push(raw_tile_ptr(Cell2D::new(cell.x + 1, cell.y + 1)));
        neighbors.push(raw_tile_ptr(Cell2D::new(cell.x + 1, cell.y - 1)));

        neighbors.push(raw_tile_ptr(Cell2D::new(cell.x - 1, cell.y)));
        neighbors.push(raw_tile_ptr(Cell2D::new(cell.x - 1, cell.y + 1)));
        neighbors.push(raw_tile_ptr(Cell2D::new(cell.x - 1, cell.y - 1)));

        // SAFETY: We assume all cell coordinates are unique, so no aliasing.
        neighbors
            .into_iter()
            .map(|opt_ptr| opt_ptr.map(|ptr| unsafe { &mut *ptr }))
            .collect()
    }

    pub fn find_exact_cell_for_point(&self,
                                     screen_point: Point2D,
                                     transform: &WorldToScreenTransform) -> Cell2D {

        let iso_point = utils::screen_to_iso_point(screen_point, transform, BASE_TILE_SIZE, false);
        let approx_cell = utils::iso_to_cell(iso_point, BASE_TILE_SIZE);

        if self.is_cell_within_bounds(approx_cell) {
            // Get the 8 possible neighboring tiles + self and test cursor intersection
            // against each so we can know precisely which tile the cursor is hovering.
            let neighbors = self.tile_neighbors(approx_cell, true);

            for neighbor in neighbors {
                if let Some(tile) = neighbor {
                    if def::is_screen_point_inside_cell(screen_point, tile.cell, tile.def, transform) {
                        return tile.cell;
                    }
                }
            }
        }

        Cell2D::invalid()
    }

    #[inline]
    fn cell_to_index(&self, cell: Cell2D) -> usize {
        debug_assert!(self.is_cell_within_bounds(cell));
        let tile_index = cell.x + (cell.y * self.size_in_cells.width);
        tile_index as usize
    }
}

// ----------------------------------------------
// TileMap
// ----------------------------------------------

pub struct TileMap<'a> {
    size_in_cells: Size2D,
    layers: ArrayVec::<Box<TileMapLayer<'a>>, TILE_MAP_LAYER_COUNT>,
}

impl<'a> TileMap<'a> {
    pub fn new(size_in_cells: Size2D) -> Self {
        let mut tile_map = Self {
            size_in_cells: size_in_cells,
            layers: ArrayVec::new(),
        };
        tile_map.clear(TileDef::empty());
        tile_map
    }

    pub fn clear(&mut self, fill_tile: &'a TileDef) {
        self.layers.clear();
        // Reset all layers to empty.
        for layer in TileMapLayerKind::iter() {
            let mut fill_tile_def = TileDef::empty();

            // Find which layer this tile belong to if we're not just setting everything to empty.
            if !fill_tile.is_empty() {
                if fill_tile.kind == layer_to_tile_kind(layer) {
                    fill_tile_def = fill_tile;
                }
            }

            self.layers.push(Box::new(TileMapLayer::new(layer, self.size_in_cells, fill_tile_def)));
        }
    }

    #[inline]
    pub fn size(&self) -> Size2D {
        self.size_in_cells
    }

    #[inline]
    pub fn is_cell_within_bounds(&self, cell: Cell2D) -> bool {
         if (cell.x < 0 || cell.x >= self.size_in_cells.width) ||
            (cell.y < 0 || cell.y >= self.size_in_cells.height) {
            return false;
        }
        true
    }

    #[inline]
    pub fn layers(&self) -> TileMapLayerRefs {
        TileMapLayerRefs {
            terrain:   self.layer(TileMapLayerKind::Terrain),
            buildings: self.layer(TileMapLayerKind::Buildings),
            units:     self.layer(TileMapLayerKind::Units),
        }
    }

    #[inline]
    pub fn layers_mut(&mut self) -> TileMapLayerMutRefs<'a> {
        // Use raw pointers to avoid borrow checker conflicts.
        let terrain   = self.layer_mut(TileMapLayerKind::Terrain)   as *mut TileMapLayer;
        let buildings = self.layer_mut(TileMapLayerKind::Buildings) as *mut TileMapLayer;
        let units     = self.layer_mut(TileMapLayerKind::Units)     as *mut TileMapLayer;

        // SAFETY: Indices are distinct and all references are valid while `self` is borrowed mutably.
        unsafe {
            TileMapLayerMutRefs {
                terrain:   &mut *terrain,
                buildings: &mut *buildings,
                units:     &mut *units,
            }
        }
    }

    #[inline]
    pub fn layer(&self, kind: TileMapLayerKind) -> &TileMapLayer {
        debug_assert!(self.layers[kind as usize].kind == kind);
        self.layers[kind as usize].as_ref()
    }

    #[inline]
    pub fn layer_mut(&mut self, kind: TileMapLayerKind) -> &mut TileMapLayer<'a> {
        debug_assert!(self.layers[kind as usize].kind == kind);
        self.layers[kind as usize].as_mut()
    }

    #[inline]
    pub fn try_tile_from_layer(&self,
                               cell: Cell2D,
                               kind: TileMapLayerKind) -> Option<&Tile> {

        let layer = self.layer(kind);
        debug_assert!(layer.kind == kind);
        layer.try_tile(cell)
    }

    #[inline]
    pub fn try_tile_from_layer_mut(&mut self,
                                   cell: Cell2D,
                                   kind: TileMapLayerKind) -> Option<&mut Tile<'a>> {

        let layer = self.layer_mut(kind);
        debug_assert!(layer.kind == kind);
        layer.try_tile_mut(cell)
    }

    #[inline]
    pub fn has_tile(&self,
                    cell: Cell2D,
                    kind: TileMapLayerKind,
                    tile_kinds: &[TileKind]) -> bool {

        self.layer(kind).has_tile(cell, tile_kinds)
    }

    #[inline]
    pub fn find_tile(&self,
                     cell: Cell2D,
                     kind: TileMapLayerKind,
                     tile_kinds: &[TileKind]) -> Option<&Tile> {

        self.layer(kind).find_tile(cell, tile_kinds)
    }

    #[inline]
    pub fn find_tile_mut(&mut self,
                         cell: Cell2D,
                         kind: TileMapLayerKind,
                         tile_kinds: &[TileKind]) -> Option<&mut Tile<'a>> {

        self.layer_mut(kind).find_tile_mut(cell, tile_kinds)
    }

    pub fn try_place_tile(&mut self,
                          target_cell: Cell2D,
                          tile_to_place: &'a TileDef) -> bool {

        self.try_place_tile_in_layer(
            target_cell,
            tile_kind_to_layer(tile_to_place.kind),
            tile_to_place)
    }

    pub fn try_place_tile_in_layer(&mut self,
                                   target_cell: Cell2D,
                                   kind: TileMapLayerKind,
                                   tile_to_place: &'a TileDef) -> bool {

        if tile_to_place.is_empty() {
            placement::try_clear_tile_from_layer(self, kind, target_cell)
        } else {
            placement::try_place_tile_in_layer(self, kind, target_cell, tile_to_place)
        }
    }

    pub fn try_place_tile_at_cursor(&mut self,
                                    cursor_screen_pos: Point2D,
                                    transform: &WorldToScreenTransform,
                                    tile_to_place: &'a TileDef) -> bool {

        if tile_to_place.is_empty() {
            placement::try_clear_tile_at_cursor(self, cursor_screen_pos, transform)
        } else {
            placement::try_place_tile_at_cursor(self, cursor_screen_pos, transform, tile_to_place)
        }
    }

    pub fn update_selection(&mut self,
                            selection: &mut TileSelection<'a>,
                            cursor_screen_pos: Point2D,
                            transform: &WorldToScreenTransform,
                            placement_candidate: Option<&'a TileDef>) {

        let map_size = self.size();
        let mut layers = self.layers_mut(); 

        selection.update(
            &mut layers,
            map_size,
            cursor_screen_pos,
            transform, 
            placement_candidate);
    }

    pub fn clear_selection(&mut self, selection: &mut TileSelection<'a>) {
        selection.clear(&mut self.layers_mut());
    }

    pub fn topmost_selected_tile(&self, selection: &TileSelection) -> Option<&Tile> {
        let selected_cell = selection.last_cell();
        if self.is_cell_within_bounds(selected_cell) {
            // Returns the tile at the topmost layer if it is not empty (unit, building, terrain),
            // or nothing if all layers are empty.
            for layer_kind in TileMapLayerKind::iter().rev() {
                if let Some(tile) = self.try_tile_from_layer(selected_cell, layer_kind) {
                    if !tile.is_empty() {
                        return Some(tile);
                    }
                }
            }
        }
        None
    }

    pub fn find_exact_cell_for_point(&self,
                                     kind: TileMapLayerKind,
                                     screen_point: Point2D,
                                     transform: &WorldToScreenTransform) -> Cell2D {

        self.layer(kind).find_exact_cell_for_point(screen_point, transform)
    }

    // Iterate all tiles on multi-tile buildings.
    pub fn for_each_building_footprint_tile<F>(&self, cell: Cell2D, mut visitor_fn: F) 
        where F: FnMut(&Tile) {

        let buildings_layer = self.layer(TileMapLayerKind::Buildings);
        let footprint = Tile::calc_exact_footprint_cells(cell, buildings_layer);

        for footprint_cell in footprint {
            if let Some(tile) = buildings_layer.find_tile(
                    footprint_cell, &[TileKind::Building, TileKind::Blocker]) {
                visitor_fn(tile);
            }
        }
    }

    pub fn for_each_building_footprint_tile_mut<F>(&mut self, cell: Cell2D, mut visitor_fn: F) 
        where F: FnMut(&mut Tile<'a>) {

        let buildings_layer = self.layer_mut(TileMapLayerKind::Buildings);
        let footprint = Tile::calc_exact_footprint_cells(cell, buildings_layer);

        for footprint_cell in footprint {
            if let Some(tile) = buildings_layer.find_tile_mut(
                    footprint_cell, &[TileKind::Building, TileKind::Blocker]) {
                visitor_fn(tile);
            }
        }
    }
}
