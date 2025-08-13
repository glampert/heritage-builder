use rand::Rng;
use proc_macros::DrawDebugUi;

use crate::{
    game_object_debug_options,
    pathfind::Path,
    debug::{self},
    imgui_ui::{
        self,
        UiSystem,
        DPadDirection
    },
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
        coords::{
            Cell,
            CellRange,
            WorldToScreenTransform
        }
    }
};

use super::{
    building::{
        BuildingKind,
        BuildingKindAndId,
        BuildingTileInfo
    },
    sim::{
        Query,
        world::UnitId,
        resources::{
            RESOURCE_KIND_COUNT,
            ResourceKind,
            ShoppingList,
            StockItem
        }
    }
};

use config::*;
use task::*;
use anim::*;
use inventory::*;
use navigation::*;

pub mod config;
pub mod runner;
pub mod task;
mod anim;
mod inventory;
mod navigation;

// ----------------------------------------------
// Helper Macros
// ----------------------------------------------

macro_rules! find_unit_tile {
    (&$unit:ident, $query:ident) => {
        $query.find_tile($unit.map_cell, TileMapLayerKind::Objects, TileKind::Unit)
            .expect("Unit should have an associated Tile in the TileMap!")
    };
    (&mut $unit:ident, $query:ident) => {
        $query.find_tile_mut($unit.map_cell, TileMapLayerKind::Objects, TileKind::Unit)
            .expect("Unit should have an associated Tile in the TileMap!")
    };
}

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
                let tile = find_unit_tile!(&mut self, query);

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

                self.update_direction_and_anim(find_unit_tile!(&mut self, query), direction);
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
        where
            Task: UnitTask + 'static
    {
        debug_assert!(self.is_spawned());
        task_manager.is_task::<Task>(self.current_task_id)
    }

    #[inline]
    pub fn current_task_as<'task, Task>(&self, task_manager: &'task UnitTaskManager) -> Option<&'task Task>
        where
            Task: UnitTask + 'static
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
        where
            Task: UnitTask,
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
        debug_assert!(kind.bits().count_ones() == 1);

        self.debug.log_resources_gained(kind, count);
        self.inventory.receive_resources(kind, count)
    }

    // Tries to relinquish up to `count` resources. Returns the number of
    // resources it was able to relinquish, which can be less or equal to `count`.
    pub fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(self.is_spawned());
        debug_assert!(kind.bits().count_ones() == 1);

        let removed_count = self.inventory.remove_resources(kind, count);
        if removed_count != 0 {
            self.debug.log_resources_lost(kind, removed_count);
        }
        removed_count
    }

    // ----------------------
    // Internal helpers:
    // ----------------------

    fn idle(&mut self, query: &Query) {
        self.update_direction_and_anim(find_unit_tile!(&mut self, query), UnitDirection::Idle);
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

        let origin_tile_name = debug::tile_name_at(origin_cell, layer);
        let destination_tile_name = debug::tile_name_at(destination_cell, layer);

        self.debug.popup_msg(format!("Goto: {} -> {}", origin_tile_name, destination_tile_name));
    }
}

// ----------------------------------------------
// Debug UI
// ----------------------------------------------

