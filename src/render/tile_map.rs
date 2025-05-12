use bitflags::bitflags;
use crate::utils::{self, Size2D, Cell2D, Point2D, IsoPoint2D, Rect2D, Color};
use super::opengl::backend::RenderBackend;
use super::tile_def::{TileDef, BASE_TILE_SIZE};
use super::tile_sets::TileSets;

// ----------------------------------------------
// Tile
// ----------------------------------------------

#[derive(Clone)]
pub struct Tile<'a> {
    pub cell: Cell2D,
    pub def: &'a TileDef,
    //z_sort: i32, // TODO: Support custom z_sort.
}

impl<'a> Tile<'a> {
    pub fn empty() -> Self {
        Self {
            cell: Cell2D::zero(),
            def: TileDef::empty(),
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
        };
    }

    pub fn tile(&self, cell: Cell2D) -> &Tile {
        let tile_index = cell.x + (cell.y * self.size_in_cells.width);
        &self.tiles[tile_index as usize]
    }
}

// ----------------------------------------------
// TileMap
// ----------------------------------------------

pub struct TileMap<'a> {
    size_in_cells: Size2D,
    layers: Vec<TileMapLayer<'a>>,
}

impl<'a> TileMap<'a> {
    pub fn new(size_in_cells: Size2D) -> Self {
        Self {
            size_in_cells: size_in_cells,
            layers: Vec::with_capacity(TILE_MAP_LAYER_COUNT),
        }
    }

    pub fn layer(&self, kind: TileMapLayerKind) -> &TileMapLayer<'a> {
        &self.layers[kind as usize]
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
            R,R,R,R,R,R,R,R, // <-- start, tile zero is the leftmost
            R,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,R,
            R,G,G,G,G,G,G,R,
            R,R,R,R,R,R,R,R,
        ];
    
        const BUILDINGS_AND_UNITS_LAYER_MAP: [i32; (MAP_WIDTH * MAP_HEIGHT) as usize] = [
            U,U,U,U,U,U,U,U, // <-- start, tile zero is the leftmost
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
                    terrain_layer.add_tile(Cell2D { x: x, y: y }, tile_def);
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

                    buildings_and_units_layer.add_tile(Cell2D { x: x, y: y }, tile_def);
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
        const DrawGridIgnoreDepth = 1 << 5; // Grid draws on top of everything.
    }
}

struct TileDrawListEntry<'a> {
    tile: &'a Tile<'a>,

    // Y value of the bottom left corner of the tile sprite for sorting.
    // Simulates a pseudo depth value so we can render units and buildings
    // correctly. Can be overwritten by the Tile data. 
    z_sort: i32
}

pub struct TileMapRenderer<'a> {
    scaling: i32,
    offset: Point2D,
    tile_spacing: i32,
    grid_color: Color,
    grid_line_thickness: f32,
    tile_draw_list: Vec<TileDrawListEntry<'a>>,
}

impl<'a> TileMapRenderer<'a> {
    pub fn new() -> Self {
        Self {
            scaling: 1,
            offset: Point2D::zero(),
            tile_spacing: 0,
            grid_color: Color::white(),
            grid_line_thickness: 1.0,
            tile_draw_list: Vec::with_capacity(512),
        }
    }

    pub fn set_draw_scaling(&mut self, scaling: i32) -> &mut Self {
        debug_assert!(scaling >= 1);
        self.scaling = scaling;
        self
    }

    pub fn set_draw_offset(&mut self, offset: Point2D) -> &mut Self {
        self.offset = offset;
        self
    }

    pub fn set_tile_spacing(&mut self, spacing: i32) -> &mut Self {
        debug_assert!(spacing >= 0);
        self.tile_spacing = spacing;
        self
    }

    pub fn set_grid_color(&mut self, color: Color) -> &mut Self {
        self.grid_color = color;
        self
    }

    pub fn set_grid_line_thickness(&mut self, thickness: f32) -> &mut Self {
        debug_assert!(thickness >= 1.0);
        self.grid_line_thickness = thickness;
        self
    }

    pub fn draw_map(&mut self, render_backend: &mut RenderBackend, tile_map: &'a TileMap, flags: TileMapRenderFlags) {
        let map_cells_width  = tile_map.size_in_cells.width;
        let map_cells_height = tile_map.size_in_cells.height;

        // Terrain:
        if flags.contains(TileMapRenderFlags::DrawTerrain) {
            let layer = tile_map.layer(TileMapLayerKind::Terrain);

            debug_assert!(layer.size_in_cells.width  == map_cells_width);
            debug_assert!(layer.size_in_cells.height == map_cells_height);

            for y in (0..map_cells_height).rev() {
                for x in (0..map_cells_width).rev() {

                    let cell = Cell2D::with_coords(x, y);
                    let tile = layer.tile(cell);

                    if tile.is_empty() {
                        continue;
                    }

                    debug_assert!(tile.is_terrain());
                    debug_assert!(tile.cell.x == cell.x && tile.cell.y == cell.y);

                    self.draw_terrain(render_backend, tile);
                }
            }
        }

        if flags.contains(TileMapRenderFlags::DrawGrid) {
            // Draw the grid now so that lines will be on top of terrain but not on top of buildings.
            self.draw_isometric_grid(render_backend, tile_map);
        }

        // Buildings & Units:
        if flags.intersects(TileMapRenderFlags::DrawBuildings | TileMapRenderFlags::DrawUnits) {
            let layer = tile_map.layer(TileMapLayerKind::BuildingsAndUnits);

            debug_assert!(layer.size_in_cells.width  == map_cells_width);
            debug_assert!(layer.size_in_cells.height == map_cells_height);

            for y in (0..map_cells_height).rev() {
                for x in (0..map_cells_width).rev() {

                    let cell = Cell2D::with_coords(x, y);
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
                    debug_assert!(tile.cell.x == cell.x && tile.cell.y == cell.y);

                    let z_sort = utils::cell_to_isometric(cell, BASE_TILE_SIZE).y +
                                                               tile.def.logical_size.height;

                    self.tile_draw_list.push(TileDrawListEntry {
                        tile: tile,
                        z_sort: z_sort, // TODO: z_sort override from tile.
                    });
                }
            }

            self.tile_draw_list.sort_by(|a, b| {
                b.z_sort.cmp(&a.z_sort)
            });

            for entry in &self.tile_draw_list {
                if entry.tile.is_building() {
                    self.draw_building(render_backend, entry.tile);
                } else if entry.tile.is_unit() {
                    self.draw_unit(render_backend, entry.tile);
                } else {
                    panic!("Only buildings and units should be on this list!");
                }
            }

            self.tile_draw_list.clear();
        }

        if flags.contains(TileMapRenderFlags::DrawGridIgnoreDepth) {
            // Allow lines to draw later and effectively bypass the draw order
            // and appear on top of everything else (useful for debugging).
            self.draw_isometric_grid(render_backend, tile_map);
        }
    }

