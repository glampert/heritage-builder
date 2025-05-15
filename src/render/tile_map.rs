use bitflags::bitflags;
use arrayvec::ArrayVec;
use crate::utils::*;
use crate::ui::system::UiSystem;
use super::system::RenderSystem;
use super::tile_def::{TileDef, TileKind, BASE_TILE_SIZE};
use super::tile_sets::TileSets;

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
    pub def: &'a TileDef,
    pub flags: TileFlags,
    //z_sort: i32, // TODO: Support custom z_sort?
}

impl<'a> Tile<'a> {
    pub fn empty() -> Self {
        Self {
            cell: Cell2D::zero(),
            def: TileDef::empty(),
            flags: TileFlags::None,
        }
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

    pub fn is_unit(&self) -> bool {
        self.def.is_unit()
    }

    pub fn get_z_sort(&self) -> i32 {
        cell_to_iso(self.cell, BASE_TILE_SIZE).y - self.def.logical_size.height
    }

    pub fn get_adjusted_iso_coords(&self) -> IsoPoint2D {
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
    pub fn get_tile_center(&self, tile_screen_pos: Point2D) -> Point2D {
        let tile_center = Point2D::new(
            tile_screen_pos.x + self.def.draw_size.width,
            tile_screen_pos.y + self.def.draw_size.height
        );
        tile_center
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
// TileMapLayer / TileMapLayerKind
// ----------------------------------------------

const TILE_MAP_LAYER_COUNT: usize = 2;

#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum TileMapLayerKind {
    Terrain,
    BuildingsAndUnits
}

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
        let tile_index = cell.x + (cell.y * self.size_in_cells.width);
        self.tiles[tile_index as usize] = Tile {
            cell: cell,
            def: tile_def,
            flags: TileFlags::None,
        };
    }

    pub fn cell_is_valid(&self, cell: Cell2D) -> bool {
         if (cell.x < 0 || cell.x >= self.size_in_cells.width) ||
            (cell.y < 0 || cell.y >= self.size_in_cells.height) {
            return false;
        }
        true
    }

    pub fn tile(&self, cell: Cell2D) -> &Tile {
        let tile_index = cell.x + (cell.y * self.size_in_cells.width);
        let tile = &self.tiles[tile_index as usize];
        debug_assert!(tile.cell == cell);
        tile
    }

    pub fn tile_mut(&mut self, cell: Cell2D) -> &mut Tile<'a> {
        let tile_index = cell.x + (cell.y * self.size_in_cells.width);
        let tile = &mut self.tiles[tile_index as usize];
        debug_assert!(tile.cell == cell);
        tile
    }

    // Fails with None if the cell indices are not within bounds.
    pub fn try_tile(&self, cell: Cell2D) -> Option<&Tile> {
        if (cell.x < 0 || cell.x >= self.size_in_cells.width) ||
           (cell.y < 0 || cell.y >= self.size_in_cells.height) {
            return None;
        }
        Some(self.tile(cell))
    }

    pub fn try_tile_mut(&mut self, cell: Cell2D) -> Option<&mut Tile<'a>> {
        if (cell.x < 0 || cell.x >= self.size_in_cells.width) ||
           (cell.y < 0 || cell.y >= self.size_in_cells.height) {
            return None;
        }
        Some(self.tile_mut(cell))
    }

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
}

// ----------------------------------------------
// TileMap
// ----------------------------------------------

pub struct TileMap<'a> {
    size_in_cells: Size2D,
    layers: Vec<TileMapLayer<'a>>,
    current_highlighted_tile_cell: Cell2D,
    current_range_selection: Vec<Cell2D>,
}

impl<'a> TileMap<'a> {
    pub fn new(size_in_cells: Size2D) -> Self {
        Self {
            size_in_cells: size_in_cells,
            layers: Vec::with_capacity(TILE_MAP_LAYER_COUNT),
            current_highlighted_tile_cell: Cell2D::invalid(),
            current_range_selection: Vec::new(),
        }
    }

