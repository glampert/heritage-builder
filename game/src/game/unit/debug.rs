use rand::Rng;
use bitflags::Flags;
use smallvec::SmallVec;
use proc_macros::DrawDebugUi;

use super::{
    navigation::{self, *},
    task::*,
    Unit, UnitId,
};
use crate::{
    log,
    game::{
        prop::PropId,
        building::{Building, BuildingKind, BuildingKindAndId, BuildingTileInfo},
        sim::{
            resources::{ResourceKind, ShoppingList, StockItem},
            Query,
        },
        world::{
            debug::{GameObjectDebugOptions, GameObjectDebugOptionsExt},
            object::{GameObject, Spawner},
        },
    },
    engine::time::CountdownTimer,
    imgui_ui::{self, UiDPadDirection, UiSystem, UiStaticVar},
    pathfind::{self, NodeKind as PathNodeKind, Path},
    tile::{
        self, Tile, TileMapLayerKind, TilePoolIndex,
        minimap::{MinimapIcon, MINIMAP_ICON_DEFAULT_LIFETIME}
    },
    utils::{
        callback::{self, Callback},
        coords::Cell,
        hash, Color,
    },
};

// ----------------------------------------------
// Unit Debug UI
// ----------------------------------------------

impl Unit {
    pub fn draw_debug_ui_overview(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let font = ui.push_font(ui_sys.fonts().dev_large);
        ui.text(format!("{} | ID{} @{}", self.name(), self.id(), self.cell()));
        font.pop();

        ui.bullet_text(format!("Anim: {} (dir: {})",
                               self.anim_sets.current_anim_name(),
                               self.direction));

        if let Some(task_id) = self.current_task() {
            if let Some((archetype, state)) =
                query.task_manager().try_get_task_archetype_and_state(task_id)
            {
                ui.bullet_text(format!("Task: {archetype} | {state}"));
            }
        }

        if let Some(goal) = self.goal() {
            ui.bullet_text(format!("Traversable: {}", self.traversable_node_kinds()));
            ui.bullet_text(format!("Goal: {}", goal.destination_debug_name()));
            if self.has_reached_goal() {
                ui.bullet_text(format!("Reached: yes (nav: {:?})", self.navigation.status()));
            } else {
                ui.bullet_text(format!("Reached: no (nav: {:?})", self.navigation.status()));
            }
        }

        if let Some(item) = self.peek_inventory() {
            ui.bullet_text(format!("Inventory: {} ({})", item.kind, item.count));
        }
    }

    pub fn draw_debug_ui_detailed(&mut self, query: &Query, ui_sys: &UiSystem) {
        self.draw_debug_ui_properties(ui_sys);
        self.draw_debug_ui_config(ui_sys);
        self.debug.draw_debug_ui(ui_sys);
        self.inventory.draw_debug_ui(ui_sys);
        self.draw_debug_ui_tasks(query, ui_sys);
        self.draw_debug_ui_navigation(query, ui_sys);
        self.draw_debug_ui_misc(query, ui_sys);
    }

    fn draw_debug_ui_properties(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        // NOTE: Use the special ##id here so we don't collide with Tile/Properties.
        if !ui.collapsing_header("Properties##_unit_properties", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        #[derive(DrawDebugUi)]
        struct DrawDebugUiVariables<'a> {
            name: &'a str,
            cell: Cell,
            id: UnitId,
            tile_index: TilePoolIndex,
        }
        let debug_vars = DrawDebugUiVariables {
            name: self.name(),
            cell: self.cell(),
            id: self.id(),
            tile_index: self.tile_index()
        };
        debug_vars.draw_debug_ui(ui_sys);
    }

    fn draw_debug_ui_config(&mut self, ui_sys: &UiSystem) {
        if let Some(config) = self.config {
            config.draw_debug_ui_with_header("Config", ui_sys);
        }
    }

    fn draw_debug_ui_tasks(&mut self, query: &Query, ui_sys: &UiSystem) {
        query.task_manager().draw_tasks_debug_ui(self, query, ui_sys);
    }

