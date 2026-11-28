use common::{coords::Cell, time::Seconds};
use proc_macros::DrawDebugUi;
use serde::{Deserialize, Serialize};
use strum::Display;

use super::anim::{UnitAnimSetKey, UnitAnimSets};
use crate::{
    building::{BuildingKind, BuildingTileInfo},
    debug::{self as debug_utils},
    pathfind::{Graph, Node, NodeKind as PathNodeKind, Path},
    sim::SimContext,
    tile::TileMapLayerKind,
};

// ----------------------------------------------
// UnitDirection
// ----------------------------------------------

// The way a unit faces: Idle, or one of the 8 screen directions. NE, NW, SE and SW are the moves
// along the grid's axes (NE +x, NW +y, SE -y, SW -x); N, S, E and W the diagonal ones between them
// (N +x+y, the screen's up; S -x-y; E +x-y, the screen's right; W -x+y), docs/plans/unit-movement.md.
// Saved by name: the diagonal ones come last, so older saves still load.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, Display, Serialize, Deserialize)]
pub enum UnitDirection {
    #[default]
    Idle,
    NE,
    NW,
    SE,
    SW,
    N,
    S,
    E,
    W,
}

impl UnitDirection {
    // The screen's upper half: NE, N and NW.
    #[inline]
    pub fn is_north(self) -> bool {
        matches!(self, Self::NE | Self::NW | Self::N)
    }

    // The screen's lower half: SE, S and SW.
    #[inline]
    pub fn is_south(self) -> bool {
        matches!(self, Self::SE | Self::SW | Self::S)
    }

    // The screen's right half: NE, E and SE.
    #[inline]
    pub fn is_east(self) -> bool {
        matches!(self, Self::NE | Self::SE | Self::E)
    }

    // The screen's left half: NW, W and SW.
    #[inline]
    pub fn is_west(self) -> bool {
        matches!(self, Self::NW | Self::SW | Self::W)
    }

    // A diagonal grid move: N, S, E or W.
    #[inline]
    pub fn is_diagonal(self) -> bool {
        matches!(self, Self::N | Self::S | Self::E | Self::W)
    }
}

#[inline]
pub fn same_axis(a: UnitDirection, b: UnitDirection) -> bool {
    (a.is_north() && b.is_north())
        || (a.is_south() && b.is_south())
        || (a.is_east() && b.is_east())
        || (a.is_west() && b.is_west())
}

// The direction of a step from `a` to `b`: a diagonal one (N, S, E, W) when the step is exactly
// diagonal, else the grid axis it moves furthest along (NE, NW, SE, SW).
#[inline]
pub fn direction_between(a: Cell, b: Cell) -> UnitDirection {
    let dx = b.x - a.x;
    let dy = b.y - a.y;

    if dx != 0 && dx.abs() == dy.abs() {
        match (dx > 0, dy > 0) {
            (true, true) => UnitDirection::N,
            (false, false) => UnitDirection::S,
            (true, false) => UnitDirection::E,
            (false, true) => UnitDirection::W,
        }
    } else {
        heading_between(a, b)
    }
}

// The grid axis `b` lies furthest along from `a` (NE, NW, SE or SW; NW or SE on a tie): a path's
// overall heading, as the patrols weigh it, which stays on the four axes.
#[inline]
pub fn heading_between(a: Cell, b: Cell) -> UnitDirection {
    let dx = b.x - a.x;
    let dy = b.y - a.y;

    if dx.abs() > dy.abs() {
        // Move horizontally in grid space
        if dx > 0 { UnitDirection::NE } else { UnitDirection::SW }
    } else if dy != 0 {
        // Move vertically in grid space
        if dy > 0 { UnitDirection::NW } else { UnitDirection::SE }
    } else {
        UnitDirection::Idle
    }
}