    pub fn cell_is_valid(&self, cell: Cell2D) -> bool {
         if (cell.x < 0 || cell.x >= self.size_in_cells.width) ||
            (cell.y < 0 || cell.y >= self.size_in_cells.height) {
            return false;
        }
        true
    }

    pub fn layer(&self, kind: TileMapLayerKind) -> &TileMapLayer<'a> {
        &self.layers[kind as usize]
    }

    pub fn layer_mut(&mut self, kind: TileMapLayerKind) -> &mut TileMapLayer<'a> {
        &mut self.layers[kind as usize]
    }

    pub fn update_selection(&mut self,
                            cursor_screen_pos: Point2D,
                            transform: &WorldToScreenTransform) {
        // Clear previous highlighted tile:
        {
            let cell = self.current_highlighted_tile_cell;
            let layer = self.layer_mut(TileMapLayerKind::Terrain);

            if let Some(tile) = layer.try_tile_mut(cell) {
                // If the tile is still inside this cell, we're done.
                // This can happen because the isometric-to-cell conversion
                // is not absolute but rather based on proximity to the cell's center.
                if cursor_inside_tile_cell(cursor_screen_pos, tile, transform) {
                    return;
                }

                tile.flags.set(TileFlags::Highlighted, false); 
            }
        }

        // Update hovered tile to be highlighted:
        {
            let mut cursor_iso_pos = screen_to_iso_point(cursor_screen_pos, transform);

            // Offset the iso point downward by half a tile (visually centers the hit test to the tile center).
            cursor_iso_pos.x -= BASE_TILE_SIZE.width  / 2;
            cursor_iso_pos.y -= BASE_TILE_SIZE.height / 2;

            let cell = iso_to_cell(cursor_iso_pos, BASE_TILE_SIZE);
            let layer = self.layer_mut(TileMapLayerKind::Terrain);

            if layer.cell_is_valid(cell) {
                let mut highlight_cell = Cell2D::invalid();
                let mut highlight_tile: Option<&mut Tile> = None;

                // Get the 8 possible neighboring tiles + self and test cursor intersection
                // against each so we can know precisely which tile to highlight.
                let neighbors = layer.tile_neighbors_mut(cell, true);
                for neighbor in neighbors {
                    if let Some(tile) = neighbor {
                        if cursor_inside_tile_cell(cursor_screen_pos, tile, transform) {
                            highlight_cell = tile.cell;
                            highlight_tile = Some(tile);
                            break;
                        }
                    }
                }

                if let Some(tile) = highlight_tile {
                    tile.flags.set(TileFlags::Highlighted, true);
                }
                self.current_highlighted_tile_cell = highlight_cell;
            } else {
                self.current_highlighted_tile_cell = Cell2D::invalid();
            }
        }
    }

    pub fn update_range_selection(&mut self,
                                  selection_screen_rect: &Rect2D,
                                  transform: &WorldToScreenTransform) {

        debug_assert!(selection_screen_rect.is_valid());

        let (cell_min, cell_max) = tile_selection_bounds(
            selection_screen_rect,
            BASE_TILE_SIZE,
            self.size_in_cells,
            transform);

        let mut range_selection = Vec::new();
        {
            let layer = self.layer_mut(TileMapLayerKind::Terrain);

            for y in cell_min.y..=cell_max.y {
                for x in cell_min.x..=cell_max.x {
                    let cell = Cell2D::new(x, y);
                    if let Some(tile) = layer.try_tile_mut(cell) {

                        let tile_iso_coords = tile.get_adjusted_iso_coords();

                        let tile_screen_rect = iso_to_screen_rect(
                            tile_iso_coords, tile.def.logical_size, transform);

                        if tile_screen_rect.intersects(&selection_screen_rect) {
                            tile.flags.set(TileFlags::Highlighted, true);
                            range_selection.push(cell);
                        }
                    }
                }
            }
        }
        self.current_range_selection.append(&mut range_selection);
    }

    pub fn clear_range_selection(&mut self) {
        if self.current_range_selection.is_empty() {
            return;
        }

        let mut range_selection = Vec::new();
        range_selection.append(&mut self.current_range_selection);

        let layer = self.layer_mut(TileMapLayerKind::Terrain);

        for cell in range_selection {
            let tile = layer.tile_mut(cell);
            tile.flags.set(TileFlags::Highlighted, false);
        }

        debug_assert!(self.current_range_selection.is_empty());
    }

    pub fn new_test_map(tile_sets: &'a mut TileSets) -> Self {
        println!("Creating test map...");

        const MAP_WIDTH:  i32 = 8;
        const MAP_HEIGHT: i32 = 8;

        const G: i32 = 0; // ground:grass (empty)
        const R: i32 = 1; // ground:road
        const H: i32 = 2; // building:house (2x2)
        const T: i32 = 3; // building:tower (3x3)
        const U: i32 = 4; // unit:ped
    
        const TILE_NAMES: [&str; 5] = [ "grass", "road", "house", "tower", "ped" ];

        const TERRAIN_LAYER_MAP: [i32; (MAP_WIDTH * MAP_HEIGHT) as usize] = [
            R,R,R,R,R,R,R,R, // <-- start, tile zero is the leftmost (top-left)
            R,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,R,
            R,R,R,R,R,R,R,R,
        ];
    
        const BUILDINGS_AND_UNITS_LAYER_MAP: [i32; (MAP_WIDTH * MAP_HEIGHT) as usize] = [
            U,U,U,U,U,U,U,U, // <-- start, tile zero is the leftmost (top-left)
            U,T,G,G,U,H,G,U,
            U,G,G,G,U,G,G,U,
            U,G,G,G,U,H,G,U,
            U,U,U,U,U,G,G,U,
            U,H,G,U,U,H,G,U,
            U,G,G,U,U,G,G,U,
            U,U,U,U,U,U,U,U,
        ];

        let mut tile_map = TileMap::new(
            Size2D{ width: MAP_WIDTH, height: MAP_HEIGHT });

        // Terrain:
        {
            let mut terrain_layer =
                TileMapLayer::new(TileMapLayerKind::Terrain, tile_map.size_in_cells);

            for x in 0..MAP_WIDTH {
                for y in 0..MAP_HEIGHT {
                    let tile_id = TERRAIN_LAYER_MAP[(x + (y * MAP_WIDTH)) as usize];
                    let tile_def = tile_sets.find_by_name(TILE_NAMES[tile_id as usize]);
                    terrain_layer.add_tile(Cell2D::new(x, y), tile_def);
                }
            }

            tile_map.layers.push(terrain_layer);
        }

        // Buildings & Units:
        {
            let mut buildings_and_units_layer =
                TileMapLayer::new(TileMapLayerKind::BuildingsAndUnits, tile_map.size_in_cells);

            for x in 0..MAP_WIDTH {
                for y in 0..MAP_HEIGHT {
                    let tile_id = BUILDINGS_AND_UNITS_LAYER_MAP[(x + (y * MAP_WIDTH)) as usize];

                    let tile_def = if tile_id == G {
                        TileDef::empty()
                    } else {
                        tile_sets.find_by_name(TILE_NAMES[tile_id as usize])
                    };

                    buildings_and_units_layer.add_tile(Cell2D::new(x, y), tile_def);
                }
            }

            tile_map.layers.push(buildings_and_units_layer);
        }

        tile_map
    }
}

