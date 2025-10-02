use anim::*;
use config::*;
use inventory::*;
use navigation::*;
use patrol::*;
use serde::{Deserialize, Serialize};
use task::*;

use super::{
    building::{Building, BuildingKind},
    sim::{
        debug::DebugUiMode,
        resources::{ResourceKind, StockItem},
        Query,
    },
    world::{
        object::{GameObject, GenerationalIndex, Spawner},
        stats::WorldStats,
    },
};
use crate::{
    game_object_debug_options,
    imgui_ui::UiSystem,
    log,
    pathfind::{NodeKind as PathNodeKind, Path},
    save::PostLoadContext,
    tile::{self, Tile, TileKind, TileMap, TileMapLayerKind, TilePoolIndex},
    utils::{
        self,
        coords::{Cell, CellRange, WorldToScreenTransform},
        hash::{self, StringHash},
        Color,
    },
};

pub mod config;
pub mod navigation;
pub mod patrol;
pub mod runner;
pub mod task;

mod anim;
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

/*
Common Unit Behavior:
 - Spawn and despawn dynamically.
 - Moves across the tile map, so map cell can change.
 - Transports resources from A to B (has a start point and a destination).
 - Patrols an area around its building to provide a service to households.
 - Most units will only walk on paved roads. Some units may go off-road.
 - Has an inventory that can cary a single ResourceKind at a time, any amount.
*/
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Unit {
    id: UnitId,
    map_cell: Cell,
    tile_index: TilePoolIndex,
    config_key_hash: StringHash,
    direction: UnitDirection,
    anim_sets: UnitAnimSets,
    inventory: UnitInventory,
    navigation: UnitNavigation,
    current_task_id: UnitTaskId, // invalid if no task.

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
    fn update(&mut self, query: &Query) {
        debug_assert!(self.config.is_some());
        self.update_tasks(query);
    }

    #[inline]
    fn tally(&self, stats: &mut WorldStats) {
        if !self.is_spawned() {
            return;
        }

        if let Some(item) = self.inventory.peek() {
            stats.add_unit_resources(item.kind, item.count);

            // Tax Collector / TaxOffice patrol.
            if item.kind == ResourceKind::Gold && self.is_patrol() {
                stats.treasury.tax_collected += item.count;
            }
        }
    }

    fn post_load(&mut self, _context: &PostLoadContext) {
        debug_assert!(self.is_spawned());
        debug_assert!(self.tile_index.is_valid());
        debug_assert!(self.config_key_hash != hash::NULL_HASH);

        let configs = UnitConfigs::get();
        let config = configs.find_config_by_hash(self.config_key_hash, "<unit>");

        self.config = Some(config);
    }

    fn draw_debug_ui(&mut self, query: &Query, ui_sys: &UiSystem, mode: DebugUiMode) {
        debug_assert!(self.is_spawned());

        match mode {
            DebugUiMode::Overview => {
                self.draw_debug_ui_overview(query, ui_sys);
            }
            DebugUiMode::Detailed => {
                let ui = ui_sys.builder();
                if ui.collapsing_header("Unit", imgui::TreeNodeFlags::empty()) {
                    ui.indent_by(10.0);
                    self.draw_debug_ui_detailed(query, ui_sys);
                    ui.unindent_by(10.0);
                }
            }
        }
    }

    fn draw_debug_popups(&mut self,
                         query: &Query,
                         ui_sys: &UiSystem,
                         transform: WorldToScreenTransform,
                         visible_range: CellRange) {
        debug_assert!(self.is_spawned());

        self.debug.draw_popup_messages(self.find_tile(query),
                                       ui_sys,
                                       transform,
                                       visible_range,
                                       query.delta_time_secs());
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
        self.config_key_hash = config.key_hash();
        self.direction = UnitDirection::Idle;

        self.anim_sets.set_anim(tile, UnitAnimSets::IDLE);
        self.navigation.set_traversable_node_kinds(config.traversable_node_kinds);
        self.navigation.set_movement_speed(config.movement_speed);
    }

    pub fn despawned(&mut self, query: &Query) {
        debug_assert!(self.is_spawned());

        self.id = UnitId::default();
        self.map_cell = Cell::default();
        self.tile_index = TilePoolIndex::default();
        self.config = None;
        self.config_key_hash = hash::NULL_HASH;
        self.direction = UnitDirection::default();

        self.anim_sets.clear();
        self.inventory.clear();
        self.navigation.reset_path_and_goal(None, None);
        self.navigation.set_traversable_node_kinds(PathNodeKind::default());
        self.debug.clear_popups();

        query.task_manager().free_task(self.current_task_id);
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

        if tile_map.try_move_tile_with_stacking(self.tile_index,
                                                self.map_cell,
                                                destination_cell,
                                                TileMapLayerKind::Objects)
        {
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

    #[inline]
    pub fn patrol_task_origin_building<'world>(&self,
                                               query: &'world Query)
                                               -> Option<&'world mut Building> {
        debug_assert!(self.is_spawned());
        if let Some(task) = self.current_task_as::<UnitTaskRandomizedPatrol>(query.task_manager()) {
            return query.world()
                        .find_building_mut(task.origin_building.kind, task.origin_building.id);
        }
        None
    }

    #[inline]
    pub fn patrol_task_building_kind(&self, query: &Query) -> Option<BuildingKind> {
        debug_assert!(self.is_spawned());
        if let Some(task) = self.current_task_as::<UnitTaskRandomizedPatrol>(query.task_manager()) {
            return Some(task.origin_building.kind);
        }
        None
    }

    #[inline]
    pub fn is(&self, config_key: UnitConfigKey) -> bool {
        debug_assert!(self.is_spawned());
        debug_assert!(config_key.is_valid());
        debug_assert!(self.config_key_hash != hash::NULL_HASH);

        self.config_key_hash == config_key.hash
    }

    #[inline]
    pub fn is_ped(&self) -> bool {
        self.is(config::UNIT_PED)
    }

    #[inline]
    pub fn is_runner(&self) -> bool {
        self.is(config::UNIT_RUNNER)
    }

    #[inline]
    pub fn is_patrol(&self) -> bool {
        self.is(config::UNIT_PATROL)
    }

    #[inline]
    pub fn is_market_vendor(&self, query: &Query) -> bool {
        self.is_patrol()
        && self.patrol_task_building_kind(query).is_some_and(|kind| kind == BuildingKind::Market)
    }

    #[inline]
    pub fn is_tax_collector(&self, query: &Query) -> bool {
        self.is_patrol()
        && self.patrol_task_building_kind(query).is_some_and(|kind| kind == BuildingKind::TaxOffice)
    }

    #[inline]
    pub fn is_settler(&self) -> bool {
        self.is(config::UNIT_SETTLER)
    }

    #[inline]
    pub fn settler_population(&self, query: &Query) -> u32 {
        debug_assert!(self.is_settler());
        let task = self.current_task_as::<UnitTaskSettler>(query.task_manager())
                       .expect("Expected unit to be running a UnitTaskSettler!");
        task.population_to_add
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
        if path.is_some() {
            self.debug.popup_msg("New Path");
        }
    }

    #[inline]
    pub fn is_following_path(&self) -> bool {
        debug_assert!(self.is_spawned());
        self.navigation.is_following_path()
    }

    #[inline]
    pub fn move_to_goal(&mut self, path: &Path, goal: UnitNavGoal) {
        debug_assert!(self.is_spawned());
        self.navigation.reset_path_and_goal(Some(path), Some(goal));
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

    pub fn update_navigation(&mut self, query: &Query) {
        debug_assert!(self.is_spawned());

        // Path following and movement:
        match self.navigation.update(query.graph(), query.delta_time_secs()) {
            UnitNavResult::Idle => {
                // Nothing.
            }
            UnitNavResult::Moving(from_cell, to_cell, progress, direction) => {
                let tile = self.find_tile_mut(query);

                let draw_size = tile.draw_size();
                let from_iso = tile::calc_unit_iso_coords(from_cell, draw_size);
                let to_iso = tile::calc_unit_iso_coords(to_cell, draw_size);

                let new_iso_coords = utils::lerp(from_iso, to_iso, progress);
                tile.set_iso_coords_f32(new_iso_coords);

                self.update_direction_and_anim(tile, direction);
            }
            UnitNavResult::AdvancedCell(cell, direction) => {
                if !self.teleport(query.tile_map(), cell) {
                    // This would normally happen if two units try to move to the
                    // same tile, so they will bump into each other for one frame.
                    // Not a critical failure, the unit can recover next update.
                    self.debug.popup_msg_color(Color::yellow(), "Bump!");
                }

                self.update_direction_and_anim(self.find_tile_mut(query), direction);
            }
            UnitNavResult::ReachedGoal(cell, _) => {
                self.teleport(query.tile_map(), cell);

                if cell == self.cell() {
                    // Goal reached, clear current path.
                    // NOTE: Not using follow_path(None) here to preserve the nav goal for unit
                    // tasks.
                    self.navigation.reset_path_only();
                    self.debug.popup_msg_color(Color::green(), "Reached Goal!");
                } else {
                    // Path was blocked, retry task.
                    self.follow_path(None);
                    self.debug.popup_msg_color(Color::red(), "Goal Blocked!");
                }

                self.idle(query);
            }
            UnitNavResult::PathBlocked => {
                // Failed to move to another tile, possibly because it has been
                // blocked since we've traced the path. Clear the navigation and stop.
                // If a task is running it should now re-route the path and retry.
                self.follow_path(None);
                self.idle(query);

                self.debug.popup_msg_color(Color::red(), "Path Blocked!");
            }
        }
    }

    // ----------------------
    // Unit Behavior / Tasks:
    // ----------------------

    #[inline]
    fn update_tasks(&mut self, query: &Query) {
        debug_assert!(self.is_spawned());
        let task_manager = query.task_manager();
        task_manager.run_unit_tasks(self, query);
    }

    #[inline]
    pub fn is_running_task<Task>(&self, task_manager: &UnitTaskManager) -> bool
        where Task: UnitTask + 'static
    {
        debug_assert!(self.is_spawned());
        task_manager.is_task::<Task>(self.current_task_id)
    }

    #[inline]
    pub fn current_task_as<'task, Task>(&self,
                                        task_manager: &'task UnitTaskManager)
                                        -> Option<&'task Task>
        where Task: UnitTask + 'static
    {
        debug_assert!(self.is_spawned());
        task_manager.try_get_task::<Task>(self.current_task_id)
    }

    #[inline]
    pub fn current_task(&self) -> Option<UnitTaskId> {
        debug_assert!(self.is_spawned());
        if self.current_task_id.is_valid() {
            Some(self.current_task_id)
        } else {
            None
        }
    }

    #[inline]
    pub fn assign_task(&mut self, task_manager: &mut UnitTaskManager, task_id: Option<UnitTaskId>) {
        debug_assert!(self.is_spawned());
        task_manager.free_task(self.current_task_id);
        self.current_task_id = task_id.unwrap_or_default();
    }

    pub fn try_spawn_with_task<Task>(query: &Query,
                                     unit_origin: Cell,
                                     unit_config: UnitConfigKey,
                                     task: Task)
                                     -> Result<&mut Unit, String>
        where Task: UnitTask,
              UnitTaskArchetype: From<Task>
    {
        debug_assert!(unit_origin.is_valid());
        debug_assert!(unit_config.is_valid());

        let task_manager = query.task_manager();
        let task_id = task_manager.new_task(task);

        let unit = match Spawner::new(query).try_spawn_unit_with_config(unit_origin, unit_config) {
            Ok(unit) => unit,
            error @ Err(_) => {
                task_manager.free_task(task_id.unwrap());
                return error;
            }
        };

        // This will start the task chain and might take some time to complete.
        unit.assign_task(task_manager, task_id);
        Ok(unit)
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
    fn find_tile<'world>(&self, query: &'world Query) -> &'world Tile {
        let tile = query.tile_map().tile_at_index(self.tile_index, TileMapLayerKind::Objects);
        debug_assert!(tile.is(TileKind::Unit));
        tile
    }

    #[inline]
    fn find_tile_mut<'world>(&self, query: &'world Query) -> &'world mut Tile {
        let tile = query.tile_map().tile_at_index_mut(self.tile_index, TileMapLayerKind::Objects);
        debug_assert!(tile.is(TileKind::Unit));
        tile
    }

    #[inline]
    fn idle(&mut self, query: &Query) {
        self.update_direction_and_anim(self.find_tile_mut(query), UnitDirection::Idle);
    }

    fn update_direction_and_anim(&mut self, tile: &mut Tile, new_direction: UnitDirection) {
        if self.direction != new_direction {
            self.direction = new_direction;
            let new_anim_set_key = anim_set_for_direction(new_direction);
            self.anim_sets.set_anim(tile, new_anim_set_key);
        }
    }

    fn log_going_to(&mut self, goal: &UnitNavGoal) {
        if !self.debug.show_popups() {
            return;
        }

        self.debug.popup_msg(format!("Goto: {} -> {}",
                                     goal.origin_debug_name(),
                                     goal.destination_debug_name()));
    }
}