// The axis direction a diagonal one is drawn as by a unit without art for it: of the two beside it,
// the one on the side of the unit's `previous` direction (a unit turning N from the east side, NE),
// else the east one for N and S and the south one for E and W. An axis direction is its own.
#[inline]
pub fn axis_fallback(direction: UnitDirection, previous: UnitDirection) -> UnitDirection {
    use UnitDirection::*;
    match direction {
        N => {
            if previous.is_west() {
                NW
            } else {
                NE
            }
        }
        S => {
            if previous.is_west() {
                SW
            } else {
                SE
            }
        }
        E => {
            if previous.is_north() {
                NE
            } else {
                SE
            }
        }
        W => {
            if previous.is_north() {
                NW
            } else {
                SW
            }
        }
        _ => direction,
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
        UnitDirection::N => UnitAnimSets::WALK_N,
        UnitDirection::S => UnitAnimSets::WALK_S,
        UnitDirection::E => UnitAnimSets::WALK_E,
        UnitDirection::W => UnitAnimSets::WALK_W,
    }
}

#[inline]
pub fn idle_anim_set_for_direction(direction: UnitDirection) -> UnitAnimSetKey {
    match direction {
        UnitDirection::Idle => UnitAnimSets::IDLE,
        UnitDirection::NE => UnitAnimSets::IDLE_NE,
        UnitDirection::NW => UnitAnimSets::IDLE_NW,
        UnitDirection::SE => UnitAnimSets::IDLE_SE,
        UnitDirection::SW => UnitAnimSets::IDLE_SW,
        UnitDirection::N => UnitAnimSets::IDLE_N,
        UnitDirection::S => UnitAnimSets::IDLE_S,
        UnitDirection::E => UnitAnimSets::IDLE_E,
        UnitDirection::W => UnitAnimSets::IDLE_W,
    }
}

// The walk anim set a unit turning to `direction` from `previous` plays: its own when the unit has
// it (`has`), else, for a diagonal direction, that of the axis direction it falls back to
// (axis_fallback). Returns the set and the direction it's drawn as.
pub fn walk_anim_set(
    direction: UnitDirection,
    previous: UnitDirection,
    mut has: impl FnMut(UnitAnimSetKey) -> bool,
) -> (UnitAnimSetKey, UnitDirection) {
    let own = anim_set_for_direction(direction);
    if !direction.is_diagonal() || has(own) {
        return (own, direction);
    }
    let fallback = axis_fallback(direction, previous);
    (anim_set_for_direction(fallback), fallback)
}

