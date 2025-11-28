use std::any::Any;

use serde::{Deserialize, Serialize};

use super::GameSystem;
use crate::{
    engine::time::UpdateTimer,
    game::{
        building::BuildingKind,
        config::GameConfigs,
        sim::Query,
        unit::{
            navigation::{self, UnitNavGoal},
            task::{
                UnitTaskArg, UnitTaskArgs, UnitTaskDespawnWithCallback,
                UnitTaskPostDespawnCallback, UnitTaskSettler,
            },
            config::UnitConfigKey,
            UnitId, UnitTaskHelper,
        },
    },
    imgui_ui::UiSystem,
    log,
    pathfind::{Node, NodeKind as PathNodeKind},
    save::PostLoadContext,
    tile::{
        sets::{TileDef, OBJECTS_BUILDINGS_CATEGORY},
        TileMapLayerKind,
    },
    utils::{
        callback::{self, Callback},
        coords::Cell,
        hash, Color,
    },
};

// ----------------------------------------------
// SettlersSpawnSystem
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct SettlersSpawnSystem {
    spawn_timer: UpdateTimer,
    population_per_settler_unit: u32,
}

impl GameSystem for SettlersSpawnSystem {
    fn name(&self) -> &str {
        "Settlers Spawn System"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update(&mut self, query: &Query) {
        if self.spawn_timer.tick(query.delta_time_secs()).should_update() {
            // Only attempt to spawn if we have any empty housing lots available.
            if Self::find_vacant_lot(query).is_some() {
                self.try_spawn(query);
            }
        }
    }

    fn post_load(&mut self, _context: &PostLoadContext) {
        let configs = GameConfigs::get();
        self.spawn_timer.post_load(configs.sim.settlers_spawn_frequency_secs);
    }

    fn draw_debug_ui(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        self.spawn_timer.draw_debug_ui("Settler Spawn", 0, ui_sys);

        let color_text = |text, cond: bool| {
            ui.text(text);
            ui.same_line();
            if cond {
                ui.text_colored(Color::green().to_array(), "yes");
            } else {
                ui.text_colored(Color::red().to_array(), "no");
            }
        };

        color_text("Has vacant lots:", Self::find_vacant_lot(query).is_some());
        color_text("Has spawn point:", Self::find_spawn_point(query).is_some());

        if ui.input_scalar("Population Per Settler Unit", &mut self.population_per_settler_unit)
             .step(1)
             .build()
        {
            self.population_per_settler_unit = self.population_per_settler_unit.max(1);
        }

        if ui.button("Force Spawn Now") {
            self.try_spawn(query);
        }
    }
}

impl SettlersSpawnSystem {
    pub fn new() -> Self {
        let configs = GameConfigs::get();
        Self { spawn_timer: UpdateTimer::new(configs.sim.settlers_spawn_frequency_secs),
               population_per_settler_unit: configs.sim.population_per_settler_unit }
    }

    pub fn register_callbacks() {
        Settler::register_callbacks();
    }

    fn find_vacant_lot(query: &Query) -> Option<Node> {
        query.graph().find_node_with_kinds(PathNodeKind::VacantLot)
    }

    fn find_spawn_point(query: &Query) -> Option<Node> {
        query.graph().find_node_with_kinds(PathNodeKind::SettlersSpawnPoint)
    }

    fn try_spawn(&self, query: &Query) {
        if let Some(spawn_point) = Self::find_spawn_point(query) {
            let mut settler = Settler::default();
            settler.try_spawn(query, spawn_point.cell, self.population_per_settler_unit);
        }
    }
}

// ----------------------------------------------
// Settler Unit helper
// ----------------------------------------------

#[derive(Default, Serialize, Deserialize)]
pub struct Settler {
    unit_id: UnitId,
    #[serde(skip)]
    failed_to_spawn: bool, // Debug flag; not serialized.
}

impl UnitTaskHelper for Settler {
    #[inline]
    fn reset(&mut self) {
        self.unit_id = UnitId::default();
        self.failed_to_spawn = false;
    }

    #[inline]
    fn on_unit_spawn(&mut self, unit_id: UnitId, failed_to_spawn: bool) {
        self.unit_id = unit_id;
        self.failed_to_spawn = failed_to_spawn;
    }

    #[inline]
    fn unit_id(&self) -> UnitId {
        self.unit_id
    }

    #[inline]
    fn failed_to_spawn(&self) -> bool {
        self.failed_to_spawn
    }
}

impl Settler {
    pub fn try_spawn(&mut self, query: &Query, unit_origin: Cell, population_to_add: u32) -> bool {
        debug_assert!(unit_origin.is_valid());
        debug_assert!(population_to_add != 0);

        self.try_spawn_with_task(
            "SettlersSpawnSystem",
            query,
            unit_origin,
            UnitConfigKey::Settler,
            UnitTaskSettler {
                completion_callback: Callback::default(),
                completion_task: query.task_manager().new_task(UnitTaskDespawnWithCallback {
                    // NOTE: We have to spawn the house building *after* the unit has
                    // despawned since we can't place a building over the unit tile.
                    post_despawn_callback: callback::create!(Settler::on_settled),
                    callback_extra_args: UnitTaskArgs::new(&[UnitTaskArg::U32(population_to_add)]),
                }),
                fallback_to_houses_with_room: true,
                return_to_spawn_point_if_failed: true,
                population_to_add,
            })
    }

    pub fn register_callbacks() {
        let _: Callback<UnitTaskPostDespawnCallback> = callback::register!(Settler::on_settled);
    }

    fn on_settled(query: &Query,
                  unit_prev_cell: Cell,
                  unit_prev_goal: Option<UnitNavGoal>,
                  extra_args: &[UnitTaskArg]) {
        let settle_new_vacant_lot =
            unit_prev_goal.is_some_and(|goal| navigation::is_goal_vacant_lot_tile(&goal, query));

        if settle_new_vacant_lot {
            if let Some(tile_def) = Self::find_house_tile_def(query) {
                let world = query.world();
                match world.try_spawn_building_with_tile_def(query, unit_prev_cell, tile_def) {
                    Ok(building) => {
                        debug_assert!(building.is(BuildingKind::House));

                        building.set_random_variation(query);

                        let population_to_add = extra_args[0].as_u32();
                        debug_assert!(population_to_add != 0);

                        let population_added = building.add_population(query, population_to_add);
                        if population_added != population_to_add {
                            log::error!(log::channel!("unit"),
                                        "Settler carried population of {population_to_add} but house accommodated {population_added}.");
                        }
                    }
                    Err(err) => {
                        log::error!(log::channel!("unit"),
                                    "SettlersSpawnSystem: Failed to place House Level 0: {err}");
                    }
                }
            } else {
                log::error!(log::channel!("unit"),
                            "SettlersSpawnSystem: House Level 0 TileDef not found!");
            }
        }
        // Else unit settled into existing household.
    }

    fn find_house_tile_def(query: &Query) -> Option<&'static TileDef> {
        query.find_tile_def(TileMapLayerKind::Objects,
                            OBJECTS_BUILDINGS_CATEGORY.hash,
                            hash::fnv1a_from_str("house0"))
    }
}
