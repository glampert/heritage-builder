use smallvec::SmallVec;

use crate::{
    imgui_ui::UiInputEvent,
    render::RenderSystem,
    app::input::{MouseButton, InputAction},
    utils::{self, Cell2D, Point2D, Rect2D, Size2D, WorldToScreenTransform}
};

use super::{
    sets::{TileDef, TileKind, BASE_TILE_SIZE},
    map::{Tile, TileFlags, TileMapLayerMutRefs},
    rendering::SELECTION_RECT_COLOR
};

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

    pub fn has_valid_placement(&self) -> bool {
        self.selection_flags != TileFlags::Invalidated
    }

    pub fn on_mouse_click(&mut self, button: MouseButton, action: InputAction, cursor_screen_pos: Point2D) -> UiInputEvent {
        if button == MouseButton::Left {
            if action == InputAction::Press {
                self.cursor_drag_start = cursor_screen_pos;
                self.left_mouse_button_held = true;
                return UiInputEvent::Handled;
            } else if action == InputAction::Release {
                self.cursor_drag_start = Point2D::zero();
                self.left_mouse_button_held = false;
            }
        }
        UiInputEvent::NotHandled
    }

    pub fn draw(&self, render_sys: &mut RenderSystem) {
        if self.is_selecting_range() {
            render_sys.draw_wireframe_rect_with_thickness(
                self.rect,
                SELECTION_RECT_COLOR,
                1.5);
        }
    }

    pub fn update(&mut self,
                  layers: &mut TileMapLayerMutRefs<'a>,
                  map_size_in_cells: Size2D,
                  cursor_screen_pos: Point2D,
                  transform: &WorldToScreenTransform,
                  placement_candidate: Option<&'a TileDef>) {

        self.current_cursor_pos  = cursor_screen_pos;
        self.placement_candidate = placement_candidate;

        if self.left_mouse_button_held {
            // Keep updating the selection rect while left mouse button is held.
            self.rect = Rect2D::from_extents(self.cursor_drag_start, cursor_screen_pos);   
        } else {
            self.rect = Rect2D::zero();
        }

        if self.is_selecting_range() {
            // Clear previous highlighted tiles:
            self.clear(layers);

            let range = bounds(
                &self.rect,
                BASE_TILE_SIZE,
                map_size_in_cells,
                transform);

            for y in range.min.y..=range.max.y {
                for x in range.min.x..=range.max.x {
                    if let Some(base_tile) = layers.terrain.try_tile(Cell2D::new(x, y)) {

                        let tile_iso_coords =
                            base_tile.calc_adjusted_iso_coords();

                        let tile_screen_rect = utils::iso_to_screen_rect(
                            tile_iso_coords,
                            base_tile.logical_size(),
                            transform,
                            false);

                        if tile_screen_rect.intersects(&self.rect) {
                            self.toggle_selection(layers, base_tile.cell, true);
                        }
                    }
                }
            }
        } else {
            // Clear previous highlighted tile for single selection:
            let cursor_screen_pos = self.current_cursor_pos;
            let last_cell = self.last_cell();

            if let Some(base_tile) = layers.terrain.try_tile(last_cell) {
                // If the cursor is still inside this cell, we're done.
                // This can happen because the isometric-to-cell conversion
                // is not absolute but rather based on proximity to the cell's center.
                if utils::is_screen_point_inside_cell(cursor_screen_pos,
                                                      base_tile.cell,
                                                      base_tile.logical_size(),
                                                      BASE_TILE_SIZE,
                                                      transform) {
                    return;
                }

                // Clear:
                let previous_selected_cell: Cell2D = base_tile.cell;
                self.toggle_selection(layers, previous_selected_cell, false);
            }

            // Set highlight:
            {
                let highlight_cell = layers.terrain.find_exact_cell_for_point(
                    cursor_screen_pos,
                    transform);

                if layers.terrain.is_cell_within_bounds(highlight_cell) {
                    self.toggle_selection(layers, highlight_cell, true);
                }
            }
        }
    }

    pub fn clear(&mut self, layers: &mut TileMapLayerMutRefs<'a>) {
        self.selection_flags = TileFlags::empty();
        while !self.cells.is_empty() {
            self.toggle_selection(layers, self.last_cell(), false);
        }
    }

    pub fn last_cell(&self) -> Cell2D {
        self.cells.last().unwrap_or(&Cell2D::invalid()).clone()
    }

    fn is_selecting_range(&self) -> bool {
        self.left_mouse_button_held && self.rect.is_valid()
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
                        layers: &mut TileMapLayerMutRefs<'a>,
                        base_cell: Cell2D,
                        selected: bool) {

        // Deal with multi-tile buildings:
        let mut footprint =
            if let Some(placement_candidate) = self.placement_candidate {
                // During placement tile hovering:
                placement_candidate.calc_footprint_cells(base_cell)
            } else {
                // During drag selection/mouse hover:
                Tile::calc_exact_footprint_cells(base_cell, layers.buildings)
            };

        // Highlight building placement overlaps:
        let mut flags = TileFlags::Highlighted;
        if let Some(placement_candidate) = self.placement_candidate {
            if placement_candidate.is_building() {
                for &footprint_cell in &footprint {
                    if !layers.terrain.is_cell_within_bounds(footprint_cell) {
                        // If any cell would fall outside of the map bounds we won't place.
                        flags = TileFlags::Invalidated;
                    }

                    if layers.buildings.has_tile(footprint_cell, &[TileKind::Building, TileKind::Blocker]) {
                        // Cannot place building here.
                        flags = TileFlags::Invalidated;

                        // Fully highlight the other building too:
                        let other_building_footprint =
                            Tile::calc_exact_footprint_cells(footprint_cell, layers.buildings);

                        for other_footprint_cell in other_building_footprint {
                            if let Some(building_tile) = layers.buildings.find_tile_mut(
                                    other_footprint_cell, &[TileKind::Building, TileKind::Blocker]) {

                                self.toggle_tile_selection(building_tile, flags, selected);
                            }
                            if let Some(terrain_tile) = layers.terrain.try_tile_mut(other_footprint_cell) {
                                // NOTE: Highlight terrain even when empty so we can correctly highlight grid cells.
                                self.toggle_tile_selection(terrain_tile, flags, selected);
                            }
                        }
                    }

                    if layers.units.has_tile(footprint_cell, &[TileKind::Unit]) {
                        // Cannot place building here.
                        flags = TileFlags::Invalidated;
                    }
                }
            } else if placement_candidate.is_unit() {
                // Trying to place unit over building?
                if layers.buildings.has_tile(base_cell, &[TileKind::Building, TileKind::Blocker]) {
                    // Cannot place unit here.
                    flags = TileFlags::Invalidated;
                    // Take the building's footprint so we'll highlight all of its tiles.
                    footprint = Tile::calc_exact_footprint_cells(base_cell, layers.buildings);
                }
            } else if placement_candidate.is_empty() {
                // Tile clearing, highlight tile to be removed:
                flags = TileFlags::Invalidated;
                if layers.buildings.has_tile(base_cell, &[TileKind::Building, TileKind::Blocker]) {
                    // If we're attempting to remove a building, take its own
                    // footprint instead, as it may consist of many tiles.
                    footprint = Tile::calc_exact_footprint_cells(base_cell, layers.buildings);
                }
            }
        }

        for footprint_cell in footprint {
            if let Some(terrain_tile) = layers.terrain.try_tile_mut(footprint_cell) {
                // NOTE: Highlight terrain even when empty so we can correctly highlight grid cells.
                self.toggle_tile_selection(terrain_tile, flags, selected);
            }

            if self.placement_candidate.is_some_and(|tile| tile.is_terrain()) {
                // No highlighting of buildings/units when placing a terrain tile
                // (terrain can always be placed underneath).
                continue;
            }

            if let Some(building_tile) = layers.buildings.find_tile_mut(
                footprint_cell, &[TileKind::Building, TileKind::Blocker]) {

                self.toggle_tile_selection(building_tile, flags, selected);
            }

            if let Some(unit_tile) = layers.units.find_tile_mut(
                footprint_cell, &[TileKind::Unit]) {

                self.toggle_tile_selection(unit_tile, flags, selected);
            }
        }

        self.selection_flags = flags;
    }
}

