use std::any::Any;

use crate::{
    log,
    save::{Save, Load},
    engine::time::Seconds,
    imgui_ui::{UiSystem, UiInputEvent},
    pathfind::{NodeKind as PathNodeKind},
    utils::{Vec2, coords::{Cell, CellRange}},
    app::input::{InputAction, InputKey, InputModifiers, MouseButton},
    game::{
        world::{object::{Spawner, SpawnerResult}, World},
        sim::{Query, Simulation}, system::GameSystems,
    },
    tile::{
        Tile, TileKind, TileMap, TileMapLayerKind, selection::TileSelection,
        sets::{TileSets, TileDef, TileDefHandle, TERRAIN_GROUND_CATEGORY, PresetTiles},
        rendering::TileMapRenderFlags, PlacementOp, camera::Camera,
        water, road::{self, RoadSegment, RoadKind},
    },
};

pub mod hud;

// ----------------------------------------------
// Helper structs
// ----------------------------------------------

pub enum GameMenusInputArgs {
    Key {
        key: InputKey,
        action: InputAction,
        modifiers: InputModifiers,
    },
    Mouse {
        button: MouseButton,
        action: InputAction,
        modifiers: InputModifiers,
    },
    Scroll {
        amount: Vec2,
    },
}

pub struct GameMenusContext<'game> {
    // UI System:
    pub ui_sys: &'game UiSystem,

    // Tile Map:
    pub tile_map: &'game mut TileMap,
    pub tile_selection: &'game mut TileSelection,

    // Sim/World/Game:
    pub sim: &'game mut Simulation,
    pub world: &'game mut World,
    pub systems: &'game mut GameSystems,

    // Camera/Input:
    pub camera: &'game mut Camera,
    pub cursor_screen_pos: Vec2,
    pub delta_time_secs: Seconds,
}

impl GameMenusContext<'_> {
    fn new_query(&mut self) -> Query {
        self.sim.new_query(self.world, self.tile_map, self.delta_time_secs)
    }

    fn can_afford_cost(&self, cost: u32) -> bool {
        self.sim.treasury().can_afford(self.world, cost)
    }

    fn topmost_selected_tile(&self) -> Option<&Tile> {
        self.tile_map.topmost_selected_tile(self.tile_selection)
    }

    fn selection_handle_mouse_button(&mut self, button: MouseButton, action: InputAction) -> bool {
        if button != MouseButton::Left {
            return false;
        }

        self.tile_selection.on_mouse_button(button,
                                            action,
                                            self.tile_map,
                                            self.cursor_screen_pos,
                                            self.camera.transform())
                                            .is_handled()
    }

    fn range_selection_cells(&self) -> Option<(Cell, Cell)> {
        self.tile_selection.range_selection_cells(self.tile_map,
                                                  self.cursor_screen_pos,
                                                  self.camera.transform())
    }

    fn update_selection(&mut self, placement_op: PlacementOp) {
        self.tile_map.update_selection(self.tile_selection,
                                       self.cursor_screen_pos,
                                       self.camera.transform(),
                                       placement_op);
    }

    fn clear_selection(&mut self) {
        self.tile_map.clear_selection(self.tile_selection);
    }
}

// ----------------------------------------------
// GameMenusSystem
// ----------------------------------------------

pub trait GameMenusSystem: Save + Load {
    fn as_any(&self) -> &dyn Any;

    fn tile_placement(&mut self) -> &mut TilePlacement;
    fn tile_palette(&mut self) -> &mut dyn TilePalette;
    fn tile_inspector(&mut self) -> Option<&mut dyn TileInspector>;

    fn selected_render_flags(&self) -> TileMapRenderFlags {
        TileMapRenderFlags::DrawTerrainAndObjects
    }

