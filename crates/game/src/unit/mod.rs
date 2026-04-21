use common::{
    self,
    Color,
    hash,
    coords::{Cell, CellRange, IsoPointF32, WorldToScreenTransform},
};
use engine::{log, ui::UiSystem};
use serde::{Deserialize, Serialize};

use anim::*;
use config::*;
use inventory::*;
use navigation::*;
use patrol::*;
use task::*;

use super::{
    building::{Building, BuildingKind},
    sim::{
        SimCmds,
        SimContext,
        commands::{SpawnPromise, SpawnQueryResult, SpawnReadyResult},
        resources::{ResourceKind, StockItem},
    },
    world::{
        object::{GameObject, GenerationalIndex},
        stats::WorldStats,
    },
};
use crate::{
    save_context::PostLoadContext,
    pathfind::{NodeKind as PathNodeKind, Path},
    debug::{
        DebugUiMode,
        game_object_debug::{GameObjectDebugOptions, debug_popup_msg, debug_popup_msg_color, game_object_debug_options},
    },
    tile::{
        self,
        Tile,
        TileDepthSortOverride,
        TileKind,
        TileMap,
        TileMapLayerKind,
        TilePoolIndex,
        placement::TilePlacementErr,
    },
};

pub mod anim;
pub mod config;
pub mod harvester;
pub mod navigation;
pub mod patrol;
pub mod runner;
pub mod task;

mod debug;
mod inventory;

// ----------------------------------------------
// UnitDebug
// ----------------------------------------------

game_object_debug_options! {
    UnitDebug,
}

// ----------------------------------------------
// Unit
// ----------------------------------------------

pub type UnitId = GenerationalIndex;

// Common Unit Behavior:
// - Spawn and despawn dynamically.
// - Moves across the tile map, so map cell can change.
// - Transports resources from A to B (has a start point and a destination).
// - Patrols an area around its building to provide a service to households.
// - Most units will only walk on paved roads. Some units may go off-road.
// - Has an inventory that can cary a single ResourceKind at a time, any amount.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Unit {
    id: UnitId,
    map_cell: Cell,
    tile_index: TilePoolIndex,
    config_key: UnitConfigKey,
    direction: UnitDirection,
    anim_sets: UnitAnimSets,
    inventory: UnitInventory,
    navigation: UnitNavigation,
    current_task_id: UnitTaskId, // invalid if no task.

    #[serde(default)]
    path_is_blocked: bool,

    #[serde(skip)]
    config: Option<&'static UnitConfig>, // patched on post_load.

    #[serde(skip)]
    debug: UnitDebug,
}

impl GameObject for Unit {
    // ----------------------
    // GameObject Interface:
    // ----------------------

    #[inline]
    fn id(&self) -> UnitId {
        self.id
    }

    #[inline]
    fn update(&mut self, cmds: &mut SimCmds, context: &SimContext) {
        debug_assert!(self.config.is_some());
        self.update_tasks(cmds, context);
    }

    #[inline]
    fn tally(&self, stats: &mut WorldStats) {
        if !self.is_spawned() {
            return;
        }

        if let Some(item) = self.inventory.peek() {
            stats.add_unit_resources(item.kind, item.count);

            // Tax Collector / TaxOffice patrol.
            if item.kind == ResourceKind::Gold && self.is(UnitConfigKey::TaxCollector) {
                stats.treasury.tax_collected += item.count;
            }
        }
    }

    fn post_load(&mut self, _context: &mut PostLoadContext) {
        debug_assert!(self.is_spawned());
        debug_assert!(self.tile_index.is_valid());

        self.config = Some(UnitConfigs::get().find_config_by_key(self.config_key));
    }

    fn draw_debug_ui(&mut self, cmds: &mut SimCmds, context: &SimContext, ui_sys: &UiSystem, mode: DebugUiMode) {
        debug_assert!(self.is_spawned());

        match mode {
            DebugUiMode::Overview => {
                self.draw_debug_ui_overview(context, ui_sys);
            }
            DebugUiMode::Detailed => {
                let ui = ui_sys.ui();
                if ui.collapsing_header("Unit", imgui::TreeNodeFlags::empty()) {
                    ui.indent_by(10.0);
                    self.draw_debug_ui_detailed(cmds, context, ui_sys);
                    ui.unindent_by(10.0);
                }
            }
        }
    }

