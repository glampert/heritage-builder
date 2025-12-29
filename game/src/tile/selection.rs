use smallvec::SmallVec;

use super::{
    placement::{self, PlacementOp},
    rendering::SELECTION_RECT_COLOR,
    Tile, TileFlags, TileKind, TileMap, TileMapLayerKind, TileMapLayerMutRefs, BASE_TILE_SIZE,
};
use crate::{
    ui::UiInputEvent,
    render::RenderSystem,
    save::{Load, PostLoadContext, Save},
    pathfind::{NodeKind as PathNodeKind},
    app::input::{InputAction, MouseButton},
    utils::{
        coords::{self, Cell, CellRange, WorldToScreenTransform},
        Rect, Size, Vec2,
    },
};

// ----------------------------------------------
// TileSelection
// ----------------------------------------------

#[derive(Default)]
pub struct TileSelection {
    rect: Rect, // Range selection rect w/ cursor click-n-drag.
    cursor_drag_start: Vec2,
    cursor_drag_start_cell: Cell,
    left_mouse_button_held: bool,
    valid_placement: bool,
    is_clearing: bool,
    cells: SmallVec<[Cell; 64]>,
}

impl TileSelection {
    pub fn has_valid_placement(&self) -> bool {
        self.valid_placement
    }

    pub fn first_cell(&self) -> Cell {
        *self.cells.first().unwrap_or(&Cell::invalid())
    }

    pub fn last_cell(&self) -> Cell {
        *self.cells.last().unwrap_or(&Cell::invalid())
    }

    pub fn cells(&self) -> &SmallVec<[Cell; 64]> {
        &self.cells
    }

    pub fn range_selection_cells(&self,
                                 tile_map: &TileMap,
                                 cursor_screen_pos: Vec2,
                                 transform: WorldToScreenTransform)
                                 -> Option<(Cell, Cell)> {
        if self.is_selecting_range() && self.cursor_drag_start_cell.is_valid() {
            let start = self.cursor_drag_start_cell;
            let end = tile_map.find_exact_cell_for_point(
                TileMapLayerKind::Terrain,
                cursor_screen_pos,
                transform);

            Some((start, end))
        } else if self.left_mouse_button_held && self.cursor_drag_start != Vec2::zero() {
            let cell = tile_map.find_exact_cell_for_point(
                TileMapLayerKind::Terrain,
                self.cursor_drag_start,
                transform);
            Some((cell, cell)) // One cell selection.
        } else {
            None
        }
    }

    pub fn on_mouse_button(&mut self,
                           button: MouseButton,
                           action: InputAction,
                           tile_map: &TileMap,
                           cursor_screen_pos: Vec2,
                           transform: WorldToScreenTransform)
                           -> UiInputEvent {
        if button == MouseButton::Left {
            if action == InputAction::Press {
                self.cursor_drag_start = cursor_screen_pos;
                self.cursor_drag_start_cell = tile_map.find_exact_cell_for_point(
                    TileMapLayerKind::Terrain,
                    cursor_screen_pos,
                    transform);
                self.left_mouse_button_held = true;
                return UiInputEvent::Handled;
            } else if action == InputAction::Release {
                self.cursor_drag_start = Vec2::zero();
                self.cursor_drag_start_cell = Cell::invalid();
                self.left_mouse_button_held = false;
                return UiInputEvent::Handled;
            }
        }
        UiInputEvent::NotHandled
    }

    pub fn draw(&self, render_sys: &mut impl RenderSystem) {
        if self.is_selecting_range() && self.is_clearing {
            render_sys.draw_wireframe_rect_with_thickness(self.rect, SELECTION_RECT_COLOR, 1.5);
        }
    }

