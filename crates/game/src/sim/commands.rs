use slab::Slab;
use strum::Display;

use common::coords::Cell;
use engine::log;

use super::SimContext;
use crate::{
    constants::INITIAL_GENERATION,
    prop::{Prop, PropId},
    building::{Building, BuildingKindAndId},
    unit::{config::UnitConfigKey, Unit, UnitId},
    world::object::{GenerationalIndex, GameObject, Spawner, SpawnerResult},
    tile::{placement::TilePlacementErr, sets::TileDef, Tile, TileMapLayerKind},
};

// ----------------------------------------------
// SpawnPromise
// ----------------------------------------------

pub struct SpawnPromise<T> {
    _marker: std::marker::PhantomData<T>,

    // Tile is guaranteed to be spawned (or fail to spawn) in the next frame,
    // so the promise is ready when current_frame > request_frame.
    request_frame: usize,

    // Handle to underlying promise state. Once the promise is queried and the
    // GameObject id is retrieved the state is freed and the promise becomes invalid.
    state_id: SpawnPromiseStateId,
}

impl<T> SpawnPromise<T> {
    fn new(request_frame: usize, state_id: SpawnPromiseStateId) -> Self {
        Self { _marker: std::marker::PhantomData, request_frame, state_id }
    }
}

#[derive(Clone)]
pub enum SpawnReadyResult {
    GameObject(GenerationalIndex), // If spawned object was a Building, Unit or Prop (with associated GameObject).
    Tile(Cell, TileMapLayerKind),  // If spawned object was a plain terrain tile (no GameObject).
}

pub enum SpawnQueryResult<T> {
    InvalidPromise,
    Pending(SpawnPromise<T>),
    Ready(SpawnReadyResult),
    Failed(TilePlacementErr),
}

impl<T> std::fmt::Display for SpawnQueryResult<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::InvalidPromise => write!(f, "Invalid Promise"),
            Self::Pending(_) => write!(f, "Pending"),
            Self::Ready(_) => write!(f, "Ready"),
            Self::Failed(err) => write!(f, "Failed [{}] - {}", err.reason, err.message),
        }
    }
}

// ----------------------------------------------
// SpawnPromiseState Internals
// ----------------------------------------------

type SpawnPromiseStateId = GenerationalIndex;

#[derive(Display)]
enum SpawnPromiseState {
    Pending,
    Ready(SpawnReadyResult),
    Failed(TilePlacementErr),
}

impl SpawnPromiseState {
    #[inline]
    fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }
}

struct SpawnPromiseStatePool {
    pool: Slab<(SpawnPromiseStateId, SpawnPromiseState)>,
    generation: u32,
}

impl SpawnPromiseStatePool {
    fn new(capacity: usize) -> Self {
        Self { pool: Slab::with_capacity(capacity), generation: INITIAL_GENERATION }
    }

    fn clear(&mut self) {
        self.pool.clear();
        self.generation = INITIAL_GENERATION;
    }

    fn allocate(&mut self) -> SpawnPromiseStateId {
        let generation = self.generation;
        self.generation += 1;

        let id = SpawnPromiseStateId::new(generation, self.pool.vacant_key());
        let index = self.pool.insert((id, SpawnPromiseState::Pending));

        debug_assert!(id == self.pool[index].0);
        id
    }

    fn free(&mut self, state_id: SpawnPromiseStateId) {
        if !state_id.is_valid() {
            return;
        }

        let index = state_id.index();

        // Handle feeing an invalid handle gracefully.
        // This will also avoid any invalid frees thanks to the generation check.
        match self.pool.get(index) {
            Some((id, _)) => {
                if *id != state_id {
                    return; // Slot reused, not same item.
                }
            }
            None => return, // Already free.
        }

        if self.pool.try_remove(index).is_none() {
            panic!("Failed to free SpawnPromiseState slot [{index}]!");
        }
    }

    fn try_get(&self, state_id: SpawnPromiseStateId) -> Option<&SpawnPromiseState> {
        if !state_id.is_valid() {
            return None;
        }

        self.pool.get(state_id.index())
            .filter(|(id, _)| *id == state_id)
            .map(|(_, state)| state)
    }