    fn draw_debug_popups(
        &mut self,
        context: &SimContext,
        ui_sys: &UiSystem,
        transform: WorldToScreenTransform,
        visible_range: CellRange,
    ) {
        debug_assert!(self.is_spawned());
        self.debug.draw_popup_messages(self.find_tile(context), ui_sys, transform, visible_range, context.delta_time_secs());
    }
}

impl Unit {
    // ----------------------
    // Spawning / Despawning:
    // ----------------------

    pub fn spawned(&mut self, tile: &mut Tile, config: &'static UnitConfig, id: UnitId) {
        debug_assert!(!self.is_spawned());
        debug_assert!(tile.is_valid());
        debug_assert!(id.is_valid());
        debug_assert!(config.key_hash() != hash::NULL_HASH);

        self.id = id;
        self.map_cell = tile.base_cell();
        self.tile_index = tile.index();
        self.config = Some(config);
        self.config_key = config.key();
        self.direction = UnitDirection::Idle;
        self.path_is_blocked = false;

        self.anim_sets.set_anim(tile, UnitAnimSets::IDLE);
        self.navigation.set_traversable_node_kinds(config.traversable_node_kinds);
        self.navigation.set_movement_speed(config.movement_speed);
        self.debug.set_show_popups(crate::debug::show_popup_messages());
    }

    pub fn despawned(&mut self, _cmds: &mut SimCmds, context: &SimContext) {
        debug_assert!(self.is_spawned());

        self.id = UnitId::default();
        self.map_cell = Cell::default();
        self.tile_index = TilePoolIndex::default();
        self.config = None;
        self.config_key = UnitConfigKey::default();
        self.direction = UnitDirection::default();

        self.anim_sets.clear();
        self.inventory.clear();
        self.navigation.reset_path_and_goal(None, None);
        self.navigation.set_traversable_node_kinds(PathNodeKind::default());
        self.debug.clear_popups();

        context.task_manager_mut().free_task(self.current_task_id);
        self.current_task_id = UnitTaskId::default();
    }

    // ----------------------
    // Utilities:
    // ----------------------

    #[inline]
    pub fn name(&self) -> &str {
        debug_assert!(self.is_spawned());
        &self.config.unwrap().name
    }

    #[inline]
    pub fn cell(&self) -> Cell {
        debug_assert!(self.is_spawned());
        self.map_cell
    }

    #[inline]
    pub fn tile_index(&self) -> TilePoolIndex {
        debug_assert!(self.is_spawned());
        self.tile_index
    }

    // Teleports to new tile cell and updates direction and animation.
    pub fn teleport(&mut self, tile_map: &mut TileMap, destination_cell: Cell) -> bool {
        debug_assert!(self.is_spawned());
        if self.map_cell == destination_cell {
            return true;
        }

        if tile_map.try_move_tile_with_stacking(self.tile_index, self.map_cell, destination_cell, TileMapLayerKind::Objects) {
            let tile = tile_map.tile_at_index_mut(self.tile_index, TileMapLayerKind::Objects);
            debug_assert!(tile.is(TileKind::Unit));

            let new_direction = direction_between(self.map_cell, destination_cell);
            self.update_direction_and_anim(tile, new_direction);

            debug_assert!(tile.base_cell() == destination_cell);
            self.map_cell = destination_cell;
            return true;
        }

        false
    }

    pub fn set_animation(&mut self, context: &SimContext, new_anim_set_key: UnitAnimSetKey) {
        let tile = self.find_tile_mut(context);
        self.anim_sets.set_anim(tile, new_anim_set_key);
    }

    pub fn set_depth_sort_override(&mut self, context: &SimContext, depth_sort_override: TileDepthSortOverride) {
        let tile = self.find_tile_mut(context);
        tile.set_depth_sort_override(depth_sort_override);
    }

