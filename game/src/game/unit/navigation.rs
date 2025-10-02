use proc_macros::DrawDebugUi;
use serde::{Deserialize, Serialize};
use strum_macros::Display;

use super::anim::{UnitAnimSetKey, UnitAnimSets};
use crate::{
    debug::{self as debug_utils},
    engine::time::Seconds,
    game::{
        building::{BuildingKind, BuildingTileInfo},
        sim::Query,
    },
    pathfind::{Graph, NodeKind as PathNodeKind, Path},
    tile::TileMapLayerKind,
    utils::coords::Cell,
};

// ----------------------------------------------
// UnitDirection
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Default, Display, Serialize, Deserialize)]
pub enum UnitDirection {
    #[default]
    Idle,
    NE,
    NW,
    SE,
    SW,
}

impl UnitDirection {
    #[inline]
    pub fn is_north(self) -> bool {
        matches!(self, Self::NE | Self::NW)
    }

    #[inline]
    pub fn is_south(self) -> bool {
        matches!(self, Self::SE | Self::SW)
    }

    #[inline]
    pub fn is_east(self) -> bool {
        matches!(self, Self::NE | Self::SE)
    }

    #[inline]
    pub fn is_west(self) -> bool {
        matches!(self, Self::NW | Self::SW)
    }
}

#[inline]
pub fn same_axis(a: UnitDirection, b: UnitDirection) -> bool {
    (a.is_north() && b.is_north())
    || (a.is_south() && b.is_south())
    || (a.is_east() && b.is_east())
    || (a.is_west() && b.is_west())
}

#[inline]
pub fn direction_between(a: Cell, b: Cell) -> UnitDirection {
    let dx = b.x - a.x;
    let dy = b.y - a.y;

    if dx.abs() > dy.abs() {
        // Move horizontally in grid space
        if dx > 0 {
            UnitDirection::NE
        } else {
            UnitDirection::SW
        }
    } else if dy != 0 {
        // Move vertically in grid space
        if dy > 0 {
            UnitDirection::NW
        } else {
            UnitDirection::SE
        }
    } else {
        UnitDirection::Idle
    }
}

#[inline]
pub fn anim_set_for_direction(direction: UnitDirection) -> UnitAnimSetKey {
    match direction {
        UnitDirection::Idle => UnitAnimSets::IDLE,
        UnitDirection::NE => UnitAnimSets::WALK_NE,
        UnitDirection::NW => UnitAnimSets::WALK_NW,
        UnitDirection::SE => UnitAnimSets::WALK_SE,
        UnitDirection::SW => UnitAnimSets::WALK_SW,
    }
}

// ----------------------------------------------
// UnitNavGoal
// ----------------------------------------------

#[derive(Copy, Clone, Serialize, Deserialize)]
pub enum UnitNavGoal {
    Building {
        origin_kind: BuildingKind,
        origin_base_cell: Cell,
        destination_kind: BuildingKind,
        destination_base_cell: Cell,
        destination_road_link: Cell,
    },
    Tile {
        origin_cell: Cell,
        destination_cell: Cell,
    },
}

impl UnitNavGoal {
    pub fn origin_cell(&self) -> Cell {
        match self {
            UnitNavGoal::Building { origin_base_cell, .. } => *origin_base_cell,
            UnitNavGoal::Tile { origin_cell, .. } => *origin_cell,
        }
    }

    pub fn destination_cell(&self) -> Cell {
        match self {
            UnitNavGoal::Building { destination_road_link, .. } => *destination_road_link,
            UnitNavGoal::Tile { destination_cell, .. } => *destination_cell,
        }
    }

    pub fn origin_debug_name(&self) -> &str {
        let (origin_cell, layer) = match self {
            Self::Building { origin_base_cell, .. } => {
                (*origin_base_cell, TileMapLayerKind::Objects)
            }
            Self::Tile { origin_cell, .. } => (*origin_cell, TileMapLayerKind::Terrain),
        };
        debug_utils::tile_name_at(origin_cell, layer)
    }

    pub fn destination_debug_name(&self) -> &str {
        let (destination_cell, layer) = match self {
            UnitNavGoal::Building { destination_base_cell, .. } => {
                (*destination_base_cell, TileMapLayerKind::Objects)
            }
            UnitNavGoal::Tile { destination_cell, .. } => {
                (*destination_cell, TileMapLayerKind::Terrain)
            }
        };
        debug_utils::tile_name_at(destination_cell, layer)
    }

    // Building Goal:

    pub fn building(origin_kind: BuildingKind,
                    origin_base_cell: Cell,
                    destination_kind: BuildingKind,
                    destination_tile: BuildingTileInfo)
                    -> Self {
        Self::Building { origin_kind,
                         origin_base_cell,
                         destination_kind,
                         destination_base_cell: destination_tile.base_cell,
                         destination_road_link: destination_tile.road_link }
    }