    fn begin_frame(&mut self, context: &mut GameMenusContext) {
        // Bail if we're hovering over an ImGui menu...
        if context.ui_sys.is_handling_mouse_input() {
            return;
        }

        // Tile hovering and selection:
        let selection = self.tile_palette().current_selection();
        let placement_op = self.tile_placement().placement_operation(selection, context);

        context.update_selection(placement_op);

        // Incrementally build road segment (drag and draw segment):
        let is_road_tile_selected = self.tile_palette().is_road_tile_selected();
        if is_road_tile_selected {
            if let Some((start, end)) = context.range_selection_cells() {
                let road_kind = self.tile_palette().selected_road_kind();
                self.tile_placement().update_road_segment(road_kind, start, end, context);
            }
        }

        // Place a regular (non-road) tile or clear a tile:
        if !is_road_tile_selected && self.tile_palette().wants_to_place_or_clear_tile() {
            let placed_building_or_unit = {
                match TilePlacement::try_place_or_clear_tile(selection, context) {
                    PlaceOrClearResult::PlacedTile(tile_def) => {
                        tile_def.is(TileKind::Building | TileKind::Unit)
                    }
                    _ => false
                }
            };

            // Exit tile placement mode if we've placed a building|unit.
            if placed_building_or_unit {
                self.tile_palette().clear_selection();
                context.clear_selection();
            }
        }
    }

    fn end_frame(&mut self, _context: &mut GameMenusContext, _visible_range: CellRange) {
        // Nothing here. Should implement the menu rendering logic.
    }

    fn handle_input(&mut self, context: &mut GameMenusContext, args: GameMenusInputArgs) -> UiInputEvent {
        match args {
            GameMenusInputArgs::Key { key, action, .. } => {
                // [ESCAPE]: Clear current selection / close tile inspector.
                if key == InputKey::Escape && action == InputAction::Press {
                    self.tile_palette().clear_selection();
                    context.clear_selection();
                    if let Some(tile_inspector) = self.tile_inspector() {
                        tile_inspector.close();
                    }
                    return UiInputEvent::Handled;
                }
            }
            GameMenusInputArgs::Mouse { button, action, .. } => {
                let is_road_tile_selected = self.tile_palette().is_road_tile_selected();
                let is_clear_selected = self.tile_palette().current_selection().is_clear();

                if !is_road_tile_selected && !is_clear_selected && self.tile_palette().has_selection() {
                    let input_event = self.tile_palette().on_mouse_button(button, action);
                    if input_event.not_handled() {
                        // Mouse button click other than [LEFT_BTN], clear selection state.
                        self.tile_palette().clear_selection();
                        context.clear_selection();
                    }
                    return input_event;
                }

                if context.selection_handle_mouse_button(button, action) {
                    // Handle road placement (drag and draw segment).
                    if is_road_tile_selected {
                        // Place road segment if valid & we can afford it.
                        self.tile_placement().try_place_road_segment(context);
                    } else if is_clear_selected && !context.tile_selection.cells().is_empty() {
                        // Clear batch of selected tiles:
                        let query = context.new_query();
                        let tile_map = query.tile_map();

                        for &cell in context.tile_selection.cells() {
                            if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects) {
                                TilePlacement::clear(&query, tile);
                            }
                            if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain) {
                                if tile.path_kind().intersects(PathNodeKind::Road | PathNodeKind::VacantLot) {
                                    TilePlacement::clear(&query, tile);
                                }
                            }
                        }

                        context.clear_selection();
                    }
                } else {
                    // Mouse button click other than [LEFT_BTN], clear selection state.
                    self.tile_palette().clear_selection();
                    context.clear_selection();
                }

                // Open inspector only if we're not in road placement or clear mode.
                if !is_road_tile_selected && !is_clear_selected {
                    if let Some(tile_inspector) = self.tile_inspector() {
                        if let Some(selected_tile) = context.topmost_selected_tile() {
                            return tile_inspector.on_mouse_button(button, action, selected_tile);
                        }
                    }
                }
            }
            _ => {}
        }

        UiInputEvent::NotHandled
    }
}

// ----------------------------------------------
// TilePlacement
// ----------------------------------------------

pub struct TilePlacement {
    current_road_segment: RoadSegment, // For road placement.
}

