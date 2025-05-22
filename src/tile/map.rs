use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{Display, EnumCount, EnumIter};
use bitflags::bitflags;
use arrayvec::ArrayVec;
use smallvec::{smallvec, SmallVec};

use crate::utils::*;
use crate::app::input::{MouseButton, InputAction};
use crate::ui::UiSystem;
use crate::render::RenderSystem;
use super::def::{TileDef, TileKind, TileFootprintList, BASE_TILE_SIZE};
use super::debug::{self};

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const TILE_HIGHLIGHT_COLOR: Color = Color::new(0.76, 0.96, 0.39, 1.0); // light green
pub const TILE_INVALID_COLOR:   Color = Color::new(0.95, 0.60, 0.60, 1.0); // light red

pub const GRID_HIGHLIGHT_COLOR: Color = Color::green();
pub const GRID_INVALID_COLOR:   Color = Color::red();

// ----------------------------------------------
// Tile / TileFlags
// ----------------------------------------------

bitflags! {
    #[derive(Copy, Clone, Default, PartialEq)]
    pub struct TileFlags: u32 {
        const None        = 0;
        const Highlighted = 1 << 1;
        const Invalidated = 1 << 2;
    }
}

#[derive(Clone)]
pub struct Tile<'a> {
    pub cell: Cell2D,
    pub owner_cell: Cell2D,
    pub def: &'a TileDef,
    pub flags: TileFlags,
}

impl<'a> Tile<'a> {
    pub const fn new(cell: Cell2D, owner_cell: Cell2D, def: &'a TileDef) -> Self {
        Self {
            cell: cell,
            owner_cell: owner_cell,
            def: def,
            flags: TileFlags::None,
        }
    }

    pub const fn empty() -> Self {
        Self::new(Cell2D::invalid(), Cell2D::invalid(), TileDef::empty())
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
    pub fn is_building_blocker(&self) -> bool {
        self.def.is_building_blocker()
    }

    #[inline]
    pub fn is_unit(&self) -> bool {
        self.def.is_unit()
    }

    #[inline]
    pub fn calc_z_sort(&self) -> i32 {
        cell_to_iso(self.cell, BASE_TILE_SIZE).y - self.def.logical_size.height
    }

    #[inline]
    pub fn calc_footprint_cells(&self) -> TileFootprintList {
        self.def.calc_footprint_cells(self.cell)
    }

    pub fn calc_adjusted_iso_coords(&self) -> IsoPoint2D {
        match self.def.kind {
            TileKind::Terrain | TileKind::Empty | TileKind::BuildingBlocker => {
                // No position adjustments needed for terrain/empty/blocker tiles.
                cell_to_iso(self.cell, BASE_TILE_SIZE)
            },
            TileKind::Building => {
                // Convert the anchor (bottom tile) to isometric screen position:
                let mut tile_iso_coords = cell_to_iso(self.cell, BASE_TILE_SIZE);

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
                let mut tile_iso_coords = cell_to_iso(self.cell, BASE_TILE_SIZE);

                // Adjust to center the unit sprite:
                tile_iso_coords.x += (BASE_TILE_SIZE.width / 2) - (self.def.draw_size.width / 2);
                tile_iso_coords.y -= self.def.draw_size.height  - (BASE_TILE_SIZE.height / 2);

                tile_iso_coords
            },
        }
    }

    // Tile center in screen coordinates (iso coords + WorldToScreenTransform).
    pub fn calc_tile_center(&self, tile_screen_pos: Point2D) -> Point2D {
        let tile_center = Point2D::new(
            tile_screen_pos.x + self.def.draw_size.width,
            tile_screen_pos.y + self.def.draw_size.height
        );
        tile_center
    }
}

// ----------------------------------------------
// TileMapLayer / TileMapLayerKind
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Debug, EnumCount, EnumIter, Display)]
pub enum TileMapLayerKind {
    Terrain,
    Buildings,
    Units,
}

pub const TILE_MAP_LAYER_COUNT: usize = TileMapLayerKind::COUNT;

#[inline]
fn tile_kind_to_layer(tile_kind: TileKind) -> TileMapLayerKind {
    match tile_kind {
        TileKind::Terrain => TileMapLayerKind::Terrain,
        TileKind::Building | TileKind::BuildingBlocker => TileMapLayerKind::Buildings,
        TileKind::Unit => TileMapLayerKind::Units,
        _ => panic!("Invalid tile map layer!")
    }
}

pub struct TileMapLayer<'a> {
    kind: TileMapLayerKind,
    size_in_cells: Size2D,
    tiles: Vec<Tile<'a>>,
}