    fn draw_debug_ui_navigation(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        if !ui.collapsing_header("Navigation", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if let Some(dir) = imgui_ui::dpad_buttons(ui) {
            let tile_map = query.tile_map();
            match dir {
                UiDPadDirection::NE => {
                    self.teleport(tile_map, Cell::new(self.cell().x + 1, self.cell().y));
                }
                UiDPadDirection::NW => {
                    self.teleport(tile_map, Cell::new(self.cell().x, self.cell().y + 1));
                }
                UiDPadDirection::SE => {
                    self.teleport(tile_map, Cell::new(self.cell().x, self.cell().y - 1));
                }
                UiDPadDirection::SW => {
                    self.teleport(tile_map, Cell::new(self.cell().x - 1, self.cell().y));
                }
            }
        }

        ui.separator();

        ui.text(format!("Cell       : {}", self.cell()));
        ui.text(format!("Iso Coords : {}", self.find_tile(query).iso_coords()));
        ui.text(format!("Direction  : {}", self.direction));
        ui.text(format!("Anim       : {}", self.anim_sets.current_anim_name()));

        if ui.button("Force Idle Anim") {
            self.idle(query);
        }

        ui.separator();

        let color = match self.navigation.status() {
            UnitNavStatus::Idle => Color::yellow(),
            UnitNavStatus::Paused => Color::red(),
            UnitNavStatus::Moving => Color::green(),
        };

        ui.text_colored(color.to_array(),
                        format!("Path Navigation Status: {:?}", self.navigation.status()));

        if let Some(goal) = self.navigation.goal() {
            ui.text(format!("Start Tile : {}, {}", goal.origin_cell(), goal.origin_debug_name()));
            ui.text(format!("Dest  Tile : {}, {}",
                            goal.destination_cell(),
                            goal.destination_debug_name()));
        }

        self.navigation.draw_debug_ui(ui_sys);
    }

    fn draw_debug_ui_misc(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if !ui.collapsing_header("Debug Misc", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if ui.button("Say Hello") {
            self.debug.popup_msg("Hello!");
        }

        if ui.button("Push Minimap Alert") {
            let minimap = query.tile_map().minimap_mut();
            minimap.push_icon(MinimapIcon::Alert,
                              self.cell(),
                              Color::default(),
                              MINIMAP_ICON_DEFAULT_LIFETIME);
        }

        if ui.button("Clear Current Task") {
            self.assign_task(query.task_manager(), None);
            self.follow_path(None);
        }

        self.debug_dropdown_despawn_tasks(query, ui_sys);
        self.debug_dropdown_runner_tasks(query, ui_sys);
        self.debug_dropdown_patrol_tasks(query, ui_sys);
        self.debug_dropdown_pathfinding_tasks(query, ui_sys);
        self.debug_dropdown_harvest_tasks(query, ui_sys);
    }

    fn debug_dropdown_despawn_tasks(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if !ui.collapsing_header("Despawn Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if ui.button("Give Despawn Task") {
            let task = query.task_manager().new_task(UnitTaskDespawn);
            self.assign_task(query.task_manager(), task);
        }

        if ui.button("Force Despawn Immediately") {
            Spawner::new(query).despawn_unit(self);
        }
    }

    fn debug_dropdown_runner_tasks(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if !ui.collapsing_header("Runner Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let task_manager = query.task_manager();
        let world = query.world();

        if ui.button("Give Deliver Resources Task") {
            // We need a building to own the task, so this assumes there's at least one of
            // these placed on the map.
            if let Some(building) = world.find_building_by_name("Market", BuildingKind::Market) {
                let start_cell = building.road_link(query).unwrap_or_default();
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
                        resource_kind_to_deliver: ResourceKind::random_food(&mut rand::rng()),
                        resource_count: 1,
                        completion_callback: callback::create!(unit_debug_delivery_task_completed),
                        completion_task,
                        allow_producer_fallback: true,
                    });
                    self.assign_task(task_manager, task);
                }
            }
        }

        if ui.button("Give Fetch Resources Task") {
            // We need a building to own the task, so this assumes there's at least one of
            // these placed on the map.
            if let Some(building) = world.find_building_by_name("Market", BuildingKind::Market) {
                let mut rng = rand::rng();
                let resources_to_fetch =
                    ShoppingList::from_items(&[StockItem { kind:
                                                               ResourceKind::random(&mut rng),
                                                           count: rng.random_range(1..5) }]);
                let start_cell = building.road_link(query).unwrap_or_default();
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
                        completion_callback: callback::create!(unit_debug_fetch_task_completed),
                        completion_task,
                        is_returning_to_origin: false,
                    });
                    self.assign_task(task_manager, task);
                }
            }
        }
    }

    fn debug_dropdown_patrol_tasks(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if !ui.collapsing_header("Patrol Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        // SAFETY: Debug code only called from the main thread (ImGui is inherently
        // single-threaded).
        static MAX_PATROL_DISTANCE: UiStaticVar<i32> = UiStaticVar::new(50);
        static PATH_BIAS_MIN: UiStaticVar<f32> = UiStaticVar::new(0.1);
        static PATH_BIAS_MAX: UiStaticVar<f32> = UiStaticVar::new(0.5);

        ui.input_int("Patrol Rounds", PATROL_ROUNDS.as_mut()).step(1).build();
        ui.input_int("Patrol Max Distance", MAX_PATROL_DISTANCE.as_mut()).step(1).build();
        ui.input_float("Patrol Path Bias Min", PATH_BIAS_MIN.as_mut())
            .display_format("%.2f")
            .step(0.1)
            .build();
        ui.input_float("Patrol Path Bias Max", PATH_BIAS_MAX.as_mut())
            .display_format("%.2f")
            .step(0.1)
            .build();

        let task_manager = query.task_manager();
        let world = query.world();

        if ui.button("Give Patrol Task") {
            // We need a building to own the task, so this assumes there's at least one of
            // these placed on the map.
            if let Some(building) = world.find_building_by_name("Market", BuildingKind::Market) {
                let start_cell = building.road_link(query).unwrap_or_default();
                if self.teleport(query.tile_map(), start_cell) {
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
                        max_distance: *MAX_PATROL_DISTANCE,
                        path_bias_min: *PATH_BIAS_MIN,
                        path_bias_max: *PATH_BIAS_MAX,
                        path_record: UnitPatrolPathRecord::default(),
                        buildings_to_visit: Some(BuildingKind::House),
                        completion_callback: callback::create!(unit_debug_patrol_task_completed),
                        completion_task,
                        idle_countdown: None,
                    });
                    self.assign_task(task_manager, task);
                }
            }
        }
    }

    fn debug_dropdown_pathfinding_tasks(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if !ui.collapsing_header("Pathfind Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let (traversable_node_kinds, max_search_distance, search_building_kind) = {
            // SAFETY: Debug code only called from the main thread (ImGui is inherently
            // single-threaded).
            static USE_ROAD_PATHS: UiStaticVar<bool> = UiStaticVar::new(true);
            static USE_DIRT_PATHS: UiStaticVar<bool> = UiStaticVar::new(false);
            static MAX_SEARCH_DISTANCE: UiStaticVar<i32> = UiStaticVar::new(50);
            static BUILDING_KIND_IDX: UiStaticVar<usize> = UiStaticVar::new(0);

            let mut building_kind_names: SmallVec<[&'static str; BuildingKind::count()]> = SmallVec::new();
            for kind in BuildingKind::FLAGS {
                building_kind_names.push(kind.name());
            }

            ui.checkbox("Road Paths", USE_ROAD_PATHS.as_mut());
            ui.checkbox("Dirt Paths", USE_DIRT_PATHS.as_mut());
            ui.input_int("Max Search Distance", MAX_SEARCH_DISTANCE.as_mut()).step(1).build();
            ui.combo_simple_string("Dest Building Kind",
                                   BUILDING_KIND_IDX.as_mut(),
                                   &building_kind_names);

            let mut traversable_node_kinds = PathNodeKind::empty();
            if *USE_ROAD_PATHS {
                traversable_node_kinds |= PathNodeKind::Road;
            }
            if *USE_DIRT_PATHS {
                traversable_node_kinds |= PathNodeKind::EmptyLand;
            }

            (traversable_node_kinds, *MAX_SEARCH_DISTANCE, *BuildingKind::FLAGS[*BUILDING_KIND_IDX].value())
        };

        if ui.button(format!("Path To Nearest Building ({})", search_building_kind))
           && !traversable_node_kinds.is_empty()
        {
            let start = self.cell();
            self.set_traversable_node_kinds(traversable_node_kinds);

            let visit_building = |building: &Building, path: &Path| -> bool {
                let tile_map = query.tile_map();

                log::info!("{} '{}' found. Path len: {}",
                           building.kind(),
                           building.name(),
                           path.len());
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

        if ui.button(format!("Test Is Near Building ({})", search_building_kind))
           && !traversable_node_kinds.is_empty()
        {
            let connected_to_road_only = traversable_node_kinds.intersects(PathNodeKind::Road)
                                         && !traversable_node_kinds.intersects(PathNodeKind::EmptyLand);

            let is_near = query.is_near_building(self.cell(),
                                                 search_building_kind,
                                                 connected_to_road_only,
                                                 max_search_distance);
            if is_near {
                self.debug
                    .popup_msg_color(Color::green(),
                                     format!("{}: Near {}!", self.cell(), search_building_kind));
            } else {
                self.debug.popup_msg_color(Color::red(),
                                           format!("{}: Not near {}!",
                                                   self.cell(),
                                                   search_building_kind));
            }
        }

        ui.separator();

        let task_manager = query.task_manager();

        if ui.button("Find Vacant House Lot") && !traversable_node_kinds.is_empty() {
            self.set_traversable_node_kinds(traversable_node_kinds);

            let completion_task = task_manager.new_task(UnitTaskDespawn);

            let task = task_manager.new_task(UnitTaskSettler {
                completion_callback: callback::create!(unit_debug_find_vacant_lot_task_completed),
                completion_task,
                fallback_to_houses_with_room: false,
                return_to_spawn_point_if_failed: false,
                population_to_add: 1,
            });

            self.assign_task(task_manager, task);
        }

        if ui.button("Find & Settle Vacant Lot | House") && !traversable_node_kinds.is_empty() {
            self.set_traversable_node_kinds(traversable_node_kinds);

            // NOTE: We have to spawn the house building *after* the unit has
            // despawned since we can't place a building over the unit tile.
            let completion_task = task_manager.new_task(UnitTaskDespawnWithCallback {
                post_despawn_callback: callback::create!(unit_debug_settle_task_post_despawn),
                callback_extra_args: UnitTaskArgs::new(&[UnitTaskArg::U32(1)]), // population_to_add
            });

            let task = task_manager.new_task(UnitTaskSettler {
                completion_callback: callback::create!(unit_debug_settle_task_completed),
                completion_task,
                fallback_to_houses_with_room: true,
                return_to_spawn_point_if_failed: false,
                population_to_add: 1,
            });

            self.assign_task(task_manager, task);
        }
    }

    fn debug_dropdown_harvest_tasks(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if !ui.collapsing_header("Harvest Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let task_manager = query.task_manager();
        let world = query.world();

        if ui.button("Give Harvest Wood Task") {
            // We need a building to own the task, so this assumes there's at least one
            // lumberyard placed on the map.
            if let Some(building) = world.find_building_by_name("Lumberyard", BuildingKind::Lumberyard) {
                let start_cell = building.road_link(query).unwrap_or_default();
                if self.teleport(query.tile_map(), start_cell) {
                    let completion_task = task_manager.new_task(UnitTaskDespawn);
                    let task = task_manager.new_task(UnitTaskHarvestWood {
                        origin_building: BuildingKindAndId {
                            kind: building.kind(),
                            id: building.id(),
                        },
                        origin_building_tile: BuildingTileInfo {
                            road_link: start_cell,
                            base_cell: building.base_cell(),
                        },
                        completion_callback: callback::create!(unit_debug_harvest_wood_task_completed),
                        completion_task,
                        harvest_timer: CountdownTimer::default(),
                        harvest_target: PropId::default(),
                        is_returning_to_origin: false,
                    });
                    self.assign_task(task_manager, task);
                }
            }
        }
    }
}

// ----------------------------------------------
// Debug callbacks
// ----------------------------------------------

pub fn register_callbacks() {
    let _: Callback<UnitTaskDeliveryCompletionCallback> =
        callback::register!(unit_debug_delivery_task_completed);
    let _: Callback<UnitTaskFetchCompletionCallback> =
        callback::register!(unit_debug_fetch_task_completed);
    let _: Callback<UnitTaskPatrolCompletionCallback> =
        callback::register!(unit_debug_patrol_task_completed);
    let _: Callback<UnitTaskSettlerCompletionCallback> =
        callback::register!(unit_debug_find_vacant_lot_task_completed);
    let _: Callback<UnitTaskSettlerCompletionCallback> =
        callback::register!(unit_debug_settle_task_completed);
    let _: Callback<UnitTaskPostDespawnCallback> =
        callback::register!(unit_debug_settle_task_post_despawn);
    let _: Callback<UnitTaskHarvestCompletionCallback> =
        callback::register!(unit_debug_harvest_wood_task_completed);
}

static PATROL_ROUNDS: UiStaticVar<i32> = UiStaticVar::new(5);

fn unit_debug_patrol_task_completed(_: &mut Building, unit: &mut Unit, _: &Query) -> bool {
    PATROL_ROUNDS.set(*PATROL_ROUNDS - 1);
    log::info!("Unit {}: Patrol Task Round {} Completed.", unit.name(), *PATROL_ROUNDS);
    *PATROL_ROUNDS <= 0 // Run the task a few times.
}

fn unit_debug_delivery_task_completed(building: &mut Building, unit: &mut Unit, _: &Query) {
    log::info!("Unit {}: Deliver Resources to: {}. Task Completed.", unit.name(), building.name());
}

fn unit_debug_fetch_task_completed(building: &mut Building, unit: &mut Unit, _: &Query) {
    let item = unit.inventory.peek().unwrap();
    log::info!("Unit {}: Fetch Resources from: {}. Task Completed. Got: {}, {}",
               unit.name(),
               building.name(),
               item.kind,
               item.count);
    unit.inventory.clear();
}

fn unit_debug_find_vacant_lot_task_completed(unit: &mut Unit, dest_tile: &Tile, _: u32, _: &Query) {
    log::info!("Unit {} reached {}.", unit.name(), dest_tile.name());
    unit.debug.popup_msg(format!("Reached {}", dest_tile.name()));
}

fn unit_debug_settle_task_completed(unit: &mut Unit,
                                    dest_tile: &Tile,
                                    population_to_add: u32,
                                    _: &Query) {
    debug_assert!(population_to_add == 1);
    log::info!("Unit {} reached {}.", unit.name(), dest_tile.name());
    unit.debug.popup_msg(format!("Reached {}", dest_tile.name()));
}

fn unit_debug_settle_task_post_despawn(query: &Query,
                                       unit_prev_cell: Cell,
                                       unit_prev_goal: Option<UnitNavGoal>,
                                       extra_args: &[UnitTaskArg]) {
    let settle_new_vacant_lot =
        unit_prev_goal.is_some_and(|goal| navigation::is_goal_vacant_lot_tile(&goal, query));

    if settle_new_vacant_lot {
        if let Some(tile_def) = query.find_tile_def(TileMapLayerKind::Objects,
                                                    tile::sets::OBJECTS_BUILDINGS_CATEGORY.hash,
                                                    hash::fnv1a_from_str("house0"))
        {
            match query.world().try_spawn_building_with_tile_def(query, unit_prev_cell, tile_def) {
                Ok(building) => {
                    debug_assert!(building.is(BuildingKind::House));

                    let population_to_add = extra_args[0].as_u32();
                    debug_assert!(population_to_add == 1);

                    let population_added = building.add_population(query, population_to_add);
                    if population_added != population_to_add {
                        log::error!(log::channel!("unit"),
                                    "Settler carried population of {population_to_add} but house accommodated {population_added}.");
                    }
                }
                Err(err) => {
                    log::error!(log::channel!("unit"), "Failed to place House Level 0: {err}")
                }
            }
        } else {
            log::error!(log::channel!("unit"), "House Level 0 TileDef not found!");
        }
    } else {
        log::info!("Unit settled into existing household.");
    }
}

fn unit_debug_harvest_wood_task_completed(building: &mut Building, unit: &mut Unit, _: &Query) {
    let item = unit.inventory.peek().unwrap();
    log::info!("Unit {}: Harvested for: {}. Task Completed. Got: {}, {}",
               unit.name(),
               building.name(),
               item.kind,
               item.count);
    unit.inventory.clear();
}
