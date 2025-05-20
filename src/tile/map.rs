use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{Display, EnumCount, EnumIter};
use bitflags::bitflags;
use arrayvec::ArrayVec;
use smallvec::{smallvec, SmallVec};

use crate::utils::*;
use crate::app::input::{MouseButton, InputAction};
use crate::ui::UiSystem;
use crate::render::RenderSystem;
use super::def::{TileDef, TileKind, BASE_TILE_SIZE};
use super::debug::{self};

// ----------------------------------------------
// Tile / TileFlags
// ----------------------------------------------

bitflags! {
    #[derive(Clone)]
    pub struct TileFlags: u32 {
        const None        = 0;
        const Highlighted = 1 << 1;
    }
}

#[derive(Clone)]
pub struct Tile<'a> {
    pub cell: Cell2D,
    pub owner_cell: Cell2D,
    pub def: &'a TileDef,
    pub flags: TileFlags,
}

type TileFootprintList = SmallVec<[Cell2D; 16]>;

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

    pub fn is_empty(&self) -> bool {
        self.def.is_empty()
    }

    pub fn is_terrain(&self) -> bool {
        self.def.is_terrain()
    }

    pub fn is_building(&self) -> bool {
        self.def.is_building()
    }

    pub fn is_building_blocker(&self) -> bool {
        self.def.is_building_blocker()
    }

    pub fn is_unit(&self) -> bool {
        self.def.is_unit()
    }

    pub fn calc_z_sort(&self) -> i32 {
        cell_to_iso(self.cell, BASE_TILE_SIZE).y - self.def.logical_size.height
    }

    pub fn calc_adjusted_iso_coords(&self) -> IsoPoint2D {
        match self.def.kind {
            TileKind::Terrain => {
                // No position adjustments needed for terrain tiles.
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
            _ => panic!("Invalid Tile kind!")
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

    pub fn calc_footprint_cells(&self) -> TileFootprintList {
        let mut footprint = TileFootprintList::new();

        if !self.is_empty() {
            let size = self.def.size_in_tiles();
            debug_assert!(size.is_valid());

            // Buildings can occupy multiple cells; Find which ones:
            let start_cell = self.cell;
            let end_cell = Cell2D::new(start_cell.x + size.width - 1, start_cell.y + size.height - 1);

            for y in (start_cell.y..=end_cell.y).rev() {
                for x in (start_cell.x..=end_cell.x).rev() {
                    footprint.push(Cell2D::new(x, y));
                }
            }

            // Last cell should be the original starting cell (selection relies on this).
            debug_assert!((*footprint.last().unwrap()) == self.cell);
        } else {
            // Empty tiles always occupy one cell.
            footprint.push(self.cell);
        }

        footprint
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

pub struct TileMapLayer<'a> {
    kind: TileMapLayerKind,
    size_in_cells: Size2D,
    tiles: Vec<Tile<'a>>,
}

impl<'a> TileMapLayer<'a> {
    pub fn new(kind: TileMapLayerKind, size_in_cells: Size2D) -> Self {
        Self {
            kind: kind,
            size_in_cells: size_in_cells,
            tiles: vec![Tile::empty(); (size_in_cells.width * size_in_cells.height) as usize],
        }
    }

    pub fn add_tile(&mut self, cell: Cell2D, tile_def: &'a TileDef) {
        let tile_index = self.cell_to_index(cell);
        self.tiles[tile_index] = Tile::new(cell, Cell2D::invalid(), tile_def);
    }

    pub fn add_empty_tile(&mut self, cell: Cell2D) {
        let tile_index = self.cell_to_index(cell);
        self.tiles[tile_index] = Tile::new(cell, Cell2D::invalid(), TileDef::empty());
    }

    pub fn add_building_blocker_tile(&mut self, cell: Cell2D, owner_cell: Cell2D) {
        let tile_index = self.cell_to_index(cell);
        self.tiles[tile_index] = Tile::new(cell, owner_cell, TileDef::building_blocker());
    }

    pub fn cell_is_valid(&self, cell: Cell2D) -> bool {
         if (cell.x < 0 || cell.x >= self.size_in_cells.width) ||
            (cell.y < 0 || cell.y >= self.size_in_cells.height) {
            return false;
        }
        true
    }

    pub fn tile(&self, cell: Cell2D) -> &Tile {
        let tile_index = self.cell_to_index(cell);
        let tile = &self.tiles[tile_index];
        debug_assert!(tile.cell == cell);
        tile
    }

    pub fn tile_mut(&mut self, cell: Cell2D) -> &mut Tile<'a> {
        let tile_index = self.cell_to_index(cell);
        let tile = &mut self.tiles[tile_index];
        debug_assert!(tile.cell == cell);
        tile
    }

    // Fails with None if the cell indices are not within bounds.
    pub fn try_tile(&self, cell: Cell2D) -> Option<&Tile> {
        if !self.cell_is_valid(cell) {
            return None;
        }
        Some(self.tile(cell))
    }

    pub fn try_tile_mut(&mut self, cell: Cell2D) -> Option<&mut Tile<'a>> {
        if !self.cell_is_valid(cell) {
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

    pub fn cell_is_valid(&self, cell: Cell2D) -> bool {
         if (cell.x < 0 || cell.x >= self.size_in_cells.width) ||
            (cell.y < 0 || cell.y >= self.size_in_cells.height) {
            return false;
        }
        true
    }

    pub fn layers(&self) -> (&TileMapLayer, &TileMapLayer, &TileMapLayer) {
        (
            self.layer(TileMapLayerKind::Terrain),
            self.layer(TileMapLayerKind::Buildings),
            self.layer(TileMapLayerKind::Units),
        )
    }

    pub fn layers_mut(&mut self) -> (&mut TileMapLayer<'a>, &mut TileMapLayer<'a>, &mut TileMapLayer<'a>) {
        // Use raw pointers to avoid borrow checker conflicts
        let terrain   = self.layer_mut(TileMapLayerKind::Terrain)   as *mut TileMapLayer;
        let buildings = self.layer_mut(TileMapLayerKind::Buildings) as *mut TileMapLayer;
        let units     = self.layer_mut(TileMapLayerKind::Units)     as *mut TileMapLayer;

        // SAFETY: Indices are distinct and all references are valid while `self` is borrowed mutably.
        unsafe {
            (&mut *terrain, &mut *buildings, &mut *units)
        }
    }

    pub fn layer(&self, layer_kind: TileMapLayerKind) -> &TileMapLayer {
        debug_assert!(self.layers[layer_kind as usize].kind == layer_kind);
        self.layers[layer_kind as usize].as_ref()
    }

    pub fn layer_mut(&mut self, layer_kind: TileMapLayerKind) -> &mut TileMapLayer<'a> {
        debug_assert!(self.layers[layer_kind as usize].kind == layer_kind);
        self.layers[layer_kind as usize].as_mut()
    }

    pub fn try_tile_from_layer(&self, cell: Cell2D, layer_kind: TileMapLayerKind) -> Option<&Tile> {
        let layer = self.layer(layer_kind);
        debug_assert!(layer.kind == layer_kind);
        layer.try_tile(cell)
    }

    pub fn try_tile_from_layer_mut(&mut self, cell: Cell2D, layer_kind: TileMapLayerKind) -> Option<&mut Tile<'a>> {
        let layer = self.layer_mut(layer_kind);
        debug_assert!(layer.kind == layer_kind);
        layer.try_tile_mut(cell)
    }

    pub fn update_selection(&mut self, selection: &mut TileSelection, transform: &WorldToScreenTransform) {
        if selection.is_range() {
            // Clear previous highlighted tiles:
            self.clear_selection(selection);

            let (cell_min, cell_max) = tile_selection_bounds(
                &selection.rect,
                BASE_TILE_SIZE,
                self.size_in_cells,
                transform);

            let (terrain_layer, buildings_layer, _) = self.layers_mut();

            for y in cell_min.y..=cell_max.y {
                for x in cell_min.x..=cell_max.x {
                    if let Some(base_tile) = terrain_layer.try_tile(Cell2D::new(x, y)) {

                        let tile_iso_coords = base_tile.calc_adjusted_iso_coords();
                        let tile_screen_rect = iso_to_screen_rect(
                            tile_iso_coords, base_tile.def.logical_size, transform);

                        if tile_screen_rect.intersects(&selection.rect) {
                            Self::toggle_selection(selection,
                                                   terrain_layer,
                                                   buildings_layer,
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

            let (terrain_layer, buildings_layer, _) = self.layers_mut();

            if let Some(base_tile) = terrain_layer.try_tile(last_cell) {
                // If the cursor is still inside this cell, we're done.
                // This can happen because the isometric-to-cell conversion
                // is not absolute but rather based on proximity to the cell's center.
                if cursor_inside_tile_cell(cursor_screen_pos, base_tile, transform) {
                    return;
                }

                // Clear:
                Self::toggle_selection(selection,
                                       terrain_layer,
                                       buildings_layer,
                                       base_tile.cell,
                                       false);
            }

            // Update hovered tile to be highlighted:
            let mut cursor_iso_pos = screen_to_iso_point(cursor_screen_pos, transform);

            // Offset the iso point downward by half a tile (visually centers the hit test to the tile center).
            cursor_iso_pos.x -= BASE_TILE_SIZE.width  / 2;
            cursor_iso_pos.y -= BASE_TILE_SIZE.height / 2;

            let hovered_cell = iso_to_cell(cursor_iso_pos, BASE_TILE_SIZE);
            if terrain_layer.cell_is_valid(hovered_cell) {
                // Get the 8 possible neighboring tiles + self and test cursor intersection
                // against each so we can know precisely which tile to highlight.
                let neighbors = terrain_layer.tile_neighbors(hovered_cell, true);
                let mut highlight_cell = Cell2D::invalid();

                for neighbor in neighbors {
                    if let Some(tile) = neighbor {
                        if cursor_inside_tile_cell(cursor_screen_pos, tile, transform) {
                            highlight_cell = tile.cell;
                            break;
                        }
                    }
                }

                if highlight_cell.is_valid() {
                    Self::toggle_selection(selection,
                                           terrain_layer,
                                           buildings_layer,
                                           highlight_cell,
                                           true);
                }
            }
        }
    }

    pub fn clear_selection(&mut self, selection: &mut TileSelection) {
        if selection.cells.is_empty() {
            return;
        }

        let (terrain_layer, buildings_layer, _) = self.layers_mut();

        while !selection.cells.is_empty() {
            Self::toggle_selection(selection,
                                   terrain_layer,
                                   buildings_layer,
                                   selection.last_cell(),
                                   false);
        }

        selection.cells.clear();
    }

    fn calc_tile_footprint_cells(tile: &Tile, buildings: &TileMapLayer) -> TileFootprintList {
        // Buildings may take up multiple cells.
        if let Some(building_layer_tile) = buildings.try_tile(tile.cell) {
            // Buildings have an origin tile and zero or more associated blockers
            // if they occupy multiple tiles, so here we might need to back-track
            // to the origin of the building from a blocker tile.
            //
            /* For instance, a 2x2 house tile `H` will have the house at its origin
            cell, and 3 other blocker tiles `B` that back-reference the house tile.
            +---+---+
            | H | B |
            +---+---+
            | B | B |
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
            smallvec![tile.cell]
        }
    }

    fn toggle_tile_selection(selection: &mut TileSelection, tile: &mut Tile, selected: bool) {
        tile.flags.set(TileFlags::Highlighted, selected);
        if selected {
            selection.cells.push(tile.cell);
        } else {
            selection.cells.pop();
        }
    }

    fn toggle_selection(selection: &mut TileSelection,
                        terrain_layer: &mut TileMapLayer<'a>,
                        buildings_layer: &TileMapLayer,
                        base_cell: Cell2D,
                        selected: bool) {
        // Deal with multi-tile buildings:
        let base_tile = terrain_layer.tile(base_cell);
        let footprint = Self::calc_tile_footprint_cells(base_tile, buildings_layer);
        for cell in footprint {
            let tile = terrain_layer.tile_mut(cell);
            Self::toggle_tile_selection(selection, tile, selected);   
        }
    }
}

// ----------------------------------------------
// TileSelection
// ----------------------------------------------

#[derive(Default)]
pub struct TileSelection {
    rect: Rect2D,
    cursor_drag_start: Point2D,
    current_cursor_pos: Point2D,
    left_mouse_button_held: bool,
    cells: SmallVec::<[Cell2D; 1]>,
}

impl TileSelection {
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

    pub fn update(&mut self, cursor_pos: Point2D) {
        self.current_cursor_pos = cursor_pos;
        if self.left_mouse_button_held {
            // Keep updating the selection rect while left mouse button is held.
            self.rect = Rect2D::from_extents(self.cursor_drag_start, cursor_pos);   
        } else {
            self.rect = Rect2D::zero();
        }
    }

    pub fn draw(&self, render_sys: &mut RenderSystem) {
        if self.is_range() {
            render_sys.draw_wireframe_rect_with_thickness(self.rect, Color::blue(), 1.5);
        }
    }

    fn last_cell(&self) -> Cell2D {
        self.cells.last().unwrap_or(&Cell2D::invalid()).clone()
    }

    fn is_range(&self) -> bool {
        self.left_mouse_button_held && self.rect.is_valid()
    }
}

// ----------------------------------------------
// Tile selection helpers
// ----------------------------------------------

fn cursor_inside_tile_cell(cursor_screen_pos: Point2D,
                           tile: &Tile,
                           transform: &WorldToScreenTransform) -> bool {
    debug_assert!(transform.is_valid());

    let screen_points = cell_to_screen_diamond_points(
        tile.cell,
        tile.def.logical_size,
        transform,
        false);

    screen_point_inside_diamond(cursor_screen_pos, &screen_points)
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
    let top_left = screen_to_iso_point(screen_rect.mins, transform);
    let bottom_right = screen_to_iso_point(screen_rect.maxs, transform);

    let top_right = screen_to_iso_point(
        Point2D::new(screen_rect.maxs.x, screen_rect.mins.y),
        transform);
    let bottom_left = screen_to_iso_point(
        Point2D::new(screen_rect.mins.x, screen_rect.maxs.y),
        transform);

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
                    }

                    if unit_tile.is_unit() && flags.contains(TileMapRenderFlags::DrawUnits) {
                        add_to_sort_list(unit_tile);
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

        let mut highlighted_cells = SmallVec::<[[Point2D; 4]; 64]>::new();

        for y in (0..map_cells.height).rev() {
            for x in (0..map_cells.width).rev() {
                let cell = Cell2D::new(x, y);

                let points = cell_to_screen_diamond_points(
                    cell, BASE_TILE_SIZE, &self.world_to_screen, false);

                // Save highlighted grid cells for drawing at the end, so they display correctly.
                let tile = terrain_layer.tile(cell);
                if tile.flags.contains(TileFlags::Highlighted) {
                    highlighted_cells.push(points);
                    continue;
                }

                // Draw diamond:
                render_sys.draw_polyline_with_thickness(&points, self.grid_color, line_thickness, true);
            }

            // Highlighted on top.
            for points in &highlighted_cells {
                render_sys.draw_polyline_with_thickness(&points, Color::red(), line_thickness, true);
            }
        }
    }

    fn draw_tile(&self,
                 render_sys: &mut RenderSystem,
                 ui_sys: &UiSystem,
                 tile_iso_coords: IsoPoint2D,
                 tile: &Tile,
                 flags: TileMapRenderFlags) {

        debug_assert!(tile.def.is_valid());

        // Only terrain and buildings might require spacing.
        let apply_spacing = if !tile.is_unit() { true } else { false };

        let tile_rect = self.world_to_screen.apply_to_rect(
            tile_iso_coords,
            tile.def.draw_size,
            apply_spacing);

        render_sys.draw_textured_colored_rect(
            tile_rect,
            &tile.def.tex_info.coords,
            tile.def.tex_info.texture,
            tile.def.color);

        debug::draw_tile_debug(
            render_sys,
            ui_sys,
            tile_iso_coords,
            tile_rect,
            tile,
            flags);
    }
}