impl<'a> TileMapLayer<'a> {
    pub fn new(kind: TileMapLayerKind, size_in_cells: Size2D) -> Self {
        let mut layer = Self {
            kind: kind,
            size_in_cells: size_in_cells,
            tiles: vec![Tile::empty(); (size_in_cells.width * size_in_cells.height) as usize]
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
    pub fn add_tile(&mut self, cell: Cell2D, tile_def: &'a TileDef) {
        if !tile_def.is_empty() {
            debug_assert!(tile_kind_to_layer(tile_def.kind) == self.kind);
        }

        let tile_index = self.cell_to_index(cell);
        self.tiles[tile_index] = Tile::new(cell, Cell2D::invalid(), tile_def);
    }

    #[inline]
    pub fn add_empty_tile(&mut self, cell: Cell2D) {
        let tile_index = self.cell_to_index(cell);
        self.tiles[tile_index] = Tile::new(cell, Cell2D::invalid(), TileDef::empty());
    }

    #[inline]
    pub fn add_building_blocker_tile(&mut self, cell: Cell2D, owner_cell: Cell2D) {
        let tile_index = self.cell_to_index(cell);
        self.tiles[tile_index] = Tile::new(cell, owner_cell, TileDef::building_blocker());
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
            debug_assert!(tile_kind_to_layer(tile.def.kind) == self.kind);
        }
        debug_assert!(tile.cell == cell);

        tile
    }

    #[inline]
    pub fn tile_mut(&mut self, cell: Cell2D) -> &mut Tile<'a> {
        let tile_index = self.cell_to_index(cell);
        let tile = &mut self.tiles[tile_index];

        if !tile.is_empty() {
            debug_assert!(tile_kind_to_layer(tile.def.kind) == self.kind);
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

    #[inline]
    fn cell_to_index(&self, cell: Cell2D) -> usize {
        let tile_index = cell.x + (cell.y * self.size_in_cells.width);
        tile_index as usize
    }
}

// ----------------------------------------------
// TileMap
// ----------------------------------------------

pub struct TileMap<'a> {
    size_in_cells: Size2D,
    layers: Vec<Box<TileMapLayer<'a>>>,
}

impl<'a> TileMap<'a> {
    pub fn new(size_in_cells: Size2D) -> Self {
        let mut tile_map = Self {
            size_in_cells: size_in_cells,
            layers: Vec::with_capacity(TILE_MAP_LAYER_COUNT),
        };

        for layer in TileMapLayerKind::iter() {
            tile_map.layers.push(Box::new(TileMapLayer::new(layer, size_in_cells)));
        }

        tile_map
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
    pub fn layers(&self) -> (&TileMapLayer, &TileMapLayer, &TileMapLayer) {
        (
            self.layer(TileMapLayerKind::Terrain),
            self.layer(TileMapLayerKind::Buildings),
            self.layer(TileMapLayerKind::Units),
        )
    }

    #[inline]
    pub fn layers_mut(&mut self) -> (&mut TileMapLayer<'a>, &mut TileMapLayer<'a>, &mut TileMapLayer<'a>) {
        // Use raw pointers to avoid borrow checker conflicts.
        let terrain   = self.layer_mut(TileMapLayerKind::Terrain)   as *mut TileMapLayer;
        let buildings = self.layer_mut(TileMapLayerKind::Buildings) as *mut TileMapLayer;
        let units     = self.layer_mut(TileMapLayerKind::Units)     as *mut TileMapLayer;

        // SAFETY: Indices are distinct and all references are valid while `self` is borrowed mutably.
        unsafe {
            (&mut *terrain, &mut *buildings, &mut *units)
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
    pub fn try_tile_from_layer(&self, cell: Cell2D, kind: TileMapLayerKind) -> Option<&Tile> {
        let layer = self.layer(kind);
        debug_assert!(layer.kind == kind);
        layer.try_tile(cell)
    }

    #[inline]
    pub fn try_tile_from_layer_mut(&mut self, cell: Cell2D, kind: TileMapLayerKind) -> Option<&mut Tile<'a>> {
        let layer = self.layer_mut(kind);
        debug_assert!(layer.kind == kind);
        layer.try_tile_mut(cell)
    }

    #[inline]
    pub fn has_tile(&self, cell: Cell2D, layer_kind: TileMapLayerKind, tile_kinds: &[TileKind]) -> bool {
        self.find_tile(cell, layer_kind, tile_kinds).is_some()
    }

    #[inline]
    pub fn find_tile(&self, cell: Cell2D, layer_kind: TileMapLayerKind, tile_kinds: &[TileKind]) -> Option<&Tile> {
        if let Some(current_tile) = self.try_tile_from_layer(cell, layer_kind) {
            for &kind in tile_kinds {
                if current_tile.def.kind == kind {
                    return Some(current_tile);
                }
            }
        }
        None
    }

    #[inline]
    pub fn place_tile(&mut self, target_cell: Cell2D, tile_to_place: &'a TileDef) -> bool {
        self.place_tile_in_layer(target_cell, tile_kind_to_layer(tile_to_place.kind), tile_to_place)
    }

    pub fn place_tile_in_layer(&mut self, target_cell: Cell2D, kind: TileMapLayerKind, tile_to_place: &'a TileDef) -> bool {
        debug_assert!(self.is_cell_within_bounds(target_cell));

        // Overlap checks for buildings:
        if tile_to_place.is_building() {
            debug_assert!(kind == TileMapLayerKind::Buildings);

            // Building -> unit overlap check:
            if self.has_tile(target_cell, TileMapLayerKind::Units, &[TileKind::Unit]) {
                return false;
            }

            // Check for building overlap:
            if self.has_tile(target_cell, kind, &[TileKind::Building, TileKind::BuildingBlocker]) {
                let current_footprint =
                    calc_tile_footprint_cells(target_cell, self.layer(kind));

                let target_footprint =
                    tile_to_place.calc_footprint_cells(target_cell);

                if cells_overlap(&current_footprint, &target_footprint) {
                    return false; // Cannot place building here.
                }
            }

            // Multi-tile building?
            if tile_to_place.has_multi_cell_footprint() {
                let target_footprint = tile_to_place.calc_footprint_cells(target_cell);

                // Check if placement is allowed:
                for footprint_cell in &target_footprint {
                    if !self.is_cell_within_bounds(*footprint_cell) {
                        // If any cell would fall outside of the map bounds we won't place.
                        return false;
                    }

                    if self.has_tile(*footprint_cell, kind, &[TileKind::Building, TileKind::BuildingBlocker]) {
                        return false; // Cannot place building here.
                    }

                    // Building blocker -> unit overlap check:
                    if self.has_tile(*footprint_cell, TileMapLayerKind::Units, &[TileKind::Unit]) {
                        return false; // Cannot place building here.
                    }
                }

                for footprint_cell in target_footprint {
                    if footprint_cell != target_cell {
                        if let Some(current_tile) = self.try_tile_from_layer_mut(footprint_cell, kind) {
                            current_tile.def = TileDef::building_blocker();
                            current_tile.owner_cell = target_cell;
                        }
                    }
                }
            }
        }
        // Unit -> building overlap check:
        else if tile_to_place.is_unit() {
            debug_assert!(kind == TileMapLayerKind::Units);

            // Check overlap with buildings:
            if self.has_tile(target_cell, TileMapLayerKind::Buildings,
                             &[TileKind::Building, TileKind::BuildingBlocker]) {
                return false; // Can't place unit over building or building blocker cell.
            }
        }
        // Tile removal/clearing: Handle removing multi-tile buildings.
        else if tile_to_place.is_empty() {
            if self.has_tile(target_cell, kind, &[TileKind::Building, TileKind::BuildingBlocker]) {
                let target_footprint =
                    calc_tile_footprint_cells(target_cell, self.layer(kind));

                for footprint_cell in target_footprint {
                    if footprint_cell != target_cell {
                        if let Some(current_tile) = self.try_tile_from_layer_mut(footprint_cell, kind) {
                            current_tile.def = TileDef::empty();
                            current_tile.owner_cell = Cell2D::invalid();
                        }
                    }
                }
            }
        }

        if let Some(current_tile) = self.try_tile_from_layer_mut(target_cell, kind) {
            current_tile.def = tile_to_place;
            return true; // Tile placed successfully.
        }

        false // Nothing placed.
    }

    pub fn try_place_tile_at_cursor(&mut self,
                                    cursor_screen_pos: Point2D,
                                    transform: &WorldToScreenTransform,
                                    tile_to_place: &'a TileDef) -> bool {

        // If placing an empty tile we will actually clear the topmost layer under that cell.
        if tile_to_place.is_empty() {
            for kind in TileMapLayerKind::iter().rev() {
                let target_cell = find_exact_cell(
                    &self,
                    kind,
                    cursor_screen_pos,
                    transform);

                if self.is_cell_within_bounds(target_cell) {
                    if let Some(existing_tile) = self.layer(kind).try_tile(target_cell) {
                        if !existing_tile.is_empty() {
                            return self.place_tile_in_layer(target_cell, kind, tile_to_place);
                        }
                    }
                }      
            }
        } else {
            let kind = tile_kind_to_layer(tile_to_place.kind);
            let target_cell = find_exact_cell(
                &self,
                kind,
                cursor_screen_pos,
                transform);

            if self.is_cell_within_bounds(target_cell) {
                return self.place_tile_in_layer(target_cell, kind, tile_to_place);
            }
        }

        false // Nothing placed.
    }

    pub fn update_selection(&mut self, selection: &mut TileSelection<'a>, transform: &WorldToScreenTransform) {
        if selection.is_selecting_range() {
            // Clear previous highlighted tiles:
            self.clear_selection(selection);

            let (cell_min, cell_max) = tile_selection_bounds(
                &selection.rect,
                BASE_TILE_SIZE,
                self.size_in_cells,
                transform);

            let (terrain_layer, buildings_layer, units_layer) =
                self.layers_mut();

            for y in cell_min.y..=cell_max.y {
                for x in cell_min.x..=cell_max.x {
                    if let Some(base_tile) = terrain_layer.try_tile(Cell2D::new(x, y)) {

                        let tile_iso_coords = base_tile.calc_adjusted_iso_coords();
                        let tile_screen_rect = iso_to_screen_rect(
                            tile_iso_coords,
                            base_tile.def.logical_size,
                            transform,
                            false);

                        if tile_screen_rect.intersects(&selection.rect) {
                            selection.toggle_selection(terrain_layer,
                                                       buildings_layer,
                                                       units_layer,
                                                       base_tile.cell,
                                                       true);
                        }
                    }
                }
            }
        } else {
            // Clear previous highlighted tile for single selection:
            let cursor_screen_pos = selection.current_cursor_pos;
            let last_cell = selection.last_cell();

            if let Some(base_tile) = self.try_tile_from_layer(last_cell, TileMapLayerKind::Terrain) {
                // If the cursor is still inside this cell, we're done.
                // This can happen because the isometric-to-cell conversion
                // is not absolute but rather based on proximity to the cell's center.
                if cursor_inside_tile_cell(cursor_screen_pos, base_tile, transform) {
                    return;
                }

                let previous_cell = base_tile.cell;
                let (terrain_layer, buildings_layer, units_layer) =
                    self.layers_mut();

                // Clear:
                selection.toggle_selection(terrain_layer,
                                           buildings_layer,
                                           units_layer,
                                           previous_cell,
                                           false);
            }

            // Set highlight:
            {
                let highlight_cell = find_exact_cell(
                    &self,
                    TileMapLayerKind::Terrain,
                    cursor_screen_pos,
                    transform);
                
                if self.is_cell_within_bounds(highlight_cell) {
                    let (terrain_layer, buildings_layer, units_layer) =
                        self.layers_mut();

                    selection.toggle_selection(terrain_layer,
                                               buildings_layer,
                                               units_layer,
                                               highlight_cell,
                                               true);
                }
            }
        }
    }

    pub fn clear_selection(&mut self, selection: &mut TileSelection<'a>) {
        let (terrain_layer, buildings_layer, units_layer) =
            self.layers_mut();

        selection.clear(terrain_layer, buildings_layer, units_layer);
    }
}

// ----------------------------------------------
// TileSelection
// ----------------------------------------------

#[derive(Default)]
pub struct TileSelection<'a> {
    rect: Rect2D, // Range selection rect w/ cursor click-n-drag.
    cursor_drag_start: Point2D,
    current_cursor_pos: Point2D,
    left_mouse_button_held: bool,
    placement_candidate: Option<&'a TileDef>, // Tile placement candidate.
    selection_flags: TileFlags,
    cells: SmallVec::<[Cell2D; 36]>,
}

impl<'a> TileSelection<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_mouse_click(&mut self, button: MouseButton, action: InputAction, cursor_pos: Point2D) -> bool {
        let mut range_selecting = false;
        if button == MouseButton::Left {
            if action == InputAction::Press {
                self.cursor_drag_start = cursor_pos;
                self.left_mouse_button_held = true;
                range_selecting = true;
            } else if action == InputAction::Release {
                self.cursor_drag_start = Point2D::zero();
                self.left_mouse_button_held = false;
            }
        }
        range_selecting
    }