enum PlaceOrClearResult {
    PlacedTile(&'static TileDef),
    ClearedTile(&'static TileDef),
    Failed,
}

impl TilePlacement {
    pub fn new() -> Self {
        Self { current_road_segment: RoadSegment::default() }
    }

    fn try_place_road_segment(&mut self, context: &mut GameMenusContext) -> bool {
        let road_segment_is_empty = self.current_road_segment.is_empty();

        // Place road segment if valid & we can afford it:
        let is_valid_road_placement =
            !road_segment_is_empty &&
            self.current_road_segment.is_valid &&
            context.can_afford_cost(self.current_road_segment.cost());

        if is_valid_road_placement {
            let query = context.new_query();
            let spawner = Spawner::new(&query);

            // Place tiles:
            for cell in &self.current_road_segment.path {
                spawner.try_spawn_tile_with_def(*cell, self.current_road_segment.tile_def());
            }

            // Update road junctions (each junction is a different variation of the same tile).
            for cell in &self.current_road_segment.path {
                road::update_junctions(context.tile_map, *cell);
            }
        }

        // Clear road segment highlight:
        if !road_segment_is_empty {
            road::mark_tiles(context.tile_map, &self.current_road_segment, false, false);
            self.current_road_segment.clear();
            context.clear_selection();
        }

        is_valid_road_placement
    }

    fn update_road_segment(&mut self, road_kind: RoadKind, start: Cell, end: Cell, context: &mut GameMenusContext) {
        // Clear previous segment highlight:
        road::mark_tiles(context.tile_map, &self.current_road_segment, false, false);

        self.current_road_segment =
            road::build_segment(context.tile_map, start, end, road_kind);

        let is_valid_road_placement =
            self.current_road_segment.is_valid &&
            context.can_afford_cost(self.current_road_segment.cost());

        // Highlight new segment:
        road::mark_tiles(context.tile_map, &self.current_road_segment, true, is_valid_road_placement);
    }

    fn placement_operation(&self, selection: TilePaletteSelection, context: &mut GameMenusContext) -> PlacementOp {
        if let Some(tile_def) = selection.as_tile_def() {
            if Spawner::new(&context.new_query()).can_afford_tile(tile_def) {
                PlacementOp::Place(tile_def)
            } else {
                PlacementOp::Invalidate(tile_def)
            }
        } else if selection.is_clear() {
            PlacementOp::Clear
        } else {
            PlacementOp::None
        }
    }

    fn try_place_or_clear_tile(selection: TilePaletteSelection,
                               context: &mut GameMenusContext)
                               -> PlaceOrClearResult {
        // If we have a selection, place it. Otherwise we want to try removing the tile
        // under the cursor. Do not remove terrain tiles though.
        if let Some(tile_def) = selection.as_tile_def() {
            let target_cell = context.tile_map.find_exact_cell_for_point(tile_def.layer_kind(),
                                                                         context.cursor_screen_pos,
                                                                         context.camera.transform());
            if target_cell.is_valid() {
                let query = context.new_query();
                return Self::place(&query, target_cell, tile_def);
            }
        } else if selection.is_clear() {
            // Clear/remove tile:
            let query = context.new_query();
            if let Some(tile) = query.tile_map().topmost_tile_at_cursor(
                    context.cursor_screen_pos,
                    context.camera.transform())
            {
                return Self::clear(&query, tile);
            }
        }
        PlaceOrClearResult::Failed
    }

    fn place(query: &Query, target_cell: Cell, tile_def: &'static TileDef) -> PlaceOrClearResult {
        let spawner = Spawner::new(query);
        let spawn_result = spawner.try_spawn_tile_with_def(target_cell, tile_def);

        match &spawn_result {
            SpawnerResult::Tile(tile) if tile.is(TileKind::Terrain) => {
                // In case we've replaced a road tile with terrain.
                road::update_junctions(query.tile_map(), target_cell);
                // In case we've placed a water tile or replaced water with terrain.
                water::update_transitions(query.tile_map(), target_cell);
            }
            SpawnerResult::Building(_) if water::is_port_or_wharf(tile_def) => {
                // If we've placed a port/wharf, select the correct
                // tile orientation in relation to the water.
                water::update_port_wharf_orientation(query.tile_map(), target_cell);
            }
            _ => {}
        }

        if spawn_result.is_ok() {
            PlaceOrClearResult::PlacedTile(tile_def)
        } else {
            PlaceOrClearResult::Failed
        }
    }

    fn clear(query: &Query, tile: &Tile) -> PlaceOrClearResult {
        let tile_def = tile.tile_def();

        let is_terrain = tile.is(TileKind::Terrain);
        let is_road = tile_def.path_kind.is_road();
        let is_vacant_lot = tile_def.path_kind.is_vacant_lot();

        // Cannot explicit remove terrain tiles except for roads and vacant lots.
        if !is_terrain || is_road || is_vacant_lot {
            let spawner = Spawner::new(query);
            spawner.despawn_tile(tile);

            if is_road || is_vacant_lot {
                let target_cell  = tile.base_cell();

                // Replace removed road tile with a regular terrain tile.
                let replacement_tile_def =
                    TileSets::get().find_tile_def_by_hash(TileMapLayerKind::Terrain,
                                                          TERRAIN_GROUND_CATEGORY.hash,
                                                          PresetTiles::Grass.hash());

                if let Some(tile_def) = replacement_tile_def {
                    let _ = spawner.try_spawn_tile_with_def(target_cell, tile_def);
                } else {
                    log::error!("Cannot find TileDef '{}' to replace removed tile!", PresetTiles::Grass);
                }

                // Update road junctions around the removed tile cell.
                if is_road {
                    road::update_junctions(query.tile_map(), target_cell);
                }
            }

            return PlaceOrClearResult::ClearedTile(tile_def);
        }

        PlaceOrClearResult::Failed
    }
}

// ----------------------------------------------
// TilePaletteSelection
// ----------------------------------------------

#[derive(Copy, Clone, Default)]
pub enum TilePaletteSelection {
    #[default]
    None,
    Clear,
    Tile(TileDefHandle),
}

impl TilePaletteSelection {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    pub fn is_clear(&self) -> bool {
        matches!(self, Self::Clear)
    }

    pub fn is_tile(&self) -> bool {
        matches!(self, Self::Tile(_))
    }

    pub fn is_tile_kind(&self, kinds: TileKind) -> bool {
        if let Some(tile_def) = self.as_tile_def() {
            return tile_def.is(kinds);
        }
        false
    }

    pub fn as_tile_def(&self) -> Option<&'static TileDef> {
        match self {
            Self::Tile(handle) => TileSets::get().handle_to_tile_def(*handle),
            _ => None,
        }
    }
}

// ----------------------------------------------
// TilePalette
// ----------------------------------------------

pub trait TilePalette {
    fn on_mouse_button(&mut self, button: MouseButton, action: InputAction) -> UiInputEvent;
    fn wants_to_place_or_clear_tile(&self) -> bool;

    fn current_selection(&self) -> TilePaletteSelection;
    fn clear_selection(&mut self);

    fn has_selection(&self) -> bool {
        !self.current_selection().is_none()
    }

    fn is_road_tile_selected(&self) -> bool {
        self.current_selection().as_tile_def().is_some_and(|tile_def| tile_def.path_kind.is_road())
    }

    fn selected_road_kind(&self) -> RoadKind {
        if let Some(tile_def) = self.current_selection().as_tile_def() {
            if tile_def.path_kind.is_road() {
                if tile_def.hash == road::tile_name(road::RoadKind::Dirt).hash {
                    return road::RoadKind::Dirt;
                } else if tile_def.hash == road::tile_name(road::RoadKind::Paved).hash {
                    return road::RoadKind::Paved;
                }
            }
        }
        panic!("No road tile selected!");
    }
}

// ----------------------------------------------
// TileInspector
// ----------------------------------------------

pub trait TileInspector {
    fn on_mouse_button(&mut self,
                       button: MouseButton,
                       action: InputAction,
                       selected_tile: &Tile)
                       -> UiInputEvent;

    fn close(&mut self);
}