// ----------------------------------------------
// UnitTaskHelper
// ----------------------------------------------

pub trait UnitTaskHelper {
    fn reset(&mut self);
    fn on_unit_spawn(&mut self, unit_id: UnitId, failed_to_spawn: bool);

    fn unit_id(&self) -> UnitId;
    fn failed_to_spawn(&self) -> bool;

    #[inline]
    fn is_spawned(&self) -> bool {
        self.unit_id().is_valid()
    }

    #[inline]
    fn try_unit<'world>(&self, query: &'world Query) -> Option<&'world Unit> {
        query.world().find_unit(self.unit_id())
    }

    #[inline]
    fn try_unit_mut<'world>(&mut self, query: &'world Query) -> Option<&'world mut Unit> {
        query.world().find_unit_mut(self.unit_id())
    }

    #[inline]
    fn unit<'world>(&self, query: &'world Query) -> &'world Unit {
        self.try_unit(query).unwrap()
    }

    #[inline]
    fn unit_mut<'world>(&mut self, query: &'world Query) -> &'world mut Unit {
        self.try_unit_mut(query).unwrap()
    }

    #[inline]
    fn is_running_task<Task>(&self, query: &Query) -> bool
        where Task: UnitTask + 'static
    {
        self.try_unit(query).is_some_and(|unit| unit.is_running_task::<Task>(query.task_manager()))
    }

    #[inline]
    fn try_spawn_with_task<Task>(&mut self,
                                 spawner_name: &str,
                                 query: &Query,
                                 unit_origin: Cell,
                                 unit_config: UnitConfigKey,
                                 task: Task)
                                 -> bool
        where Task: UnitTask,
              UnitTaskArchetype: From<Task>
    {
        debug_assert!(!self.is_spawned(), "Unit already spawned! reset() first.");

        match Unit::try_spawn_with_task(query, unit_origin, unit_config, task) {
            Ok(unit) => {
                self.on_unit_spawn(unit.id(), false);
                true
            }
            Err(err) => {
                log::error!(log::channel!("unit"), "{}: {}", spawner_name, err);
                self.on_unit_spawn(UnitId::invalid(), true);
                false
            }
        }
    }
}