    pub fn update(&mut self,
                  mut layers: TileMapLayerMutRefs,
                  map_size_in_cells: Size,
                  cursor_screen_pos: Vec2,
                  transform: WorldToScreenTransform,
                  placement_op: PlacementOp) {
        if self.left_mouse_button_held {
            // Keep updating the selection rect while left mouse button is held.
            self.rect = Rect::from_extents(self.cursor_drag_start, cursor_screen_pos);
        } else {
            self.rect = Rect::zero();
        }

        // Only using range selection for batch tile clearing/removal.
        self.is_clearing = matches!(placement_op, PlacementOp::Clear);

        if self.is_selecting_range() && self.is_clearing {
            // Clear previous highlighted tiles:
            self.clear(layers);

            let range = bounds(&self.rect, BASE_TILE_SIZE, map_size_in_cells, transform);

            for cell in &range {
                if let Some(base_tile) = layers.get(TileMapLayerKind::Terrain).try_tile(cell) {
                    let tile_iso_coords = base_tile.iso_coords();
                    let tile_screen_rect = coords::iso_to_screen_rect(tile_iso_coords,
                                                                      base_tile.logical_size(),
                                                                      transform);

                    if tile_screen_rect.intersects(&self.rect) {
                        let base_cell = base_tile.base_cell();
                        self.toggle_selection(layers, base_cell, placement_op);
                    }
                }
            }
        } else {
            // Clear previous highlighted tile for single selection:
            let last_cell = self.last_cell();

            if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile(last_cell) {
                // If the cursor is still inside this cell, we're done.
                // This can happen because the isometric-to-cell conversion
                // is not absolute but rather based on proximity to the cell's center.
                if tile.is_screen_point_inside_base_cell(cursor_screen_pos, transform) {
                    return;
                }

                self.clear(layers);
            }

            // Set new selection highlight:
            let highlight_cell = layers.get(TileMapLayerKind::Terrain)
                                       .find_exact_cell_for_point(cursor_screen_pos, transform);

            self.toggle_selection(layers, highlight_cell, placement_op);
        }
    }

    pub fn clear(&mut self, mut layers: TileMapLayerMutRefs) {
        for cell in &self.cells {
            if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile_mut(*cell) {
                tile.set_flags(TileFlags::Highlighted | TileFlags::Invalidated, false);
            }
            if let Some(tile) = layers.get(TileMapLayerKind::Objects).try_tile_mut(*cell) {
                tile.set_flags(TileFlags::Highlighted | TileFlags::Invalidated, false);
            }
        }

        self.valid_placement = false;
        self.cells.clear();
    }

    fn is_selecting_range(&self) -> bool {
        self.left_mouse_button_held && self.rect.is_valid()
    }

    fn select_tile(&mut self, tile: &mut Tile, selection_flags: TileFlags) {
        tile.set_flags(selection_flags, true);
        self.select_tile_no_flags(tile);
    }

    fn select_tile_no_flags(&mut self, tile: &Tile) {
        // Last cell should be the original starting cell (base cell).
        // Iterate in reverse.
        for cell in tile.cell_range().iter_rev() {
            self.cells.push(cell);
        }

        debug_assert!(self.last_cell() == tile.base_cell());
    }

    fn toggle_selection(&mut self,
                        mut layers: TileMapLayerMutRefs,
                        base_cell: Cell,
                        placement_op: PlacementOp) {
        if !layers.get(TileMapLayerKind::Terrain).is_cell_within_bounds(base_cell) {
            self.valid_placement = false;
            return;
        }

        // Highlight object layer tiles if we are placing an object, clearing tiles or
        // just mouse hovering. Don't highlight objects if placing terrain tiles.
        let highlight_objects = match placement_op {
            PlacementOp::Place(tile_def) | PlacementOp::Invalidate(tile_def) => {
                tile_def.is(TileKind::Object)
            }
            PlacementOp::Clear | PlacementOp::None => true,
        };

        let selection_flags = match placement_op {
            // Check if our placement candidate tile overlaps with any other Object.
            PlacementOp::Place(tile_def) => {
                let mut flags = TileFlags::Highlighted;

                if placement::is_placement_on_terrain_valid(layers.to_refs(),
                                                            base_cell,
                                                            tile_def).is_err() {
                    flags = TileFlags::Invalidated;
                } else {
                    for cell in &tile_def.cell_range(base_cell) {
                        // Placement candidate not fully within map bounds?
                        if !layers.get(TileMapLayerKind::Terrain).is_cell_within_bounds(cell) {
                            flags = TileFlags::Invalidated;
                            break;
                        }

                        // Terrain tiles can always be placed anywhere, so don't invalidate for
                        // terrain.
                        if !tile_def.is(TileKind::Terrain) {
                            // Placement candidate would overlap another object?
                            if layers.get(TileMapLayerKind::Objects).try_tile(cell).is_some() {
                                flags = TileFlags::Invalidated;
                                break;
                            }
                        }
                    }
                }

                flags
            }
            // Explicit request to invalidate the whole range of tiles occupied by the TileDef.
            PlacementOp::Invalidate(_) => TileFlags::Invalidated,
            // Tile clearing, highlight tile to be removed with the Invalidated flag instead.
            PlacementOp::Clear => TileFlags::Invalidated,
            // Tile mouse hover; normal highlight.
            PlacementOp::None => TileFlags::Highlighted,
        };

        let is_batch_clearing = self.is_selecting_range() && matches!(placement_op, PlacementOp::Clear);

        // Highlight Terrain:
        if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile_mut(base_cell) {
            if is_batch_clearing && !tile.path_kind().intersects(PathNodeKind::Road | PathNodeKind::VacantLot) {
                // Don't highlight regular terrain tiles when batch clearing (except for roads and
                // vacant lots which we want to visually highlight when clearing).
                self.select_tile_no_flags(tile);
            } else {
                self.select_tile(tile, selection_flags);
            }

            // Highlight all Terrain tiles this placement candidate would occupy.
            match placement_op {
                PlacementOp::Place(tile_def) | PlacementOp::Invalidate(tile_def) => {
                    for cell in tile_def.cell_range(base_cell).iter_rev() {
                        if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile_mut(cell) {
                            self.select_tile(tile, selection_flags);
                        }
                    }
                }
                PlacementOp::Clear | PlacementOp::None => {}
            }
        }

        // Highlight Objects:
        if highlight_objects {
            if let Some(object) = layers.get(TileMapLayerKind::Objects).try_tile_mut(base_cell) {
                self.select_tile(object, selection_flags);

                // Highlight all terrain tiles this building occupies, unless we're batch clearing.
                if !is_batch_clearing {
                    for cell in object.cell_range().iter_rev() {
                        if let Some(tile) = layers.get(TileMapLayerKind::Terrain).try_tile_mut(cell) {
                            self.select_tile(tile, selection_flags);
                        }
                    }
                }
            }
        }

        self.valid_placement = !selection_flags.intersects(TileFlags::Invalidated);
    }
}

