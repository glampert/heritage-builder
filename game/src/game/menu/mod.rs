use std::any::Any;

use crate::{
    log,
    save::{Save, Load},
    ui::UiInputEvent,
    engine::{Engine, time::Seconds},
    utils::{Vec2, coords::{Cell, CellRange}, hash::SmallSet},
    app::input::{InputAction, InputKey, InputModifiers, MouseButton},
    game::{
        world::{object::{Spawner, SpawnerResult}, World},
        sim::{Query, Simulation}, system::GameSystems,
        undo_redo::{self, EditAction, EditedLayer},
    },
    tile::{
        Tile, TileKind, TileMap, TileMapLayerKind, selection::TileSelection,
        sets::{TileSets, TileDef, TileDefHandle, PresetTiles},
        rendering::TileMapRenderFlags, PlacementOp, camera::Camera,
        water, road::{self, RoadSegment, RoadKind},
    },
};

pub mod widgets;
pub mod home;
pub mod hud;

mod button;
mod palette;
mod modal;
mod bar;

// ----------------------------------------------
// Helper structs
// ----------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GameMenusMode {
    DevEditor,
    InGameHud,
    Home,
}

#[derive(Copy, Clone)]
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
    // Engine:
    pub engine: &'game mut dyn Engine,

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

pub trait GameMenusSystem: Any + Save + Load {
    fn as_any(&self) -> &dyn Any;
    fn mode(&self) -> GameMenusMode;

    fn tile_placement(&mut self) -> Option<&mut TilePlacement>;
    fn tile_palette(&mut self) -> Option<&mut dyn TilePalette>;
    fn tile_inspector(&mut self) -> Option<&mut dyn TileInspector>;

    fn selected_render_flags(&self) -> TileMapRenderFlags {
        TileMapRenderFlags::DrawTerrainAndObjects
    }

    fn begin_frame(&mut self, context: &mut GameMenusContext) {
        // Bail if we're hovering over an ImGui menu...
        if context.engine.ui_system().is_handling_mouse_input() {
            return;
        }

        // Tile hovering and selection:
        let selection = self.tile_palette().unwrap().current_selection();
        let placement_op = self.tile_placement().unwrap().placement_operation(selection, context);

        context.update_selection(placement_op);

        // Incrementally build road segment (drag and draw segment):
        let is_road_tile_selected = self.tile_palette().unwrap().is_road_tile_selected();
        if is_road_tile_selected {
            if let Some((start, end)) = context.range_selection_cells() {
                let road_kind = self.tile_palette().unwrap().selected_road_kind();
                self.tile_placement().unwrap().update_road_segment(road_kind, start, end, context);
            }
        }

        // Place a regular (non-road) tile or clear a tile:
        if !is_road_tile_selected && self.tile_palette().unwrap().wants_to_place_or_clear_tile() {
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
                self.tile_palette().unwrap().clear_selection();
                context.clear_selection();
            }
        }
    }

    fn end_frame(&mut self, _context: &mut GameMenusContext, _visible_range: CellRange) {
        // Nothing here. Should implement the menu rendering logic.
    }

    fn handle_input(&mut self, context: &mut GameMenusContext, args: GameMenusInputArgs) -> UiInputEvent {
        if self.handle_custom_input(context, args).is_handled() {
            return UiInputEvent::Handled;
        }

        match args {
            GameMenusInputArgs::Key { key, action, modifiers } => {
                if action == InputAction::Press {
                    // [ESCAPE]: Clear current selection / close tile inspector.
                    if key == InputKey::Escape {
                        self.tile_palette().unwrap().clear_selection();
                        context.clear_selection();
                        if let Some(tile_inspector) = self.tile_inspector() {
                            tile_inspector.close();
                        }
                        return UiInputEvent::Handled;
                    }

                    let shift = modifiers.intersects(InputModifiers::Shift);
                    let ctrl_or_cmd = modifiers.intersects(InputModifiers::Control | InputModifiers::Super);

                    // [SHIFT]+[CTRL]+[Z] / [SHIFT]+[CMD]+[Z] (MacOS): Redo last action.
                    if key == InputKey::Z && ctrl_or_cmd && shift {
                        undo_redo::redo(&context.new_query());
                        return UiInputEvent::Handled;
                    }

                    // [CTRL]+[Z] / [CMD]+[Z] (MacOs): Undo last action.
                    if key == InputKey::Z && ctrl_or_cmd {
                        undo_redo::undo(&context.new_query());
                        return UiInputEvent::Handled;
                    }
                }
            }
            GameMenusInputArgs::Mouse { button, action, .. } => {
                let is_road_tile_selected = self.tile_palette().unwrap().is_road_tile_selected();
                let is_clear_selected = self.tile_palette().unwrap().current_selection().is_clear();

                if !is_road_tile_selected && !is_clear_selected && self.tile_palette().unwrap().has_selection() {
                    let input_event = self.tile_palette().unwrap().on_mouse_button(button, action);
                    if input_event.not_handled() {
                        // Mouse button click other than [LEFT_BTN], clear selection state.
                        self.tile_palette().unwrap().clear_selection();
                        context.clear_selection();
                    }
                    return input_event;
                }

                if context.selection_handle_mouse_button(button, action) {
                    // Handle road placement (drag and draw segment).
                    if is_road_tile_selected {
                        // Place road segment if valid & we can afford it.
                        self.tile_placement().unwrap().try_place_road_segment(context);
                    } else if is_clear_selected && !context.tile_selection.cells().is_empty() {
                        // Clear batch of selected tiles:
                        let query = context.new_query();
                        let tile_map = query.tile_map();

                        // Ensure each cell is unique with a hash set.
                        let mut clearable_cells: SmallSet<64, Cell> = SmallSet::new();
                        let mut layers = EditedLayer::empty();

                        for &cell in context.tile_selection.cells() {
                            if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects) {
                                if TilePlacement::can_clear(tile) {
                                    clearable_cells.insert(cell);
                                    layers |= EditedLayer::Objects;
                                }
                            } else if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain) {
                                if TilePlacement::can_clear(tile) {
                                    clearable_cells.insert(cell);
                                    layers |= EditedLayer::Terrain;
                                }
                            }
                        }

                        if !clearable_cells.is_empty() {
                            undo_redo::record(EditAction::ClearingTiles,
                                              clearable_cells.iter(),
                                              layers,
                                              context.tile_map,
                                              context.world);

                            for (&cell, _) in clearable_cells.iter() {
                                if layers.intersects(EditedLayer::Objects) {
                                    if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects) {
                                        TilePlacement::clear(&query, tile, false, false);
                                    }
                                }

                                if layers.intersects(EditedLayer::Terrain) {
                                    if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain) {
                                        TilePlacement::clear(&query, tile, false, false);
                                    }
                                }
                            }

                            context.clear_selection();
                        }
                    }
                } else {
                    // Mouse button click other than [LEFT_BTN], clear selection state.
                    self.tile_palette().unwrap().clear_selection();
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

    // Optional override to add extended input handling behavior on top of the default handle_input().
    // This is called before handle_input(), so returning UiInputEvent::Handled will stop handle_input() logic from running.
    fn handle_custom_input(&mut self, _context: &mut GameMenusContext, _args: GameMenusInputArgs) -> UiInputEvent {
        UiInputEvent::NotHandled
    }
}