    #[inline]
    pub fn patrol_task_origin_building<'game>(&self, context: &'game SimContext) -> Option<&'game mut Building> {
        debug_assert!(self.is_spawned());
        if let Some(task) = self.current_task_as::<UnitTaskRandomizedPatrol>(context.task_manager()) {
            return context.find_building_mut(task.origin_building.kind, task.origin_building.id);
        }
        None
    }

    #[inline]
    pub fn patrol_task_building_kind(&self, context: &SimContext) -> Option<BuildingKind> {
        debug_assert!(self.is_spawned());
        if let Some(task) = self.current_task_as::<UnitTaskRandomizedPatrol>(context.task_manager()) {
            return Some(task.origin_building.kind);
        }
        None
    }

    #[inline]
    pub fn is(&self, config_key: UnitConfigKey) -> bool {
        debug_assert!(self.is_spawned());
        self.config_key == config_key
    }

    #[inline]
    pub fn is_settler(&self) -> bool {
        self.is(UnitConfigKey::Settler)
    }

    #[inline]
    pub fn settler_population(&self, context: &SimContext) -> u32 {
        debug_assert!(self.is_settler());
        let task = self
            .current_task_as::<UnitTaskSettler>(context.task_manager())
            .expect("Expected unit to be running a UnitTaskSettler!");
        task.population_to_add
    }

    #[inline]
    pub fn is_market_vendor(&self, context: &SimContext) -> bool {
        self.is(UnitConfigKey::Vendor)
            && self.patrol_task_building_kind(context).is_some_and(|kind| kind == BuildingKind::Market)
    }

    #[inline]
    pub fn is_tax_collector(&self, context: &SimContext) -> bool {
        self.is(UnitConfigKey::TaxCollector)
            && self.patrol_task_building_kind(context).is_some_and(|kind| kind == BuildingKind::TaxOffice)
    }

    // ----------------------
    // Path Navigation:
    // ----------------------

    #[inline]
    pub fn traversable_node_kinds(&self) -> PathNodeKind {
        debug_assert!(self.is_spawned());
        self.navigation.traversable_node_kinds()
    }

    #[inline]
    pub fn set_traversable_node_kinds(&mut self, traversable_node_kinds: PathNodeKind) {
        debug_assert!(self.is_spawned());
        self.navigation.set_traversable_node_kinds(traversable_node_kinds);
    }

    #[inline]
    pub fn follow_path(&mut self, path: Option<&Path>) {
        debug_assert!(self.is_spawned());
        self.navigation.reset_path_and_goal(path, None);
        self.path_is_blocked = false;
        if path.is_some() {
            debug_popup_msg!(self.debug, "New Path");
        }
    }

    #[inline]
    pub fn is_following_path(&self) -> bool {
        debug_assert!(self.is_spawned());
        self.navigation.is_following_path()
    }

    #[inline]
    pub fn path_is_blocked(&self) -> bool {
        self.path_is_blocked
    }

    #[inline]
    pub fn move_to_goal(&mut self, path: &Path, goal: UnitNavGoal) {
        debug_assert!(self.is_spawned());
        debug_assert_eq!(path.last().unwrap().cell, goal.destination_cell());

        self.navigation.reset_path_and_goal(Some(path), Some(goal));
        self.path_is_blocked = false;
        self.log_going_to(&goal);
    }

    #[inline]
    pub fn goal(&self) -> Option<&UnitNavGoal> {
        debug_assert!(self.is_spawned());
        self.navigation.goal()
    }

    #[inline]
    pub fn has_reached_goal(&self) -> bool {
        debug_assert!(self.is_spawned());
        self.navigation.goal().is_some_and(|goal| self.cell() == goal.destination_cell())
    }

    pub fn update_navigation(&mut self, context: &SimContext) {
        debug_assert!(self.is_spawned());

        // Path following and movement:
        match self.navigation.update(context.graph(), context.delta_time_secs()) {
            UnitNavResult::Idle => {
                // Nothing.
            }
            UnitNavResult::Moving(from_cell, to_cell, progress, direction) => {
                let tile = self.find_tile_mut(context);

                let draw_size = tile.draw_size();
                let from_iso = tile::calc_unit_iso_coords(from_cell, draw_size);
                let to_iso = tile::calc_unit_iso_coords(to_cell, draw_size);

                let new_iso_coords = IsoPointF32(common::lerp(from_iso.0, to_iso.0, progress));
                tile.set_iso_coords_f32(new_iso_coords);

                self.update_direction_and_anim(tile, direction);
            }
            UnitNavResult::AdvancedCell(cell, direction) => {
                if !self.teleport(context.tile_map_mut(), cell) {
                    // This would normally happen if two units try to move to the
                    // same tile, so they will bump into each other for one frame.
                    // Not a critical failure, the unit can recover next update.
                    debug_popup_msg_color!(self.debug, Color::yellow(), "Bump!");
                }

                self.update_direction_and_anim(self.find_tile_mut(context), direction);
            }
            UnitNavResult::ReachedGoal(cell, _) => {
                self.teleport(context.tile_map_mut(), cell);

                if cell == self.cell() {
                    // Goal reached, clear current path.
                    // NOTE: Not using follow_path(None) here to preserve the nav goal for unit tasks.
                    self.navigation.reset_path_only();
                    debug_popup_msg_color!(self.debug, Color::green(), "Reached Goal!");
                } else {
                    // Path was blocked, retry task.
                    self.follow_path(None);
                    self.path_is_blocked = true;
                    debug_popup_msg_color!(self.debug, Color::red(), "Goal Blocked!");
                }

                self.idle(context);
            }
            UnitNavResult::PathBlocked => {
                // Failed to move to another tile, possibly because it has been
                // blocked since we've traced the path. Clear the navigation and stop.
                // If a task is running it should now re-route the path and retry.
                self.follow_path(None);
                self.idle(context);
                self.path_is_blocked = true;
                debug_popup_msg_color!(self.debug, Color::red(), "Path Blocked!");
            }
        }
    }

    // ----------------------
    // Unit Behavior / Tasks:
    // ----------------------

    #[inline]
    fn update_tasks(&mut self, cmds: &mut SimCmds, context: &SimContext) {
        debug_assert!(self.is_spawned());
        let task_manager = context.task_manager_mut();
        task_manager.run_unit_tasks(self, cmds, context);
    }

    #[inline]
    pub fn is_running_task<Task>(&self, task_manager: &UnitTaskManager) -> bool
    where
        Task: UnitTask + 'static,
    {
        debug_assert!(self.is_spawned());
        task_manager.is_task::<Task>(self.current_task_id)
    }

    #[inline]
    pub fn current_task_as<'task, Task>(&self, task_manager: &'task UnitTaskManager) -> Option<&'task Task>
    where
        Task: UnitTask + 'static,
    {
        debug_assert!(self.is_spawned());
        task_manager.try_get_task::<Task>(self.current_task_id)
    }

    #[inline]
    pub fn current_task_as_mut<'task, Task>(&mut self, task_manager: &'task mut UnitTaskManager) -> Option<&'task mut Task>
    where
        Task: UnitTask + 'static,
    {
        debug_assert!(self.is_spawned());
        task_manager.try_get_task_mut::<Task>(self.current_task_id)
    }

    #[inline]
    pub fn current_task(&self) -> Option<UnitTaskId> {
        debug_assert!(self.is_spawned());
        if self.current_task_id.is_valid() { Some(self.current_task_id) } else { None }
    }

    #[inline]
    pub fn assign_task(&mut self, task_manager: &mut UnitTaskManager, task_id: Option<UnitTaskId>) {
        debug_assert!(self.is_spawned());
        task_manager.free_task(self.current_task_id);
        self.current_task_id = task_id.unwrap_or_default();
    }

    // Deferred spawn, pushes a command into the sim command queue.
    // Returns a promise that must be polled for completion.
    // Can be called while the world is locked for update.
    #[must_use]
    pub fn try_spawn_with_task_deferred_promise<Task, F>(
        cmds: &mut SimCmds,
        context: &SimContext,
        unit_origin: Cell,
        unit_config: UnitConfigKey,
        task: Task,
        on_spawned: F,
    ) -> SpawnPromise<Unit>
    where
        Task: UnitTask,
        UnitTaskArchetype: From<Task>,
        F: Fn(&SimContext, Result<&mut Unit, TilePlacementErr>) + 'static,
    {
        debug_assert!(unit_origin.is_valid());
        let task_id = context.task_manager_mut().new_task(task);

        cmds.spawn_unit_with_config_promise(unit_origin, unit_config, move |context, result| {
            let unit = match result {
                Ok(unit) => unit,
                error @ Err(_) => {
                    context.task_manager_mut().free_task(task_id.unwrap());
                    // SpawnPromise also contains a copy of the error.
                    on_spawned(context, error);
                    return;
                }
            };

            // This will start the task chain and might take some time to complete.
            unit.assign_task(context.task_manager_mut(), task_id);
            on_spawned(context, Ok(unit));
        })
    }

    // Deferred spawn, pushes a command into the sim command queue.
    // Receives a callback to be invoked when the command is executed.
    // Can be called while the world is locked for update.
    pub fn try_spawn_with_task_deferred_cb<Task, F>(
        cmds: &mut SimCmds,
        context: &SimContext,
        unit_origin: Cell,
        unit_config: UnitConfigKey,
        task: Task,
        on_spawned: F,
    )
    where
        Task: UnitTask,
        UnitTaskArchetype: From<Task>,
        F: Fn(&SimContext, Result<&mut Unit, TilePlacementErr>) + 'static,
    {
        debug_assert!(unit_origin.is_valid());
        let task_id = context.task_manager_mut().new_task(task);

        cmds.spawn_unit_with_config_cb(unit_origin, unit_config, move |context, result| {
            let unit = match result {
                Ok(unit) => unit,
                error @ Err(_) => {
                    context.task_manager_mut().free_task(task_id.unwrap());
                    on_spawned(context, error);
                    return;
                }
            };

            // This will start the task chain and might take some time to complete.
            unit.assign_task(context.task_manager_mut(), task_id);
            on_spawned(context, Ok(unit));
        });
    }

    // ----------------------
    // Inventory / Resources:
    // ----------------------

    #[inline]
    pub fn peek_inventory(&self) -> Option<StockItem> {
        debug_assert!(self.is_spawned());
        self.inventory.peek()
    }

    #[inline]
    pub fn inventory_is_empty(&self) -> bool {
        debug_assert!(self.is_spawned());
        self.inventory.is_empty()
    }

    pub fn clear_inventory(&mut self) {
        debug_assert!(self.is_spawned());
        self.inventory.clear();
    }

    // Returns number of resources it was able to accommodate.
    // Unit inventories can always accommodate all resources received.
    pub fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(self.is_spawned());
        debug_assert!(kind.is_single_resource());

        let received_count = self.inventory.receive_resources(kind, count);
        self.debug.log_resources_gained(kind, received_count);
        received_count
    }

    // Tries to relinquish up to `count` resources. Returns the number of
    // resources it was able to relinquish, which can be less or equal to `count`.
    pub fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(self.is_spawned());
        debug_assert!(kind.is_single_resource());

        let removed_count = self.inventory.remove_resources(kind, count);
        self.debug.log_resources_lost(kind, removed_count);
        removed_count
    }

    // ----------------------
    // Callbacks:
    // ----------------------

    pub fn register_callbacks() {
        debug::register_callbacks();
        Patrol::register_callbacks();
    }

    // ----------------------
    // Internal helpers:
    // ----------------------

    #[inline]
    fn find_tile<'game>(&self, context: &'game SimContext) -> &'game Tile {
        let tile = context.tile_at_index(self.tile_index, TileMapLayerKind::Objects);
        debug_assert!(tile.is(TileKind::Unit));
        tile
    }

    #[inline]
    fn find_tile_mut<'game>(&self, context: &'game SimContext) -> &'game mut Tile {
        let tile = context.tile_at_index_mut(self.tile_index, TileMapLayerKind::Objects);
        debug_assert!(tile.is(TileKind::Unit));
        tile
    }

    fn idle(&mut self, context: &SimContext) {
        if self.direction != UnitDirection::Idle {
            let idle_anim_set_key = navigation::idle_anim_set_for_direction(self.direction);

            let tile = self.find_tile_mut(context);
            if !self.anim_sets.set_anim(tile, idle_anim_set_key) {
                // Fallback to generic idle if no directional anim.
                self.anim_sets.set_anim(tile, UnitAnimSets::IDLE);
            }

            self.direction = UnitDirection::Idle;
        }
    }

    fn update_direction_and_anim(&mut self, tile: &mut Tile, new_direction: UnitDirection) {
        if self.direction != new_direction {
            let new_anim_set_key = anim_set_for_direction(new_direction);
            self.anim_sets.set_anim(tile, new_anim_set_key);
            self.direction = new_direction;
        }
    }

    fn log_going_to(&mut self, goal: &UnitNavGoal) {
        debug_popup_msg!(self.debug, "Goto: {} -> {}", goal.origin_debug_name(), goal.destination_debug_name());
    }
}