// The idle anim set a unit that stops facing `direction` plays: its own directional one when it has
// it, else, for a diagonal direction, that of the axis direction its walk was drawn as (`drawn_as`,
// when known; else axis_fallback's default), else the generic idle.
pub fn idle_anim_set(
    direction: UnitDirection,
    drawn_as: Option<UnitDirection>,
    mut has: impl FnMut(UnitAnimSetKey) -> bool,
) -> UnitAnimSetKey {
    let own = idle_anim_set_for_direction(direction);
    if has(own) {
        return own;
    }
    if direction.is_diagonal() {
        let axis = drawn_as.filter(|drawn| !drawn.is_diagonal() && *drawn != UnitDirection::Idle);
        let fallback = idle_anim_set_for_direction(axis.unwrap_or_else(|| axis_fallback(direction, UnitDirection::Idle)));
        if has(fallback) {
            return fallback;
        }
    }
    UnitAnimSets::IDLE
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
            Self::Building { origin_base_cell, .. } => *origin_base_cell,
            Self::Tile { origin_cell, .. } => *origin_cell,
        }
    }

    pub fn destination_cell(&self) -> Cell {
        match self {
            Self::Building { destination_road_link, .. } => *destination_road_link,
            Self::Tile { destination_cell, .. } => *destination_cell,
        }
    }

    pub fn origin_debug_name(&self) -> &str {
        let (origin_cell, layer) = match self {
            Self::Building { origin_base_cell, .. } => (*origin_base_cell, TileMapLayerKind::Objects),
            Self::Tile { origin_cell, .. } => (*origin_cell, TileMapLayerKind::Terrain),
        };
        debug_utils::tile_name_at(origin_cell, layer)
    }

    pub fn destination_debug_name(&self) -> &str {
        let (destination_cell, layer) = match self {
            Self::Building { destination_base_cell, .. } => (*destination_base_cell, TileMapLayerKind::Objects),
            Self::Tile { destination_cell, .. } => (*destination_cell, TileMapLayerKind::Terrain),
        };
        debug_utils::tile_name_at(destination_cell, layer)
    }

    // Building Goal:

    pub fn building(
        origin_kind: BuildingKind,
        origin_base_cell: Cell,
        destination_kind: BuildingKind,
        destination_tile: BuildingTileInfo,
    ) -> Self {
        Self::Building {
            origin_kind,
            origin_base_cell,
            destination_kind,
            destination_base_cell: destination_tile.base_cell,
            destination_road_link: destination_tile.road_link,
        }
    }

    pub fn is_building(&self) -> bool {
        matches!(self, Self::Building { .. })
    }

    pub fn building_origin(&self) -> (BuildingKind, Cell) {
        match self {
            Self::Building { origin_kind, origin_base_cell, .. } => (*origin_kind, *origin_base_cell),
            _ => panic!("UnitNavGoal not a Building goal!"),
        }
    }

    pub fn building_destination(&self) -> (BuildingKind, Cell) {
        match self {
            Self::Building { destination_kind, destination_base_cell, .. } => (*destination_kind, *destination_base_cell),
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

pub fn is_goal_vacant_lot_tile(goal: &UnitNavGoal, context: &SimContext) -> bool {
    if goal.is_tile() {
        let maybe_tile = context.try_tile_from_layer(goal.tile_destination(), TileMapLayerKind::Terrain);
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
    goal: Option<UnitNavGoal>, // (origin_cell, destination_cell) may be different from path start/end.

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
    // `flies`: the unit's config's (the bird's): its path is a flight made for it, not a walk, so
    // what stands beside a step and the ground's heights don't stop it, and every step takes as long.
    pub fn update(&mut self, graph: &Graph, mut delta_time_secs: Seconds, flies: bool) -> UnitNavResult {
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

        // Blocked by what stands there now (or, across a corner, beside the step), or by a height edit
        // that left a cliff across the way. A flight only by a cell of no kind it crosses.
        let open = if flies {
            graph.node_kind(to).is_some_and(|kind| kind.intersects(self.traversable_node_kinds))
        } else {
            graph.can_take_step(from, to, self.traversable_node_kinds)
        };
        if !open {
            return UnitNavResult::PathBlocked;
        }

        // A diagonal takes √2 times as long and a ramp 1.5 times (Graph::step_time_scale; 1 for a
        // straight step on flat ground), so a walker or a ship moves at one speed every way.
        let time_scale = if flies { 1.0 } else { graph.step_time_scale(from, to) };
        self.progress += delta_time_secs / (self.segment_duration * time_scale);

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

    pub fn reset_path_and_goal(&mut self, new_path: Option<&Path>, optional_goal: Option<UnitNavGoal>) {
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

    // The path it follows (empty once it's reached the end), and the index of the cell it's on.
    #[inline]
    pub fn path(&self) -> &[Node] {
        &self.path
    }

    #[inline]
    pub fn path_index(&self) -> usize {
        self.path_index
    }

    // How many steps of its path go across a corner (docs/plans/unit-movement.md).
    pub fn diagonal_steps(&self) -> usize {
        self.path.windows(2).filter(|step| step[0].is_diagonal_to(step[1])).count()
    }

    // How many cells of its path are still ahead of it (the inspector's distance to go).
    #[inline]
    pub fn remaining_cells(&self) -> usize {
        self.path.len().saturating_sub(self.path_index + 1)
    }

    // The steps still ahead of it, each counted by how long it takes against a straight one on flat
    // ground (Graph::step_time_scale: √2 across a corner, 1.5 over a ramp), the one it's on in full:
    // the inspector's time to arrive, at its speed.
    pub fn remaining_steps(&self, graph: &Graph) -> f32 {
        let ahead = self.path.get(self.path_index..).unwrap_or_default();
        ahead.windows(2).map(|step| graph.step_time_scale(step[0], step[1])).sum()
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

#[cfg(test)]
mod tests {
    use UnitDirection::*;

    use super::*;

    const AXES: [UnitDirection; 4] = [NE, NW, SE, SW];
    const DIAGONALS: [UnitDirection; 4] = [N, S, E, W];

    // A unit's anim sets, by name, as a `has` lookup.
    fn has_sets(names: &'static [&'static str]) -> impl FnMut(UnitAnimSetKey) -> bool {
        move |key| names.contains(&key.string)
    }

    // Today's walkers: the four axis directions' walks and idles (and the generic idle).
    const FOUR_WAY: &[&str] =
        &["idle", "idle_ne", "idle_nw", "idle_se", "idle_sw", "walk_ne", "walk_nw", "walk_se", "walk_sw"];
    // A walker with art for all eight.
    const EIGHT_WAY: &[&str] = &[
        "idle", "idle_ne", "idle_nw", "idle_se", "idle_sw", "idle_n", "idle_s", "idle_e", "idle_w", "walk_ne", "walk_nw",
        "walk_se", "walk_sw", "walk_n", "walk_s", "walk_e", "walk_w",
    ];

    #[test]
    fn steps_map_to_eight_directions() {
        let origin = Cell::new(5, 5);
        let step = |dx, dy| direction_between(origin, Cell::new(5 + dx, 5 + dy));
        assert_eq!(step(1, 0), NE);
        assert_eq!(step(0, 1), NW);
        assert_eq!(step(0, -1), SE);
        assert_eq!(step(-1, 0), SW);
        assert_eq!(step(1, 1), N);
        assert_eq!(step(-1, -1), S);
        assert_eq!(step(1, -1), E);
        assert_eq!(step(-1, 1), W);
        assert_eq!(step(0, 0), Idle);
        // Longer: exactly diagonal is diagonal, anything else its longer axis.
        assert_eq!(step(3, 3), N);
        assert_eq!(step(3, 2), NE);
        assert_eq!(step(-1, -4), SE);
    }

    #[test]
    fn headings_stay_on_the_axes() {
        let origin = Cell::new(5, 5);
        let heading = |dx, dy| heading_between(origin, Cell::new(5 + dx, 5 + dy));
        // A tie goes to the y axis, as direction_between did before the diagonals.
        assert_eq!(heading(3, 3), NW);
        assert_eq!(heading(-2, -2), SE);
        assert_eq!(heading(2, -2), SE);
        assert_eq!(heading(-2, 2), NW);
        assert_eq!(heading(4, 1), NE);
        assert_eq!(heading(-4, 1), SW);
        assert_eq!(heading(0, 0), Idle);
    }

    #[test]
    fn screen_halves() {
        // (direction, north, south, east, west)
        let table = [
            (Idle, false, false, false, false),
            (NE, true, false, true, false),
            (NW, true, false, false, true),
            (SE, false, true, true, false),
            (SW, false, true, false, true),
            (N, true, false, false, false),
            (S, false, true, false, false),
            (E, false, false, true, false),
            (W, false, false, false, true),
        ];
        for (direction, north, south, east, west) in table {
            assert_eq!(direction.is_north(), north, "{direction} north");
            assert_eq!(direction.is_south(), south, "{direction} south");
            assert_eq!(direction.is_east(), east, "{direction} east");
            assert_eq!(direction.is_west(), west, "{direction} west");
            assert_eq!(direction.is_diagonal(), DIAGONALS.contains(&direction), "{direction} diagonal");
        }
        assert!(same_axis(N, NE) && same_axis(N, NW) && !same_axis(N, S));
        assert!(same_axis(E, SE) && !same_axis(E, W) && !same_axis(E, N));
    }

    #[test]
    fn diagonals_fall_back_to_the_side_they_came_from() {
        // (direction, previous, falls back to)
        let table = [
            (N, NE, NE),
            (N, E, NE),
            (N, SE, NE),
            (N, NW, NW),
            (N, W, NW),
            (N, SW, NW),
            (N, Idle, NE),
            (N, S, NE),
            (S, SE, SE),
            (S, W, SW),
            (S, Idle, SE),
            (E, NE, NE),
            (E, N, NE),
            (E, SW, SE),
            (E, Idle, SE),
            (W, NW, NW),
            (W, NE, NW),
            (W, S, SW),
            (W, Idle, SW),
        ];
        for (direction, previous, expected) in table {
            assert_eq!(axis_fallback(direction, previous), expected, "{direction} after {previous}");
        }
        for direction in AXES {
            assert_eq!(axis_fallback(direction, N), direction);
        }
    }

    #[test]
    fn walk_sets_for_four_and_eight_way_units() {
        for direction in AXES {
            let (key, drawn) = walk_anim_set(direction, Idle, has_sets(FOUR_WAY));
            assert_eq!(key.hash, anim_set_for_direction(direction).hash);
            assert_eq!(drawn, direction);
        }
        // Without art, a diagonal is drawn as its axis neighbour on the side it came from.
        let four = |direction, previous| walk_anim_set(direction, previous, has_sets(FOUR_WAY));
        assert_eq!(four(N, Idle).0.string, "walk_ne");
        assert_eq!(four(N, NW).0.string, "walk_nw");
        let (key, drawn) = four(S, SW);
        assert_eq!((key.string, drawn), ("walk_sw", SW));
        assert_eq!(four(E, NE).0.string, "walk_ne");
        assert_eq!(four(W, Idle).0.string, "walk_sw");
        // With art, its own.
        for direction in DIAGONALS {
            let (key, drawn) = walk_anim_set(direction, NE, has_sets(EIGHT_WAY));
            assert_eq!(key.hash, anim_set_for_direction(direction).hash);
            assert_eq!(drawn, direction);
        }
        assert_eq!(walk_anim_set(E, Idle, has_sets(EIGHT_WAY)).0.string, "walk_e");
    }

    #[test]
    fn idle_sets_for_four_and_eight_way_units() {
        for direction in AXES {
            assert_eq!(idle_anim_set(direction, None, has_sets(FOUR_WAY)).hash, idle_anim_set_for_direction(direction).hash);
        }
        // A diagonal stands as its walk was drawn, else its default side.
        assert_eq!(idle_anim_set(N, Some(NW), has_sets(FOUR_WAY)).string, "idle_nw");
        assert_eq!(idle_anim_set(N, None, has_sets(FOUR_WAY)).string, "idle_ne");
        assert_eq!(idle_anim_set(W, Some(Idle), has_sets(FOUR_WAY)).string, "idle_sw");
        assert_eq!(idle_anim_set(E, Some(N), has_sets(FOUR_WAY)).string, "idle_se");
        // An eight-way unit stands facing its own way.
        for direction in DIAGONALS {
            assert_eq!(
                idle_anim_set(direction, Some(NE), has_sets(EIGHT_WAY)).hash,
                idle_anim_set_for_direction(direction).hash
            );
        }
        // A unit with no directional idle (the bird) takes the generic one, as before.
        assert_eq!(idle_anim_set(NE, None, has_sets(&["walk_se"])).string, "idle");
        assert_eq!(idle_anim_set(N, Some(SE), has_sets(&["walk_se"])).string, "idle");
    }
}
