use std::any::Any;
use std::collections::VecDeque;
use bitflags::bitflags;

use crate::{
    log,
    singleton_late_init,
    utils::coords::Cell,
    pathfind::NodeKind as PathNodeKind,
    game::{world::World, sim::Query, menu::TilePlacement},
    tile::{Tile, TileKind, TileFlags, TileMap, TileMapLayerKind, sets::TileDef},
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

const UNDO_REDO_STACK_MAX_SIZE: usize = 16;

// We can undo/redo placing/deleting of roads and vacant lots.
const SUPPORTED_TERRAIN_KINDS: PathNodeKind =
    PathNodeKind::from_bits_retain(
        PathNodeKind::Road.bits()
        | PathNodeKind::VacantLot.bits()
    );

// We can undo/redo placing/deleting of buildings and props.
const SUPPORTED_OBJECT_KINDS: TileKind =
    TileKind::from_bits_retain(
        TileKind::Building.bits()
        | TileKind::Rocks.bits()
        | TileKind::Vegetation.bits()
    );

// ----------------------------------------------
// Macros
// ----------------------------------------------

#[macro_export]
macro_rules! game_object_undo_redo_state {
    ($struct_name:ident) => {
        impl $crate::game::undo_redo::GameObjectSavedState for $struct_name {
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }

        impl $struct_name {
            fn new_state(instance: $struct_name) -> Option<Box<dyn $crate::game::undo_redo::GameObjectSavedState>> {
                Some(Box::new(instance))
            }
            fn downcast(state: &dyn $crate::game::undo_redo::GameObjectSavedState) -> &$struct_name {
                state.as_any()
                    .downcast_ref::<$struct_name>()
                    .unwrap_or_else(|| panic!("Expected an {} instance!", stringify!($struct_name)))
            }
        }
    };
}

// ----------------------------------------------
// Helper types
// ----------------------------------------------

pub trait GameObjectSavedState {
    fn as_any(&self) -> &dyn Any;
}

struct TileSavedState {
    tile_base_cell: Cell,
    tile_def: &'static TileDef,
    tile_flags: TileFlags,
    tile_variation_index: u32,
}

#[derive(Default)]
struct SavedState {
    terrain_tile_state: Option<TileSavedState>,
    object_tile_state:  Option<TileSavedState>,
    game_object_state:  Option<Box<dyn GameObjectSavedState>>,
}

impl SavedState {
    fn is_empty(&self) -> bool {
        self.terrain_tile_state.is_none() &&
        self.object_tile_state.is_none()  &&
        self.game_object_state.is_none()
    }
}

fn record_tile_state(tile: &Tile) -> Option<TileSavedState> {
    Some(TileSavedState {
        tile_base_cell: tile.base_cell(),
        tile_def: tile.tile_def(),
        tile_flags: tile.flags(),
        tile_variation_index: tile.variation_index().try_into().unwrap(),
    })
}

pub trait CellKey {
    fn to_cell(&self) -> &Cell;
}

impl CellKey for &Cell {
    #[inline]
    fn to_cell(&self) -> &Cell { self }
}

// Allows us to get Cells from HashMap pairs.
impl<T> CellKey for (&Cell, T) {
    #[inline]
    fn to_cell(&self) -> &Cell { self.0 }
}

#[derive(Copy, Clone)]
enum Command {
    Undo,
    Redo,
}

struct Record {
    action: EditAction,
    saved_states: Vec<SavedState>,
}

impl Record {
    // Undo:
    // - Place Tiles: Delete tiles added.
    // - Clear Tiles: Place back deleted tiles.
    //
    // Redo:
    // - Place Tiles: Place back deleted tiles.
    // - Clear Tiles: Delete tiles added.
    fn apply_action(&self, command: Command, query: &Query) {
        match command {
            Command::Undo => {
                match self.action {
                    EditAction::PlacedTiles   => self.clear_tiles(query, true),
                    EditAction::ClearingTiles => self.place_tiles(query, false),
                }
            }
            Command::Redo => {
                match self.action {
                    EditAction::PlacedTiles   => self.place_tiles(query, true),
                    EditAction::ClearingTiles => self.clear_tiles(query, false),
                }
            }
        }
    }

    fn place_tiles(&self, query: &Query, subtract_tile_cost: bool) {
        for state in &self.saved_states {
            if let Some(terrain_tile_state) = &state.terrain_tile_state {
                Self::place_tile(query, TileMapLayerKind::Terrain, terrain_tile_state, subtract_tile_cost);
            }

            let maybe_object_tile = {
                if let Some(object_tile_state) = &state.object_tile_state {
                    Self::place_tile(query, TileMapLayerKind::Objects, object_tile_state, subtract_tile_cost)
                } else {
                    None
                }
            };

            if let Some(object_tile) = maybe_object_tile {
                if let Some(game_object_state) = &state.game_object_state {
                    if let Some(game_object) =
                        query.world().find_game_object_for_tile_mut(object_tile)
                    {
                        game_object.undo_redo_apply(game_object_state.as_ref());
                    }
                }
            }
        }
    }

    fn place_tile<'world>(query: &'world Query,
                          layer: TileMapLayerKind,
                          tile_state: &TileSavedState,
                          subtract_tile_cost: bool)
                          -> Option<&'world Tile> {
        let target_cell = tile_state.tile_base_cell;
        let tile_def = tile_state.tile_def;
        let tile_variation_index = tile_state.tile_variation_index;

        let mut tile_flags = tile_state.tile_flags;
        tile_flags.set(TileFlags::Highlighted | TileFlags::Invalidated, false); // clear these.

        debug_assert!(tile_def.layer_kind() == layer);

        if TilePlacement::place(query, target_cell, tile_def, subtract_tile_cost, false).is_ok() {
            if let Some(tile) = query.find_tile_mut(target_cell, layer, tile_def.kind()) {
                tile.set_variation_index(tile_variation_index as usize);
                if !tile_flags.is_empty() {
                    tile.set_flags(tile_flags, true);
                }
                return Some(tile);
            }
        }

        None
    }

    fn clear_tiles(&self, query: &Query, restore_tile_cost: bool) {
        for state in &self.saved_states {
            if let Some(terrain_tile_state) = &state.terrain_tile_state {
                Self::clear_tile(query, TileMapLayerKind::Terrain, terrain_tile_state, restore_tile_cost);
            }

            if let Some(object_tile_state) = &state.object_tile_state {
                Self::clear_tile(query, TileMapLayerKind::Objects, object_tile_state, restore_tile_cost);
            }
        }
    }

    fn clear_tile(query: &Query, layer: TileMapLayerKind, tile_state: &TileSavedState, restore_tile_cost: bool) {
        let tile_def = tile_state.tile_def;
        debug_assert!(tile_def.layer_kind() == layer);

        if let Some(tile) = query.find_tile(tile_state.tile_base_cell, layer, tile_def.kind()) {
            TilePlacement::clear(query, tile, restore_tile_cost, false);
        }
    }
}

