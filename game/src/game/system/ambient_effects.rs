use std::any::Any;
use smallvec::SmallVec;
use rand::seq::{IndexedRandom, IteratorRandom};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum::EnumIter;

use super::GameSystem;
use crate::{
    log,
    engine::Engine,
    save::PostLoadContext,
    pathfind::{Path, Node},
    tile::{TileDepthSortOverride},
    utils::{callback::Callback, coords::Cell, time::UpdateTimer},
    game::{
        config::GameConfigs,
        sim::SimContext,
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
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update(&mut self, _engine: &mut dyn Engine, context: &SimContext) {
        if self.bird_spawn_timer.tick(context.delta_time_secs()).should_update() {
            spawn_bird_with_random_flight_path(context);
        }
    }

    fn reset(&mut self, _engine: &mut dyn Engine) {
        self.bird_spawn_timer.reset();
    }

    fn post_load(&mut self, _context: &mut PostLoadContext) {
        let configs = GameConfigs::get();
        self.bird_spawn_timer.post_load(configs.sim.birds_spawn_frequency);
    }

    fn draw_debug_ui(&mut self, engine: &mut dyn Engine, context: &SimContext) {
        self.bird_spawn_timer.draw_debug_ui("Bird Spawn", 0, engine.ui_system());

        let ui = engine.ui_system().ui();

        if ui.button("Spawn Bird (left-to-right path") {
            spawn_bird(context, BirdFlightPath::LeftToRight);
        }

        if ui.button("Spawn Bird (right-to-left path)") {
            spawn_bird(context, BirdFlightPath::RightToLeft);
        }

        if ui.button("Spawn Big Flock") {
            for _ in 0..50 {
                spawn_bird_with_random_flight_path(context);
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

fn spawn_bird(context: &SimContext, flight_path: BirdFlightPath) {
    let (path, anim_set_key) = {
        match flight_path {
            BirdFlightPath::LeftToRight => {
                (make_left_to_right_randomized_path(context), UnitAnimSets::WALK_SE)
            }
            BirdFlightPath::RightToLeft => {
                (make_right_to_left_randomized_path(context), UnitAnimSets::WALK_SW)
            }
        }
    };

    let result = Unit::try_spawn_with_task(
        context,
        path.first().unwrap().cell,
        UnitConfigKey::Bird,
        UnitTaskFollowPath {
            path,
            completion_callback: Callback::default(),
            completion_task: context.task_manager_mut().new_task(UnitTaskDespawn),
            terminate_if_stuck: true,
        });

    match result {
        Ok(unit) => {
            unit.set_animation(context, anim_set_key);
            unit.set_depth_sort_override(context, TileDepthSortOverride::Topmost);
        }
        Err(err) => {
            log::warning!(log::channel!("ambient_effects"), "Failed to spawn bird: {}", err.message);
        }
    }
}

fn spawn_bird_with_random_flight_path(context: &SimContext) {
    let flight_path = BirdFlightPath::iter().choose(context.rng_mut()).unwrap();
    spawn_bird(context, flight_path);
}

fn make_left_to_right_randomized_path(context: &SimContext) -> Path {
    let map_size = context.tile_map().size_in_cells();

    let randomized_spawn_point = || {
        let min_cell = Cell::new(0, (map_size.height / 2) - 1);
        let max_cell = Cell::new((map_size.width / 2) - 1, map_size.height - 1);

        let mut cell = min_cell;
        let mut cells = SmallVec::<[Cell; 256]>::new();

        loop {
            cells.push(cell);
            if cell.x == max_cell.x || cell.y == max_cell.y {
                break;
            }
            cell.x += 1;
            cell.y += 1;
        }

        *cells.choose(context.rng_mut()).unwrap()
    };

    let mut cell = randomized_spawn_point();
    let mut path = Path::new();

    let half_width = map_size.width / 2;
    for _ in 0..half_width {
        path.push(Node::new(cell));
        cell.x += 1;
        cell.y -= 1;
    }

    path
}

fn make_right_to_left_randomized_path(context: &SimContext) -> Path {
    let map_size = context.tile_map().size_in_cells();

    let randomized_spawn_point = || {
        let min_cell = Cell::new((map_size.width / 2) - 1, 0);
        let max_cell = Cell::new(map_size.width - 1, map_size.height / 2);

        let mut cell = min_cell;
        let mut cells = SmallVec::<[Cell; 256]>::new();

        loop {
            cells.push(cell);
            if cell.x == max_cell.x || cell.y == max_cell.y {
                break;
            }
            cell.x += 1;
            cell.y += 1;
        }

        *cells.choose(context.rng_mut()).unwrap()
    };

    let mut cell = randomized_spawn_point();
    let mut path = Path::new();

    let half_width = map_size.width / 2;
    for _ in 0..half_width {
        path.push(Node::new(cell));
        cell.x -= 1;
        cell.y += 1;
    }

    path
}