    pub fn update(&mut self, cursor_pos: Point2D, placement_candidate: Option<&'a TileDef>) {
        self.current_cursor_pos = cursor_pos;
        self.placement_candidate = placement_candidate;
        if self.left_mouse_button_held {
            // Keep updating the selection rect while left mouse button is held.
            self.rect = Rect2D::from_extents(self.cursor_drag_start, cursor_pos);   
        } else {
            self.rect = Rect2D::zero();
        }
    }

    pub fn draw(&self, render_sys: &mut RenderSystem) {
        if self.is_selecting_range() {
            render_sys.draw_wireframe_rect_with_thickness(self.rect, Color::new(0.2, 0.7, 0.2, 1.0), 1.5);
        }
    }

    pub fn has_valid_placement(&self) -> bool {
        self.selection_flags != TileFlags::Invalidated
    }

    fn last_cell(&self) -> Cell2D {
        self.cells.last().unwrap_or(&Cell2D::invalid()).clone()
    }

    fn is_selecting_range(&self) -> bool {
        self.left_mouse_button_held && self.rect.is_valid()
    }

    fn clear(&mut self,
             terrain_layer: &mut TileMapLayer<'a>,
             buildings_layer: &mut TileMapLayer<'a>,
             units_layer: &mut TileMapLayer<'a>) {

        self.selection_flags = TileFlags::None;

        while !self.cells.is_empty() {
            self.toggle_selection(terrain_layer,
                                  buildings_layer,
                                  units_layer,
                                  self.last_cell(),
                                  false);
        }
    }