// ----------------------------------------------
// TilePlacement
// ----------------------------------------------

pub struct TilePlacement {
    current_road_segment: RoadSegment, // For road placement.
}

pub enum PlaceOrClearResult {
    PlacedTile(&'static TileDef),
    ClearedTile(&'static TileDef),
    Failed,
}

impl PlaceOrClearResult {
    #[inline]
    pub fn is_ok(&self) -> bool {
        !self.failed()
    }

    #[inline]
    pub fn failed(&self) -> bool {
        matches!(self, Self::Failed)
    }
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

            undo_redo::record(EditAction::PlacedTiles,
                              &self.current_road_segment.path,
                              EditedLayer::Terrain,
                              context.tile_map,
                              context.world);
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
                return Self::place(&query, target_cell, tile_def, true, true);
            }
        } else if selection.is_clear() {
            // Clear/remove tile:
            let query = context.new_query();
            if let Some(tile) = query.tile_map().topmost_tile_at_cursor(
                    context.cursor_screen_pos,
                    context.camera.transform())
            {
                return Self::clear(&query, tile, false, true);
            }
        }
        PlaceOrClearResult::Failed
    }

    pub fn place(query: &Query,
                 target_cell: Cell,
                 tile_def: &'static TileDef,
                 subtract_tile_cost: bool,
                 undo_redo: bool)
                 -> PlaceOrClearResult {
        let mut spawner = Spawner::new(query);
        spawner.set_subtract_tile_cost(subtract_tile_cost);

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
            if undo_redo {
                let layers = if tile_def.is(TileKind::Terrain) {
                    EditedLayer::Terrain
                } else {
                    EditedLayer::Objects
                };
                undo_redo::record(EditAction::PlacedTiles,
                                  &[target_cell],
                                  layers,
                                  query.tile_map(),
                                  query.world());
            }
            PlaceOrClearResult::PlacedTile(tile_def)
        } else {
            PlaceOrClearResult::Failed
        }
    }

    pub fn clear(query: &Query,
                 tile: &Tile,
                 restore_tile_cost: bool,
                 undo_redo: bool)
                 -> PlaceOrClearResult {
        let tile_def = tile.tile_def();

        let is_terrain = tile.is(TileKind::Terrain);
        let is_road = tile_def.path_kind.is_road();
        let is_vacant_lot = tile_def.path_kind.is_vacant_lot();

        // Cannot explicit remove terrain tiles except for roads and vacant lots.
        if !is_terrain || is_road || is_vacant_lot {
            let target_cell  = tile.base_cell();

            if undo_redo {
                let layers = if is_terrain {
                    EditedLayer::Terrain
                } else {
                    EditedLayer::Objects
                };
                undo_redo::record(EditAction::ClearingTiles,
                                  &[target_cell],
                                  layers,
                                  query.tile_map(),
                                  query.world());
            }

            let mut spawner = Spawner::new(query);
            spawner.set_restore_tile_cost(restore_tile_cost);

            spawner.despawn_tile(tile);

            if is_road || is_vacant_lot {
                // Replace removed road tile with a regular terrain tile.
                if let Some(terrain_tile_def) = PresetTiles::Grass.find_tile_def() {
                    if let SpawnerResult::Err(err) = spawner.try_spawn_tile_with_def(target_cell, terrain_tile_def) {
                        log::error!("Failed to place tile '{}': {}", terrain_tile_def.name, err);
                    }
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

    fn can_clear(tile: &Tile) -> bool {
        let tile_def = tile.tile_def();

        let is_terrain = tile.is(TileKind::Terrain);
        let is_road = tile_def.path_kind.is_road();
        let is_vacant_lot = tile_def.path_kind.is_vacant_lot();

        !is_terrain || is_road || is_vacant_lot
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
