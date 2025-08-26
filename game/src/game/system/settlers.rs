use crate::{
    imgui_ui::UiSystem,
    sim::{Query, UpdateTimer},
    utils::{Color, coords::Cell, hash::{self}},
    pathfind::{NodeKind as PathNodeKind},
    tile::{TileMapLayerKind, sets::{TileDef, OBJECTS_BUILDINGS_CATEGORY}},
    game::{
        constants::SETTLERS_SPAWN_FREQUENCY_SECS,
        unit::{
            Unit,
            config::{self},
            navigation::{self, UnitNavGoal},
            task::{UnitTaskDespawnWithCallback, UnitTaskFindVacantHouseLot}
        }
    }
};

use super::{
    GameSystem
};

// ----------------------------------------------
// SettlersSpawnSystem
// ----------------------------------------------

pub struct SettlersSpawnSystem {
    spawn_timer: UpdateTimer,
}

impl GameSystem for SettlersSpawnSystem {
    fn update(&mut self, query: &Query) {
        if self.spawn_timer.tick(query.delta_time_secs()).should_update() {
            // Only attempt to spawn if we have any empty housing lots available.
            if Self::has_vacant_lots(query) {
                Self::try_spawn(query);
            }
        }
    }

    fn draw_debug_ui(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        self.spawn_timer.draw_debug_ui("Settler Spawn", 0, ui_sys);

        let color_bool_val = |text, cond: bool| {
            ui.text(text);
            ui.same_line();
            if cond {
                ui.text_colored(Color::green().to_array(), "yes");
            } else {
                ui.text_colored(Color::red().to_array(), "no");
            }
        };

        color_bool_val("Has vacant lots:", Self::has_vacant_lots(query));
        color_bool_val("Has spawn point:", Self::has_spawn_point(query));

        if ui.button("Force Spawn Now") {
            Self::try_spawn(query);
        }
    }
}

impl SettlersSpawnSystem {
    pub fn new() -> Self {
        Self {
            spawn_timer: UpdateTimer::new(SETTLERS_SPAWN_FREQUENCY_SECS),
        }
    }

    fn has_vacant_lots(query: &Query) -> bool {
        query.graph().find_node_with_kinds(PathNodeKind::VacantLot).is_some()
    }

    fn has_spawn_point(query: &Query) -> bool {
        query.graph().find_node_with_kinds(PathNodeKind::SettlersSpawnPoint).is_some()
    }

    fn try_spawn(query: &Query) {
        if let Some(spawn_point) = query.graph().find_node_with_kinds(PathNodeKind::SettlersSpawnPoint) {
            match query.try_spawn_unit(spawn_point.cell, config::UNIT_SETTLER) {
                Ok(settler) => {
                    Self::give_task(settler, query);
                },
                Err(err) => {
                    eprintln!("SettlersSpawnSystem: Failed to spawn new Settler: {}", err);
                },
            }
        }
    }

    fn give_task(settler: &mut Unit, query: &Query) {
        settler.set_traversable_node_kinds(PathNodeKind::Dirt | PathNodeKind::Road | PathNodeKind::VacantLot);

        let task_manager = query.task_manager();

        // NOTE: We have to spawn the house building *after* the unit has
        // despawned since we can't place a building over the unit tile.
        let completion_task = task_manager.new_task(UnitTaskDespawnWithCallback {
            callback: Some(Self::on_settled)
        });

        let task = task_manager.new_task(UnitTaskFindVacantHouseLot {
            completion_callback: None,
            completion_task,
            fallback_to_houses_with_room: true,
        });

        settler.assign_task(task_manager, task);
    }

    fn on_settled(query: &Query, unit_prev_cell: Cell, unit_prev_goal: Option<UnitNavGoal>) {
        let settle_new_vacant_lot = unit_prev_goal
            .is_some_and(|goal| navigation::is_goal_vacant_lot_tile(&goal, query) );

        if settle_new_vacant_lot {
            if let Some(tile_def) = Self::find_house_tile_def(query) {
                let world = query.world();
                let tile_map = query.tile_map();

                match world.try_spawn_building_with_tile_def(tile_map, unit_prev_cell, tile_def) {
                    Ok(building) => {
                        let house = building.as_house_mut();
                        house.add_population(1);  
                    },
                    Err(err) => {
                        eprintln!("SettlersSpawnSystem: Failed to place House Level 0: {err}");
                    },
                }
            } else {
                eprintln!("SettlersSpawnSystem: House Level 0 TileDef not found!");
            }
        }
        // Else unit settled into existing household.
    }

    fn find_house_tile_def<'tile_sets>(query: &'tile_sets Query) -> Option<&'tile_sets TileDef> {
        query.find_tile_def(
            TileMapLayerKind::Objects,
            OBJECTS_BUILDINGS_CATEGORY.hash,
            hash::fnv1a_from_str("house0"))
    }
}
