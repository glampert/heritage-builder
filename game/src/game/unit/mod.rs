use crate::{
    game_object_debug_options,
    debug::{self as debug_utils},
    pathfind::Path,
    tile::{
        self,
        Tile,
        TileKind,
        TileMap,
        TileMapLayerKind
    },
    utils::{
        self,
        Color,
        coords::Cell
    }
};

use super::{
    building::Building,
    sim::{
        Query,
        world::UnitId,
        resources::{ResourceKind, ServiceKind, StockItem}
    }
};

use config::*;
use task::*;
use anim::*;
use inventory::*;
use navigation::*;

pub mod config;
pub mod patrol;
pub mod runner;
pub mod task;

mod anim;
mod debug;
mod inventory;
mod navigation;

// ----------------------------------------------
// UnitDebug
// ----------------------------------------------

game_object_debug_options! {
    UnitDebug,
}

// ----------------------------------------------
// Unit  
// ----------------------------------------------

/*
Common Unit Behavior:
 - Spawn and despawn dynamically.
 - Moves across the tile map, so map cell can change.
 - Transports resources from A to B (has a start point and a destination).
 - Patrols an area around its building to provide a service to households.
 - Most units will only walk on paved roads. Some units may go off-road.
 - Has an inventory that can cary a single ResourceKind at a time, any amount.
*/
#[derive(Clone, Default)]
pub struct Unit<'config> {
    config: Option<&'config UnitConfig>,
    map_cell: Cell,
    id: UnitId,
    direction: UnitDirection,
    anim_sets: UnitAnimSets,
    inventory: UnitInventory,
    navigation: UnitNavigation,
    current_task_id: UnitTaskId, // invalid if no task.
    debug: UnitDebug,
}

impl<'config> Unit<'config> {
    // ----------------------
    // Spawning / Despawning:
    // ----------------------

    pub fn new(tile: &mut Tile, config: &'config UnitConfig, id: UnitId) -> Self {
        let mut unit = Unit::default();
        unit.spawned(tile, config, id);
        unit
    }

    pub fn spawned(&mut self, tile: &mut Tile, config: &'config UnitConfig, id: UnitId) {
        debug_assert!(!self.is_spawned());
        debug_assert!(tile.is_valid());
        debug_assert!(id.is_valid());

        self.config    = Some(config);
        self.map_cell  = tile.base_cell();
        self.id        = id;
        self.direction = UnitDirection::Idle;

        self.anim_sets.set_anim(tile, UnitAnimSets::IDLE);
    }

    pub fn despawned(&mut self, task_manager: &mut UnitTaskManager) {
        debug_assert!(self.is_spawned());

        self.config    = None;
        self.map_cell  = Cell::default();
        self.id        = UnitId::default();
        self.direction = UnitDirection::default();

        self.anim_sets.clear();
        self.inventory.clear();
        self.navigation.reset_path_and_goal(None, None);
        self.debug.clear_popups();

        task_manager.free_task(self.current_task_id);
        self.current_task_id = UnitTaskId::default();
    }

    #[inline]
    pub fn is_spawned(&self) -> bool {
        self.id.is_valid()
    }

