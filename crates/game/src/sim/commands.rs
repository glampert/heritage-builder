use slab::Slab;
use strum::Display;
use smallvec::SmallVec;
use smallbox::{SmallBox, smallbox};

use common::coords::Cell;
use engine::log;

use super::SimContext;
use crate::{
    constants::{SIM_CMDS_CAPACITY, INITIAL_GENERATION},
    prop::{Prop, PropId},
    building::{Building, BuildingKindAndId, BuildingVisitResult},
    unit::{config::UnitConfigKey, Unit, UnitId},
    world::object::{GenerationalIndex, GameObject, Spawner, SpawnerResult},
    tile::{placement::TilePlacementErr, sets::TileDef, Tile, TileMapLayerKind},
};

// ----------------------------------------------
// SpawnPromise
// ----------------------------------------------

#[derive(Clone, Default)]
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

impl<T> std::fmt::Debug for SpawnPromise<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpawnPromise")
            .field("request_frame", &self.request_frame)
            .field("state_id", &self.state_id)
            .finish()
    }
}

// ----------------------------------------------
// SpawnPromiseState Internals
// ----------------------------------------------

#[derive(Copy, Clone, Debug, Default)]
struct SpawnPromiseStateId(GenerationalIndex);

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

impl Default for SpawnPromiseStatePool {
    fn default() -> Self {
        Self { pool: Slab::default(), generation: INITIAL_GENERATION }
    }
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

        let id = SpawnPromiseStateId(GenerationalIndex::new(generation, self.pool.vacant_key()));
        let index = self.pool.insert((id, SpawnPromiseState::Pending));

