use bitflags::Flags;
use rand::Rng;
use smallvec::SmallVec;
use proc_macros::DrawDebugUi;

use crate::{
    pathfind::{self, Path, NodeKind as PathNodeKind},
    tile::{self, TileMapLayerKind},
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
            debug::{
                GameObjectDebugOptions,
                GameObjectDebugOptionsExt
            },
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
        self.draw_debug_ui_tasks(query, ui_sys);
        self.draw_debug_ui_navigation(query, ui_sys);
        self.draw_debug_ui_misc(query, ui_sys);
    }

    pub fn draw_debug_popups(&mut self,
                             query: &Query,
                             ui_sys: &UiSystem,
                             transform: &WorldToScreenTransform,
                             visible_range: CellRange) {
        self.debug.draw_popup_messages(
            self.find_tile(query),
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

    fn draw_debug_ui_tasks(&mut self, query: &Query, ui_sys: &UiSystem) {
        query.task_manager().draw_tasks_debug_ui(self, query, ui_sys);
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
            ui.text(format!("Start Tile : {}, {}", goal.origin_cell(), goal.origin_debug_name()));
            ui.text(format!("Dest  Tile : {}, {}", goal.destination_cell(), goal.destination_debug_name()));
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

        if ui.button("Clear Current Task") {
            self.assign_task(query.task_manager(), None);
            self.follow_path(None);
        }

        self.debug_dropdown_despawn_tasks(query, ui_sys);
        self.debug_dropdown_runner_tasks(query, ui_sys);
        self.debug_dropdown_patrol_tasks(query, ui_sys);
        self.debug_dropdown_pathfinding_tasks(query, ui_sys);
    }

    fn teleport_if_needed(&mut self, query: &Query, destination_cell: Cell) -> bool {
        if self.cell() == destination_cell {
            return true;
        }
        self.teleport(query.tile_map(), destination_cell)
    }

    fn debug_dropdown_despawn_tasks(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if !ui.collapsing_header("Despawn Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if ui.button("Give Despawn Task") {
            let task = query.task_manager().new_task(UnitTaskDespawn);
            self.assign_task(query.task_manager(), task);
        }

        if ui.button("Force Despawn Immediately") {
            query.despawn_unit(self);
        }
    }

    fn debug_dropdown_runner_tasks(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if !ui.collapsing_header("Runner Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let task_manager = query.task_manager();
        let world = query.world();

        if ui.button("Give Deliver Resources Task") {
            // We need a building to own the task, so this assumes there's at least one of these placed on the map.
            if let Some(building) = world.find_building_by_name("Market", BuildingKind::Market) {
                let start_cell = building.road_link(query).unwrap_or_default();
                if self.teleport_if_needed(query, start_cell) {
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
                if self.teleport_if_needed(query, start_cell) {
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
    }

    fn debug_dropdown_patrol_tasks(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if !ui.collapsing_header("Patrol Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

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

        let task_manager = query.task_manager();
        let world = query.world();

        if ui.button("Give Patrol Task") {
            // We need a building to own the task, so this assumes there's at least one of these placed on the map.
            if let Some(building) = world.find_building_by_name("Market", BuildingKind::Market) {
                let start_cell = building.road_link(query).unwrap_or_default();
                if self.teleport_if_needed(query, start_cell) {
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
    }

    fn debug_dropdown_pathfinding_tasks(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if !ui.collapsing_header("Pathfind Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        #[allow(static_mut_refs)]
        let (traversable_node_kinds, max_search_distance, search_building_kind) = unsafe {
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

            let mut traversable_node_kinds = PathNodeKind::empty();
            if USE_ROAD_PATHS {
                traversable_node_kinds |= PathNodeKind::Road;
            }
            if USE_DIRT_PATHS {
                traversable_node_kinds |= PathNodeKind::Dirt;
            }

            (traversable_node_kinds, MAX_SEARCH_DISTANCE, *BuildingKind::FLAGS[BUILDING_KIND_IDX].value())
        };

        if ui.button(format!("Path To Nearest Building ({})", search_building_kind)) && !traversable_node_kinds.is_empty() {
            let start = self.cell();
            self.set_traversable_node_kinds(traversable_node_kinds);

            let visit_building = |building: &Building, path: &Path| -> bool {
                let tile_map = query.tile_map();

                println!("{} '{}' found. Path len: {}", building.kind(), building.name(), path.len());
                debug_assert!(building.is(search_building_kind)); // The building we're looking for.

                // Highlight the path to take:
                pathfind::highlight_path_tiles(tile_map, path);

                // Highlight building access tiles and road link:
                pathfind::highlight_building_access_tiles(tile_map, building.cell_range());
                building.set_show_road_link_debug(query, true);

                self.follow_path(Some(path));
                false // done
            };

            query.find_nearest_buildings(start,
                                         search_building_kind,
                                         traversable_node_kinds,
                                         Some(max_search_distance),
                                         visit_building);
        }

        if ui.button(format!("Test Is Near Building ({})", search_building_kind)) {
            let connected_to_road_only =
                 traversable_node_kinds.intersects(PathNodeKind::Road) &&
                !traversable_node_kinds.intersects(PathNodeKind::Dirt);

            let is_near = query.is_near_building(self.cell(),
                                                 search_building_kind,
                                                 connected_to_road_only,
                                                 max_search_distance);
            if is_near {
                self.debug.popup_msg_color(Color::green(), format!("{}: Near {}!", self.cell(), search_building_kind));
            } else {
                self.debug.popup_msg_color(Color::red(), format!("{}: Not near {}!", self.cell(), search_building_kind));
            }
        }

        let task_manager = query.task_manager();

        if ui.button("Find Vacant House Lot") && !traversable_node_kinds.is_empty() {
            self.set_traversable_node_kinds(traversable_node_kinds | PathNodeKind::VacantLot);

            let completion_task = task_manager.new_task(UnitTaskDespawn);
            let task = task_manager.new_task(UnitTaskFindVacantHouseLot {
                completion_callback: Some(|unit, vacant_lot, _| {
                    println!("Unit {} reached {}.", unit.name(), vacant_lot.name());
                    unit.debug.popup_msg("Reached vacant lot");
                }),
                completion_task,
            });

            self.assign_task(task_manager, task);
        }

        if ui.button("Find & Settle Vacant House Lot") && !traversable_node_kinds.is_empty() {
            self.set_traversable_node_kinds(traversable_node_kinds | PathNodeKind::VacantLot);

            let completion_task = task_manager.new_task(UnitTaskDespawnWithCallback {
                callback: Some(|query, unit_prev_cell| {
                    if let Some(tile_def) = query.tile_sets().find_tile_def_by_name(
                        TileMapLayerKind::Objects,
                        tile::sets::OBJECTS_BUILDINGS_CATEGORY.string,
                        "house0")
                    {
                        if let Err(err) = query.world().try_spawn_building_with_tile_def(
                            query.tile_map(),
                            unit_prev_cell,
                            tile_def)
                        {
                            eprintln!("Failed to place House Level 0: {err}");
                        }
                    } else {
                        eprintln!("House Level 0 TileDef not found!");
                    }
                })
            });

            let task = task_manager.new_task(UnitTaskFindVacantHouseLot {
                completion_callback: Some(|unit, vacant_lot, _| {
                    println!("Unit {} reached {}.", unit.name(), vacant_lot.name());
                    unit.debug.popup_msg("Reached vacant lot");
                }),
                completion_task,
            });

            self.assign_task(task_manager, task);
        }
    }
}