// ----------------------------------------------
// UnitSpawnState
// ----------------------------------------------

#[derive(Clone, Debug, Default)]
pub enum UnitSpawnState {
    #[default]
    Unset,                       // Invalid UnitId
    Failed,                      // Invalid UnitId
    Pending(SpawnPromise<Unit>), // Invalid UnitId
    Spawned(UnitId),             // Valid UnitId
}

// ----------------------------------------------
// UnitTaskHelper
// ----------------------------------------------

pub trait UnitTaskHelper: Sized {
    fn reset(&mut self);

    fn get_pending_promise(&mut self) -> Option<SpawnPromise<Unit>>;
    fn set_spawn_state(&mut self, state: UnitSpawnState);

    fn spawn_state(&self) -> &UnitSpawnState;
    fn unit_id(&self) -> UnitId;

    #[inline]
    fn failed_to_spawn(&self) -> bool {
        matches!(self.spawn_state(), UnitSpawnState::Failed)
    }

    #[inline]
    fn is_pending_spawn(&self) -> bool {
        matches!(self.spawn_state(), UnitSpawnState::Pending(_))
    }

    #[inline]
    fn is_spawned(&self) -> bool {
        matches!(self.spawn_state(), UnitSpawnState::Spawned(_))
    }

    #[inline]
    fn is_spawned_or_pending_spawn(&self) -> bool {
        matches!(self.spawn_state(), UnitSpawnState::Spawned(_) | UnitSpawnState::Pending(_))
    }

