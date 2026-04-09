use std::any::Any;

use common::{
    Color,
    callback::{self, Callback},
    coords::Cell,
    hash,
    time::UpdateTimer,
};
use engine::{Engine, log};
use serde::{Deserialize, Serialize};

use super::GameSystem;
use crate::{
    building::BuildingKind,
    config::GameConfigs,
    debug::utils::UpdateTimerDebugUi,
    pathfind::{Node, NodeKind as PathNodeKind},
    save_context::PostLoadContext,
    sim::{SimCmds, SimContext},
    tile::{
        TileFlags,
        TileKind,
        TileMapLayerKind,
        sets::{OBJECTS_BUILDINGS_CATEGORY, TileDef},
    },
    unit::{
        UnitId,
        UnitTaskHelper,
        config::UnitConfigKey,
        navigation::{self, UnitNavGoal},
        task::{UnitTaskArg, UnitTaskArgs, UnitTaskDespawnWithCallback, UnitTaskPostDespawnCallback, UnitTaskSettler},
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
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update(&mut self, _engine: &mut Engine, _cmds: &mut SimCmds, context: &SimContext) {
        if self.spawn_timer.tick(context.delta_time_secs()).should_update() {
            // Only attempt to spawn if we have any empty housing lots available.
            if Self::has_vacant_lots(context) {
                self.try_spawn(context);
            }
        }
    }

    fn reset(&mut self, _engine: &mut Engine) {
        self.spawn_timer.reset();
    }

    fn post_load(&mut self, context: &mut PostLoadContext) {
        self.spawn_timer.post_load(context.configs().sim.settlers_spawn_frequency_secs);
    }

    fn draw_debug_ui(&mut self, engine: &mut Engine, _cmds: &mut SimCmds, context: &SimContext) {
        self.spawn_timer.draw_debug_ui("Settler Spawn", 0, engine.ui_system());

        let ui = engine.ui_system().ui();

        let color_text = |text: &str, cond: bool| {
            ui.text(text);
            ui.same_line();
            if cond {
                ui.text_colored(Color::green().to_array(), "yes");
            } else {
                ui.text_colored(Color::red().to_array(), "no");
            }
        };

        color_text("Has vacant lots:", Self::has_vacant_lots(context));

        let spawn_point = Self::find_spawn_point(context);
        ui.text(format!("Spawn Point: {}", spawn_point.cell));

        if ui.input_scalar("Population Per Settler Unit", &mut self.population_per_settler_unit).step(1).build() {
            self.population_per_settler_unit = self.population_per_settler_unit.max(1);
        }

        if ui.button("Force Spawn Now") {
            self.try_spawn(context);
        }

        if ui.button("Highlight Spawn Point") {
            if let Some(tile) = context.find_tile_mut(spawn_point.cell, TileMapLayerKind::Terrain, TileKind::Terrain) {
                tile.set_flags(TileFlags::Highlighted | TileFlags::DrawDebugBounds, true);
            }
        }
    }

    fn register_callbacks(&self) {
        Settler::register_callbacks();
    }
}

impl Default for SettlersSpawnSystem {
    fn default() -> Self {
        let configs = GameConfigs::get();
        Self {
            spawn_timer: UpdateTimer::new(configs.sim.settlers_spawn_frequency_secs),
            population_per_settler_unit: configs.sim.population_per_settler_unit,
        }
    }
}

impl SettlersSpawnSystem {
    #[inline]
    fn has_vacant_lots(context: &SimContext) -> bool {
        context.graph().has_node_with_kinds(PathNodeKind::VacantLot)
    }

    #[inline]
    fn find_spawn_point(context: &SimContext) -> Node {
        context.graph().settlers_spawn_point().unwrap_or_else(|| {
            // Fallback to map playable area top-left corner cell if no spawn point it set.
            let map_size = context.tile_map().size_in_cells();
            let x = (map_size.width / 2) - 1;
            let y = map_size.height - 1;
            Node::new(Cell::new(x, y))
        })
    }

    fn try_spawn(&self, context: &SimContext) {
        let mut settler = Settler::default();
        let spawn_point = Self::find_spawn_point(context);
        settler.try_spawn(context, spawn_point.cell, self.population_per_settler_unit);
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
    pub fn try_spawn(&mut self, context: &SimContext, unit_origin: Cell, population_to_add: u32) -> bool {
        debug_assert!(unit_origin.is_valid());
        debug_assert!(population_to_add != 0);

        self.try_spawn_with_task("SettlersSpawnSystem", context, unit_origin, UnitConfigKey::Settler, UnitTaskSettler {
            completion_callback: Callback::default(),
            completion_task: context.task_manager_mut().new_task(UnitTaskDespawnWithCallback {
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

    fn on_settled(
        context: &SimContext,
        unit_prev_cell: Cell,
        unit_prev_goal: Option<UnitNavGoal>,
        extra_args: &[UnitTaskArg],
    ) {
        let settle_new_vacant_lot = unit_prev_goal.is_some_and(|goal| navigation::is_goal_vacant_lot_tile(&goal, context));

        if settle_new_vacant_lot {
            if let Some(tile_def) = Self::find_house_tile_def(context) {
                let world = context.world_mut();
                match world.try_spawn_building_with_tile_def(context, unit_prev_cell, tile_def) {
                    Ok(building) => {
                        debug_assert!(building.is(BuildingKind::House));

                        building.set_random_variation(context);

                        let population_to_add = extra_args[0].as_u32();
                        debug_assert!(population_to_add != 0);

                        let population_added = building.add_population(context, population_to_add);
                        if population_added != population_to_add {
                            log::error!(
                                log::channel!("unit"),
                                "Settler carried population of {population_to_add} but house accommodated {population_added}."
                            );
                        }
                    }
                    Err(err) => {
                        log::error!(
                            log::channel!("unit"),
                            "SettlersSpawnSystem: Failed to place House Level 0: {}",
                            err.message
                        );
                    }
                }
            } else {
                log::error!(log::channel!("unit"), "SettlersSpawnSystem: House Level 0 TileDef not found!");
            }
        }
        // Else unit settled into existing household.
    }

    fn find_house_tile_def(context: &SimContext) -> Option<&'static TileDef> {
        context.find_tile_def(TileMapLayerKind::Objects, OBJECTS_BUILDINGS_CATEGORY.hash, hash::fnv1a_from_str("house0"))
    }
}