    fn toggle_tile_selection(&mut self, tile: &mut Tile, flags: TileFlags, selected: bool) {
        if selected {
            tile.flags.set(flags, true);
            self.cells.push(tile.cell);
        } else {
            tile.flags.set(TileFlags::Highlighted | TileFlags::Invalidated, false);
            self.cells.pop();
        }
    }

    fn toggle_selection(&mut self,
                        terrain_layer: &mut TileMapLayer<'a>,
                        buildings_layer: &mut TileMapLayer<'a>,
                        units_layer: &mut TileMapLayer<'a>,
                        base_cell: Cell2D,
                        selected: bool) {

        // Deal with multi-tile buildings:
        let mut footprint =
            if let Some(placement_candidate) = self.placement_candidate {
                // During placement tile hovering:
                placement_candidate.calc_footprint_cells(base_cell)
            } else {
                // During drag selection/mouse hover:
                calc_tile_footprint_cells(base_cell, buildings_layer)
            };

        // Highlight building placement overlaps:
        let mut flags = TileFlags::Highlighted;
        if let Some(placement_candidate) = self.placement_candidate {
            if placement_candidate.is_building() {
                for &footprint_cell in &footprint {
                    if !terrain_layer.is_cell_within_bounds(footprint_cell) {
                        // If any cell would fall outside of the map bounds we won't place.
                        flags = TileFlags::Invalidated;
                    }

                    if let Some(current_tile) = buildings_layer.try_tile(footprint_cell) {
                        if current_tile.is_building() || current_tile.is_building_blocker() {
                            // Cannot place building here.
                            flags = TileFlags::Invalidated;

                            // Fully highlight the other building too:
                            let other_building_footprint =
                                calc_tile_footprint_cells(footprint_cell, buildings_layer);

                            for other_footprint_cell in other_building_footprint {
                                if let Some(tile) = buildings_layer.try_tile_mut(other_footprint_cell) {
                                    if !tile.is_empty() {
                                        self.toggle_tile_selection(tile, flags, selected);
                                    }
                                }
                                if let Some(tile) = terrain_layer.try_tile_mut(other_footprint_cell) {
                                    // NOTE: Highlight terrain even when empty so we can correctly highlight grid cells.
                                    self.toggle_tile_selection(tile, flags, selected);
                                }
                            }
                        }
                    }

                    if let Some(current_tile) = units_layer.try_tile(footprint_cell) {
                        if current_tile.is_unit() {
                            // Cannot place building here.
                            flags = TileFlags::Invalidated;
                        }
                    }
                }
            } else if placement_candidate.is_unit() {
                // Trying to place unit over building?
                if let Some(current_tile) = buildings_layer.try_tile(base_cell) {
                    if current_tile.is_building() || current_tile.is_building_blocker() {
                        // Cannot place unit here.
                        flags = TileFlags::Invalidated;
                        // Take the building's footprint so we'll highlight all of its tiles.
                        footprint = calc_tile_footprint_cells(base_cell, buildings_layer);
                    }
                }
            } else if placement_candidate.is_empty() {
                // Tile clearing, highlight tile to be removed:
                flags = TileFlags::Invalidated;
                if let Some(current_tile) = buildings_layer.try_tile(base_cell) {
                    if current_tile.is_building() || current_tile.is_building_blocker() {
                        // If we're attempting to remove a building, take its own
                        // footprint instead, as it may consist of many tiles.
                        footprint = calc_tile_footprint_cells(base_cell, buildings_layer);
                    }
                }
            }
        }

        for footprint_cell in footprint {
            if let Some(tile) = terrain_layer.try_tile_mut(footprint_cell) {
                // NOTE: Highlight terrain even when empty so we can correctly highlight grid cells.
                self.toggle_tile_selection(tile, flags, selected);
            }

            if self.placement_candidate.is_some_and(|t| t.is_terrain()) {
                // No highlighting of buildings/units when placing a terrain tile (terrain can always be placed underneath).
                continue;
            }

            if let Some(tile) = buildings_layer.try_tile_mut(footprint_cell) {
                if !tile.is_empty() {
                    self.toggle_tile_selection(tile, flags, selected);
                }
            }

            if let Some(tile) = units_layer.try_tile_mut(footprint_cell) {
                if !tile.is_empty() {
                    self.toggle_tile_selection(tile, flags, selected);
                }
            }
        }

        self.selection_flags = flags;
    }
}

