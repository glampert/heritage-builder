use std::collections::VecDeque;

use crate::{
    singleton_late_init,
    utils::coords::Cell,
    game::world::{World, object::GameObject},
    tile::{Tile, TileMap, TileMapLayerKind},
};

// ----------------------------------------------
// Undo / Redo Support
// ----------------------------------------------

// Undo:
// - Place Tiles: Delete tiles added.
// - Clear Tiles: Place back deleted tiles.
//
// Redo:
// - Place Tiles: Place back deleted tiles.
// - Clear Tiles: Delete tiles added.

/*
enum EditAction {
    Undo(UndoCommand),
    Redo(RedoCommand),
}

enum UndoCommand {
    // List of tiles to delete.
    PlaceTiles(SmallVec<[TileInfo; 1]>),

    // List of tiles to place back in the world.
    ClearTiles(SmallVec<[TileSavedState; 1]>),
}

enum RedoCommand {
    // List of tiles to place back in the world.
    PlaceTiles(SmallVec<[TileSavedState; 1]>),

    // List of tiles to delete.
    ClearTiles(SmallVec<[TileInfo; 1]>),
}

struct TileInfo {
    // ???
}

struct TileSavedState {
    // Tile:
    tile_def: &'static TileDef,
    tile_flags: TileFlags,
    tile_variation_index: TileVariationIndex,

    // Game Object:
    game_object: Box<dyn GameObject>,
}
*/

struct TileSavedState {

}

struct GameObjectSavedState {

}

struct SavedState {
    tile_base_cell: Cell,
    terrain_tile_state: Option<TileSavedState>,
    object_tile_state: Option<TileSavedState>,
    game_object_state: Option<GameObjectSavedState>,
}

impl SavedState {
    fn new(tile_base_cell: Cell) -> Self {
        Self {
            tile_base_cell,
            terrain_tile_state: None,
            object_tile_state: None,
            game_object_state: None,
        }
    }
}

fn serialize_tile_state(_tile: &Tile) -> Option<TileSavedState> {

    Some(TileSavedState {})
}

fn serialize_game_object_state(_object: &dyn GameObject) -> Option<GameObjectSavedState> {

    Some(GameObjectSavedState {})
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

// ----------------------------------------------
// UndoRedoSingleton
// ----------------------------------------------

struct UndoRedoSingleton {
    undo_stack: VecDeque<UndoRedoRecord>,
    redo_stack: VecDeque<UndoRedoRecord>,
}

struct UndoRedoRecord {
    action: EditAction,
    saved_states: Vec<SavedState>,
}

const UNDO_REDO_STACK_MAX_SIZE: usize = 4;

impl UndoRedoSingleton {
    fn new() -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
        }
    }

    fn record<I, C>(&mut self, action: EditAction, affected_cells: I, tile_map: &TileMap, world: &World)
        where
            I: IntoIterator<Item = C>,
            C: CellKey,
    {
        let mut saved_states = Vec::new();

        for cell_key in affected_cells {
            let cell = *cell_key.to_cell();
            let mut saved_state = SavedState::new(cell);

            if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain) {
                saved_state.terrain_tile_state = serialize_tile_state(tile);
            }

            if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects) {
                saved_state.object_tile_state = serialize_tile_state(tile);

                if let Some(game_object) = world.find_game_object_for_tile(tile) {
                    saved_state.game_object_state = serialize_game_object_state(game_object);
                }
            }

            saved_states.push(saved_state);
        }

        self.push_undo_record(UndoRedoRecord { action, saved_states });
    }

    fn push_undo_record(&mut self, record: UndoRedoRecord) {
        if self.undo_stack.len() >= UNDO_REDO_STACK_MAX_SIZE {
            self.undo_stack.pop_front();
        }
        self.undo_stack.push_back(record);
    }

    fn push_redo_record(&mut self, record: UndoRedoRecord) {
        if self.redo_stack.len() >= UNDO_REDO_STACK_MAX_SIZE {
            self.redo_stack.pop_front();
        }
        self.redo_stack.push_back(record);
    }

    fn undo(&mut self, _tile_map: &mut TileMap, _world: &mut World) {
        if let Some(record) = self.undo_stack.pop_back() {

            // TODO: revert world state to record

            self.push_redo_record(record);
        }
    }

    fn redo(&mut self, _tile_map: &mut TileMap, _world: &mut World) {
        if let Some(record) = self.redo_stack.pop_back() {

            // TODO: revert world state to record

            self.push_undo_record(record);
        }
    }
}

// Global instance:
singleton_late_init! { UNDO_REDO_SINGLETON, UndoRedoSingleton }

// ----------------------------------------------
// Public API
// ----------------------------------------------

#[derive(Copy, Clone)]
pub enum EditAction {
    PlacedTiles,
    ClearingTiles,
}

pub fn initialize() {
    UndoRedoSingleton::initialize(UndoRedoSingleton::new());
}

pub fn record<I, C>(action: EditAction, affected_cells: I, tile_map: &TileMap, world: &World)
    where
        I: IntoIterator<Item = C>,
        C: CellKey,
{
    UndoRedoSingleton::get_mut().record(action, affected_cells, tile_map, world);
}

pub fn undo(tile_map: &mut TileMap, world: &mut World) {
    UndoRedoSingleton::get_mut().undo(tile_map, world);
}

pub fn redo(tile_map: &mut TileMap, world: &mut World) {
    UndoRedoSingleton::get_mut().redo(tile_map, world);
}