    #[inline]
    fn try_unit<'game>(&self, context: &'game SimContext) -> Option<&'game Unit> {
        context.find_unit(self.unit_id())
    }

    #[inline]
    fn try_unit_mut<'game>(&mut self, context: &'game SimContext) -> Option<&'game mut Unit> {
        context.find_unit_mut(self.unit_id())
    }

    #[inline]
    fn unit<'game>(&self, context: &'game SimContext) -> &'game Unit {
        self.try_unit(context).unwrap()
    }

    #[inline]
    fn unit_mut<'game>(&mut self, context: &'game SimContext) -> &'game mut Unit {
        self.try_unit_mut(context).unwrap()
    }

    #[inline]
    fn is_running_task<Task>(&self, context: &SimContext) -> bool
    where
        Task: UnitTask + 'static,
    {
        self.try_unit(context).is_some_and(|unit| unit.is_running_task::<Task>(context.task_manager()))
    }

    #[inline]
    fn try_spawn_with_task<Task>(
        &mut self,
        spawner_name: &'static str,
        cmds: &mut SimCmds,
        context: &SimContext,
        unit_origin: Cell,
        unit_config: UnitConfigKey,
        task: Task,
    )
    where
        Task: UnitTask,
        UnitTaskArchetype: From<Task>,
    {
        debug_assert!(!self.is_spawned_or_pending_spawn(), "Unit already spawned! reset() first.");

        let promise = Unit::try_spawn_with_task_deferred_promise(
            cmds, context, unit_origin, unit_config, task,
            move |_context, result| {
                if let Err(err) = result {
                    log::error!(log::channel!("unit"), "{}: {}", spawner_name, err.message);
                }
            });

        self.set_spawn_state(UnitSpawnState::Pending(promise));
    }

    #[inline]
    fn update(&mut self, cmds: &mut SimCmds) {
        if let Some(promise) = self.get_pending_promise() {
            debug_assert!(self.is_pending_spawn());

            match cmds.query_promise(promise) {
                SpawnQueryResult::InvalidPromise | SpawnQueryResult::Failed(_) => {
                    self.set_spawn_state(UnitSpawnState::Failed);
                }
                SpawnQueryResult::Pending(promise) => {
                    self.set_spawn_state(UnitSpawnState::Pending(promise));
                }
                SpawnQueryResult::Ready(result) => {
                    if let SpawnReadyResult::GameObject(id) = result {
                        self.set_spawn_state(UnitSpawnState::Spawned(id));
                    } else {
                        panic!("Unit: Expected SpawnReadyResult::GameObject id!");
                    }
                }
            }
        } else {
            debug_assert!(matches!(
                self.spawn_state(),
                UnitSpawnState::Spawned(_) |
                UnitSpawnState::Failed |
                UnitSpawnState::Unset
            ));
        }
    }

    #[inline]
    fn pre_save(&mut self, cmds: &mut SimCmds) {
        // Resolve spawn promise before saving, in case it is still pending.
        self.update(cmds);
    }

    #[inline]
    fn post_save(&mut self) {
        // Check post save invariants.
        debug_assert!(matches!(
            self.spawn_state(),
            UnitSpawnState::Spawned(_) |
            UnitSpawnState::Failed |
            UnitSpawnState::Unset
        ));
        debug_assert!(self.get_pending_promise().is_none());
    }

    #[inline]
    fn post_load(&mut self) {
        // Check post load invariants.
        debug_assert!(matches!(
            self.spawn_state(),
            UnitSpawnState::Spawned(_) |
            UnitSpawnState::Failed |
            UnitSpawnState::Unset
        ));
        debug_assert!(self.get_pending_promise().is_none());
    }

    #[inline]
    fn discard_spawn_promise(&mut self, cmds: &mut SimCmds) {
        if let Some(promise) = self.get_pending_promise() {
            self.set_spawn_state(UnitSpawnState::Unset);
            cmds.discard_promise(promise);
        }
    }
}