// ----------------------------------------------
// UndoRedoSingleton
// ----------------------------------------------

// Holds the undo/redo stacks.
struct UndoRedoSingleton {
    undo_stack: VecDeque<Record>,
    redo_stack: VecDeque<Record>,
}

impl UndoRedoSingleton {
    fn new() -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
        }
    }

    fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    fn record<I, C>(&mut self, action: EditAction, affected_cells: I, layers: EditedLayer, tile_map: &TileMap, world: &World)
        where
            I: IntoIterator<Item = C>,
            C: CellKey,
    {
        let mut saved_states = Vec::new();

        for cell_key in affected_cells {
            let cell = *cell_key.to_cell();
            let mut saved_state = SavedState::default();

            if layers.intersects(EditedLayer::Terrain) {
                if let Some(terrain_tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain) {
                    if terrain_tile.path_kind().intersects(SUPPORTED_TERRAIN_KINDS) {
                        saved_state.terrain_tile_state = record_tile_state(terrain_tile);
                    }
                }
            }

            if layers.intersects(EditedLayer::Objects) {
                if let Some(object_tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects) {
                    if object_tile.is(SUPPORTED_OBJECT_KINDS) {
                        saved_state.object_tile_state = record_tile_state(object_tile);

                        if let Some(game_object) = world.find_game_object_for_tile(object_tile) {
                            saved_state.game_object_state = game_object.undo_redo_record();
                        }
                    }
                }
            }

            if !saved_state.is_empty() {
                saved_states.push(saved_state);
            }
        }

        if !saved_states.is_empty() {
            self.push_undo_record(Record { action, saved_states });
        }
    }

    fn push_undo_record(&mut self, record: Record) {
        if self.undo_stack.len() >= UNDO_REDO_STACK_MAX_SIZE {
            self.undo_stack.pop_front();
        }
        self.undo_stack.push_back(record);
    }

    fn push_redo_record(&mut self, record: Record) {
        if self.redo_stack.len() >= UNDO_REDO_STACK_MAX_SIZE {
            self.redo_stack.pop_front();
        }
        self.redo_stack.push_back(record);
    }

    fn undo(&mut self, query: &Query) {
        if let Some(record) = self.undo_stack.pop_back() {
            log::info!(log::channel!("undo_redo"), "Undo: {:?} ({} items)", record.action, record.saved_states.len());
            record.apply_action(Command::Undo, query);
            self.push_redo_record(record);
        }
    }

    fn redo(&mut self, query: &Query) {
        if let Some(record) = self.redo_stack.pop_back() {
            log::info!(log::channel!("undo_redo"), "Redo: {:?} ({} items)", record.action, record.saved_states.len());
            record.apply_action(Command::Redo, query);
            self.push_undo_record(record);
        }
    }
}

// Global instance:
singleton_late_init! { UNDO_REDO_SINGLETON, UndoRedoSingleton }

// ----------------------------------------------
// Public API
// ----------------------------------------------

#[derive(Copy, Clone, Debug)]
pub enum EditAction {
    PlacedTiles,
    ClearingTiles,
}

bitflags! {
    #[derive(Copy, Clone)]
    pub struct EditedLayer: u32 {
        const Terrain = 1 << 0;
        const Objects = 1 << 1;
    }
}

pub fn initialize() {
    UndoRedoSingleton::initialize(UndoRedoSingleton::new());
}

pub fn clear() {
    UndoRedoSingleton::get_mut().clear();
}

pub fn record<I, C>(action: EditAction, affected_cells: I, layers: EditedLayer, tile_map: &TileMap, world: &World)
    where
        I: IntoIterator<Item = C>,
        C: CellKey,
{
    UndoRedoSingleton::get_mut().record(action, affected_cells, layers, tile_map, world);
}

pub fn undo(query: &Query) {
    UndoRedoSingleton::get_mut().undo(query);
}

pub fn redo(query: &Query) {
    UndoRedoSingleton::get_mut().redo(query);
}