    pub fn is_building(&self) -> bool {
        matches!(self, Self::Building { .. })
    }

    pub fn building_origin(&self) -> (BuildingKind, Cell) {
        match self {
            Self::Building { origin_kind, origin_base_cell, .. } => {
                (*origin_kind, *origin_base_cell)
            }
            _ => panic!("UnitNavGoal not a Building goal!"),
        }
    }

    pub fn building_destination(&self) -> (BuildingKind, Cell) {
        match self {
            Self::Building { destination_kind, destination_base_cell, .. } => {
                (*destination_kind, *destination_base_cell)
            }
            _ => panic!("UnitNavGoal not a Building goal!"),
        }
    }

    // Tile Goal:

    pub fn tile(origin_cell: Cell, path: &Path) -> Self {
        debug_assert!(!path.is_empty());
        Self::Tile { origin_cell, destination_cell: path.last().unwrap().cell }
    }

    pub fn is_tile(&self) -> bool {
        matches!(self, Self::Tile { .. })
    }

    pub fn tile_origin(&self) -> Cell {
        match self {
            Self::Tile { origin_cell, .. } => *origin_cell,
            _ => panic!("UnitNavGoal not a Tile goal!"),
        }
    }

    pub fn tile_destination(&self) -> Cell {
        match self {
            Self::Tile { destination_cell, .. } => *destination_cell,
            _ => panic!("UnitNavGoal not a Tile goal!"),
        }
    }
}

pub fn is_goal_vacant_lot_tile(goal: &UnitNavGoal, query: &Query) -> bool {
    if goal.is_tile() {
        let maybe_tile =
            query.tile_map()
                 .try_tile_from_layer(goal.tile_destination(), TileMapLayerKind::Terrain);
        return maybe_tile.is_some_and(|tile| tile.path_kind() == PathNodeKind::VacantLot);
    }
    false
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

#[derive(Clone, Default, DrawDebugUi, Serialize, Deserialize)]
pub struct UnitNavigation {
    #[debug_ui(skip)]
    path: Path,
    path_index: usize,
    progress: f32, // 0.0 to 1.0 for the current segment.
    direction: UnitDirection,

    traversable_node_kinds: PathNodeKind,

    #[debug_ui(separator)]
    segment_duration: f32,

    #[debug_ui(skip)]
    goal: Option<UnitNavGoal>, /* (origin_cell, destination_cell) may be different from path
                                * start/end. */

    // Debug:
    #[serde(skip)]
    #[debug_ui(edit)]
    pause_current_path: bool,

    #[serde(skip)]
    #[debug_ui(edit)]
    single_step: bool,

    #[serde(skip)]
    #[debug_ui(edit, step = "0.01")]
    step_size: f32,

    #[serde(skip)]
    #[debug_ui(edit, widget = "button")]
    advance_one_step: bool,
}

impl UnitNavigation {
    pub fn update(&mut self, graph: &Graph, mut delta_time_secs: Seconds) -> UnitNavResult {
        debug_assert!(self.segment_duration.is_finite() && self.segment_duration != 0.0);

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
        let to = self.path[self.path_index + 1];

        if graph.node_kind(to).is_none_or(|kind| !kind.intersects(self.traversable_node_kinds)) {
            return UnitNavResult::PathBlocked;
        }

        self.progress += delta_time_secs / self.segment_duration;

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

    pub fn reset_path_only(&mut self) {
        self.path.clear();
        self.path_index = 0;
        self.progress = 0.0;
        self.direction = UnitDirection::default();
    }

    pub fn reset_path_and_goal(&mut self,
                               new_path: Option<&Path>,
                               optional_goal: Option<UnitNavGoal>) {
        self.reset_path_only();
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
    pub fn is_following_path(&self) -> bool {
        if self.path.is_empty() || (self.path_index + 1 >= self.path.len()) {
            // No path to follow or reached destination.
            return false;
        }
        true
    }

    #[inline]
    pub fn goal(&self) -> Option<&UnitNavGoal> {
        self.goal.as_ref()
    }

    #[inline]
    pub fn traversable_node_kinds(&self) -> PathNodeKind {
        self.traversable_node_kinds
    }

    #[inline]
    pub fn set_traversable_node_kinds(&mut self, traversable_node_kinds: PathNodeKind) {
        debug_assert!(!traversable_node_kinds.is_empty());
        self.traversable_node_kinds = traversable_node_kinds;
    }

    #[inline]
    pub fn set_movement_speed(&mut self, movement_speed: f32) {
        debug_assert!(movement_speed.is_finite() && movement_speed != 0.0);
        self.segment_duration = 1.0 / movement_speed;
    }
}