    #[inline]
    pub fn id(&self) -> UnitId {
        self.id
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

    // Teleports to new tile cell and updates direction and animation.
    pub fn teleport(&mut self, tile_map: &mut TileMap, destination_cell: Cell) -> bool {
        debug_assert!(self.is_spawned());
        if tile_map.try_move_tile(self.map_cell, destination_cell, TileMapLayerKind::Objects) {
            let tile = tile_map.find_tile_mut(
                destination_cell,
                TileMapLayerKind::Objects,
                TileKind::Unit)
                .unwrap();

            let new_direction = direction_between(self.map_cell, destination_cell);    
            self.update_direction_and_anim(tile, new_direction);

            debug_assert!(destination_cell == tile.base_cell());
            self.map_cell = destination_cell;
            return true;
        }
        false
    }

    #[inline]
    pub fn patrol_service_building(&self, query: &'config Query) -> Option<&mut Building<'config>> {
        if let Some(task) = self.current_task_as::<UnitTaskRandomizedPatrol>(query.task_manager()) {
            return query.world().find_building_mut(task.origin_building.kind, task.origin_building.id);
        }
        None
    }

    #[inline]
    pub fn patrol_service_kind(&self, query: &Query) -> Option<ServiceKind> {
        if let Some(task) = self.current_task_as::<UnitTaskRandomizedPatrol>(query.task_manager()) {
            return Some(task.origin_building.kind)
        }
        None
    }

    #[inline]
    pub fn is_market_patrol(&self, query: &Query) -> bool {
        self.patrol_service_kind(query).is_some_and(|kind| kind == ServiceKind::Market)
    }

    // ----------------------
    // Path Navigation:
    // ----------------------

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
        self.navigation.goal().is_some_and(|goal| {
            let destination_cell = match goal {
                UnitNavGoal::Building { destination_road_link, .. } => *destination_road_link,
                UnitNavGoal::Tile { destination_cell, .. } => *destination_cell,
            };
            self.cell() == destination_cell
        })
    }

    pub fn update_navigation(&mut self, query: &Query) {
        debug_assert!(self.is_spawned());

        // Path following and movement:
        match self.navigation.update(query.graph(), query.delta_time_secs()) {
            UnitNavResult::Idle => {
                // Nothing.
            },
            UnitNavResult::Moving(from_cell, to_cell, progress, direction) => {
                let tile = self.find_tile_mut(query);

                let draw_size = tile.draw_size();
                let from_iso = tile::calc_unit_iso_coords(from_cell, draw_size);
                let to_iso = tile::calc_unit_iso_coords(to_cell, draw_size);

                let new_iso_coords = utils::lerp(from_iso, to_iso, progress);
                tile.set_iso_coords_f32(new_iso_coords);

                self.update_direction_and_anim(tile, direction);
            },
            UnitNavResult::AdvancedCell(cell, direction) => {
                if !self.teleport(query.tile_map(), cell) {
                    // This would normally happen if two units try to move to the
                    // same tile, so they will bump into each other for one frame.
                    // Not a critical failure, the unit can recover next update.
                    self.debug.popup_msg_color(Color::yellow(), "Bump!");
                }

                self.update_direction_and_anim(self.find_tile_mut(query), direction);
            },
            UnitNavResult::ReachedGoal(cell, _) => {
                self.teleport(query.tile_map(), cell);

                if cell == self.cell() {
                    // Goal reached, clear current path.
                    // NOTE: Not using follow_path(None) here to preserve the nav goal for unit tasks.
                    self.navigation.reset_path_only();
                    self.debug.popup_msg_color(Color::green(), "Reached Goal!");
                } else {
                    // Path was blocked, retry task.
                    self.follow_path(None);
                    self.debug.popup_msg_color(Color::red(), "Goal Blocked!");
                }

                self.idle(query);
            },
            UnitNavResult::PathBlocked => {
                // Failed to move to another tile, possibly because it has been
                // blocked since we've traced the path. Clear the navigation and stop.
                // If a task is running it should now re-route the path and retry.
                self.follow_path(None);
                self.idle(query);

                self.debug.popup_msg_color(Color::red(), "Path Blocked!");
            },
        }
    }

    // ----------------------
    // Unit Behavior / Tasks:
    // ----------------------

    #[inline]
    pub fn update(&mut self, query: &Query) {
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
    pub fn current_task_as<'task, Task>(&self, task_manager: &'task UnitTaskManager) -> Option<&'task Task>
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

    pub fn try_spawn_with_task<Task>(query: &'config Query,
                                     unit_origin: Cell,
                                     unit_config: UnitConfigKey,
                                     task: Task) -> Result<&'config mut Unit<'config>, String>
        where Task: UnitTask,
              UnitTaskArchetype: From<Task>
    {
        debug_assert!(unit_origin.is_valid());
        debug_assert!(unit_config.is_valid());

        let task_manager = query.task_manager();
        let task_id = task_manager.new_task(task);

        let unit = match query.try_spawn_unit(unit_origin, unit_config) {
            Ok(unit) => unit,
            error @ Err(_) => {
                task_manager.free_task(task_id.unwrap());
                return error;
            },
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
    // Internal helpers:
    // ----------------------

    #[inline]
    fn find_tile<'a>(&self, query: &'a Query) -> &'a Tile<'a> {
        query.find_tile(self.map_cell, TileMapLayerKind::Objects, TileKind::Unit)
            .expect("Unit should have an associated Tile in the TileMap!")
    }

    #[inline]
    fn find_tile_mut<'a>(&self, query: &'a Query) -> &'a mut Tile<'a> {
        query.find_tile_mut(self.map_cell, TileMapLayerKind::Objects, TileKind::Unit)
            .expect("Unit should have an associated Tile in the TileMap!")
    }

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

        let (origin_cell, destination_cell, layer) = match goal {
            UnitNavGoal::Building { origin_base_cell, destination_base_cell, .. } => {
                (*origin_base_cell, *destination_base_cell, TileMapLayerKind::Objects)
            },
            UnitNavGoal::Tile { origin_cell, destination_cell } => {
                (*origin_cell, *destination_cell, TileMapLayerKind::Terrain)
            },
        };

        let origin_tile_name = debug_utils::tile_name_at(origin_cell, layer);
        let destination_tile_name = debug_utils::tile_name_at(destination_cell, layer);

        self.debug.popup_msg(format!("Goto: {} -> {}", origin_tile_name, destination_tile_name));
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
    fn try_unit<'config>(&self, query: &'config Query) -> Option<&'config Unit<'config>> {
        if self.unit_id().is_valid() {
            query.world().find_unit(self.unit_id())
        } else {
            None
        }
    }

    #[inline]
    fn try_unit_mut<'config>(&mut self, query: &'config Query) -> Option<&'config mut Unit<'config>> {
        if self.unit_id().is_valid() {
            query.world().find_unit_mut(self.unit_id())
        } else {
            None
        }
    }

    #[inline]
    fn unit<'config>(&self, query: &'config Query) -> &'config Unit<'config> {
        self.try_unit(query).unwrap()
    }

    #[inline]
    fn unit_mut<'config>(&mut self, query: &'config Query) -> &'config mut Unit<'config> {
        self.try_unit_mut(query).unwrap()
    }

    #[inline]
    fn is_running_task<Task>(&self, query: &Query) -> bool
        where Task: UnitTask + 'static
    {
        self.try_unit(query).is_some_and(|unit| {
            unit.is_running_task::<Task>(query.task_manager())
        })
    }

    #[inline]
    fn try_spawn_with_task<Task>(&mut self,
                                 spawner_name: &str,
                                 query: &Query,
                                 unit_origin: Cell,
                                 unit_config: UnitConfigKey,
                                 task: Task) -> bool
        where Task: UnitTask,
              UnitTaskArchetype: From<Task>
    {
        debug_assert!(!self.is_spawned(), "Unit already spawned! reset() first.");

        match Unit::try_spawn_with_task(query, unit_origin, unit_config, task) {
            Ok(unit) => {
                self.on_unit_spawn(unit.id(), false);
                true
            },
            Err(err) => {
                eprintln!("{}: Failed to spawn Unit at cell {}: {}", spawner_name, unit_origin, err);
                self.on_unit_spawn(UnitId::invalid(), true);
                false
            },
        }
    }
}
