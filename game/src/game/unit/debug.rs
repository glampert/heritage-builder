use bitflags::Flags;
use rand::Rng;
use smallvec::SmallVec;
use proc_macros::DrawDebugUi;

use crate::{
    debug::{self as debug_utils},
    tile::TileMapLayerKind,
    pathfind::{self, Path, NodeKind as PathNodeKind},
    imgui_ui::{
        self,
        UiSystem,
        DPadDirection
    },
    utils::{
        Color,
        coords::{
            Cell,
            CellRange,
            WorldToScreenTransform
        }
    },
    game::{
        building::{
            Building,
            BuildingKind,
            BuildingKindAndId,
            BuildingTileInfo
        },
        sim::{
            Query,
            world::UnitId,
            debug::GameObjectDebugOptions,
            resources::{
                ResourceKind,
                ShoppingList,
                StockItem
            }
        }
    }
};

use super::{
    Unit,
    task::*,
    navigation::*
};

// ----------------------------------------------
// Unit Debug UI
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
        let tile = self.find_tile(query);
        self.debug.draw_popup_messages(
            || tile,
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
        ui.text(format!("Iso Coords : {}", self.find_tile(query).iso_coords()));
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

            let origin_tile_name = debug_utils::tile_name_at(origin_cell, layer);
            let destination_tile_name = debug_utils::tile_name_at(destination_cell, layer);

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

        let teleport_if_needed = |unit: &mut Unit, destination_cell: Cell| {
            if unit.cell() == destination_cell {
                return true;
            }
            unit.teleport(query.tile_map(), destination_cell)
        };

        ui.separator();
        ui.text("Runner Tasks:");

        if ui.button("Give Deliver Resources Task") {
            // We need a building to own the task, so this assumes there's at least one of these placed on the map.
            if let Some(building) = world.find_building_by_name("Market", BuildingKind::Market) {
                let start_cell = building.road_link(query).unwrap_or_default();
                if teleport_if_needed(self, start_cell) {
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
                        resource_kind_to_deliver: ResourceKind::random_food(&mut rand::rng()),
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
                let resources_to_fetch = ShoppingList::from_items(
                    &[StockItem { kind: ResourceKind::random(&mut rng), count: rng.random_range(1..5) }]
                );
                let start_cell = building.road_link(query).unwrap_or_default();
                if teleport_if_needed(self, start_cell) {
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

        static mut PATROL_ROUNDS: i32 = 5;

        #[allow(static_mut_refs)]
        let (max_patrol_distance, path_bias_min, path_bias_max) = unsafe {
            // SAFETY: Debug code only called from the main thread (ImGui is inherently single-threaded).
            static mut MAX_PATROL_DISTANCE: i32 = 50;
            static mut PATH_BIAS_MIN: f32 = 0.1;
            static mut PATH_BIAS_MAX: f32 = 0.5;

            ui.input_int("Patrol Rounds", &mut PATROL_ROUNDS).step(1).build();
            ui.input_int("Patrol Max Distance", &mut MAX_PATROL_DISTANCE).step(1).build();
            ui.input_float("Patrol Path Bias Min", &mut PATH_BIAS_MIN).display_format("%.2f").step(0.1).build();
            ui.input_float("Patrol Path Bias Max", &mut PATH_BIAS_MAX).display_format("%.2f").step(0.1).build();

            (MAX_PATROL_DISTANCE, PATH_BIAS_MIN, PATH_BIAS_MAX)
        };

        if ui.button("Give Patrol Task") {
            // We need a building to own the task, so this assumes there's at least one of these placed on the map.
            if let Some(building) = world.find_building_by_name("Market", BuildingKind::Market) {
                let start_cell = building.road_link(query).unwrap_or_default();
                if teleport_if_needed(self, start_cell) {
                    let completion_task = task_manager.new_task(UnitTaskDespawn);
                    let task = task_manager.new_task(UnitTaskRandomizedPatrol {
                        origin_building: BuildingKindAndId {
                            kind: building.kind(),
                            id: building.id(),
                        },
                        origin_building_tile: BuildingTileInfo {
                            road_link: start_cell,
                            base_cell: building.base_cell(),
                        },
                        max_distance: max_patrol_distance,
                        path_bias_min,
                        path_bias_max,
                        path_record: UnitPatrolPathRecord::default(),
                        buildings_to_visit: Some(BuildingKind::House),
                        completion_callback: Some(|_, _, _| {
                            unsafe {
                                let patrol_round = PATROL_ROUNDS;
                                println!("Patrol Task Round {patrol_round} Completed.");
                                PATROL_ROUNDS -= 1;
                                PATROL_ROUNDS <= 0 // Run the task a few times.
                            }
                        }),
                        completion_task,
                    });
                    self.assign_task(task_manager, task);
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

        ui.separator();
        ui.text("Path Finding:");

        #[allow(static_mut_refs)]
        let (use_road_paths, use_dirt_paths, max_search_distance, building_kind) = unsafe {
            // SAFETY: Debug code only called from the main thread (ImGui is inherently single-threaded).
            static mut USE_ROAD_PATHS: bool = true;
            static mut USE_DIRT_PATHS: bool = false;
            static mut MAX_SEARCH_DISTANCE: i32 = 50;
            static mut BUILDING_KIND_IDX: usize = 0;

            let mut building_kind_names: SmallVec<[&'static str; BuildingKind::count()]> = SmallVec::new();
            for kind in BuildingKind::FLAGS {
                building_kind_names.push(kind.name());
            }

            ui.checkbox("Road Paths", &mut USE_ROAD_PATHS);
            ui.checkbox("Dirt Paths", &mut USE_DIRT_PATHS);
            ui.input_int("Max Search Distance", &mut MAX_SEARCH_DISTANCE).step(1).build();
            ui.combo_simple_string("Dest Building Kind", &mut BUILDING_KIND_IDX, &building_kind_names);

            (USE_ROAD_PATHS, USE_DIRT_PATHS, Some(MAX_SEARCH_DISTANCE), *BuildingKind::FLAGS[BUILDING_KIND_IDX].value())
        };

        if ui.button(format!("Path To Nearest Building ({})", building_kind)) {
            let mut traversable_node_kinds = PathNodeKind::empty();
            if use_road_paths {
                traversable_node_kinds |= PathNodeKind::Road;
            }
            if use_dirt_paths {
                traversable_node_kinds |= PathNodeKind::Dirt;
            }

            if !traversable_node_kinds.is_empty() {
                let start = self.cell();
                self.navigation.set_traversable_node_kinds(traversable_node_kinds);

                let visit_building = |building: &Building, path: &Path| -> bool {
                    let tile_map = query.tile_map();

                    println!("{} '{}' found. Path len: {}", building.kind(), building.name(), path.len());
                    debug_assert!(building.is(building_kind)); // The building we're looking for.

                    // Highlight the path to take:
                    pathfind::highlight_path_tiles(tile_map, path);

                    // Highlight building access tiles and road link:
                    pathfind::highlight_building_access_tiles(tile_map, building.cell_range());
                    building.set_show_road_link_debug(query, true);

                    self.follow_path(Some(path));
                    false // done
                };

                query.find_nearest_buildings(start,
                                             building_kind,
                                             traversable_node_kinds,
                                             max_search_distance,
                                             visit_building);
            }
        }
    }
}
