#![allow(clippy::too_many_arguments)]

use proc_macros::DrawDebugUi;
use serde::{Deserialize, Serialize};

use super::{
    config::UnitConfigKey,
    task::{
        UnitPatrolPathRecord, UnitTaskDespawn,
        UnitTaskPatrolCompletionCallback,
        UnitTaskRandomizedPatrol,
    },
    Unit, UnitId, UnitTaskHelper,
};
use crate::{
    imgui_ui::UiSystem,
    engine::time::{Seconds, CountdownTimer},
    game::{
        sim::Query,
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
                                      query: &Query)
                                      -> bool {
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
