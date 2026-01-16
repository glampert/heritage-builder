use std::any::Any;
use rand::{Rng, seq::IteratorRandom};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use super::GameSystem;
use crate::{
    log,
    ui::UiSystem,
    save::PostLoadContext,
    pathfind::{Path, Node},
    engine::time::UpdateTimer,
    tile::{TileDepthSortOverride},
    utils::{callback::Callback, coords::Cell},
    game::{
        config::GameConfigs,
        sim::Query,
        unit::{
            Unit,
            anim::UnitAnimSets,
            config::UnitConfigKey,
            task::{UnitTaskDespawn, UnitTaskFollowPath},
        },
    },
};

// ----------------------------------------------
// AmbientEffectsSystem
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct AmbientEffectsSystem {
    bird_spawn_timer: UpdateTimer,
}

impl GameSystem for AmbientEffectsSystem {
    fn name(&self) -> &str {
        "Ambient Effects System"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update(&mut self, query: &Query) {
        if self.bird_spawn_timer.tick(query.delta_time_secs()).should_update() {
            // FIXME: Need to handle new inner-rect playable area. Spawn on the inner-rect edge instead.
            //spawn_bird_with_random_flight_path(query);
        }
    }

    fn post_load(&mut self, _context: &PostLoadContext) {
        let configs = GameConfigs::get();
        self.bird_spawn_timer.post_load(configs.sim.birds_spawn_frequency);
    }

    fn draw_debug_ui(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        self.bird_spawn_timer.draw_debug_ui("Bird Spawn", 0, ui_sys);

        if ui.button("Spawn Bird (left-to-right path") {
            spawn_bird(query, BirdFlightPath::LeftToRight);
        }

        if ui.button("Spawn Bird (right-to-left path)") {
            spawn_bird(query, BirdFlightPath::RightToLeft);
        }

        if ui.button("Spawn Big Flock") {
            for _ in 0..50 {
                spawn_bird_with_random_flight_path(query);
            }
        }
    }
}

impl Default for AmbientEffectsSystem {
    fn default() -> Self {
        let configs = GameConfigs::get();
        Self {
            bird_spawn_timer: UpdateTimer::new(configs.sim.birds_spawn_frequency),
        }
    }
}

// ----------------------------------------------
// Bird Flocks
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, EnumIter)]
enum BirdFlightPath {
    LeftToRight,
    RightToLeft,
}

fn spawn_bird(query: &Query, flight_path: BirdFlightPath) {
    let (path, anim_set_key) = {
        match flight_path {
            BirdFlightPath::LeftToRight => {
                (make_left_to_right_randomized_path(query), UnitAnimSets::WALK_SE)
            }
            BirdFlightPath::RightToLeft => {
                (make_right_to_left_randomized_path(query), UnitAnimSets::WALK_SW)
            }
        }
    };

    let result = Unit::try_spawn_with_task(
        query,
        path.first().unwrap().cell,
        UnitConfigKey::Bird,
        UnitTaskFollowPath {
            path,
            completion_callback: Callback::default(),
            completion_task: query.task_manager().new_task(UnitTaskDespawn),
        });

    match result {
        Ok(unit) => {
            unit.set_animation(query, anim_set_key);
            unit.set_depth_sort_override(query, TileDepthSortOverride::Topmost);
        }
        Err(err) => {
            log::warn!(log::channel!("ambient_effects"), "Failed to spawn bird: {err}");
        }
    }
}

fn spawn_bird_with_random_flight_path(query: &Query) {
    let flight_path = BirdFlightPath::iter().choose(query.rng()).unwrap();
    spawn_bird(query, flight_path);
}

fn make_left_to_right_randomized_path(query: &Query) -> Path {
    let map_size = query.tile_map().size_in_cells();

    let randomized_spawn_point = || {
        let rng = query.rng();
        let mut start = 0;

        loop {
            // Randomize either the X or Y axis.
            let cell = {
                if rng.random_bool(0.5) {
                    Cell::new(rng.random_range(start..map_size.width), map_size.height - 1) // X
                } else {
                    Cell::new(0, rng.random_range(start..map_size.height)) // Y
                }
            };

            if query.tile_map().is_cell_within_playable_area(cell) {
                return cell;
            }

            // Retry if the randomized spawn point falls outside the playable area.
            start += 1;
        }
    };

    let mut cell = randomized_spawn_point();
    let mut path = Path::new();

    for _ in 0..map_size.width {
        path.push(Node::new(cell));
        if cell.x == (map_size.width - 1) || cell.y == 0 {
            break;
        }
        cell.x += 1;
        cell.y -= 1;
    }

    path
}

fn make_right_to_left_randomized_path(query: &Query) -> Path {
    let map_size = query.tile_map().size_in_cells();

    let randomized_spawn_point = || {
        let rng = query.rng();
        let mut start = 0;

        loop {
            // Randomize either the X or Y axis.
            let cell = {
                if rng.random_bool(0.5) {
                    Cell::new(rng.random_range(start..map_size.width), 0) // X
                } else {
                    Cell::new(map_size.width - 1, rng.random_range(start..map_size.height)) // Y
                }
            };

            if query.tile_map().is_cell_within_playable_area(cell) {
                return cell;
            }

            // Retry if the randomized spawn point falls outside the playable area.
            start += 1;
        }
    };

    let mut cell = randomized_spawn_point();
    let mut path = Path::new();

    for _ in 0..map_size.width {
        path.push(Node::new(cell));
        if cell.x == 0 || cell.y == (map_size.height - 1) {
            break;
        }
        cell.x -= 1;
        cell.y += 1;
    }

    path
}