impl Unit<'_> {
    pub fn draw_debug_ui(&mut self, query: &Query, ui_sys: &UiSystem) {
        self.draw_debug_ui_properties(ui_sys);
        self.draw_debug_ui_config(ui_sys);
        self.debug.draw_debug_ui(ui_sys);
        self.inventory.draw_debug_ui(ui_sys);
        query.task_manager().draw_tasks_debug_ui(self, query, ui_sys);
        self.draw_debug_ui_navigation(query, ui_sys);
        self.draw_debug_ui_misc(query, ui_sys);
    }

    pub fn draw_debug_popups(&mut self,
                             query: &Query,
                             ui_sys: &UiSystem,
                             transform: &WorldToScreenTransform,
                             visible_range: CellRange) {
        self.debug.draw_popup_messages(
            || find_unit_tile!(&self, query),
            ui_sys,
            transform,
            visible_range,
            query.delta_time_secs());
    }

    fn draw_debug_ui_properties(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        // NOTE: Use the special ##id here so we don't collide with Tile/Properties.
        if !ui.collapsing_header("Properties##_unit_properties", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        #[derive(DrawDebugUi)]
        struct DrawDebugUiVariables<'a> {
            name: &'a str,
            cell: Cell,
            id: UnitId,
        }
        let debug_vars = DrawDebugUiVariables {
            name: self.name(),
            cell: self.cell(),
            id: self.id(),
        };
        debug_vars.draw_debug_ui(ui_sys);
    }

    fn draw_debug_ui_config(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        if let Some(config) = self.config {
            if ui.collapsing_header("Config", imgui::TreeNodeFlags::empty()) {
                config.draw_debug_ui(ui_sys);
            }
        }
    }

    fn draw_debug_ui_navigation(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        if !ui.collapsing_header("Navigation", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if let Some(dir) = imgui_ui::dpad_buttons(ui) {
            let tile_map = query.tile_map();
            match dir {
                DPadDirection::NE => {
                    self.teleport(tile_map, Cell::new(self.cell().x + 1, self.cell().y));
                },
                DPadDirection::NW => {
                    self.teleport(tile_map, Cell::new(self.cell().x, self.cell().y + 1));
                },
                DPadDirection::SE => {
                    self.teleport(tile_map, Cell::new(self.cell().x, self.cell().y - 1));
                },
                DPadDirection::SW => {
                    self.teleport(tile_map, Cell::new(self.cell().x - 1, self.cell().y));
                },
            }
        }

        ui.separator();

        ui.text(format!("Cell       : {}", self.cell()));
        ui.text(format!("Iso Coords : {}", find_unit_tile!(&self, query).iso_coords()));
        ui.text(format!("Direction  : {}", self.direction));
        ui.text(format!("Anim       : {}", self.anim_sets.current_anim().string));

        if ui.button("Force Idle Anim") {
            self.idle(query);
        }

        ui.separator();

        let color = match self.navigation.status() {
            UnitNavStatus::Idle   => Color::yellow(),
            UnitNavStatus::Paused => Color::red(),
            UnitNavStatus::Moving => Color::green(),
        };

        ui.text_colored(color.to_array(), format!("Path Navigation Status: {:?}", self.navigation.status()));

        if let Some(goal) = self.navigation.goal() {
            let (origin_cell, destination_cell, layer) = match goal {
                UnitNavGoal::Building { origin_base_cell, destination_base_cell, .. } => {
                    (*origin_base_cell, *destination_base_cell, TileMapLayerKind::Objects)
                },
                UnitNavGoal::Tile { origin_cell, destination_cell } => {
                    (*origin_cell, *destination_cell, TileMapLayerKind::Terrain)
                },
            };

            let origin_tile_name = debug::tile_name_at(origin_cell, layer);
            let destination_tile_name = debug::tile_name_at(destination_cell, layer);

            ui.text(format!("Start Tile : {}, {}", origin_cell, origin_tile_name));
            ui.text(format!("Dest  Tile : {}, {}", destination_cell, destination_tile_name));
        }

        self.navigation.draw_debug_ui(ui_sys);
    }

    fn draw_debug_ui_misc(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        if !ui.collapsing_header("Debug Misc", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if ui.button("Say Hello") {
            self.debug.popup_msg("Hello!");
        }

        let task_manager = query.task_manager();
        let world = query.world();

        if ui.button("Clear Current Task") {
            self.assign_task(task_manager, None);
            self.follow_path(None);
        }

        ui.separator();
        ui.text("Runner Tasks:");

        if ui.button("Give Deliver Resources Task") {
            // We need a building to own the task, so this assumes there's at least one of these placed on the map.
            if let Some(building) = world.find_building_by_name("Market", BuildingKind::Market) {
                let start_cell = building.find_nearest_road_link(query).unwrap_or_default();
                if self.teleport(query.tile_map(), start_cell) {
                    let completion_task = task_manager.new_task(UnitTaskDespawn);
                    let task = task_manager.new_task(UnitTaskDeliverToStorage {
                        origin_building: BuildingKindAndId {
                            kind: building.kind(),
                            id: building.id(),
                        },
                        origin_building_tile: BuildingTileInfo {
                            road_link: start_cell,
                            base_cell: building.base_cell(),
                        },
                        storage_buildings_accepted: BuildingKind::storage(), // any storage
                        resource_kind_to_deliver: ResourceKind::random(&mut rand::rng()),
                        resource_count: 1,
                        completion_callback: Some(|_, _, _| {
                            println!("Deliver Resources Task Completed.");
                        }),
                        completion_task,
                        allow_producer_fallback: true,
                    });
                    self.assign_task(task_manager, task);
                }
            }
        }

        if ui.button("Give Fetch Resources Task") {
            // We need a building to own the task, so this assumes there's at least one of these placed on the map.
            if let Some(building) = world.find_building_by_name("Market", BuildingKind::Market) {
                let mut rng = rand::rng();
                let resources_to_fetch = ShoppingList::from(
                    [StockItem { kind: ResourceKind::random(&mut rng), count: rng.random_range(1..5) }; RESOURCE_KIND_COUNT]
                );
                let start_cell = building.find_nearest_road_link(query).unwrap_or_default();
                if self.teleport(query.tile_map(), start_cell) {
                    let completion_task = task_manager.new_task(UnitTaskDespawn);
                    let task = task_manager.new_task(UnitTaskFetchFromStorage {
                        origin_building: BuildingKindAndId {
                            kind: building.kind(),
                            id: building.id(),
                        },
                        origin_building_tile: BuildingTileInfo {
                            road_link: start_cell,
                            base_cell: building.base_cell(),
                        },
                        storage_buildings_accepted: BuildingKind::storage(), // any storage
                        resources_to_fetch,
                        completion_callback: Some(|_, unit, _| {
                            let item = unit.inventory.peek().unwrap();
                            println!("Fetch Resources Task Completed. Got {}, {}", item.kind, item.count);
                            unit.inventory.clear();
                        }),
                        completion_task,
                    });
                    self.assign_task(task_manager, task);
                }
            }
        }

        ui.separator();
        ui.text("Patrol Task:");

        #[allow(static_mut_refs)]
        unsafe {
            // SAFETY: Debug code only called from the main thread (ImGui is inherently single-threaded).
            static mut PATROL_ROUNDS: i32 = 5;
            static mut MAX_DISTANCE:  i32 = 10;
            static mut BIAS_MIN: f32 = 0.1;
            static mut BIAS_MAX: f32 = 0.5;

            ui.input_int("Patrol Rounds", &mut PATROL_ROUNDS).step(1).build();
            ui.input_int("Patrol Max Distance", &mut MAX_DISTANCE).step(1).build();
            ui.input_float("Patrol Path Bias Min", &mut BIAS_MIN).display_format("%.2f").step(0.1).build();
            ui.input_float("Patrol Path Bias Max", &mut BIAS_MAX).display_format("%.2f").step(0.1).build();

            if ui.button("Give Patrol Task") {
                // We need a building to own the task, so this assumes there's at least one of these placed on the map.
                if let Some(building) = world.find_building_by_name("Market", BuildingKind::Market) {
                    let start_cell = building.find_nearest_road_link(query).unwrap_or_default();
                    if self.teleport(query.tile_map(), start_cell) {
                        let completion_task = task_manager.new_task(UnitTaskDespawn);
                        let task = task_manager.new_task(UnitTaskPatrol {
                            origin_building: BuildingKindAndId {
                                kind: building.kind(),
                                id: building.id(),
                            },
                            origin_building_tile: BuildingTileInfo {
                                road_link: start_cell,
                                base_cell: building.base_cell(),
                            },
                            max_distance: MAX_DISTANCE,
                            path_bias_min: BIAS_MIN,
                            path_bias_max: BIAS_MAX,
                            path_record: UnitPatrolPathRecord::default(),
                            completion_callback: Some(|_, _, _| {
                                println!("Patrol Task Round {} Completed.", PATROL_ROUNDS);
                                PATROL_ROUNDS -= 1;
                                PATROL_ROUNDS <= 0 // Run the task a few times.
                            }),
                            completion_task,
                        });
                        self.assign_task(task_manager, task);
                    }
                }
            }
        }

        ui.separator();
        ui.text("Despawn Task:");

        if ui.button("Give Despawn Task") {
            let task = task_manager.new_task(UnitTaskDespawn);
            self.assign_task(task_manager, task);
        }

        if ui.button("Force Despawn Immediately") {
            query.despawn_unit(self);
        }
    }
}