    fn try_get_mut(&mut self, state_id: SpawnPromiseStateId) -> Option<&mut SpawnPromiseState> {
        if !state_id.is_valid() {
            return None;
        }

        self.pool.get_mut(state_id.index())
            .filter(|(id, _)| *id == state_id)
            .map(|(_, state)| state)
    }
}

// Detect any leaked instances.
impl Drop for SpawnPromiseStatePool {
    fn drop(&mut self) {
        if self.pool.is_empty() {
            return;
        }

        log::error!("----------------------------");
        log::error!("  SPAWN PROMISE POOL LEAKS  ");
        log::error!("----------------------------");

        for (index, (id, state)) in &self.pool {
            log::error!("Leaked SpawnPromiseState[{index}]: {id}, {state}");
        }

        if cfg!(debug_assertions) {
            panic!("SpawnPromisePool dropped with {} remaining entries (generation: {}).", self.pool.len(), self.generation);
        } else {
            log::error!(
                "SpawnPromisePool dropped with {} remaining entries (generation: {}).",
                self.pool.len(),
                self.generation
            );
        }
    }
}

// ----------------------------------------------
// SimCmd
// ----------------------------------------------

#[derive(Display)]
enum SimCmd {
    // -- Tile operations -----------------------
    SpawnTileWithTileDef { cell: Cell, tile_def: &'static TileDef, state_id: SpawnPromiseStateId },
    DespawnTileAtCell { cell: Cell, layer_kind: TileMapLayerKind },

    // -- Unit operations -----------------------
    SpawnUnitWithConfig { origin: Cell, config: UnitConfigKey, state_id: SpawnPromiseStateId },
    SpawnUnitWithTileDef { origin: Cell, tile_def: &'static TileDef, state_id: SpawnPromiseStateId },
    DespawnUnitWithId { id: UnitId },

    // -- Building operations -------------------
    SpawnBuildingWithTileDef { base_cell: Cell, tile_def: &'static TileDef, state_id: SpawnPromiseStateId },
    DespawnBuildingWithId { kind_and_id: BuildingKindAndId },

    // -- Prop operations -----------------------
    SpawnPropWithTileDef { origin: Cell, tile_def: &'static TileDef, state_id: SpawnPromiseStateId },
    DespawnPropWithId { id: PropId },

    // TODO: Add other commands.
    // E.g.: VisitBuilding, UpgradeHouse, AddGold/RemoveGold, etc.
}

// ----------------------------------------------
// SimCmds
// ----------------------------------------------

const SIM_CMDS_INITIAL_CAPACITY: usize = 64;

// Deferred command queue populated during simulation updates.
// Any world or tile map modification is done via a deferred command.
// Commands are applied after all game objects have been updated.
pub struct SimCmds {
    current_frame: usize,
    cmds: Vec<SimCmd>,
    promises: SpawnPromiseStatePool,
}

impl Default for SimCmds {
    fn default() -> Self {
        Self::new()
    }
}

impl SimCmds {
    pub fn new() -> Self {
        Self {
            current_frame: 0,
            cmds: Vec::with_capacity(SIM_CMDS_INITIAL_CAPACITY),
            promises: SpawnPromiseStatePool::new(SIM_CMDS_INITIAL_CAPACITY),
        }
    }

    pub fn reset(&mut self) {
        self.current_frame = 0;
        self.cmds.clear();
        self.promises.clear();
    }

    pub fn pre_load(&mut self) {
        self.reset();
    }

    pub fn post_load(&mut self) {
        // Nothing currently.
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }

    // -- SpawnPromise state query --------------

    #[inline]
    pub fn is_promise_ready<T>(&self, promise: &SpawnPromise<T>) -> bool {
        // True if the spawn command was executed. It may have succeeded or failed.
        self.current_frame > promise.request_frame
    }

    pub fn query_promise<T>(&mut self, promise: SpawnPromise<T>) -> SpawnQueryResult<T> {
        if !self.is_promise_ready(&promise) {
            // Quick early out when called in the same frame.
            return SpawnQueryResult::Pending(promise);
        }

        let state_id = promise.state_id;

        let (result, consumed) = match self.promises.try_get(state_id) {
            Some(state) => {
                match state {
                    SpawnPromiseState::Ready(ready) => {
                        (SpawnQueryResult::Ready(ready.clone()), true)
                    }
                    SpawnPromiseState::Failed(err) => {
                        (SpawnQueryResult::Failed(err.clone()), true)
                    }
                    SpawnPromiseState::Pending => {
                        // Should't happen if the is_spawned() test above passed...
                        log::error!(log::channel!("sim"), "Unexpected SpawnPromiseState::Pending for {state_id}!");
                        (SpawnQueryResult::Pending(promise), false)
                    }
                }
            }
            None => (SpawnQueryResult::InvalidPromise, false),
        };

        if consumed {
            // Once the promise is successfully queried (Ready or Failed)
            // it is consumed and removed from the state pool.
            self.promises.free(state_id);
        }

        result
    }

    fn discard_promise<T>(&mut self, promise: SpawnPromise<T>) {
        // Free the promise state without checking for completion.
        self.promises.free(promise.state_id);
    }

    // -- Tile operations -----------------------

    #[inline]
    pub fn spawn_tile_with_tile_def(&mut self, cell: Cell, tile_def: &'static TileDef) -> SpawnPromise<Tile> {
        let state_id = self.promises.allocate();
        self.cmds.push(SimCmd::SpawnTileWithTileDef { cell, tile_def, state_id });
        SpawnPromise::new(self.current_frame, state_id)
    }

    #[inline]
    pub fn despawn_tile_at_cell(&mut self, cell: Cell, layer_kind: TileMapLayerKind) {
        self.cmds.push(SimCmd::DespawnTileAtCell { cell, layer_kind });
    }

    // -- Unit operations -----------------------

    #[inline]
    pub fn spawn_unit_with_config(&mut self, origin: Cell, config: UnitConfigKey) -> SpawnPromise<Unit> {
        let state_id = self.promises.allocate();
        self.cmds.push(SimCmd::SpawnUnitWithConfig { origin, config, state_id });
        SpawnPromise::new(self.current_frame, state_id)
    }

    #[inline]
    pub fn spawn_unit_with_tile_def(&mut self, origin: Cell, tile_def: &'static TileDef) -> SpawnPromise<Unit> {
        let state_id = self.promises.allocate();
        self.cmds.push(SimCmd::SpawnUnitWithTileDef { origin, tile_def, state_id });
        SpawnPromise::new(self.current_frame, state_id)
    }

    #[inline]
    pub fn despawn_unit_with_id(&mut self, id: UnitId) {
        self.cmds.push(SimCmd::DespawnUnitWithId { id });
    }

    // -- Building operations -------------------

    #[inline]
    pub fn spawn_building_with_tile_def(&mut self, base_cell: Cell, tile_def: &'static TileDef) -> SpawnPromise<Building> {
        let state_id = self.promises.allocate();
        self.cmds.push(SimCmd::SpawnBuildingWithTileDef { base_cell, tile_def, state_id });
        SpawnPromise::new(self.current_frame, state_id)
    }

    #[inline]
    pub fn despawn_building_with_id(&mut self, kind_and_id: BuildingKindAndId) {
        self.cmds.push(SimCmd::DespawnBuildingWithId { kind_and_id });
    }

    // -- Prop operations -----------------------

    #[inline]
    pub fn spawn_prop_with_tile_def(&mut self, origin: Cell, tile_def: &'static TileDef) -> SpawnPromise<Prop> {
        let state_id = self.promises.allocate();
        self.cmds.push(SimCmd::SpawnPropWithTileDef { origin, tile_def, state_id });
        SpawnPromise::new(self.current_frame, state_id)
    }

    #[inline]
    pub fn despawn_prop_with_id(&mut self, id: PropId) {
        self.cmds.push(SimCmd::DespawnPropWithId { id });
    }

    // -- Apply operations ----------------------

    pub fn execute(&mut self, context: &SimContext) {
        if self.cmds.is_empty() {
            return;
        }

        let spawner = Spawner::new(context);

        for cmd in &self.cmds {
            Self::execute_cmd(&mut self.promises, cmd, &spawner);
        }

        self.cmds.clear();

        // All commands for the previous frame have been executed.
        // Spawn Promises for current_frame are now marked as completed.
        self.current_frame += 1;
    }

    fn execute_cmd(promises: &mut SpawnPromiseStatePool, cmd: &SimCmd, spawner: &Spawner) {
        match cmd {
            // --------------
            // Tiles:
            // --------------
            SimCmd::SpawnTileWithTileDef { cell, tile_def, state_id } => {
                let promise = promises.try_get_mut(*state_id)
                    .unwrap_or_else(|| panic!("{cmd}: Invalid SpawnPromiseStateId: {state_id}"));

                debug_assert!(promise.is_pending());

                *promise = match spawner.try_spawn_tile_with_def(*cell, tile_def) {
                    SpawnerResult::Building(building) => {
                        SpawnPromiseState::Ready(SpawnReadyResult::GameObject(building.id()))
                    }
                    SpawnerResult::Unit(unit) => {
                        SpawnPromiseState::Ready(SpawnReadyResult::GameObject(unit.id()))
                    }
                    SpawnerResult::Prop(prop) => {
                        SpawnPromiseState::Ready(SpawnReadyResult::GameObject(prop.id()))
                    }
                    SpawnerResult::Tile(tile) => {
                        SpawnPromiseState::Ready(SpawnReadyResult::Tile(tile.base_cell(), tile.layer_kind()))
                    }
                    SpawnerResult::Err(err) => {
                        SpawnPromiseState::Failed(err)
                    }
                };
            }
            SimCmd::DespawnTileAtCell { cell, layer_kind } => {
                spawner.despawn_tile_at_cell(*cell, *layer_kind);
            }
            // --------------
            // Units:
            // --------------
            SimCmd::SpawnUnitWithConfig { origin, config, state_id } => {
                let promise = promises.try_get_mut(*state_id)
                    .unwrap_or_else(|| panic!("{cmd}: Invalid SpawnPromiseStateId: {state_id}"));

                debug_assert!(promise.is_pending());

                *promise = match spawner.try_spawn_unit_with_config(*origin, *config) {
                    Ok(unit) => SpawnPromiseState::Ready(SpawnReadyResult::GameObject(unit.id())),
                    Err(err) => SpawnPromiseState::Failed(err),
                };
            }
            SimCmd::SpawnUnitWithTileDef { origin, tile_def, state_id } => {
                let promise = promises.try_get_mut(*state_id)
                    .unwrap_or_else(|| panic!("{cmd}: Invalid SpawnPromiseStateId: {state_id}"));

                debug_assert!(promise.is_pending());

                *promise = match spawner.try_spawn_unit_with_tile_def(*origin, tile_def) {
                    Ok(unit) => SpawnPromiseState::Ready(SpawnReadyResult::GameObject(unit.id())),
                    Err(err) => SpawnPromiseState::Failed(err),
                };
            }
            SimCmd::DespawnUnitWithId { id } => {
                spawner.despawn_unit_with_id(*id);
            }
            // --------------
            // Buildings:
            // --------------
            SimCmd::SpawnBuildingWithTileDef { base_cell, tile_def, state_id } => {
                let promise = promises.try_get_mut(*state_id)
                    .unwrap_or_else(|| panic!("{cmd}: Invalid SpawnPromiseStateId: {state_id}"));

                debug_assert!(promise.is_pending());

                *promise = match spawner.try_spawn_building_with_tile_def(*base_cell, tile_def) {
                    Ok(building) => SpawnPromiseState::Ready(SpawnReadyResult::GameObject(building.id())),
                    Err(err) => SpawnPromiseState::Failed(err),
                };
            }
            SimCmd::DespawnBuildingWithId { kind_and_id } => {
                spawner.despawn_building_with_id(*kind_and_id);
            }
            // --------------
            // Props:
            // --------------
            SimCmd::SpawnPropWithTileDef { origin, tile_def, state_id } => {
                let promise = promises.try_get_mut(*state_id)
                    .unwrap_or_else(|| panic!("{cmd}: Invalid SpawnPromiseStateId: {state_id}"));

                debug_assert!(promise.is_pending());

                *promise = match spawner.try_spawn_prop_with_tile_def(*origin, tile_def) {
                    Ok(prop) => SpawnPromiseState::Ready(SpawnReadyResult::GameObject(prop.id())),
                    Err(err) => SpawnPromiseState::Failed(err),
                };
            }
            SimCmd::DespawnPropWithId { id } => {
                spawner.despawn_prop_with_id(*id);
            }
        }
    }
}