// ----------------------------------------------
// SpawnedUnitWithTask
// ----------------------------------------------

#[derive(Clone, Default)]
pub struct SpawnedUnitWithTask {
    spawn_state: UnitSpawnState,
}

impl UnitTaskHelper for SpawnedUnitWithTask {
    #[inline]
    fn reset(&mut self) {
        self.spawn_state = UnitSpawnState::default();
    }

    #[inline]
    fn get_pending_promise(&mut self) -> Option<SpawnPromise<Unit>> {
        if let UnitSpawnState::Pending(promise) = &mut self.spawn_state {
            Some(std::mem::take(promise))
        } else {
            None
        }
    }

    #[inline]
    fn set_spawn_state(&mut self, state: UnitSpawnState) {
        self.spawn_state = state;
    }

    #[inline]
    fn spawn_state(&self) -> &UnitSpawnState {
        &self.spawn_state
    }

    #[inline]
    fn unit_id(&self) -> UnitId {
        if let UnitSpawnState::Spawned(unit_id) = self.spawn_state {
            unit_id
        } else {
            UnitId::invalid()
        }
    }
}

#[cfg(debug_assertions)]
impl Drop for SpawnedUnitWithTask {
    fn drop(&mut self) {
        debug_assert!(
            !self.is_pending_spawn(),
            "SpawnedUnitWithTask dropped while holding a pending SpawnPromise!",
        );
    }
}

// This keeps backwards compatibility with the existing save format.
// Previous UnitTaskHelpers saved a single `unit_id` field.
#[derive(Serialize, Deserialize)]
struct SpawnedUnitWithTaskSerializedData {
    unit_id: UnitId,
}

impl Serialize for SpawnedUnitWithTask {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Ensure spawn promise is resolved by now.
        debug_assert!(
            !self.is_pending_spawn(),
            "SpawnedUnitWithTask should have completed spawning before serialization!",
        );

        // Serialize unit id only.
        let unit_id = self.unit_id();
        SpawnedUnitWithTaskSerializedData { unit_id }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SpawnedUnitWithTask {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let ser_data = SpawnedUnitWithTaskSerializedData::deserialize(deserializer)?;

        let spawn_state = {
            if ser_data.unit_id.is_valid() {
                UnitSpawnState::Spawned(ser_data.unit_id)
            } else {
                // Assume unset if the unit id was invalid. Could also mean Failed,
                // both states are effectively equivalent, so we choose the most likely one.
                UnitSpawnState::Unset
            }
        };

        Ok(Self { spawn_state })
    }
}