// ----------------------------------------------
// Tile selection helpers
// ----------------------------------------------

fn cells_overlap(lhs_cells: &TileFootprintList, rhs_cells: &TileFootprintList) -> bool {
    for lhs_cell in lhs_cells {
        for rhs_cell in rhs_cells {
            if lhs_cell == rhs_cell {
                return true;
            }
        }
    }
    false
}

fn find_exact_cell(tile_map: &TileMap,
                   kind: TileMapLayerKind,
                   cursor_screen_pos: Point2D,
                   transform: &WorldToScreenTransform) -> Cell2D {

    let cursor_iso_pos = screen_to_iso_point(
        cursor_screen_pos, transform, BASE_TILE_SIZE, false);

    let approx_cell = iso_to_cell(cursor_iso_pos, BASE_TILE_SIZE);

    if tile_map.is_cell_within_bounds(approx_cell) {
        // Get the 8 possible neighboring tiles + self and test cursor intersection
        // against each so we can know precisely which tile the cursor is hovering.
        let neighbors = tile_map.layer(kind).tile_neighbors(approx_cell, true);
        for neighbor in neighbors {
            if let Some(tile) = neighbor {
                if cursor_inside_tile_cell(cursor_screen_pos, tile, transform) {
                    return tile.cell;
                }
            }
        }
    }

    Cell2D::invalid()
}