// ----------------------------------------------
// Tile selection helpers
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct CellRange {
    pub min: Cell2D,
    pub max: Cell2D,
}

// "Broad-Phase" tile selection based on the 4 corners of a rectangle.
// Given the layout of the isometric tile map, this algorithm is quite greedy
// and will select more tiles than actually intersect the rect, so a refinement
// pass must be done after to intersect each tile's rect with the selection rect.
pub fn bounds(screen_rect: &Rect2D,
              tile_size: Size2D,
              map_size_in_cells: Size2D,
              transform: &WorldToScreenTransform) -> CellRange {

    debug_assert!(screen_rect.is_valid());

    // Convert screen-space corners to isometric space:
    let top_left = utils::screen_to_iso_point(
        screen_rect.min, transform, BASE_TILE_SIZE, false);
    let bottom_right = utils::screen_to_iso_point(
        screen_rect.max, transform, BASE_TILE_SIZE, false);
    let top_right = utils::screen_to_iso_point(
        Point2D::new(screen_rect.max.x, screen_rect.min.y),
        transform, BASE_TILE_SIZE, false);
    let bottom_left = utils::screen_to_iso_point(
        Point2D::new(screen_rect.min.x, screen_rect.max.y),
        transform, BASE_TILE_SIZE, false);

    // Convert isometric points to cell coordinates:
    let cell_tl = utils::iso_to_cell(top_left, tile_size);
    let cell_tr = utils::iso_to_cell(top_right, tile_size);
    let cell_bl = utils::iso_to_cell(bottom_left, tile_size);
    let cell_br = utils::iso_to_cell(bottom_right, tile_size);

    // Compute bounding min/max cell coordinates:
    let mut min_x = cell_tl.x.min(cell_tr.x).min(cell_bl.x).min(cell_br.x);
    let mut max_x = cell_tl.x.max(cell_tr.x).max(cell_bl.x).max(cell_br.x);
    let mut min_y = cell_tl.y.min(cell_tr.y).min(cell_bl.y).min(cell_br.y);
    let mut max_y = cell_tl.y.max(cell_tr.y).max(cell_bl.y).max(cell_br.y);

    // Clamp to map bounds:
    min_x = min_x.clamp(0, map_size_in_cells.width  - 1);
    max_x = max_x.clamp(0, map_size_in_cells.width  - 1);
    min_y = min_y.clamp(0, map_size_in_cells.height - 1);
    max_y = max_y.clamp(0, map_size_in_cells.height - 1);

    CellRange {
        min: Cell2D::new(min_x, min_y),
        max: Cell2D::new(max_x, max_y),
    }
}
