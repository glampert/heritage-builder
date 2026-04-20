use rand::Rng;
use bitflags::Flags;
use smallvec::SmallVec;

use common::{
    Color,
    callback::{self, Callback},
    coords::Cell,
    hash,
    time::CountdownTimer,
};
use engine::{
    log,
    ui::{self, UiDPadDirection, UiFontScale, UiStaticVar, UiSystem},
};
use proc_macros::DrawDebugUi;

use super::{
    Unit,
    UnitId,
    navigation::{self, *},
    task::*,
};
use crate::{
    building::{Building, BuildingKind, BuildingKindAndId, BuildingTileInfo},
    debug::game_object_debug::{GameObjectDebugOptions, debug_popup_msg, debug_popup_msg_color},
    pathfind::{self, NodeKind as PathNodeKind, Path},
    world::object::GameObject,
    prop::PropId,
    sim::{
        SimCmds,
        SimContext,
        resources::{ResourceKind, ShoppingList, StockItem},
    },
    tile::{
        self,
        Tile,
        TileMapLayerKind,
        TilePoolIndex,
        minimap::{MINIMAP_ICON_DEFAULT_LIFETIME, MinimapIcon},
    },
};

// ----------------------------------------------
// Unit Debug UI
// ----------------------------------------------

impl Unit {
    pub fn draw_debug_ui_overview(&mut self, context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        ui_sys.set_window_font_scale(UiFontScale(1.2));
        ui.text(format!("{} | ID{} @{}", self.name(), self.id(), self.cell()));
        ui_sys.set_window_font_scale(UiFontScale::default());

        ui.bullet_text(format!("Anim: {} (dir: {})", self.anim_sets.current_anim_name(), self.direction));

        if let Some(task_id) = self.current_task() {
            if let Some((archetype, state)) = context.task_manager().try_get_task_archetype_and_state(task_id) {
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

    pub fn draw_debug_ui_detailed(&mut self, cmds: &mut SimCmds, context: &SimContext, ui_sys: &UiSystem) {
        self.draw_debug_ui_properties(ui_sys);
        self.draw_debug_ui_config(ui_sys);
        self.debug.draw_debug_ui(ui_sys);
        self.inventory.draw_debug_ui(ui_sys);
        self.draw_debug_ui_tasks(context, ui_sys);
        self.draw_debug_ui_navigation(context, ui_sys);
        self.draw_debug_ui_misc(cmds, context, ui_sys);
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

    fn draw_debug_ui_tasks(&mut self, context: &SimContext, ui_sys: &UiSystem) {
        context.task_manager_mut().draw_tasks_debug_ui(self, context, ui_sys);
    }

    fn draw_debug_ui_navigation(&mut self, context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        if !ui.collapsing_header("Navigation", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if let Some(dir) = ui::dpad_buttons(ui) {
            let tile_map = context.tile_map_mut();
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
        ui.text(format!("Iso Coords : {}", self.find_tile(context).iso_coords()));
        ui.text(format!("Direction  : {}", self.direction));
        ui.text(format!("Anim       : {}", self.anim_sets.current_anim_name()));

        if ui.button("Force Idle Anim") {
            self.idle(context);
        }

        ui.separator();

        if self.path_is_blocked {
            ui.text_colored(Color::red().to_array(), "PATH BLOCKED!");
        } else {
            let color = match self.navigation.status() {
                UnitNavStatus::Idle => Color::yellow(),
                UnitNavStatus::Paused => Color::red(),
                UnitNavStatus::Moving => Color::green(),
            };

            ui.text_colored(color.to_array(), format!("Path Navigation Status: {:?}", self.navigation.status()));
        }

        if let Some(goal) = self.navigation.goal() {
            ui.text(format!("Start Tile : {}, {}", goal.origin_cell(), goal.origin_debug_name()));
            ui.text(format!("Dest  Tile : {}, {}", goal.destination_cell(), goal.destination_debug_name()));
        }

        self.navigation.draw_debug_ui(ui_sys);
    }

    fn draw_debug_ui_misc(&mut self, cmds: &mut SimCmds, context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if !ui.collapsing_header("Debug Misc", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if ui.button("Say Hello") {
            debug_popup_msg!(self.debug, "Hello!");
        }

        if ui.button("Push Minimap Alert") {
            let minimap = context.tile_map_mut().minimap_mut();
            minimap.push_icon(MinimapIcon::Alert, self.cell(), Color::default(), MINIMAP_ICON_DEFAULT_LIFETIME);
        }

        if ui.button("Clear Current Task") {
            self.assign_task(context.task_manager_mut(), None);
            self.follow_path(None);
        }

        self.debug_dropdown_despawn_tasks(cmds, context, ui_sys);
        self.debug_dropdown_runner_tasks(context, ui_sys);
        self.debug_dropdown_patrol_tasks(context, ui_sys);
        self.debug_dropdown_pathfinding_tasks(context, ui_sys);
        self.debug_dropdown_harvest_tasks(context, ui_sys);
    }

    fn debug_dropdown_despawn_tasks(&mut self, cmds: &mut SimCmds, context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if !ui.collapsing_header("Despawn Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if ui.button("Give Despawn Task") {
            let task = context.task_manager_mut().new_task(UnitTaskDespawn);
            self.assign_task(context.task_manager_mut(), task);
        }

        if ui.button("Force Despawn Immediately") {
            cmds.despawn_unit_with_id(self.id());
        }
    }

    fn debug_dropdown_runner_tasks(&mut self, context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if !ui.collapsing_header("Runner Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let task_manager = context.task_manager_mut();
        let world = context.world();

        if ui.button("Give Deliver Resources Task") {
            // We need a building to own the task, so this assumes there's at least one of
            // these placed on the map.
            if let Some(building) = world.find_building_by_name("Market", BuildingKind::Market) {
                let start_cell = building.road_link(context).unwrap_or_default();
                if self.teleport(context.tile_map_mut(), start_cell) {
                    let completion_task = task_manager.new_task(UnitTaskDespawn);
                    let task = task_manager.new_task(UnitTaskDeliverToStorage {
                        origin_building: BuildingKindAndId { kind: building.kind(), id: building.id() },
                        origin_building_tile: BuildingTileInfo { road_link: start_cell, base_cell: building.base_cell() },
                        storage_buildings_accepted: BuildingKind::storage(), // any storage
                        resource_kind_to_deliver: ResourceKind::random_food(&mut rand::rng()),
                        resource_count: 1,
                        completion_callback: callback::create!(unit_debug_delivery_task_completed),
                        completion_task,
                        allow_producer_fallback: true,
                        internal_state: UnitTaskDeliveryState::default(),
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
                let resources_to_fetch = ShoppingList::from_items(&[StockItem {
                    kind: ResourceKind::random(&mut rng),
                    count: rng.random_range(1..5),
                }]);
                let start_cell = building.road_link(context).unwrap_or_default();
                if self.teleport(context.tile_map_mut(), start_cell) {
                    let completion_task = task_manager.new_task(UnitTaskDespawn);
                    let task = task_manager.new_task(UnitTaskFetchFromStorage {
                        origin_building: BuildingKindAndId { kind: building.kind(), id: building.id() },
                        origin_building_tile: BuildingTileInfo { road_link: start_cell, base_cell: building.base_cell() },
                        storage_buildings_accepted: BuildingKind::storage(), // any storage
                        resources_to_fetch,
                        completion_callback: callback::create!(unit_debug_fetch_task_completed),
                        completion_task,
                        internal_state: UnitTaskFetchState::default(),
                    });
                    self.assign_task(task_manager, task);
                }
            }
        }
    }

    fn debug_dropdown_patrol_tasks(&mut self, context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if !ui.collapsing_header("Patrol Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        // SAFETY: Debug code only called from the main thread (ImGui is inherently single-threaded).
        static MAX_PATROL_DISTANCE: UiStaticVar<i32> = UiStaticVar::new(50);
        static PATH_BIAS_MIN: UiStaticVar<f32> = UiStaticVar::new(0.1);
        static PATH_BIAS_MAX: UiStaticVar<f32> = UiStaticVar::new(0.5);

        ui.input_int("Patrol Max Distance", MAX_PATROL_DISTANCE.as_mut()).step(1).build();
        ui.input_float("Patrol Path Bias Min", PATH_BIAS_MIN.as_mut()).display_format("%.2f").step(0.1).build();
        ui.input_float("Patrol Path Bias Max", PATH_BIAS_MAX.as_mut()).display_format("%.2f").step(0.1).build();

        let task_manager = context.task_manager_mut();
        let world = context.world();

        if ui.button("Give Patrol Task") {
            // We need a building to own the task, so this assumes there's at least one of
            // these placed on the map.
            if let Some(building) = world.find_building_by_name("Market", BuildingKind::Market) {
                let start_cell = building.road_link(context).unwrap_or_default();
                if self.teleport(context.tile_map_mut(), start_cell) {
                    let completion_task = task_manager.new_task(UnitTaskDespawn);
                    let task = task_manager.new_task(UnitTaskRandomizedPatrol {
                        origin_building: BuildingKindAndId { kind: building.kind(), id: building.id() },
                        origin_building_tile: BuildingTileInfo { road_link: start_cell, base_cell: building.base_cell() },
                        max_distance: *MAX_PATROL_DISTANCE,
                        path_bias_min: *PATH_BIAS_MIN,
                        path_bias_max: *PATH_BIAS_MAX,
                        path_record: UnitPatrolPathRecord::default(),
                        buildings_to_visit: Some(BuildingKind::House),
                        completion_callback: callback::create!(unit_debug_patrol_task_completed),
                        completion_task,
                        idle_countdown: None,
                        internal_state: UnitTaskPatrolState::default(),
                    });
                    self.assign_task(task_manager, task);
                }
            }
        }
    }

    fn debug_dropdown_pathfinding_tasks(&mut self, context: &SimContext, ui_sys: &UiSystem) {
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
            ui.combo_simple_string("Dest Building Kind", BUILDING_KIND_IDX.as_mut(), &building_kind_names);

            let mut traversable_node_kinds = PathNodeKind::empty();
            if *USE_ROAD_PATHS {
                traversable_node_kinds |= PathNodeKind::Road;
            }
            if *USE_DIRT_PATHS {
                traversable_node_kinds |= PathNodeKind::EmptyLand;
            }

            (traversable_node_kinds, *MAX_SEARCH_DISTANCE, *BuildingKind::FLAGS[*BUILDING_KIND_IDX].value())
        };

        if ui.button(format!("Path To Nearest Building ({})", search_building_kind)) && !traversable_node_kinds.is_empty() {
            let start = self.cell();
            self.set_traversable_node_kinds(traversable_node_kinds);

            let visit_building = |building: &Building, path: &Path| -> bool {
                let tile_map = context.tile_map_mut();

                log::info!("{} '{}' found. Path len: {}", building.kind(), building.name(), path.len());
                debug_assert!(building.is(search_building_kind)); // The building we're looking for.

                // Highlight the path to take:
                pathfind::highlight_path_tiles(tile_map, path);

                // Highlight building access tiles and road link:
                pathfind::highlight_building_access_tiles(tile_map, building.cell_range());
                building.set_show_road_link_debug(context, true);

                self.follow_path(Some(path));
                false // done
            };

            context.find_nearest_buildings(
                start,
                search_building_kind,
                traversable_node_kinds,
                Some(max_search_distance),
                visit_building,
            );
        }

        if ui.button(format!("Test Is Near Building ({})", search_building_kind)) && !traversable_node_kinds.is_empty() {
            let connected_to_road_only = traversable_node_kinds.intersects(PathNodeKind::Road)
                && !traversable_node_kinds.intersects(PathNodeKind::EmptyLand);

            let is_near =
                context.is_near_building(self.cell(), search_building_kind, connected_to_road_only, max_search_distance);

            if is_near {
                debug_popup_msg_color!(self.debug, Color::green(), "{}: Near {}!", self.cell(), search_building_kind);
            } else {
                debug_popup_msg_color!(self.debug, Color::red(), "{}: Not near {}!", self.cell(), search_building_kind);
            }
        }

        ui.separator();

        let task_manager = context.task_manager_mut();

        if ui.button("Find Vacant House Lot") && !traversable_node_kinds.is_empty() {
            self.set_traversable_node_kinds(traversable_node_kinds);

            let completion_task = task_manager.new_task(UnitTaskDespawn);

            let task = task_manager.new_task(UnitTaskSettler {
                completion_callback: callback::create!(unit_debug_find_vacant_lot_task_completed),
                completion_task,
                fallback_to_houses_with_room: false,
                return_to_spawn_point_if_failed: false,
                population_to_add: 1,
                internal_state: UnitTaskSettlerState::default(),
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
                internal_state: UnitTaskSettlerState::default(),
            });

            self.assign_task(task_manager, task);
        }
    }

    fn debug_dropdown_harvest_tasks(&mut self, context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if !ui.collapsing_header("Harvest Tasks", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let task_manager = context.task_manager_mut();
        let world = context.world();

        if ui.button("Give Harvest Wood Task") {
            // We need a building to own the task, so this assumes there's at least one
            // lumberyard placed on the map.
            if let Some(building) = world.find_building_by_name("Lumberyard", BuildingKind::Lumberyard) {
                let start_cell = building.road_link(context).unwrap_or_default();
                if self.teleport(context.tile_map_mut(), start_cell) {
                    let completion_task = task_manager.new_task(UnitTaskDespawn);
                    let task = task_manager.new_task(UnitTaskHarvestWood {
                        origin_building: BuildingKindAndId { kind: building.kind(), id: building.id() },
                        origin_building_tile: BuildingTileInfo { road_link: start_cell, base_cell: building.base_cell() },
                        completion_callback: callback::create!(unit_debug_harvest_wood_task_completed),
                        completion_task,
                        harvest_timer: CountdownTimer::default(),
                        harvest_target: PropId::default(),
                        is_returning_to_origin: false,
                        internal_state: UnitTaskHarvestState::default(),
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

fn unit_debug_patrol_task_completed(_: &SimContext, building: &mut Building, unit: &mut Unit) {
    log::info!("Unit {}: Patrol Task From {} Completed.", unit.name(), building.name());
}

fn unit_debug_delivery_task_completed(_: &SimContext, building: &mut Building, unit: &mut Unit) {
    log::info!("Unit {}: Deliver Resources to: {}. Task Completed.", unit.name(), building.name());
}

fn unit_debug_fetch_task_completed(_: &SimContext, building: &mut Building, unit: &mut Unit) {
    let item = unit.inventory.peek().unwrap();
    log::info!(
        "Unit {}: Fetch Resources from: {}. Task Completed. Got: {}, {}",
        unit.name(),
        building.name(),
        item.kind,
        item.count
    );
    unit.inventory.clear();
}

fn unit_debug_find_vacant_lot_task_completed(_: &SimContext, unit: &mut Unit, dest_tile: &Tile, _: u32) {
    log::info!("Unit {} reached {}.", unit.name(), dest_tile.name());
    debug_popup_msg!(unit.debug, "Reached {}", dest_tile.name());
}

fn unit_debug_settle_task_completed(_: &SimContext, unit: &mut Unit, dest_tile: &Tile, population_to_add: u32) {
    debug_assert!(population_to_add == 1);
    log::info!("Unit {} reached {}.", unit.name(), dest_tile.name());
    debug_popup_msg!(unit.debug, "Reached {}", dest_tile.name());
}

fn unit_debug_settle_task_post_despawn(
    cmds: &mut SimCmds,
    context: &SimContext,
    unit_prev_cell: Cell,
    unit_prev_goal: Option<UnitNavGoal>,
    extra_args: &[UnitTaskArg],
) {
    let settle_new_vacant_lot =
        unit_prev_goal.is_some_and(|goal| navigation::is_goal_vacant_lot_tile(&goal, context));

    if settle_new_vacant_lot {
        if let Some(tile_def) = context.find_tile_def(
            TileMapLayerKind::Objects,
            tile::sets::OBJECTS_BUILDINGS_CATEGORY.hash,
            hash::fnv1a_from_str("house0"),
        ) {
            let population_to_add = extra_args[0].as_u32();
            debug_assert!(population_to_add == 1);

            cmds.spawn_building_with_tile_def_cb(unit_prev_cell, tile_def, move |context, result| {
                match result {
                    Ok(building) => {
                        debug_assert!(building.is(BuildingKind::House));

                        let population_added = building.add_population(context, population_to_add);
                        if population_added != population_to_add {
                            log::error!(
                                log::channel!("unit"),
                                "Settler carried population of {population_to_add} but house accommodated {population_added}."
                            );
                        }
                    }
                    Err(err) => {
                        log::error!(log::channel!("unit"), "Failed to place House Level 0: {}", err.message)
                    }
                }
            });
        } else {
            log::error!(log::channel!("unit"), "House Level 0 TileDef not found!");
        }
    } else {
        log::info!("Unit settled into existing household.");
    }
}

fn unit_debug_harvest_wood_task_completed(_: &SimContext, building: &mut Building, unit: &mut Unit) {
    let item = unit.inventory.peek().unwrap();
    log::info!(
        "Unit {}: Harvested for: {}. Task Completed. Got: {}, {}",
        unit.name(),
        building.name(),
        item.kind,
        item.count
    );
    unit.inventory.clear();
}