        debug_assert!(id.0 == self.pool[index].0.0);
        id
    }

    fn free(&mut self, state_id: SpawnPromiseStateId) {
        if !state_id.0.is_valid() {
            return;
        }

        let index = state_id.0.index();

        // Handle freeing an invalid handle gracefully.
        // This will also avoid any invalid frees thanks to the generation check.
        match self.pool.get(index) {
            Some((id, _)) => {
                if id.0 != state_id.0 {
                    return; // Slot reused, not same item.
                }
            }
            None => return, // Already free.
        }

        // Check above guarantees the slot is occupied.
        self.pool.remove(index);
    }

    fn try_get(&self, state_id: SpawnPromiseStateId) -> Option<&SpawnPromiseState> {
        if !state_id.0.is_valid() {
            return None;
        }

        self.pool.get(state_id.0.index())
            .filter(|(id, _)| id.0 == state_id.0)
            .map(|(_, state)| state)
    }

    fn try_get_mut(&mut self, state_id: SpawnPromiseStateId) -> Option<&mut SpawnPromiseState> {
        if !state_id.0.is_valid() {
            return None;
        }

        self.pool.get_mut(state_id.0.index())
            .filter(|(id, _)| id.0 == state_id.0)
            .map(|(_, state)| state)
    }

    fn debug_leak_check(&self) {
        if self.pool.is_empty() {
            return;
        }

        log::error!("----------------------------");
        log::error!("  SPAWN PROMISE POOL LEAKS  ");
        log::error!("----------------------------");

        for (index, (id, state)) in &self.pool {
            log::error!("Leaked SpawnPromiseState[{index}]: {}, {}", id.0, state);
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

// Detect any leaked instances upon destruction.
impl Drop for SpawnPromiseStatePool {
    fn drop(&mut self) {
        self.debug_leak_check();
    }
}

// ----------------------------------------------
// SimCmd
// ----------------------------------------------

enum SimCmd {
    // -- Tile operations -----------------------
    SpawnTileWithTileDef {
        cell: Cell,
        tile_def: &'static TileDef,
        state_id: Option<SpawnPromiseStateId>,
        on_spawned: SpawnCallbackBox<TileSpawnedCallback>,
    },
    DespawnTileAtCell {
        cell: Cell,
        layer_kind: TileMapLayerKind,
    },

    // -- Unit operations -----------------------
    SpawnUnitWithConfig {
        origin: Cell,
        config: UnitConfigKey,
        state_id: Option<SpawnPromiseStateId>,
        on_spawned: SpawnCallbackBox<GameObjectSpawnedCallback<Unit>>,
    },
    SpawnUnitWithTileDef {
        origin: Cell,
        tile_def: &'static TileDef,
        state_id: Option<SpawnPromiseStateId>,
        on_spawned: SpawnCallbackBox<GameObjectSpawnedCallback<Unit>>,
    },
    DespawnUnitWithId {
        id: UnitId,
    },

    // -- Building operations -------------------
    SpawnBuildingWithTileDef {
        base_cell: Cell,
        tile_def: &'static TileDef,
        state_id: Option<SpawnPromiseStateId>,
        on_spawned: SpawnCallbackBox<GameObjectSpawnedCallback<Building>>,
    },
    DespawnBuildingWithId {
        kind_and_id: BuildingKindAndId,
    },
    VisitBuilding {
        kind_and_id: BuildingKindAndId,
        unit_id: UnitId,
        on_post_visit: SpawnCallbackBox<BuildingVisitedCallback>,
    },
    DeferBuildingUpdate {
        kind_and_id: BuildingKindAndId,
        callback: SpawnCallbackBox<GameObjectDeferredCallback<Building>>,
    },

    // -- Prop operations -----------------------
    SpawnPropWithTileDef {
        origin: Cell,
        tile_def: &'static TileDef,
        state_id: Option<SpawnPromiseStateId>,
        on_spawned: SpawnCallbackBox<GameObjectSpawnedCallback<Prop>>,
    },
    DespawnPropWithId {
        id: PropId,
    },
}

// ----------------------------------------------
// Internal callback signatures
// ----------------------------------------------

// Inline storage budget for boxed spawn callbacks. S8 = 8 machine words (64B on 64-bit),
// chosen to fit closures that capture a task id plus a small user closure without
// silently spilling to the heap.
type SpawnCallbackBox<F> = SmallBox<F, smallbox::space::S8>;

// Game object callback: receives `&mut T` so the closure can initialize the
// freshly-spawned object before it goes live (e.g. assigning a task to a new Unit).
type GameObjectSpawnedCallback<T> = dyn Fn(&SimContext, Result<&mut T, TilePlacementErr>) + 'static;

// Tile placement callback: receives borrowed refs to a small data enum.
// No underlying mutable game object exists, so the borrowed shape avoids
// the need to clone TilePlacementErr in the failure path.
type TileSpawnedCallback = dyn Fn(&SimContext, Result<SpawnReadyResult, TilePlacementErr>) + 'static;

// Generic deferred update callback for units/buildings/props.
type GameObjectDeferredCallback<T> = dyn Fn(&SimContext, &mut T);

// Optional post building visit callback. Receives the same arguments as Building::visited_by
type BuildingVisitedCallback = dyn Fn(&SimContext, &mut Building, &mut Unit, BuildingVisitResult);

// ----------------------------------------------
// SimCmds
// ----------------------------------------------

// Deferred command queue populated during simulation updates.
// Any world or tile map modification is done via a deferred command.
// Commands are applied after all game objects have been updated.
#[derive(Default)]
pub struct SimCmds {
    current_frame: usize,
    promises: SpawnPromiseStatePool,
    cmds: SmallVec<[SimCmd; SIM_CMDS_CAPACITY]>,
}

impl SimCmds {
    pub fn new() -> Self {
        Self {
            current_frame: 0,
            promises: SpawnPromiseStatePool::new(SIM_CMDS_CAPACITY),
            cmds: SmallVec::new(),
        }
    }

    pub fn reset(&mut self) {
        // Outstanding promises would dangle across a reset: their state slots are
        // wiped and the generation counter restarts, so a stale handle could even
        // collide with a freshly-allocated one. Callers must drop or query all
        // promises before resetting.
        if !self.promises.pool.is_empty() {
            log::error!(log::channel!("sim"), "SimCmds::reset() called with outstanding spawn promises!");
            self.promises.debug_leak_check();
        }

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
    pub fn is_promise_resolved<T>(&self, promise: &SpawnPromise<T>) -> bool {
        // True if the spawn command was executed. It may have succeeded or failed.
        self.current_frame > promise.request_frame
    }

    #[must_use]
    pub fn query_promise<T>(&mut self, promise: SpawnPromise<T>) -> SpawnQueryResult<T> {
        if !self.is_promise_resolved(&promise) {
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
                        // Shouldn't happen if the is_promise_resolved() test above passed...
                        log::error!(log::channel!("sim"), "Unexpected SpawnPromiseState::Pending for {}!", state_id.0);
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

    pub fn discard_promise<T>(&mut self, promise: SpawnPromise<T>) {
        // Free the promise state without checking for completion.
        self.promises.free(promise.state_id);
    }

    // -- No callback sentinel values -----------

    // No-op tile spawn callback.
    #[inline]
    pub fn no_tile_callback() -> fn(&SimContext, Result<SpawnReadyResult, TilePlacementErr>) {
        |_ctx, _result| {}
    }

    // No-op game object spawn callback. `T` is inferred from the surrounding spawn call.
    #[inline]
    pub fn no_object_callback<T>() -> fn(&SimContext, Result<&mut T, TilePlacementErr>) {
        |_ctx, _result| {}
    }

    // -- Tile operations -----------------------

    #[inline]
    #[must_use]
    pub fn spawn_tile_with_tile_def_promise<F>(&mut self, cell: Cell, tile_def: &'static TileDef, on_spawned: F) -> SpawnPromise<Tile>
    where
        F: Fn(&SimContext, Result<SpawnReadyResult, TilePlacementErr>) + 'static
    {
        let state_id = self.promises.allocate();
        self.cmds.push(SimCmd::SpawnTileWithTileDef { cell, tile_def, state_id: Some(state_id), on_spawned: smallbox!(on_spawned) });
        SpawnPromise::new(self.current_frame, state_id)
    }

    #[inline]
    pub fn spawn_tile_with_tile_def_cb<F>(&mut self, cell: Cell, tile_def: &'static TileDef, on_spawned: F)
    where
        F: Fn(&SimContext, Result<SpawnReadyResult, TilePlacementErr>) + 'static
    {
        self.cmds.push(SimCmd::SpawnTileWithTileDef { cell, tile_def, state_id: None, on_spawned: smallbox!(on_spawned) });
    }

    #[inline]
    pub fn despawn_tile_at_cell(&mut self, cell: Cell, layer_kind: TileMapLayerKind) {
        self.cmds.push(SimCmd::DespawnTileAtCell { cell, layer_kind });
    }

    // -- Unit operations -----------------------

    #[inline]
    #[must_use]
    pub fn spawn_unit_with_config_promise<F>(&mut self, origin: Cell, config: UnitConfigKey, on_spawned: F) -> SpawnPromise<Unit>
    where
        F: Fn(&SimContext, Result<&mut Unit, TilePlacementErr>) + 'static
    {
        let state_id = self.promises.allocate();
        self.cmds.push(SimCmd::SpawnUnitWithConfig { origin, config, state_id: Some(state_id), on_spawned: smallbox!(on_spawned) });
        SpawnPromise::new(self.current_frame, state_id)
    }

    #[inline]
    pub fn spawn_unit_with_config_cb<F>(&mut self, origin: Cell, config: UnitConfigKey, on_spawned: F)
    where
        F: Fn(&SimContext, Result<&mut Unit, TilePlacementErr>) + 'static
    {
        self.cmds.push(SimCmd::SpawnUnitWithConfig { origin, config, state_id: None, on_spawned: smallbox!(on_spawned) });
    }

    #[inline]
    #[must_use]
    pub fn spawn_unit_with_tile_def_promise<F>(&mut self, origin: Cell, tile_def: &'static TileDef, on_spawned: F) -> SpawnPromise<Unit>
    where
        F: Fn(&SimContext, Result<&mut Unit, TilePlacementErr>) + 'static
    {
        let state_id = self.promises.allocate();
        self.cmds.push(SimCmd::SpawnUnitWithTileDef { origin, tile_def, state_id: Some(state_id), on_spawned: smallbox!(on_spawned) });
        SpawnPromise::new(self.current_frame, state_id)
    }

    #[inline]
    pub fn spawn_unit_with_tile_def_cb<F>(&mut self, origin: Cell, tile_def: &'static TileDef, on_spawned: F)
    where
        F: Fn(&SimContext, Result<&mut Unit, TilePlacementErr>) + 'static
    {
        self.cmds.push(SimCmd::SpawnUnitWithTileDef { origin, tile_def, state_id: None, on_spawned: smallbox!(on_spawned) });
    }

    #[inline]
    pub fn despawn_unit_with_id(&mut self, id: UnitId) {
        self.cmds.push(SimCmd::DespawnUnitWithId { id });
    }

    // -- Building operations -------------------

    #[inline]
    #[must_use]
    pub fn spawn_building_with_tile_def_promise<F>(&mut self, base_cell: Cell, tile_def: &'static TileDef, on_spawned: F) -> SpawnPromise<Building>
    where
        F: Fn(&SimContext, Result<&mut Building, TilePlacementErr>) + 'static
    {
        let state_id = self.promises.allocate();
        self.cmds.push(SimCmd::SpawnBuildingWithTileDef { base_cell, tile_def, state_id: Some(state_id), on_spawned: smallbox!(on_spawned) });
        SpawnPromise::new(self.current_frame, state_id)
    }

    #[inline]
    pub fn spawn_building_with_tile_def_cb<F>(&mut self, base_cell: Cell, tile_def: &'static TileDef, on_spawned: F)
    where
        F: Fn(&SimContext, Result<&mut Building, TilePlacementErr>) + 'static
    {
        self.cmds.push(SimCmd::SpawnBuildingWithTileDef { base_cell, tile_def, state_id: None, on_spawned: smallbox!(on_spawned) });
    }

    #[inline]
    pub fn despawn_building_with_id(&mut self, kind_and_id: BuildingKindAndId) {
        self.cmds.push(SimCmd::DespawnBuildingWithId { kind_and_id });
    }

    #[inline]
    pub fn visit_building(&mut self, kind_and_id: BuildingKindAndId, unit_id: UnitId) {
        // No user defined completion callback.
        fn empty_cb(_ctx: &SimContext, _building: &mut Building, _unit: &mut Unit, _result: BuildingVisitResult) {}
        self.cmds.push(SimCmd::VisitBuilding { kind_and_id, unit_id, on_post_visit: smallbox!(empty_cb) });
    }

    #[inline]
    pub fn visit_building_with_cb<F>(&mut self, kind_and_id: BuildingKindAndId, unit_id: UnitId, on_post_visit: F)
    where
        F: Fn(&SimContext, &mut Building, &mut Unit, BuildingVisitResult) + 'static
    {
        self.cmds.push(SimCmd::VisitBuilding { kind_and_id, unit_id, on_post_visit: smallbox!(on_post_visit) });
    }

    #[inline]
    pub fn defer_building_update<F>(&mut self, kind_and_id: BuildingKindAndId, callback: F)
    where
        F: Fn(&SimContext, &mut Building) + 'static
    {
        self.cmds.push(SimCmd::DeferBuildingUpdate { kind_and_id, callback: smallbox!(callback) });
    }

    // -- Prop operations -----------------------

    #[inline]
    #[must_use]
    pub fn spawn_prop_with_tile_def_promise<F>(&mut self, origin: Cell, tile_def: &'static TileDef, on_spawned: F) -> SpawnPromise<Prop>
    where
        F: Fn(&SimContext, Result<&mut Prop, TilePlacementErr>) + 'static
    {
        let state_id = self.promises.allocate();
        self.cmds.push(SimCmd::SpawnPropWithTileDef { origin, tile_def, state_id: Some(state_id), on_spawned: smallbox!(on_spawned) });
        SpawnPromise::new(self.current_frame, state_id)
    }

    #[inline]
    pub fn spawn_prop_with_tile_def_cb<F>(&mut self, origin: Cell, tile_def: &'static TileDef, on_spawned: F)
    where
        F: Fn(&SimContext, Result<&mut Prop, TilePlacementErr>) + 'static
    {
        self.cmds.push(SimCmd::SpawnPropWithTileDef { origin, tile_def, state_id: None, on_spawned: smallbox!(on_spawned) });
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
            Self::execute_cmd(&mut self.promises, cmd, context, &spawner);
        }

        self.cmds.clear();

        // All commands for the previous frame have been executed.
        // Spawn Promises for current_frame are now marked as completed.
        self.current_frame += 1;
    }

    fn execute_cmd(promises: &mut SpawnPromiseStatePool, cmd: &SimCmd, context: &SimContext, spawner: &Spawner) {
        match cmd {
            // --------------
            // Tiles:
            // --------------
            SimCmd::SpawnTileWithTileDef { cell, tile_def, state_id, on_spawned } => {
                let result = match spawner.try_spawn_tile_with_def(*cell, tile_def) {
                    SpawnerResult::Building(b) => Ok(SpawnReadyResult::GameObject(b.id())),
                    SpawnerResult::Unit(u)     => Ok(SpawnReadyResult::GameObject(u.id())),
                    SpawnerResult::Prop(p)     => Ok(SpawnReadyResult::GameObject(p.id())),
                    SpawnerResult::Tile(t)     => Ok(SpawnReadyResult::Tile(t.base_cell(), t.layer_kind())),
                    SpawnerResult::Err(err)    => Err(err),
                };

                if let Some(state_id) = state_id {
                    let promise = promises.try_get_mut(*state_id)
                        .unwrap_or_else(|| panic!("SpawnTileWithTileDef: Invalid SpawnPromiseStateId: {}", state_id.0));

                    debug_assert!(promise.is_pending());

                    *promise = match &result {
                        Ok(ready) => SpawnPromiseState::Ready(ready.clone()),
                        Err(err)  => SpawnPromiseState::Failed(err.clone()),
                    };
                }

                on_spawned(context, result);
            }
            SimCmd::DespawnTileAtCell { cell, layer_kind } => {
                spawner.despawn_tile_at_cell(*cell, *layer_kind);
            }

            // --------------
            // Units:
            // --------------
            SimCmd::SpawnUnitWithConfig { origin, config, state_id, on_spawned } => {
                let result = spawner.try_spawn_unit_with_config(*origin, *config);
                Self::resolve_game_object_spawn(promises, state_id, on_spawned, context, result);
            }
            SimCmd::SpawnUnitWithTileDef { origin, tile_def, state_id, on_spawned } => {
                let result = spawner.try_spawn_unit_with_tile_def(*origin, tile_def);
                Self::resolve_game_object_spawn(promises, state_id, on_spawned, context, result);
            }
            SimCmd::DespawnUnitWithId { id } => {
                spawner.despawn_unit_with_id(*id);
            }

            // --------------
            // Buildings:
            // --------------
            SimCmd::SpawnBuildingWithTileDef { base_cell, tile_def, state_id, on_spawned } => {
                let result = spawner.try_spawn_building_with_tile_def(*base_cell, tile_def);
                Self::resolve_game_object_spawn(promises, state_id, on_spawned, context, result);
            }
            SimCmd::DespawnBuildingWithId { kind_and_id } => {
                spawner.despawn_building_with_id(*kind_and_id);
            }
            SimCmd::VisitBuilding { kind_and_id, unit_id, on_post_visit } => {
                let building = context.world_mut()
                    .find_building_mut(kind_and_id.kind, kind_and_id.id)
                    .unwrap_or_else(|| panic!("SimCmd::VisitBuilding invalid building kind/id: {} {}", kind_and_id.kind, kind_and_id.id));

                let unit = context.world_mut()
                    .find_unit_mut(*unit_id)
                    .unwrap_or_else(|| panic!("SimCmd::VisitBuilding invalid unit id: {unit_id}"));

                let result = building.visited_by(unit, context);

                // Optional post visit user callback.
                on_post_visit(context, building, unit, result);
            }
            SimCmd::DeferBuildingUpdate { kind_and_id, callback } => {
                let building = context.world_mut()
                    .find_building_mut(kind_and_id.kind, kind_and_id.id)
                    .unwrap_or_else(|| panic!("SimCmd::DeferBuildingUpdate invalid building kind/id: {} {}", kind_and_id.kind, kind_and_id.id));

                callback(context, building);
            }

            // --------------
            // Props:
            // --------------
            SimCmd::SpawnPropWithTileDef { origin, tile_def, state_id, on_spawned } => {
                let result = spawner.try_spawn_prop_with_tile_def(*origin, tile_def);
                Self::resolve_game_object_spawn(promises, state_id, on_spawned, context, result);
            }
            SimCmd::DespawnPropWithId { id } => {
                spawner.despawn_prop_with_id(*id);
            }
        }
    }

    // Shared resolution path for Unit/Building/Prop spawn commands.
    // Either updates the SpawnPromise slot with the result, or invokes the
    // user callback with the borrowed mutable reference to initialize the new object.
    fn resolve_game_object_spawn<T: GameObject>(
        promises: &mut SpawnPromiseStatePool,
        state_id: &Option<SpawnPromiseStateId>,
        on_spawned: &SpawnCallbackBox<GameObjectSpawnedCallback<T>>,
        context: &SimContext,
        result: Result<&mut T, TilePlacementErr>,
    ) {
        if let Some(state_id) = state_id {
            let promise = promises.try_get_mut(*state_id)
                .unwrap_or_else(|| panic!("Invalid SpawnPromiseStateId: {}", state_id.0));

            debug_assert!(promise.is_pending());

            *promise = match &result {
                Ok(obj)  => SpawnPromiseState::Ready(SpawnReadyResult::GameObject(obj.id())),
                Err(err) => SpawnPromiseState::Failed(err.clone()),
            };
        }

        on_spawned(context, result);
    }
}
