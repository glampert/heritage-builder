use std::any::Any;
use rand::Rng;
use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};

use super::GameSystem;
use crate::{
    ui::UiSystem,
    save::PostLoadContext,
    pathfind::{Path, Node},
    engine::time::UpdateTimer,
    utils::{callback::Callback, coords::Cell, Size},
    game::{
        config::GameConfigs,
        sim::{Query, RandomGenerator},
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
            spawn_bird_with_random_flight_path(query);
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
#[derive(Copy, Clone, TryFromPrimitive)]
enum BirdFlightPath {
    LeftToRight,
    RightToLeft,
}

fn spawn_bird(query: &Query, flight_path: BirdFlightPath) {
    let map_size = query.tile_map().size_in_cells();

    let (path, anim_set_key) = {
        match flight_path {
            BirdFlightPath::LeftToRight => {
                (make_left_to_right_randomized_path(query.rng(), map_size), UnitAnimSets::WALK_SE)
            }
            BirdFlightPath::RightToLeft => {
                (make_right_to_left_randomized_path(query.rng(), map_size), UnitAnimSets::WALK_SW)
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

    if let Ok(unit) = result {
        unit.set_animation(query, anim_set_key);
    }
}

fn spawn_bird_with_random_flight_path(query: &Query) {
    let min = BirdFlightPath::LeftToRight as u32;
    let max = BirdFlightPath::RightToLeft as u32;
    let dir = query.random_range(min..=max);
    spawn_bird(query, BirdFlightPath::try_from_primitive(dir).unwrap());
}

fn make_left_to_right_randomized_path(rng: &mut RandomGenerator, map_size: Size) -> Path {
    let mut path = Path::new();

    // Randomize either the X or Y axis.
    let (mut x, mut y) = {
        if rng.random_bool(0.5) {
            (rng.random_range(0..map_size.width), map_size.height - 1) // X
        } else {
            (0, rng.random_range(0..map_size.height)) // Y
        }
    };

    for _ in 0..map_size.width {
        path.push(Node::new(Cell::new(x, y)));
        if x == (map_size.width - 1) || y == 0 {
            break;
        }
        x += 1;
        y -= 1;
    }

    path
}

fn make_right_to_left_randomized_path(rng: &mut RandomGenerator, map_size: Size) -> Path {
    let mut path = Path::new();

    // Randomize either the X or Y axis.
    let (mut x, mut y) = {
        if rng.random_bool(0.5) {
            (rng.random_range(0..map_size.width), 0) // X
        } else {
            (map_size.width - 1, rng.random_range(0..map_size.height)) // Y
        }
    };

    for _ in 0..map_size.width {
        path.push(Node::new(Cell::new(x, y)));
        if x == 0 || y == (map_size.height - 1) {
            break;
        }
        x -= 1;
        y += 1;
    }

    path
}
