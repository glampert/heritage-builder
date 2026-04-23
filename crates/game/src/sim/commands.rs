use slab::Slab;
use strum::Display;
use smallvec::SmallVec;
use smallbox::{SmallBox, smallbox};

use common::coords::Cell;
use engine::{log, platform::DebugBacktrace};

use super::SimContext;
use crate::{
    constants::{SIM_CMDS_CAPACITY, INITIAL_GENERATION},
    prop::{Prop, PropId},
    unit::{config::UnitConfigKey, Unit, UnitId},
    building::{Building, BuildingKindAndId, BuildingVisitResult, HouseUpgradeDirection},
    world::object::{GenerationalIndex, GameObject, Spawner, SpawnerResult},
    tile::{placement::TilePlacementErr, sets::TileDef, Tile, TileKind, TileMapLayerKind},
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

#[derive(Display)]
enum SimCmd {
    // -- Tile operations -----------------------
    SpawnTileWithTileDef {
        cell: Cell,
        tile_def: &'static TileDef,
        state_id: Option<SpawnPromiseStateId>,
        on_spawned: CallbackBox<TileSpawnedCallback>,
    },
    DespawnTileAtCell {
        cell: Cell,
        layer_kind: TileMapLayerKind,
    },
    DeferTileUpdate {
        cell: Cell,
        kind: TileKind,
        callback: CallbackBox<DeferredCallback<Tile>>,
    },

    // -- Unit operations -----------------------
    SpawnUnitWithConfig {
        origin: Cell,
        config: UnitConfigKey,
        state_id: Option<SpawnPromiseStateId>,
        on_spawned: CallbackBox<GameObjectSpawnedCallback<Unit>>,
    },
    SpawnUnitWithTileDef {
        origin: Cell,
        tile_def: &'static TileDef,
        state_id: Option<SpawnPromiseStateId>,
        on_spawned: CallbackBox<GameObjectSpawnedCallback<Unit>>,
    },
    DespawnUnitWithId {
        id: UnitId,
    },
    DeferUnitUpdate {
        id: UnitId,
        callback: CallbackBox<DeferredCallback<Unit>>,
    },

    // -- Building operations -------------------
    SpawnBuildingWithTileDef {
        base_cell: Cell,
        tile_def: &'static TileDef,
        state_id: Option<SpawnPromiseStateId>,
        on_spawned: CallbackBox<GameObjectSpawnedCallback<Building>>,
    },
    DespawnBuildingWithId {
        kind_and_id: BuildingKindAndId,
    },
    VisitBuilding {
        kind_and_id: BuildingKindAndId,
        unit_id: UnitId,
        on_post_visit: Option<CallbackBox<BuildingVisitedCallback>>,
    },
    DeferBuildingTaskStep {
        kind_and_id: BuildingKindAndId,
        unit_id: UnitId,
        callback: CallbackBox<BuildingTaskCallback>,
        // Optional callback invoked after the main one, e.g. to notify
        // the owning task that the deferred callback has been executed.
        on_complete: Option<CallbackBox<BuildingTaskCallback>>,
    },
    DeferBuildingUpdate {
        kind_and_id: BuildingKindAndId,
        callback: CallbackBox<DeferredCallback<Building>>,
    },
    UpgradeHouse {
        kind_and_id: BuildingKindAndId,
        dir: HouseUpgradeDirection,
    },

    // -- Prop operations -----------------------
    SpawnPropWithTileDef {
        origin: Cell,
        tile_def: &'static TileDef,
        state_id: Option<SpawnPromiseStateId>,
        on_spawned: CallbackBox<GameObjectSpawnedCallback<Prop>>,
    },
    DespawnPropWithId {
        id: PropId,
    },
    DeferPropUpdate {
        id: PropId,
        callback: CallbackBox<DeferredCallback<Prop>>,
    },
}

impl SimCmd {
    // Command types we delay execution until after all other commands have been executed.
    #[inline]
    fn is_delayed_execution(&self) -> bool {
        // UpgradeHouse:
        // - Executes after all other commands because it may trigger multiple building despawns and house mergers.
        matches!(self, Self::UpgradeHouse { .. })
    }
}

type QueuedSimCmd = QueuedSimCmdNoBacktrace;

// ----------------------------------------------
// QueuedSimCmdWithBacktrace (WITH DebugBacktrace)
// ----------------------------------------------

struct QueuedSimCmdWithBacktrace {
    cmd: SimCmd,
    backtrace: DebugBacktrace,
}

impl QueuedSimCmdWithBacktrace {
    #[inline]
    fn new(cmd: SimCmd) -> Self {
        Self { cmd, backtrace: DebugBacktrace::capture() }
    }

    #[cold]
    fn error_panic<S: AsRef<str> + std::fmt::Display>(&self, message: S) -> ! {
        let cmd = &self.cmd;

        // NOTE: Skip SimCmds internal methods + DebugBacktrace boilerplate.
        let skip_top = 7;
        let skip_bottom = 6;
        let backtrace_str = self.backtrace.to_string(skip_top, skip_bottom);

        panic!("\n\
            ---------------------------------------\n\
            ERROR: {cmd}\n\
            ---------------------------------------\n\
            {message}\n\
            BACKTRACE:\n\
            {backtrace_str}\n\
        ");
    }
}

// ----------------------------------------------
// QueuedSimCmdNoBacktrace (WITHOUT DebugBacktrace)
// ----------------------------------------------

struct QueuedSimCmdNoBacktrace {
    cmd: SimCmd,
}

impl QueuedSimCmdNoBacktrace {
    #[inline]
    fn new(cmd: SimCmd) -> Self {
        Self { cmd }
    }

    #[cold]
    fn error_panic<S: AsRef<str> + std::fmt::Display>(&self, message: S) -> ! {
        let cmd = &self.cmd;
        panic!("\n\
            ---------------------------------------\n\
            ERROR: {cmd}\n\
            ---------------------------------------\n\
            {message}\n\
        ");
    }
}

// ----------------------------------------------
// Internal callback signatures
// ----------------------------------------------

// Inline storage budget for boxed callbacks. S8 = 8 machine words (64B on 64-bit),
// chosen to fit closures that capture an object id plus a small user closure without
// silently spilling to the heap.
type CallbackBox<F> = SmallBox<F, smallbox::space::S8>;

// Game object callback: receives `&mut T` so the closure can initialize the
// freshly-spawned object before it goes live (e.g. assigning a task to a new Unit).
type GameObjectSpawnedCallback<T> = dyn Fn(&SimContext, Result<&mut T, TilePlacementErr>) + 'static;

// Tile placement callback: receives borrowed refs to a small data enum.
// No underlying mutable game object exists, so the borrowed shape avoids
// the need to clone TilePlacementErr in the failure path.
type TileSpawnedCallback = dyn Fn(&SimContext, Result<SpawnReadyResult, TilePlacementErr>) + 'static;

// Generic deferred update callback for tiles/units/buildings/props.
type DeferredCallback<T> = dyn Fn(&SimContext, &mut T);

// Optional post building visit callback. Receives the same arguments as Building::visited_by
type BuildingVisitedCallback = dyn Fn(&SimContext, &mut Building, &mut Unit, BuildingVisitResult);

// Task completion callback for building + unit.
type BuildingTaskCallback = dyn Fn(&SimContext, &mut Building, &mut Unit);

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
    cmds: SmallVec<[QueuedSimCmd; SIM_CMDS_CAPACITY]>,
}

