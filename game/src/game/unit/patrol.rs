#![allow(clippy::too_many_arguments)]

use rand::Rng;
use proc_macros::DrawDebugUi;
use serde::{Deserialize, Serialize};

use super::{
    config::UnitConfigKey,
    Unit, UnitId, UnitTaskHelper,
    task::{
        UnitPatrolPathRecord, UnitTaskDespawn,
        UnitTaskPatrolCompletionCallback,
        UnitTaskRandomizedPatrol,
    }
};
use crate::{
    ui::UiSystem,
    save::PostLoadContext,
    engine::time::{
        Seconds, CountdownTimer, UpdateTimer
    },
    game::{
        world::object::GameObject,
        sim::{Query, RandomGenerator},
        building::{Building, BuildingContext, BuildingKind},
    },
    utils::{
        callback::{self, Callback},
        coords::Cell,
    }
};

// ----------------------------------------------
// PatrolInternalState
// ----------------------------------------------

pub type PatrolCompletionCallback = fn(&mut Building, &mut Unit, &Query) -> bool;

#[derive(Clone, DrawDebugUi, Serialize, Deserialize)]
struct PatrolInternalState {
    // Patrol task tunable parameters:
    #[debug_ui(edit)]
    max_distance: i32,
    #[debug_ui(edit)]
    path_bias_min: f32,
    #[debug_ui(edit)]
    path_bias_max: f32,

    #[debug_ui(skip)]
    path_record: UnitPatrolPathRecord,

    #[debug_ui(skip)]
    completion_callback: Callback<PatrolCompletionCallback>,

    #[serde(skip)]
    #[debug_ui(skip)]
    failed_to_spawn: bool, // Debug flag; not serialized.
}

impl PatrolInternalState {
    #[inline]
    fn reset(&mut self) {
        self.completion_callback = Callback::default();
        self.failed_to_spawn = false;
    }
}

// ----------------------------------------------
// Patrol Unit helper
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Patrol {
    unit_id: UnitId,
    state: Option<Box<PatrolInternalState>>, // Lazily initialized.
}

impl UnitTaskHelper for Patrol {
    #[inline]
    fn reset(&mut self) {
        self.unit_id = UnitId::default();
        if let Some(state) = self.try_get_state_mut() {
            state.reset();
        }
    }

    #[inline]
    fn on_unit_spawn(&mut self, unit_id: UnitId, failed_to_spawn: bool) {
        self.unit_id = unit_id;
        if let Some(state) = self.try_get_state_mut() {
            state.failed_to_spawn = failed_to_spawn;
        }
    }

    #[inline]
    fn unit_id(&self) -> UnitId {
        self.unit_id
    }

    #[inline]
    fn failed_to_spawn(&self) -> bool {
        if let Some(state) = self.try_get_state() {
            state.failed_to_spawn
        } else {
            false
        }
    }
}

impl Patrol {
    pub fn start_randomized_patrol(&mut self,
                                   context: &BuildingContext,
                                   unit_origin: Cell,
                                   unit_config: UnitConfigKey,
                                   max_patrol_distance: i32,
                                   buildings_to_visit: Option<BuildingKind>,
                                   completion_callback: Callback<PatrolCompletionCallback>,
                                   idle_countdown_secs: Option<Seconds>)
                                   -> bool {
        debug_assert!(unit_origin.is_valid());
        debug_assert!(max_patrol_distance > 0, "Patrol max distance cannot be zero!");
        debug_assert!(!self.is_spawned(), "Patrol Unit already spawned! reset() first.");

        let (max_distance, path_bias_min, path_bias_max, path_record) = {
            let state = self.get_or_init_state(max_patrol_distance);
            state.completion_callback = completion_callback;
            (state.max_distance,
             state.path_bias_min,
             state.path_bias_max,
             state.path_record.clone())
        };

        self.try_spawn_with_task(
            context.debug_name(),
            context.query,
            unit_origin,
            unit_config,
            UnitTaskRandomizedPatrol {
                origin_building: context.kind_and_id(),
                origin_building_tile: context.tile_info(),
                max_distance,
                path_bias_min,
                path_bias_max,
                path_record,
                buildings_to_visit,
                completion_callback: callback::create!(Patrol::on_randomized_patrol_completed),
                completion_task: context.query.task_manager().new_task(UnitTaskDespawn),
                idle_countdown: idle_countdown_secs.map(|countdown| (CountdownTimer::new(countdown), countdown)),
            }
        )
    }

    pub fn register_callbacks() {
        let _: Callback<UnitTaskPatrolCompletionCallback> =
            callback::register!(Patrol::on_randomized_patrol_completed);

        let _: Callback<UnitTaskPatrolCompletionCallback> =
            callback::register!(TimedAmbientPatrol::on_timed_patrol_completed);
    }

    pub fn post_load(&mut self) {
        if let Some(state) = self.try_get_state_mut() {
            state.completion_callback.post_load();
        }
    }

    pub fn draw_debug_ui(&mut self, label: &str, ui_sys: &UiSystem) {
        let unit_id = self.unit_id();
        if let Some(state) = self.try_get_state_mut() {
            let ui = ui_sys.ui();
            if ui.collapsing_header(label, imgui::TreeNodeFlags::empty()) {
                ui.text(format!("Unit Id : {}", unit_id));
                state.path_record.draw_debug_ui(ui_sys);
                state.draw_debug_ui(ui_sys);
            }
        }
    }