fn calc_tile_footprint_cells(base_cell: Cell2D, buildings: &TileMapLayer) -> TileFootprintList {
    // Buildings may take up multiple cells.
    if let Some(building_layer_tile) = buildings.try_tile(base_cell) {
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
        if building_layer_tile.is_building_blocker() {
            debug_assert!(building_layer_tile.owner_cell.is_valid());
            let owning_building_tile = buildings.tile(building_layer_tile.owner_cell);
            owning_building_tile.calc_footprint_cells()
        } else {
            building_layer_tile.calc_footprint_cells()
        }
    } else {
        smallvec![base_cell]
    }
}

fn cursor_inside_tile_cell(cursor_screen_pos: Point2D,
                           tile: &Tile,
                           transform: &WorldToScreenTransform) -> bool {

    debug_assert!(transform.is_valid());

    let screen_points = cell_to_screen_diamond_points(
        tile.cell,
        tile.def.logical_size,
        transform);

    is_screen_point_inside_diamond(cursor_screen_pos, &screen_points)
}

// "Broad-Phase" tile selection based on the 4 corners of a rectangle.
// Given the layout of the isometric tile map, this algorithm is quite greedy
// and will select more tiles than actually intersect the rect, so a refinement
// pass must be done after to intersect each tile's rect with the selection rect.
fn tile_selection_bounds(screen_rect: &Rect2D,
                         tile_size: Size2D,
                         map_size: Size2D,
                         transform: &WorldToScreenTransform) -> (Cell2D, Cell2D) {

    debug_assert!(screen_rect.is_valid());

    // Convert screen-space corners to isometric space:
    let top_left = screen_to_iso_point(
        screen_rect.mins, transform, BASE_TILE_SIZE, false);
    let bottom_right = screen_to_iso_point(
        screen_rect.maxs, transform, BASE_TILE_SIZE, false);
    let top_right = screen_to_iso_point(
        Point2D::new(screen_rect.maxs.x, screen_rect.mins.y),
        transform, BASE_TILE_SIZE, false);
    let bottom_left = screen_to_iso_point(
        Point2D::new(screen_rect.mins.x, screen_rect.maxs.y),
        transform, BASE_TILE_SIZE, false);

    // Convert isometric points to cell coordinates:
    let cell_tl = iso_to_cell(top_left, tile_size);
    let cell_tr = iso_to_cell(top_right, tile_size);
    let cell_bl = iso_to_cell(bottom_left, tile_size);
    let cell_br = iso_to_cell(bottom_right, tile_size);

    // Compute bounding min/max cell coordinates:
    let mut min_x = cell_tl.x.min(cell_tr.x).min(cell_bl.x).min(cell_br.x);
    let mut max_x = cell_tl.x.max(cell_tr.x).max(cell_bl.x).max(cell_br.x);
    let mut min_y = cell_tl.y.min(cell_tr.y).min(cell_bl.y).min(cell_br.y);
    let mut max_y = cell_tl.y.max(cell_tr.y).max(cell_bl.y).max(cell_br.y);

    // Clamp to map bounds:
    min_x = min_x.clamp(0, map_size.width  - 1);
    max_x = max_x.clamp(0, map_size.width  - 1);
    min_y = min_y.clamp(0, map_size.height - 1);
    max_y = max_y.clamp(0, map_size.height - 1);

    (Cell2D::new(min_x, min_y), Cell2D::new(max_x, max_y))
}