impl SimCmds {
    pub fn new() -> Self {
        Self {
            current_frame: 0,
            promises: SpawnPromiseStatePool::new(SIM_CMDS_CAPACITY),
            cmds: SmallVec::new(),
        }
    }

    pub(super) fn reset(&mut self) {
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

    pub(super) fn pre_load(&mut self) {
        self.reset();
    }

    pub(super) fn post_load(&mut self) {
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
        self.push_cmd(SimCmd::SpawnTileWithTileDef { cell, tile_def, state_id: Some(state_id), on_spawned: smallbox!(on_spawned) });
        SpawnPromise::new(self.current_frame, state_id)
    }

    #[inline]
    pub fn spawn_tile_with_tile_def_cb<F>(&mut self, cell: Cell, tile_def: &'static TileDef, on_spawned: F)
    where
        F: Fn(&SimContext, Result<SpawnReadyResult, TilePlacementErr>) + 'static
    {
        self.push_cmd(SimCmd::SpawnTileWithTileDef { cell, tile_def, state_id: None, on_spawned: smallbox!(on_spawned) });
    }

    #[inline]
    pub fn despawn_tile_at_cell(&mut self, cell: Cell, layer_kind: TileMapLayerKind) {
        self.push_cmd(SimCmd::DespawnTileAtCell { cell, layer_kind });
    }

    #[inline]
    pub fn defer_tile_update<F>(&mut self, cell: Cell, kind: TileKind, callback: F)
    where
        F: Fn(&SimContext, &mut Tile) + 'static
    {
        self.push_cmd(SimCmd::DeferTileUpdate { cell, kind, callback: smallbox!(callback) });
    }

    // -- Unit operations -----------------------

    #[inline]
    #[must_use]
    pub fn spawn_unit_with_config_promise<F>(&mut self, origin: Cell, config: UnitConfigKey, on_spawned: F) -> SpawnPromise<Unit>
    where
        F: Fn(&SimContext, Result<&mut Unit, TilePlacementErr>) + 'static
    {
        let state_id = self.promises.allocate();
        self.push_cmd(SimCmd::SpawnUnitWithConfig { origin, config, state_id: Some(state_id), on_spawned: smallbox!(on_spawned) });
        SpawnPromise::new(self.current_frame, state_id)
    }

    #[inline]
    pub fn spawn_unit_with_config_cb<F>(&mut self, origin: Cell, config: UnitConfigKey, on_spawned: F)
    where
        F: Fn(&SimContext, Result<&mut Unit, TilePlacementErr>) + 'static
    {
        self.push_cmd(SimCmd::SpawnUnitWithConfig { origin, config, state_id: None, on_spawned: smallbox!(on_spawned) });
    }

    #[inline]
    #[must_use]
    pub fn spawn_unit_with_tile_def_promise<F>(&mut self, origin: Cell, tile_def: &'static TileDef, on_spawned: F) -> SpawnPromise<Unit>
    where
        F: Fn(&SimContext, Result<&mut Unit, TilePlacementErr>) + 'static
    {
        let state_id = self.promises.allocate();
        self.push_cmd(SimCmd::SpawnUnitWithTileDef { origin, tile_def, state_id: Some(state_id), on_spawned: smallbox!(on_spawned) });
        SpawnPromise::new(self.current_frame, state_id)
    }

    #[inline]
    pub fn spawn_unit_with_tile_def_cb<F>(&mut self, origin: Cell, tile_def: &'static TileDef, on_spawned: F)
    where
        F: Fn(&SimContext, Result<&mut Unit, TilePlacementErr>) + 'static
    {
        self.push_cmd(SimCmd::SpawnUnitWithTileDef { origin, tile_def, state_id: None, on_spawned: smallbox!(on_spawned) });
    }

    #[inline]
    pub fn despawn_unit_with_id(&mut self, id: UnitId) {
        self.push_cmd(SimCmd::DespawnUnitWithId { id });
    }

    #[inline]
    pub fn defer_unit_update<F>(&mut self, id: UnitId, callback: F)
    where
        F: Fn(&SimContext, &mut Unit) + 'static
    {
        self.push_cmd(SimCmd::DeferUnitUpdate { id, callback: smallbox!(callback) });
    }

    // -- Building operations -------------------

    #[inline]
    #[must_use]
    pub fn spawn_building_with_tile_def_promise<F>(&mut self, base_cell: Cell, tile_def: &'static TileDef, on_spawned: F) -> SpawnPromise<Building>
    where
        F: Fn(&SimContext, Result<&mut Building, TilePlacementErr>) + 'static
    {
        let state_id = self.promises.allocate();
        self.push_cmd(SimCmd::SpawnBuildingWithTileDef { base_cell, tile_def, state_id: Some(state_id), on_spawned: smallbox!(on_spawned) });
        SpawnPromise::new(self.current_frame, state_id)
    }

    #[inline]
    pub fn spawn_building_with_tile_def_cb<F>(&mut self, base_cell: Cell, tile_def: &'static TileDef, on_spawned: F)
    where
        F: Fn(&SimContext, Result<&mut Building, TilePlacementErr>) + 'static
    {
        self.push_cmd(SimCmd::SpawnBuildingWithTileDef { base_cell, tile_def, state_id: None, on_spawned: smallbox!(on_spawned) });
    }

    #[inline]
    pub fn despawn_building_with_id(&mut self, kind_and_id: BuildingKindAndId) {
        self.push_cmd(SimCmd::DespawnBuildingWithId { kind_and_id });
    }

    #[inline]
    pub fn visit_building(&mut self, kind_and_id: BuildingKindAndId, unit_id: UnitId) {
        // Without user-specified completion callback.
        self.push_cmd(SimCmd::VisitBuilding { kind_and_id, unit_id, on_post_visit: None });
    }

    #[inline]
    pub fn visit_building_with_completion<F>(&mut self, kind_and_id: BuildingKindAndId, unit_id: UnitId, on_post_visit: F)
    where
        F: Fn(&SimContext, &mut Building, &mut Unit, BuildingVisitResult) + 'static
    {
        self.push_cmd(SimCmd::VisitBuilding { kind_and_id, unit_id, on_post_visit: Some(smallbox!(on_post_visit)) });
    }

    #[inline]
    pub fn defer_task_step<F>(&mut self, kind_and_id: BuildingKindAndId, unit_id: UnitId, callback: F)
    where
        F: Fn(&SimContext, &mut Building, &mut Unit) + 'static
    {
        self.push_cmd(SimCmd::DeferBuildingTaskStep { kind_and_id, unit_id, callback: smallbox!(callback), on_complete: None });
    }

    #[inline]
    pub fn defer_task_step_with_completion<F>(&mut self, kind_and_id: BuildingKindAndId, unit_id: UnitId, callback: F, on_complete: F)
    where
        F: Fn(&SimContext, &mut Building, &mut Unit) + 'static
    {
        self.push_cmd(SimCmd::DeferBuildingTaskStep { kind_and_id, unit_id, callback: smallbox!(callback), on_complete: Some(smallbox!(on_complete)) });
    }

    #[inline]
    pub fn defer_building_update<F>(&mut self, kind_and_id: BuildingKindAndId, callback: F)
    where
        F: Fn(&SimContext, &mut Building) + 'static
    {
        self.push_cmd(SimCmd::DeferBuildingUpdate { kind_and_id, callback: smallbox!(callback) });
    }

    #[inline]
    pub fn upgrade_house(&mut self, kind_and_id: BuildingKindAndId, dir: HouseUpgradeDirection) {
        self.push_cmd(SimCmd::UpgradeHouse { kind_and_id, dir });
    }

    // -- Prop operations -----------------------

    #[inline]
    #[must_use]
    pub fn spawn_prop_with_tile_def_promise<F>(&mut self, origin: Cell, tile_def: &'static TileDef, on_spawned: F) -> SpawnPromise<Prop>
    where
        F: Fn(&SimContext, Result<&mut Prop, TilePlacementErr>) + 'static
    {
        let state_id = self.promises.allocate();
        self.push_cmd(SimCmd::SpawnPropWithTileDef { origin, tile_def, state_id: Some(state_id), on_spawned: smallbox!(on_spawned) });
        SpawnPromise::new(self.current_frame, state_id)
    }

    #[inline]
    pub fn spawn_prop_with_tile_def_cb<F>(&mut self, origin: Cell, tile_def: &'static TileDef, on_spawned: F)
    where
        F: Fn(&SimContext, Result<&mut Prop, TilePlacementErr>) + 'static
    {
        self.push_cmd(SimCmd::SpawnPropWithTileDef { origin, tile_def, state_id: None, on_spawned: smallbox!(on_spawned) });
    }

    #[inline]
    pub fn despawn_prop_with_id(&mut self, id: PropId) {
        self.push_cmd(SimCmd::DespawnPropWithId { id });
    }

    #[inline]
    pub fn defer_prop_update<F>(&mut self, id: PropId, callback: F)
    where
        F: Fn(&SimContext, &mut Prop) + 'static
    {
        self.push_cmd(SimCmd::DeferPropUpdate { id, callback: smallbox!(callback) });
    }

    // -- Apply operations ----------------------

    pub fn execute(&mut self, context: &SimContext) {
        if self.cmds.is_empty() {
            return;
        }

        let spawner = Spawner::new(context);
        let mut delayed_cmds = SmallVec::<[&QueuedSimCmd; SIM_CMDS_CAPACITY]>::new();

        for queued in &self.cmds {
            if queued.cmd.is_delayed_execution() {
                // Delay till all other commands are executed.
                delayed_cmds.push(queued);
                continue;
            }

            Self::execute_cmd(&mut self.promises, queued, context, &spawner);
        }

        // Run delayed commands now:
        for queued in delayed_cmds {
            Self::execute_cmd(&mut self.promises, queued, context, &spawner);
        }

        self.cmds.clear();

        // All commands for the previous frame have been executed.
        // Spawn Promises for current_frame are now marked as completed.
        self.current_frame += 1;
    }

    fn execute_cmd(promises: &mut SpawnPromiseStatePool, queued: &QueuedSimCmd, context: &SimContext, spawner: &Spawner) {
        match &queued.cmd {
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
                        .unwrap_or_else(|| queued.error_panic(format!("Invalid SpawnPromiseStateId: {}", state_id.0)));

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
            SimCmd::DeferTileUpdate { cell, kind, callback } => {
                let tile = context.find_tile_mut(*cell, *kind)
                    .unwrap_or_else(|| queued.error_panic(format!("Invalid tile cell/kind: {cell} {kind}")));

                callback(context, tile);
            }

            // --------------
            // Units:
            // --------------
            SimCmd::SpawnUnitWithConfig { origin, config, state_id, on_spawned } => {
                let result = spawner.try_spawn_unit_with_config(*origin, *config);
                Self::resolve_game_object_spawn(promises, queued, state_id, on_spawned, context, result);
            }
            SimCmd::SpawnUnitWithTileDef { origin, tile_def, state_id, on_spawned } => {
                let result = spawner.try_spawn_unit_with_tile_def(*origin, tile_def);
                Self::resolve_game_object_spawn(promises, queued, state_id, on_spawned, context, result);
            }
            SimCmd::DespawnUnitWithId { id } => {
                spawner.despawn_unit_with_id(*id);
            }
            SimCmd::DeferUnitUpdate { id, callback } => {
                let unit = context
                    .find_unit_mut(*id)
                    .unwrap_or_else(|| queued.error_panic(format!("Invalid unit id: {id}")));

                callback(context, unit);
            }

            // --------------
            // Buildings:
            // --------------
            SimCmd::SpawnBuildingWithTileDef { base_cell, tile_def, state_id, on_spawned } => {
                let result = spawner.try_spawn_building_with_tile_def(*base_cell, tile_def);
                Self::resolve_game_object_spawn(promises, queued, state_id, on_spawned, context, result);
            }
            SimCmd::DespawnBuildingWithId { kind_and_id } => {
                spawner.despawn_building_with_id(*kind_and_id);
            }
            SimCmd::VisitBuilding { kind_and_id, unit_id, on_post_visit } => {
                let building = context
                    .find_building_mut(kind_and_id.kind, kind_and_id.id)
                    .unwrap_or_else(|| queued.error_panic(format!("Invalid building kind/id: {} {}", kind_and_id.kind, kind_and_id.id)));

                let unit = context
                    .find_unit_mut(*unit_id)
                    .unwrap_or_else(|| queued.error_panic(format!("Invalid unit id: {unit_id}")));

                let result = building.visited_by(unit, context);

                // Optional post visit user callback.
                if let Some(on_post_visit) = on_post_visit {
                    on_post_visit(context, building, unit, result);
                }
            }
            SimCmd::DeferBuildingTaskStep { kind_and_id, unit_id, callback, on_complete } => {
                let building = context
                    .find_building_mut(kind_and_id.kind, kind_and_id.id)
                    .unwrap_or_else(|| queued.error_panic(format!("Invalid building kind/id: {} {}", kind_and_id.kind, kind_and_id.id)));

                let unit = context
                    .find_unit_mut(*unit_id)
                    .unwrap_or_else(|| queued.error_panic(format!("Invalid unit id: {unit_id}")));

                callback(context, building, unit);

                // Optional post-callback. Runs after the main callback with the same refs,
                // letting the caller observe completion (e.g. to advance a task state machine).
                if let Some(on_complete) = on_complete {
                    on_complete(context, building, unit);
                }
            }
            SimCmd::DeferBuildingUpdate { kind_and_id, callback } => {
                let building = context
                    .find_building_mut(kind_and_id.kind, kind_and_id.id)
                    .unwrap_or_else(|| queued.error_panic(format!("Invalid building kind/id: {} {}", kind_and_id.kind, kind_and_id.id)));

                callback(context, building);
            }
            SimCmd::UpgradeHouse { kind_and_id, dir } => {
                // NOTE: Ignore an invalid BuildingId here. We may have multiple upgrade commands
                // referencing nearby houses that will be merged. The first command to execute wins,
                // merging and despawning the neighboring houses, thus making any remaining commands
                // no longer valid.
                if let Some(building) = context.find_building_mut(kind_and_id.kind, kind_and_id.id) {
                    let mut cmds = SimCmds::default();

                    let building_ctx = building.new_context(context);
                    building.as_house_mut().perform_upgrade(&mut cmds, &building_ctx, *dir);

                    cmds.execute(context);
                }
            }

            // --------------
            // Props:
            // --------------
            SimCmd::SpawnPropWithTileDef { origin, tile_def, state_id, on_spawned } => {
                let result = spawner.try_spawn_prop_with_tile_def(*origin, tile_def);
                Self::resolve_game_object_spawn(promises, queued, state_id, on_spawned, context, result);
            }
            SimCmd::DespawnPropWithId { id } => {
                spawner.despawn_prop_with_id(*id);
            }
            SimCmd::DeferPropUpdate { id, callback } => {
                let prop = context
                    .find_prop_mut(*id)
                    .unwrap_or_else(|| queued.error_panic(format!("Invalid prop id: {id}")));

                callback(context, prop);
            }
        }
    }

    // Shared resolution path for Unit/Building/Prop spawn commands.
    // Either updates the SpawnPromise slot with the result, or invokes the
    // user callback with the borrowed mutable reference to initialize the new object.
    fn resolve_game_object_spawn<T: GameObject>(
        promises: &mut SpawnPromiseStatePool,
        queued: &QueuedSimCmd,
        state_id: &Option<SpawnPromiseStateId>,
        on_spawned: &CallbackBox<GameObjectSpawnedCallback<T>>,
        context: &SimContext,
        result: Result<&mut T, TilePlacementErr>,
    ) {
        if let Some(state_id) = state_id {
            let promise = promises.try_get_mut(*state_id)
                .unwrap_or_else(|| queued.error_panic(format!("Invalid SpawnPromiseStateId: {}", state_id.0)));

            debug_assert!(promise.is_pending());

            *promise = match &result {
                Ok(obj)  => SpawnPromiseState::Ready(SpawnReadyResult::GameObject(obj.id())),
                Err(err) => SpawnPromiseState::Failed(err.clone()),
            };
        }

        on_spawned(context, result);
    }

    #[inline]
    fn push_cmd(&mut self, cmd: SimCmd) {
        self.cmds.push(QueuedSimCmd::new(cmd));
    }
}