// ----------------------------------------------
// Save/Load for TileSelection
// ----------------------------------------------

impl Save for TileSelection {}

impl Load for TileSelection {
    fn post_load(&mut self, _context: &PostLoadContext) {
        // Rest any tile selection on load.
        *self = Self::default();
    }
}

// ----------------------------------------------
// Tile selection helpers
// ----------------------------------------------

// "Broad-Phase" tile selection based on the 4 corners of a rectangle.
// Given the layout of the isometric tile map, this algorithm is quite greedy
// and will select more tiles than actually intersect the rect, so a refinement
// pass must be done after to intersect each tile's rect with the selection rect.
pub fn bounds(screen_rect: &Rect,
              tile_size: Size,
              map_size_in_cells: Size,
              transform: WorldToScreenTransform)
              -> CellRange {
    debug_assert!(screen_rect.is_valid());

    // Convert screen-space corners to isometric space:
    let top_left = coords::screen_to_iso_point(
        screen_rect.min,
        transform,
        BASE_TILE_SIZE);

    let bottom_right = coords::screen_to_iso_point(
        screen_rect.max,
        transform,
        BASE_TILE_SIZE);

    let top_right = coords::screen_to_iso_point(
        Vec2::new(screen_rect.max.x, screen_rect.min.y),
        transform,
        BASE_TILE_SIZE);

    let bottom_left = coords::screen_to_iso_point(
        Vec2::new(screen_rect.min.x, screen_rect.max.y),
        transform,
        BASE_TILE_SIZE);

    // Convert isometric points to cell coordinates:
    let cell_tl = coords::iso_to_cell(top_left, tile_size);
    let cell_tr = coords::iso_to_cell(top_right, tile_size);
    let cell_bl = coords::iso_to_cell(bottom_left, tile_size);
    let cell_br = coords::iso_to_cell(bottom_right, tile_size);

    // Compute bounding min/max cell coordinates:
    let mut min_x = cell_tl.x.min(cell_tr.x).min(cell_bl.x).min(cell_br.x);
    let mut max_x = cell_tl.x.max(cell_tr.x).max(cell_bl.x).max(cell_br.x);
    let mut min_y = cell_tl.y.min(cell_tr.y).min(cell_bl.y).min(cell_br.y);
    let mut max_y = cell_tl.y.max(cell_tr.y).max(cell_bl.y).max(cell_br.y);

    // Clamp to map bounds:
    min_x = min_x.clamp(0, map_size_in_cells.width - 1);
    max_x = max_x.clamp(0, map_size_in_cells.width - 1);
    min_y = min_y.clamp(0, map_size_in_cells.height - 1);
    max_y = max_y.clamp(0, map_size_in_cells.height - 1);

    CellRange::new(Cell::new(min_x, min_y), Cell::new(max_x, max_y))
}