    fn draw_isometric_grid(&self, render_backend: &mut RenderBackend, tile_map: &'a TileMap) {
        const CELL_WIDTH:  i32 = BASE_TILE_SIZE.width;
        const CELL_HEIGHT: i32 = BASE_TILE_SIZE.height;

        let map_cells_width  = tile_map.size_in_cells.width;
        let map_cells_height = tile_map.size_in_cells.height;
    
        let grid_color   = self.grid_color;
        let line_thickness = self.grid_line_thickness * (self.scaling as f32);

        for y in 0..map_cells_height {
            for x in 0..map_cells_width {
                let mut center = utils::cell_to_isometric(Cell2D::with_coords(x, y), BASE_TILE_SIZE);

                // Apply scale and offset:
                center.x = (center.x * self.scaling) + self.offset.x;
                center.y = (center.y * self.scaling) + self.offset.y;

                // We're off by one cell, so adjust:
                center.x += CELL_WIDTH;
                center.y += CELL_HEIGHT;

                // 4 corners of the tile:
                let top    = Point2D::with_coords(center.x,              center.y - CELL_HEIGHT);
                let right  = Point2D::with_coords(center.x + CELL_WIDTH, center.y);
                let bottom = Point2D::with_coords(center.x,              center.y + CELL_HEIGHT);
                let left   = Point2D::with_coords(center.x - CELL_WIDTH, center.y);
    
                // Draw diamond:
                let points = [ top, right, bottom, left ];
                render_backend.draw_polyline_with_thickness(&points, grid_color, line_thickness, true);
            }
        }
    }

    fn draw_tile(&self, render_backend: &mut RenderBackend, tile_iso_coords: IsoPoint2D, tile_def: &TileDef) {
        debug_assert!(tile_def.is_valid());

        // Inter-tile spacing.
        let spacing = self.tile_spacing;
        let half_spacing = if spacing > 0 { spacing / 2 } else { 0 };

        let tile_rect = Rect2D::with_xy_and_size(
            ((tile_iso_coords.x + half_spacing) * self.scaling) + self.offset.x,
            ((tile_iso_coords.y + half_spacing) * self.scaling) + self.offset.y,
            Size2D {
                width:  (tile_def.draw_size.width  - spacing) * self.scaling,
                height: (tile_def.draw_size.height - spacing) * self.scaling
            });

        render_backend.draw_textured_colored_rect(
            tile_rect,
            &tile_def.tex_info.coords,
            tile_def.tex_info.texture,
            tile_def.color);
    }

    fn draw_terrain(&self, render_backend: &mut RenderBackend, tile: &Tile) {
        // Terrain tiles size is constrained.
        debug_assert!(tile.def.logical_size.width  == BASE_TILE_SIZE.width);
        debug_assert!(tile.def.logical_size.height == BASE_TILE_SIZE.height);

        let tile_iso_coords = utils::cell_to_isometric(tile.cell, BASE_TILE_SIZE);
        self.draw_tile(render_backend, tile_iso_coords, tile.def);
    }

    fn draw_building(&self, render_backend: &mut RenderBackend, tile: &Tile) {
        // Convert the base tile into isometric screen coordinates:
        let mut tile_iso_coords = utils::cell_to_isometric(tile.cell, BASE_TILE_SIZE);

        // Adjust to center the building image:
        tile_iso_coords.x += (BASE_TILE_SIZE.width / 2) - (tile.def.logical_size.width / 2);

        self.draw_tile(render_backend, tile_iso_coords, tile.def);
    }

    fn draw_unit(&self, render_backend: &mut RenderBackend, tile: &Tile) {
        // Convert the base tile into isometric screen coordinates:
        let mut tile_iso_coords = utils::cell_to_isometric(tile.cell, BASE_TILE_SIZE);

        // Adjust to center the unit sprite:
        tile_iso_coords.x += (BASE_TILE_SIZE.width / 2) - (tile.def.draw_size.width / 2);
        tile_iso_coords.y += tile.def.draw_size.height / 2;

        self.draw_tile(render_backend, tile_iso_coords, tile.def);
    }
}