// ----------------------------------------------
// TileMapRenderer / TileMapRenderFlags
// ----------------------------------------------

bitflags! {
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
    // correctly. Can be overwritten by the Tile data. 
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
            let layer = tile_map.layer(TileMapLayerKind::Terrain);

            debug_assert!(layer.size_in_cells == map_cells);

            for y in (0..map_cells.height).rev() {
                for x in (0..map_cells.width).rev() {

                    let cell = Cell2D::new(x, y);
                    let tile = layer.tile(cell);

                    if tile.is_empty() {
                        continue;
                    }

                    debug_assert!(tile.is_terrain());
                    debug_assert!(tile.def.logical_size == BASE_TILE_SIZE); // Terrain tiles size is constrained.

                    let tile_iso_coords = tile.get_adjusted_iso_coords();

                    self.draw_tile(render_sys, ui_sys, tile_iso_coords, tile,
                        flags.contains(TileMapRenderFlags::DrawTerrainTileDebugInfo),
                        flags.contains(TileMapRenderFlags::DrawTileDebugBounds));
                }
            }
        }

        if flags.contains(TileMapRenderFlags::DrawGrid) {
            // Draw the grid now so that lines will be on top of the terrain but not on top of buildings.
            self.draw_isometric_grid(render_sys, tile_map);
        }

        // Buildings & Units:
        if flags.intersects(TileMapRenderFlags::DrawBuildings | TileMapRenderFlags::DrawUnits) {
            let layer = tile_map.layer(TileMapLayerKind::BuildingsAndUnits);

            debug_assert!(layer.size_in_cells == map_cells);

            for y in (0..map_cells.height).rev() {
                for x in (0..map_cells.width).rev() {

                    let cell = Cell2D::new(x, y);
                    let tile = layer.tile(cell);

                    if tile.is_empty() {
                        continue;
                    }
                    if tile.is_building() && !flags.contains(TileMapRenderFlags::DrawBuildings) {
                        continue;
                    }
                    if tile.is_unit() && !flags.contains(TileMapRenderFlags::DrawUnits) {
                        continue;
                    }

                    debug_assert!(tile.is_building() || tile.is_unit());

                    self.temp_tile_sort_list.push(TileDrawListEntry {
                        tile_ptr: tile as *const Tile<'_> as *const Tile<'static>,
                        z_sort: tile.get_z_sort(),
                    });
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

                let tile_iso_coords = tile.get_adjusted_iso_coords();

                let draw_debug_info =
                    flags.contains(TileMapRenderFlags::DrawBuildingsTileDebugInfo) ||
                    flags.contains(TileMapRenderFlags::DrawUnitsTileDebugInfo);
                
                let draw_debug_bounds =
                    flags.contains(TileMapRenderFlags::DrawTileDebugBounds);

                self.draw_tile(render_sys, ui_sys, tile_iso_coords, tile, draw_debug_info, draw_debug_bounds);
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

        let line_thickness = self.grid_line_thickness * (self.world_to_screen.scaling as f32);

        let mut highlighted_cells = Vec::new();

        for y in (0..map_cells.height).rev() {
            for x in (0..map_cells.width).rev() {
                let cell = Cell2D::new(x, y);

                let points = cell_to_screen_diamond_points(
                    cell, BASE_TILE_SIZE, &self.world_to_screen, false);

                // Save highlighted grid cells for drawing at the end, so they display correctly.
                let tile = tile_map.layer(TileMapLayerKind::Terrain).tile(cell);
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

    fn draw_tile_debug_info_overlay(ui_sys: &UiSystem,
                                    debug_overlay_pos: Point2D,
                                    tile_screen_pos: Point2D,
                                    tile_iso_pos: IsoPoint2D,
                                    tile: &Tile) {

        // Make the window background transparent and remove decorations:
        let window_flags =
            imgui::WindowFlags::NO_DECORATION |
            imgui::WindowFlags::NO_MOVE |
            imgui::WindowFlags::NO_SAVED_SETTINGS |
            imgui::WindowFlags::NO_FOCUS_ON_APPEARING |
            imgui::WindowFlags::NO_NAV |
            imgui::WindowFlags::NO_MOUSE_INPUTS;

        // NOTE: Label has to be unique for each tile because it will be used as the ImGui ID for this widget.
        let label = format!("{}_{}_{}", tile.def.name, tile.cell.x, tile.cell.y);
        let position = [ debug_overlay_pos.x as f32, debug_overlay_pos.y as f32 ];

        let bg_color = match tile.def.kind {
            TileKind::Building => Color::yellow().to_array(),
            TileKind::Unit => Color::cyan().to_array(),
            _ => Color::black().to_array()
        };

        let text_color = match tile.def.kind {
            TileKind::Terrain => Color::white().to_array(),
            _ => Color::black().to_array()
        };

        let ui = ui_sys.builder();

        // Adjust window background color based on tile kind.
        // The returned tokens take care of popping back to the previous color/font.
        let _0 = ui.push_style_color(imgui::StyleColor::WindowBg, bg_color);
        let _1 = ui.push_style_color(imgui::StyleColor::Text, text_color);
        let _2  = ui.push_font(ui_sys.fonts().small);

        ui.window(label)
            .position(position, imgui::Condition::Always)
            .flags(window_flags)
            .always_auto_resize(true)
            .bg_alpha(0.4) // Semi-transparent
            .build(|| {
                ui.text(format!("C:{},{}", tile.cell.x,       tile.cell.y));       // Cell position
                ui.text(format!("S:{},{}", tile_screen_pos.x, tile_screen_pos.y)); // 2D screen position
                ui.text(format!("I:{},{}", tile_iso_pos.x,    tile_iso_pos.y));    // 2D isometric position
            });
    }

    fn draw_tile_debug_info(render_sys: &mut RenderSystem,
                            ui_sys: &UiSystem,
                            tile_screen_pos: Point2D,
                            tile_iso_pos: IsoPoint2D,
                            tile: &Tile) {

        let debug_overlay_offsets = match tile.def.kind {
            TileKind::Terrain  => (tile.def.logical_size.width / 2, tile.def.logical_size.height / 2),
            TileKind::Building => (tile.def.logical_size.width, tile.def.logical_size.height),
            TileKind::Unit     => (tile.def.draw_size.width, tile.def.draw_size.height),
            _ => (0, 0)
        };

        let debug_overlay_pos = Point2D::new(
            tile_screen_pos.x + debug_overlay_offsets.0,
            tile_screen_pos.y + debug_overlay_offsets.1);

        Self::draw_tile_debug_info_overlay(
            ui_sys,
            debug_overlay_pos,
            tile_screen_pos,
            tile_iso_pos,
            tile);

        // Put a red dot at the tile's center.
        render_sys.draw_point_fast(
            tile.get_tile_center(tile_screen_pos),
            Color::red(),
            10.0);
    }

    fn draw_tile(&self,
                 render_sys: &mut RenderSystem,
                 ui_sys: &UiSystem,
                 tile_iso_coords: IsoPoint2D,
                 tile: &Tile,
                 draw_debug_info: bool,
                 draw_debug_bounds: bool) {

        debug_assert!(tile.def.is_valid());

        let tile_rect = iso_to_screen_rect(
            tile_iso_coords,
            tile.def.draw_size,
            &self.world_to_screen);

        render_sys.draw_textured_colored_rect(
            tile_rect,
            &tile.def.tex_info.coords,
            tile.def.tex_info.texture,
            tile.def.color);

        if draw_debug_info {
            Self::draw_tile_debug_info(
                render_sys,
                ui_sys,
                tile_rect.position(),
                tile_iso_coords,
                tile);
        }

        if draw_debug_bounds {
            let color = match tile.def.kind {
                TileKind::Building => Color::yellow(),
                TileKind::Unit => Color::cyan(),
                _ => Color::red()
            };

            // Tile bounding box (of the actual sprite image): 
            render_sys.draw_wireframe_rect_fast(tile_rect, color);
        }
    }
}
