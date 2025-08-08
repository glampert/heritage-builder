use strum_macros::Display;
use proc_macros::DrawDebugUi;

use crate::{
    imgui_ui::UiSystem,
    game::building::BuildingKind,
    utils::{Seconds, coords::Cell},
    pathfind::{Graph, Path, NodeKind as PathNodeKind},
};

use super::{
    anim::{UnitAnimSets, UnitAnimSetKey}
};

// ----------------------------------------------
// UnitDirection
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Default, Display)]
pub enum UnitDirection {
    #[default]
    Idle,
    NE,
    NW,
    SE,
    SW,
}

#[inline]
pub fn direction_between(a: Cell, b: Cell) -> UnitDirection {
    match (b.x - a.x, b.y - a.y) {
        ( 1,  0 ) => UnitDirection::NE,
        ( 0,  1 ) => UnitDirection::NW,
        ( 0, -1 ) => UnitDirection::SE,
        (-1,  0 ) => UnitDirection::SW,
        _ => UnitDirection::Idle,
    }
}

#[inline]
pub fn anim_set_for_direction(direction: UnitDirection) -> UnitAnimSetKey {
    match direction {
        UnitDirection::Idle => UnitAnimSets::IDLE,
        UnitDirection::NE   => UnitAnimSets::WALK_NE,
        UnitDirection::NW   => UnitAnimSets::WALK_NW,
        UnitDirection::SE   => UnitAnimSets::WALK_SE,
        UnitDirection::SW   => UnitAnimSets::WALK_SW,
    }
}

// ----------------------------------------------
// UnitNavigation
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum UnitNavStatus {
    Idle,
    Moving,
    Paused,
}

#[derive(Copy, Clone)]
pub enum UnitNavResult {
    Idle,                                   // Do nothing (also returned when no path).
    Moving(Cell, Cell, f32, UnitDirection), // From -> To cells and progress between them.
    AdvancedCell(Cell, UnitDirection),      // Cell we've just entered, new direction to turn.
    ReachedGoal(Cell, UnitDirection),       // Goal Cell, current direction.
    PathBlocked,
}

#[derive(Copy, Clone)]
pub struct UnitNavGoal {
    pub origin_kind: BuildingKind,
    pub origin_base_cell: Cell,

    pub destination_kind: BuildingKind,
    pub destination_base_cell: Cell,
    pub destination_road_link: Cell,
}

#[derive(Clone, Default, DrawDebugUi)]
pub struct UnitNavigation {
    #[debug_ui(skip)]
    path: Path,
    path_index: usize,
    progress: f32, // 0.0 to 1.0 for the current segment.

    #[debug_ui(separator)]
    direction: UnitDirection,

    #[debug_ui(skip)]
    goal: Option<UnitNavGoal>, // (origin_cell, destination_cell) may be different from path start/end.

    // Debug:
    #[debug_ui(edit)]
    pause_current_path: bool,
    #[debug_ui(edit)]
    single_step: bool,
    #[debug_ui(edit, step = "0.01")]
    step_size: f32,
    #[debug_ui(edit, widget = "button")]
    advance_one_step: bool,
}

impl UnitNavigation {
    // TODO: Make this part of UnitConfig:
    //  config.speed = 1.5; // tiles per second
    //  config.segment_duration = 1.0 / config.speed;
    const SEGMENT_DURATION: f32 = 0.6;

    pub fn update(&mut self, graph: &Graph, mut delta_time_secs: Seconds) -> UnitNavResult {
        if self.pause_current_path || self.path.is_empty() {
            // No path to follow.
            return UnitNavResult::Idle;
        }

        // Single step debug:
        if self.single_step {
            if !self.advance_one_step {
                return UnitNavResult::Idle;
            }
            self.advance_one_step = false;
            delta_time_secs = self.step_size;
        }

        if self.path_index + 1 >= self.path.len() {
            // Reached destination.
            return UnitNavResult::ReachedGoal(self.path[self.path_index].cell, self.direction);
        }

        let from = self.path[self.path_index];
        let to   = self.path[self.path_index + 1];

        if graph.node_kind(to).is_none_or(|kind| kind != PathNodeKind::Road) {
            return UnitNavResult::PathBlocked;
        }

        self.progress += delta_time_secs / Self::SEGMENT_DURATION;

        if self.progress >= 1.0 {
            self.path_index += 1;
            self.progress = 0.0;

            // Look ahead for next turn:
            if self.path_index + 1 < self.path.len() {
                self.direction = direction_between(to.cell, self.path[self.path_index + 1].cell);
            }

            return UnitNavResult::AdvancedCell(to.cell, self.direction);
        }

        // Make sure we start off with the correct heading.
        if self.path_index == 0 {
            self.direction = direction_between(from.cell, to.cell);
        }

        UnitNavResult::Moving(from.cell, to.cell, self.progress, self.direction)
    }

    pub fn reset_path(&mut self) {
        self.path.clear();
        self.path_index = 0;
        self.progress   = 0.0;
        self.direction  = UnitDirection::default();
    }

    pub fn reset(&mut self, new_path: Option<&Path>, optional_goal: Option<UnitNavGoal>) {
        self.reset_path();
        self.goal = optional_goal;

        if let Some(new_path) = new_path {
            debug_assert!(!new_path.is_empty());
            // NOTE: Use extend() instead of direct assignment so
            // we can reuse the previous allocation of `self.path`.
            self.path.extend(new_path.iter().copied());
        }
    }

    pub fn status(&self) -> UnitNavStatus {
        if self.pause_current_path || (self.single_step && !self.advance_one_step) {
            // Paused/waiting on single step.
            return UnitNavStatus::Paused;
        }
        if self.path.is_empty() || (self.path_index + 1 >= self.path.len()) {
            // No path to follow or reached destination.
            return UnitNavStatus::Idle;
        }
        UnitNavStatus::Moving
    }

    #[inline]
    pub fn goal(&self) -> Option<&UnitNavGoal> {
        self.goal.as_ref()
    }
}
