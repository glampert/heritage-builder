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
        sets::TileKind,
        map::{self, Tile, TileMap, TileMapLayerKind},
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
    building::BuildingTileInfo,
    sim::{
        Query,
        world::UnitId,
        resources::{ResourceKind, StockItem}
    }
};

pub mod anim;
pub mod config;
pub mod inventory;
pub mod navigation;
pub mod task;

use anim::*;
use config::*;
use inventory::*;
use navigation::*;
use task::*;

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
        self.navigation.reset(None, None);
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
        self.navigation.reset(path, None);
        if path.is_some() {
            self.debug.popup_msg("New Goal");
        }
    }

    #[inline]
    pub fn go_to_building(&mut self, path: &Path, origin: &BuildingTileInfo, destination: &BuildingTileInfo) {
        debug_assert!(self.is_spawned());

        let goal = UnitNavGoal {
            origin_kind: origin.kind,
            origin_cell: origin.base_cell,
            destination_kind: destination.kind,
            destination_cell: destination.base_cell,
            destination_road_link: destination.road_link,
        };

        self.navigation.reset(Some(path), Some(goal));
        self.log_going_to(origin.base_cell, destination.base_cell);
    }

    #[inline]
    pub fn has_reached_goal(&self) -> bool {
        self.navigation.goal().is_some_and(|goal| self.map_cell == goal.destination_road_link)
    }

    #[inline]
    pub fn goal(&self) -> Option<&UnitNavGoal> {
        self.navigation.goal()
    }

    pub fn update_navigation(&mut self, query: &Query) {
        debug_assert!(self.is_spawned());

        // Path following and movement:
        match self.navigation.update(query.tile_map(), query.delta_time_secs()) {
            UnitNavResult::Idle => {
                // Nothing.
            },
            UnitNavResult::Moving(from_cell, to_cell, progress, direction) => {
                let tile = find_unit_tile!(&mut self, query);

                let draw_size = tile.draw_size();
                let from_iso = map::calc_unit_iso_coords(from_cell, draw_size);
                let to_iso = map::calc_unit_iso_coords(to_cell, draw_size);

                let new_iso_coords = utils::lerp(from_iso, to_iso, progress);
                tile.set_iso_coords_f32(new_iso_coords);

                self.update_direction_and_anim(tile, direction);
            },
            UnitNavResult::AdvancedCell(cell, direction) => {
                let did_teleport = self.teleport(query.tile_map(), cell);
                debug_assert!(did_teleport, "Failed to advance unit tile cell!");

                let tile = find_unit_tile!(&mut self, query);
                debug_assert!(self.map_cell == cell && tile.base_cell() == cell);

                self.update_direction_and_anim(tile, direction);
            },
            UnitNavResult::ReachedGoal(cell, direction) => {
                let tile = find_unit_tile!(&mut self, query);

                debug_assert!(self.direction == direction);
                debug_assert!(self.map_cell == cell && tile.base_cell() == cell);

                self.debug.popup_msg("Reached Goal");

                // Clear current path.
                // NOTE: Not using follow_path(None) here to preserve the nav goals for the unit tasks.
                self.navigation.reset_path();

                // Go idle.
                self.update_direction_and_anim(tile, UnitDirection::Idle);
            },
            UnitNavResult::PathBlocked => {
                let tile = find_unit_tile!(&mut self, query);

                // Failed to move to another tile, possibly because it has been
                // blocked since we've traced the path. Clear the navigation and stop.
                // If a task is running it should now re-route the path and retry.
                self.follow_path(None);
                self.update_direction_and_anim(tile, UnitDirection::Idle);

                self.debug.popup_msg_color(Color::red(), "Blocked!");
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
                                     task: Task) -> Result<&'config mut Unit<'config>, &'static str>
        where
            Task: UnitTask,
            UnitTaskArchetype: From<Task>
    {
        // Handle root tasks here. These will start the task chain and might take some time to complete.

        let unit = match query.try_spawn_unit(unit_origin, unit_config) {
            Some(unit) => unit,
            None => return Err("Couldn't spawn new unit!"),
        };

        let task_manager = query.task_manager();
        let new_task_id = task_manager.new_task(task);
        unit.assign_task(task_manager, new_task_id);

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
    pub fn is_inventory_empty(&self) -> bool {
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

    // Tries to gives away up to `count` resources. Returns the number
    // of resources it was able to give, which can be less or equal to `count`.
    pub fn give_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(self.is_spawned());
        debug_assert!(kind.bits().count_ones() == 1);

        self.debug.log_resources_lost(kind, count);
        self.inventory.give_resources(kind, count)
    }

    // ----------------------
    // Internal helpers:
    // ----------------------

    fn update_direction_and_anim(&mut self, tile: &mut Tile, new_direction: UnitDirection) {
        if self.direction != new_direction {
            self.direction = new_direction;
            let new_anim_set_key = anim_set_for_direction(new_direction);
            self.anim_sets.set_anim(tile, new_anim_set_key);
        }
    }

    fn log_going_to(&mut self, origin: Cell, destination: Cell) {
        if !self.debug.show_popups() {
            return;
        }
        let origin_building_name = debug::tile_name_at(origin, TileMapLayerKind::Objects);
        let destination_building_name = debug::tile_name_at(destination, TileMapLayerKind::Objects);
        self.debug.popup_msg(format!("Goto: {} -> {}", origin_building_name, destination_building_name));
    }
}

// ----------------------------------------------
// Debug UI
// ----------------------------------------------

impl Unit<'_> {
    pub fn draw_debug_ui(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        /* TODO
        if ui.collapsing_header("Properties", imgui::TreeNodeFlags::empty()) {
            ...
        }

        if ui.collapsing_header("Config", imgui::TreeNodeFlags::empty()) {
            self.config.draw_debug_ui(ui_sys);
        }
        */

        self.debug.draw_debug_ui(ui_sys);
        self.inventory.draw_debug_ui(ui_sys);
        query.task_manager().draw_tasks_debug_ui(self, query, ui_sys);

        if ui.collapsing_header("Navigation", imgui::TreeNodeFlags::empty()) {
            if let Some(dir) = imgui_ui::dpad_buttons(ui) {
                let tile_map = query.tile_map();
                match dir {
                    DPadDirection::NE => {
                        self.teleport(tile_map, Cell::new(self.map_cell.x + 1, self.map_cell.y));
                    },
                    DPadDirection::NW => {
                        self.teleport(tile_map, Cell::new(self.map_cell.x, self.map_cell.y + 1));
                    },
                    DPadDirection::SE => {
                        self.teleport(tile_map, Cell::new(self.map_cell.x, self.map_cell.y - 1));
                    },
                    DPadDirection::SW => {
                        self.teleport(tile_map, Cell::new(self.map_cell.x - 1, self.map_cell.y));
                    },
                }
            }

            ui.separator();

            ui.text(format!("Cell       : {}", self.map_cell));
            ui.text(format!("Iso Coords : {}", find_unit_tile!(&self, query).iso_coords()));
            ui.text(format!("Direction  : {}", self.direction));
            ui.text(format!("Anim       : {}", self.anim_sets.current_anim().string));

            if ui.button("Force Idle Anim") {
                self.update_direction_and_anim(find_unit_tile!(&mut self, query), UnitDirection::Idle);
            }

            ui.separator();

            let color = match self.navigation.status() {
                UnitNavStatus::Idle   => Color::yellow(),
                UnitNavStatus::Paused => Color::red(),
                UnitNavStatus::Moving => Color::green(),
            };

            ui.text_colored(color.to_array(), format!("Path Navigation Status: {:?}", self.navigation.status()));

            if let Some(goals) = self.navigation.goal() {
                let origin_building_name = debug::tile_name_at(goals.origin_cell, TileMapLayerKind::Objects);
                let destination_building_name = debug::tile_name_at(goals.destination_cell, TileMapLayerKind::Objects);
                ui.text(format!("Start Building : {}, {}", goals.origin_cell, origin_building_name));
                ui.text(format!("Dest  Building : {}, {}", goals.destination_cell, destination_building_name));
            }

            self.navigation.draw_debug_ui(ui_sys);
        }
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
}