    fn on_randomized_patrol_completed(origin_building: &mut Building,
                                      patrol_unit: &mut Unit,
                                      query: &Query) -> bool {
        let patrol_task =
            patrol_unit.current_task_as::<UnitTaskRandomizedPatrol>(query.task_manager())
                       .expect("Expected Patrol Unit to be running a UnitTaskRandomizedPatrol!");

        let this_patrol =
            origin_building.active_patrol()
                           .expect("Origin building should have sent out a Patrol Unit!");

        let state = this_patrol.try_get_state_mut().expect("Missing PatrolInternalState!");

        // Update path record and invoke the Building's completion callback:
        state.path_record = patrol_task.path_record.clone();

        if state.completion_callback.is_valid() {
            let callback = state.completion_callback.get();
            return callback(origin_building, patrol_unit, query);
        }

        true // Task completed.
    }

    #[inline]
    fn get_or_init_state(&mut self, max_distance: i32) -> &mut PatrolInternalState {
        if self.state.is_none() {
            self.state =
                Some(Box::new(PatrolInternalState { max_distance,
                                                    path_bias_min: 0.1,
                                                    path_bias_max: 0.5,
                                                    path_record: UnitPatrolPathRecord::default(),
                                                    completion_callback: Callback::default(),
                                                    failed_to_spawn: false }));
        }
        self.state.as_deref_mut().unwrap()
    }

    #[inline]
    fn try_get_state(&self) -> Option<&PatrolInternalState> {
        match &self.state {
            Some(state) => Some(state.as_ref()),
            None => None,
        }
    }

    #[inline]
    fn try_get_state_mut(&mut self) -> Option<&mut PatrolInternalState> {
        match &mut self.state {
            Some(state) => Some(state.as_mut()),
            None => None,
        }
    }
}

// ----------------------------------------------
// AmbientPatrolConfig
// ----------------------------------------------

#[derive(Default, DrawDebugUi, Serialize, Deserialize)]
#[serde(default)] // Default all fields.
pub struct AmbientPatrolConfig {
    #[debug_ui(format = "Patrol Unit : {:?}")]
    pub unit: Option<UnitConfigKey>,

    // Ambient patrol min/max spawn frequency (randomized in this range).
    #[debug_ui(format = "Spawn Frequency Secs : {:?}")]
    pub spawn_frequency_secs: [Seconds; 2],

    // [0,100] % chance of spawning an ambient patrol unit every `spawn_frequency_secs`.
    pub spawn_chance: u32,

    // How far the parol unit will walk before returning home.
    pub max_distance: i32,
}

// ----------------------------------------------
// TimedAmbientPatrol
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct TimedAmbientPatrol {
    pub patrol: Patrol,
    pub spawn_timer: UpdateTimer, // Min time before we can spawn a new patrol unit.
}

impl TimedAmbientPatrol {
    pub fn new(rng: &mut RandomGenerator, spawn_frequency_secs: [Seconds; 2]) -> Self {
        let frequency_secs = Self::randomized_spawn_frequency(rng, spawn_frequency_secs);
        Self {
            patrol: Patrol::default(),
            spawn_timer: UpdateTimer::new(frequency_secs),
        }
    }

    pub fn post_load(&mut self, context: &PostLoadContext, spawn_frequency_secs: [Seconds; 2]) {
        self.patrol.post_load();

        let frequency_secs = Self::randomized_spawn_frequency(context.rng(), spawn_frequency_secs);
        self.spawn_timer.post_load(frequency_secs);
    }

    pub fn try_spawn_unit(&mut self,
                          context: &BuildingContext,
                          unit_config: UnitConfigKey,
                          spawn_chance: u32,
                          max_patrol_distance: i32,
                          idle_countdown_secs: f32,
                          force_spawn: bool) -> bool {
        if self.patrol.is_spawned() || max_patrol_distance <= 0 {
            return false; // A unit is already spawned. Try again later.
        }

        if !force_spawn {
            let chance = spawn_chance.min(100); // 0-100%
            if chance == 0 {
                return false;
            }

            let should_spawn = context.query.rng().random_bool(chance as f64 / 100.0);
            if !should_spawn {
                return false;
            }
        }

        // Unit spawns at the nearest road link.
        let unit_origin = match context.road_link {
            Some(road_link) => road_link,
            None => return false, // We are not connected to a road!
        };

        self.patrol.start_randomized_patrol(context,
            unit_origin,
            unit_config,
            max_patrol_distance,
            None,
            callback::create!(TimedAmbientPatrol::on_timed_patrol_completed),
            Some(idle_countdown_secs.round()))
    }

    fn on_timed_patrol_completed(this_building: &mut Building,
                                 patrol_unit: &mut Unit,
                                 query: &Query) -> bool {
        let patrol = this_building.active_patrol()
            .expect("Expected building to have an active TimedAmbientPatrol!");

        debug_assert!(patrol.unit_id() == patrol_unit.id());
        debug_assert!(patrol.is_running_task::<UnitTaskRandomizedPatrol>(query),
                      "No timed ambient unit patrol was sent out by this building!");

        patrol.reset();
        true
    }

    fn randomized_spawn_frequency(rng: &mut RandomGenerator, spawn_frequency_secs: [Seconds; 2]) -> Seconds {
        let frequency_min = spawn_frequency_secs[0];
        let frequency_max = spawn_frequency_secs[1];

        if frequency_min == frequency_max {
            return frequency_min;
        }

        rng.random_range(frequency_min..frequency_max).round()
    }
}