// Creates an isometric-aligned diamond rectangle for the given tile size and cell location.
pub fn cell_to_screen_diamond_points(cell: Cell2D,
                                     tile_size: Size2D,
                                     transform: &WorldToScreenTransform) -> [Point2D; 4] {

    let iso_center = cell_to_iso(cell, BASE_TILE_SIZE);
    let screen_center = iso_to_screen_point(iso_center, transform, BASE_TILE_SIZE, false);

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

// ----------------------------------------------
// TileMapRenderer / TileMapRenderFlags
// ----------------------------------------------

bitflags! {
    #[derive(Copy, Clone)]
    pub struct TileMapRenderFlags: u32 {
        const None                = 0;
        const DrawTerrain         = 1 << 1;
        const DrawBuildings       = 1 << 2;
        const DrawUnits           = 1 << 3;
        const DrawGrid            = 1 << 4; // Grid draws on top of terrain but under buildings/units.
        const DrawGridIgnoreDepth = 1 << 5; // Grid draws on top of everything ignoring z-sort order.

        // Debug flags:
        const DrawTerrainTileDebugInfo   = 1 << 6;
        const DrawBuildingsTileDebugInfo = 1 << 7;
        const DrawUnitsTileDebugInfo     = 1 << 8;
        const DrawTileDebugBounds        = 1 << 9;
        const DrawDebugBuildingBlockers  = 1 << 10;
    }
}

struct TileDrawListEntry {
    // NOTE: Raw pointer, no lifetime.
    // This is only a temporary reference that lives
    // for the scope of TileMapRenderer::draw_map().
    // Since we store this in a temp vector that is a member
    // of TileMapRenderer we need to bypass the borrow checker.
    // Not ideal but avoids having to allocate a new temporary
    // local Vec each time draw_map() is called.
    tile_ptr: *const Tile<'static>,

    // Y value of the bottom left corner of the tile sprite for sorting.
    // Simulates a pseudo depth value so we can render units and buildings
    // correctly.
    z_sort: i32,
}

pub struct TileMapRenderer {
    world_to_screen: WorldToScreenTransform,
    grid_color: Color,
    grid_line_thickness: f32,
    temp_tile_sort_list: Vec<TileDrawListEntry>, // For z-sorting.
}

impl TileMapRenderer {
    pub fn new() -> Self {
        Self {
            world_to_screen: WorldToScreenTransform::default(),
            grid_color: Color::white(),
            grid_line_thickness: 1.0,
            temp_tile_sort_list: Vec::with_capacity(512),
        }
    }

    pub fn set_draw_scaling(&mut self, scaling: i32) -> &mut Self {
        debug_assert!(scaling > 0);
        self.world_to_screen.scaling = scaling;
        self
    }

    pub fn set_draw_offset(&mut self, offset: Point2D) -> &mut Self {
        self.world_to_screen.offset = offset;
        self
    }

    pub fn set_tile_spacing(&mut self, spacing: i32) -> &mut Self {
        debug_assert!(spacing >= 0);
        self.world_to_screen.tile_spacing = spacing;
        self
    }

    pub fn set_grid_color(&mut self, color: Color) -> &mut Self {
        self.grid_color = color;
        self
    }

    pub fn set_grid_line_thickness(&mut self, thickness: f32) -> &mut Self {
        debug_assert!(thickness > 0.0);
        self.grid_line_thickness = thickness;
        self
    }

    pub fn world_to_screen_transform(&self) -> WorldToScreenTransform {
        self.world_to_screen
    }

    pub fn draw_map(&mut self,
                    render_sys: &mut RenderSystem,
                    ui_sys: &UiSystem,
                    tile_map: &TileMap,
                    flags: TileMapRenderFlags) {

        debug_assert!(self.temp_tile_sort_list.is_empty());
        let map_cells = tile_map.size_in_cells;

        // Terrain:
        if flags.contains(TileMapRenderFlags::DrawTerrain) {
            let terrain_layer = tile_map.layer(TileMapLayerKind::Terrain);
            debug_assert!(terrain_layer.size_in_cells == map_cells);

            for y in (0..map_cells.height).rev() {
                for x in (0..map_cells.width).rev() {

                    let tile = terrain_layer.tile(Cell2D::new(x, y));
                    if tile.is_empty() {
                        continue;
                    }

                    // Terrain tiles size is constrained.
                    debug_assert!(tile.is_terrain() && tile.def.logical_size == BASE_TILE_SIZE);

                    let tile_iso_coords = tile.calc_adjusted_iso_coords();
                    self.draw_tile(render_sys, ui_sys, tile_iso_coords, tile, flags);
                }
            }
        }

        if flags.contains(TileMapRenderFlags::DrawGrid) &&
          !flags.contains(TileMapRenderFlags::DrawGridIgnoreDepth) {
            // Draw the grid now so that lines will be on top of the terrain but not on top of buildings.
            self.draw_isometric_grid(render_sys, tile_map);
        }

        // Buildings & Units:
        if flags.intersects(TileMapRenderFlags::DrawBuildings | TileMapRenderFlags::DrawUnits) {
            let buildings_layer = tile_map.layer(TileMapLayerKind::Buildings);
            let units_layer = tile_map.layer(TileMapLayerKind::Units);

            debug_assert!(buildings_layer.size_in_cells == map_cells);
            debug_assert!(units_layer.size_in_cells == map_cells);

            let mut add_to_sort_list = |tile: &Tile| {
                self.temp_tile_sort_list.push(TileDrawListEntry {
                    tile_ptr: tile as *const Tile<'_> as *const Tile<'static>,
                    z_sort: tile.calc_z_sort(),
                });
            };

            for y in (0..map_cells.height).rev() {
                for x in (0..map_cells.width).rev() {

                    let cell = Cell2D::new(x, y);
                    let building_tile = buildings_layer.tile(cell);
                    let unit_tile = units_layer.tile(cell);

                    if building_tile.is_building() && flags.contains(TileMapRenderFlags::DrawBuildings) {
                        add_to_sort_list(building_tile);
                    } else if unit_tile.is_unit() && flags.contains(TileMapRenderFlags::DrawUnits) {
                        add_to_sort_list(unit_tile);
                    } else if building_tile.is_building_blocker() && // DEBUG:
                              flags.contains(TileMapRenderFlags::DrawDebugBuildingBlockers) {

                        let tile_iso_coords = building_tile.calc_adjusted_iso_coords();
                        let tile_rect = iso_to_screen_rect(
                            tile_iso_coords,
                            building_tile.def.draw_size,
                            &self.world_to_screen,
                            true);

                        debug::draw_tile_debug(
                            render_sys,
                            ui_sys,
                            tile_iso_coords,
                            tile_rect,
                            &self.world_to_screen,
                            building_tile,
                            flags);
                    }
                }
            }

            self.temp_tile_sort_list.sort_by(|a, b| {
                a.z_sort.cmp(&b.z_sort)
            });

            for entry in &self.temp_tile_sort_list {
                // SAFETY: This reference only lives for the scope of this function.
                // The only reason we store it in a member Vec is to avoid the memory
                // allocation cost of a temp local Vec. temp_tile_draw_list is always
                // cleared at the end of this function.
                debug_assert!(entry.tile_ptr.is_null() == false);
                let tile = unsafe { &*entry.tile_ptr };

                debug_assert!(tile.is_building() || tile.is_unit());

                let tile_iso_coords = tile.calc_adjusted_iso_coords();
                self.draw_tile(render_sys, ui_sys, tile_iso_coords, tile, flags);
            }

            self.temp_tile_sort_list.clear();
        }

        if flags.contains(TileMapRenderFlags::DrawGridIgnoreDepth) {
            // Allow lines to draw later and effectively bypass the draw order
            // and appear on top of everything else (useful for debugging).
            self.draw_isometric_grid(render_sys, tile_map);
        }
    }

    fn draw_isometric_grid(&self,
                           render_sys: &mut RenderSystem,
                           tile_map: &TileMap) {
    
        let map_cells = tile_map.size_in_cells;
        let terrain_layer = tile_map.layer(TileMapLayerKind::Terrain);
        let line_thickness = self.grid_line_thickness * (self.world_to_screen.scaling as f32);

        let mut highlighted_cells = SmallVec::<[[Point2D; 4]; 128]>::new();
        let mut invalidated_cells = SmallVec::<[[Point2D; 4]; 128]>::new();

        for y in (0..map_cells.height).rev() {
            for x in (0..map_cells.width).rev() {
                let cell = Cell2D::new(x, y);
                let points = cell_to_screen_diamond_points(cell, BASE_TILE_SIZE, &self.world_to_screen);

                // Save highlighted grid cells for drawing at the end, so they display in the right order.
                let tile = terrain_layer.tile(cell);
    
                if tile.flags.contains(TileFlags::Highlighted) {
                    highlighted_cells.push(points);
                    continue;
                }

                if tile.flags.contains(TileFlags::Invalidated) {
                    invalidated_cells.push(points);
                    continue;
                }

                // Draw diamond:
                render_sys.draw_polyline_with_thickness(&points, self.grid_color, line_thickness, true);
            }

            // Highlighted on top:
            for points in &highlighted_cells {
                render_sys.draw_polyline_with_thickness(&points, GRID_HIGHLIGHT_COLOR, line_thickness, true);
            }

            for points in &invalidated_cells {
                render_sys.draw_polyline_with_thickness(&points, GRID_INVALID_COLOR, line_thickness, true);
            }
        }
    }

    fn draw_tile(&self,
                 render_sys: &mut RenderSystem,
                 ui_sys: &UiSystem,
                 tile_iso_coords: IsoPoint2D,
                 tile: &Tile,
                 flags: TileMapRenderFlags) {

        debug_assert!(tile.def.is_valid() && !tile.is_empty());

        // Only terrain and buildings might require spacing.
        let apply_spacing = if !tile.is_unit() { true } else { false };

        let tile_rect = iso_to_screen_rect(
            tile_iso_coords,
            tile.def.draw_size,
            &self.world_to_screen,
            apply_spacing);

        let highlight_color =
            if tile.flags.contains(TileFlags::Highlighted) {
                TILE_HIGHLIGHT_COLOR
            } else if tile.flags.contains(TileFlags::Invalidated) {
                TILE_INVALID_COLOR
            } else {
                Color::white()
            };

        render_sys.draw_textured_colored_rect(
            tile_rect,
            &tile.def.tex_info.coords,
            tile.def.tex_info.texture,
            tile.def.color * highlight_color);

        debug::draw_tile_debug(
            render_sys,
            ui_sys,
            tile_iso_coords,
            tile_rect,
            &self.world_to_screen,
            tile,
            flags);
    }
}
