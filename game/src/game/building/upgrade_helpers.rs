use heapless;
use num_enum::TryFromPrimitive;
use small_map;
use smallvec::SmallVec;
use strum::IntoEnumIterator;

use crate::{
    log,
    imgui_ui::UiSystem,
    game::sim::world::BuildingId,
    pathfind::{Node, NodeKind as PathNodeKind},
    utils::{Size, coords::{Cell, CellRange}},
    tile::{
        TileFlags,
        TileKind,
        TileMapLayerKind,
        sets::TileDef
    }
};

use super::{
    Building,
    BuildingKind,
    BuildingBehavior,
    BuildingContext,
    house::{HouseLevel, HouseLevelConfig}
};

// ----------------------------------------------
// Public API
// ----------------------------------------------

// Can the house expand one cell & upgrade one level? E.g.: 1x1 -> 2x2.
pub fn can_expand_house(context: &BuildingContext,
                        house_id: BuildingId,
                        current_level: HouseLevel,
                        target_level: HouseLevel,
                        current_cell_range: CellRange) -> bool {

    if current_level.is_max() {
        return false;
    }

    // We can only advance one level at a time (expanding by one tile in each dimension).
    debug_assert!(target_level == current_level.next());

    let best_result =
        find_best_expanded_rect_and_candidates(context, house_id, current_cell_range);

    if best_result.is_none() || find_tile_def_for_level(context, target_level).is_none() {
        return false; // Not possible to expand.
    }

    true
}

// Attempts to expand the house by *one cell* in each dimension, e.g.: 1x1 -> 2x2.
pub fn try_expand_house(context: &BuildingContext,
                        house_id: BuildingId,
                        current_level: HouseLevel,
                        target_level: HouseLevel,
                        current_cell_range: CellRange) -> bool {

    if current_level.is_max() {
        return false;
    }

    // We can only advance one level at a time (expanding by one tile in each dimension).
    debug_assert!(target_level == current_level.next());

    let best_result =
        find_best_expanded_rect_and_candidates(context, house_id, current_cell_range);

    let (target_rect, merge_ids) = match best_result {
        Some(best_result) => best_result,
        None => return false,
    };

    let mut house_ids_to_merge = SmallVec::<[BuildingId; 32]>::new();
    for (id, _) in merge_ids.iter() {
        if *id != house_id {
            house_ids_to_merge.push(*id);
        }
    }

    let start_cell = Cell::new(target_rect.min_x, target_rect.min_y);
    let target_level_config = context.query.building_configs().find_house_level_config(target_level);

    let target_tile_def = match context.find_tile_def(target_level_config.tile_def_name_hash) {
        Some(tile_def) => tile_def,
        None => {
            log::error!(log::channel!("house"), "Missing TileDef for house level {:?}: {}",
                        target_level, target_level_config.tile_def_name);
            return false;
        },
    };

    merge_and_replace_tile(
        context,
        target_level_config,
        target_tile_def,
        start_cell,
        current_cell_range.size(),
        house_id,
        &house_ids_to_merge);

    true
}

// ----------------------------------------------
// Internal
// ----------------------------------------------

fn find_best_expanded_rect_and_candidates(context: &BuildingContext,
                                          house_id: BuildingId,
                                          current_cell_range: CellRange)
                                          -> Option<(CellRect, HouseIdsSet)> {

    debug_assert!(house_id.is_valid());
    debug_assert!(current_cell_range.is_valid());

    let mut best_score = -1;
    let mut best_result: Option<(CellRect, HouseIdsSet)> = None;

    for target_rect in candidate_target_rect(current_cell_range) {
        if let Some((score, merge_ids)) = evaluate_target_rect(context, house_id, current_cell_range, target_rect) {
            if score > best_score {
                best_score  = score;
                best_result = Some((target_rect, merge_ids));
            }
        }
    }

    // We should have expanded by one cell in each dimension exactly.
    debug_assert!(best_result.as_ref().is_none_or(|(target_rect, _)| target_rect.size() == current_cell_range.size() + 1));
    best_result
}

fn evaluate_target_rect(context: &BuildingContext,
                        house_id: BuildingId,
                        current_cell_range: CellRange,
                        target_rect: CellRect) -> Option<(i32, HouseIdsSet)> {

    debug_assert!(house_id.is_valid());
    debug_assert!(current_cell_range.is_valid());
    debug_assert!(target_rect.is_valid());

    let current_size = current_cell_range.size();

    let mut visited  = HouseIdsSet::new();
    let mut to_merge = HouseIdsSet::new();

    // BFS restricted to expanded target rect:
    for cell in target_rect.iter_cells() {
        if let Some(neighbor_house) = find_house_for_cell(context, cell) {
            if valid_merge_sizes(current_size, neighbor_house) {
                let search_start_id = neighbor_house.id();
                if !visited.contains(search_start_id) {
                    collect_merge_candidates(
                        context,
                        search_start_id,
                        current_size,
                        target_rect,
                        &mut visited,
                        &mut to_merge);
                }
            } else {
                return None;
            }
        } else if !can_expand_into_cell(context, cell) {
            // Cell occupied by another kind of building or not expandable.
            // We cannot use this target rect to expand.
            return None;
        }
    }

    // Always include the expanding house.
    to_merge.insert(house_id);

    // Score = how many house tiles in the expanded target rect are filled by merging.
    let score = to_merge.len() as i32;
    Some((score, to_merge))
}

fn valid_merge_sizes(current_size: Size, neighbor_house: &Building) -> bool {
    // We can only merge with neighbor houses that are of the same size or smaller.
    // E.g.:
    //  1x1 can only merge with 1x1
    //  2x2 can merge with 1x1 & 2x2
    //  3x3 can merge with 1x1, 2x2 & 3x3
    //  etc...
    let neighbor_size = neighbor_house.cell_range().size();
    neighbor_size <= current_size
}

fn can_expand_into_cell(context: &BuildingContext, cell: Cell) -> bool {
    let graph = context.query.graph();

    let node_kind = match graph.node_kind(Node::new(cell)) {
        Some(node_kind) => node_kind,
        None => return false,
    };

    if !node_kind.intersects(PathNodeKind::Dirt | PathNodeKind::VacantLot | PathNodeKind::Building) {
        return false; // Not an expandable node.
    }

    if node_kind.intersects(PathNodeKind::Building) && find_house_for_cell(context, cell).is_none() {
        return false; // Not a house building.
    }

    debug_assert!(!node_kind.intersects(
                  PathNodeKind::Road | PathNodeKind::Water |
                  PathNodeKind::BuildingRoadLink | PathNodeKind::SettlersSpawnPoint),
                  "Mixing incompatible path node kinds!");
    true
}

struct MergeCandidateQueue(heapless::Deque<BuildingId, 32>);

impl MergeCandidateQueue {
    #[inline]
    fn new() -> Self {
        Self(heapless::Deque::new())
    }

    #[inline]
    fn push_back(&mut self, id: BuildingId) {
        self.0.push_back(id)
            .expect("BFS candidates queue overflow! Increase max size.");
    }

    #[inline]
    fn pop_front(&mut self) -> Option<BuildingId> {
        self.0.pop_front()
    }
}

// Breadth First Search (BFS) for possible merge candidate houses.
fn collect_merge_candidates(context: &BuildingContext,
                            search_start_id: BuildingId,
                            current_size: Size,
                            limit_rect: CellRect,
                            visited: &mut HouseIdsSet,
                            to_merge: &mut HouseIdsSet) {

    debug_assert!(search_start_id.is_valid());
    debug_assert!(current_size.is_valid());
    debug_assert!(limit_rect.is_valid());

    let mut queue = MergeCandidateQueue::new();
    queue.push_back(search_start_id);
    visited.insert(search_start_id);

    while let Some(id) = queue.pop_front() {
        to_merge.insert(id);

        let candidate_house = house_for_id(context, id);

        for cell in &candidate_house.cell_range() {
            if !limit_rect.contains(cell) {
                continue; // Respect target boundary.
            }

            // Explore 4-neighbors (N/E/S/W) for adjacency:
            for neighbor_cell in [
                Cell::new(cell.x + 1, cell.y),
                Cell::new(cell.x - 1, cell.y),
                Cell::new(cell.x, cell.y + 1),
                Cell::new(cell.x, cell.y - 1),
            ] {
                if !limit_rect.contains(neighbor_cell) {
                    continue;
                }

                if let Some(neighbor_house) = find_house_for_cell(context, neighbor_cell) {
                    if valid_merge_sizes(current_size, neighbor_house) {
                        let neighbor_id = neighbor_house.id();
                        if !visited.contains(neighbor_id) {
                            visited.insert(neighbor_id);
                            queue.push_back(neighbor_id);
                        }
                    }
                }
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct CellRect {
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
}

impl CellRect {
    #[inline] fn width(&self)  -> i32 { self.max_x - self.min_x + 1 }
    #[inline] fn height(&self) -> i32 { self.max_y - self.min_y + 1 }

    #[inline]
    fn size(&self) -> Size {
        Size::new(self.width(), self.height())
    }

    #[inline]
    fn is_valid(&self) -> bool {
        self.min_x > 0 && self.min_y > 0 &&
        self.max_x > 0 && self.max_y > 0
    }

    #[inline]
    fn contains(&self, cell: Cell) -> bool {
        if cell.x < self.min_x || cell.y < self.min_y {
            return false;
        }
        if cell.x > self.max_x || cell.y > self.max_y {
            return false;
        }
        true
    }

    #[inline]
    fn iter_cells(&self) -> impl Iterator<Item = Cell> {
        (self.min_x..=self.max_x)
            .flat_map(move |x| (self.min_y..=self.max_y).map(move |y| Cell::new(x, y)))
    }
}

const CANDIDATE_RECTS_COUNT: usize = 4;

fn candidate_target_rect(current_cell_range: CellRange) -> [CellRect; CANDIDATE_RECTS_COUNT] {
    let rect = CellRect {
        min_x: current_cell_range.start.x,
        min_y: current_cell_range.start.y,
        max_x: current_cell_range.end.x,
        max_y: current_cell_range.end.y,
    };

    let size = rect.size();   // current size (N)
    let next_size = size + 1; // desired size (N+1)

    [
        // Anchor top-left
        CellRect {
            min_x: rect.min_x,
            min_y: rect.min_y,
            max_x: rect.min_x + next_size.width  - 1,
            max_y: rect.min_y + next_size.height - 1,
        },
        // Anchor top-right
        CellRect {
            min_x: rect.max_x - (next_size.width - 1),
            min_y: rect.min_y,
            max_x: rect.max_x,
            max_y: rect.min_y + next_size.height - 1,
        },
        // Anchor bottom-left
        CellRect {
            min_x: rect.min_x,
            min_y: rect.max_y - (next_size.width - 1),
            max_x: rect.min_x + next_size.height - 1,
            max_y: rect.max_y,
        },
        // Anchor bottom-right
        CellRect {
            min_x: rect.max_x - (next_size.width  - 1),
            min_y: rect.max_y - (next_size.height - 1),
            max_x: rect.max_x,
            max_y: rect.max_y,
        },
    ]
}

fn merge_and_replace_tile<'tile_sets>(context: &BuildingContext<'_, 'tile_sets, '_>,
                                      target_level_config: &HouseLevelConfig,
                                      target_tile_def: &'tile_sets TileDef,
                                      start_cell: Cell,
                                      current_size: Size,
                                      dest_id: BuildingId,
                                      ids_to_merge: &[BuildingId]) {

    let new_cell_range = target_tile_def.cell_range(start_cell);
    debug_assert!(new_cell_range.size() == current_size + 1); // Expand by one cell in each dimension.

    if !ids_to_merge.is_empty() {
        merge_houses(context, target_level_config, dest_id, ids_to_merge);
    }
    // Else this house is expanding into vacant lots / empty terrain. Nothing to merge.

    replace_tile(context, dest_id, target_tile_def, new_cell_range);
}

// Replaces house tile with a new (possibly bigger) tile.
// Assumes there is enough room to place the new tile
// (neighboring houses already merged and cleared).
fn replace_tile<'tile_sets>(context: &BuildingContext<'_, 'tile_sets, '_>,
                            dest_id: BuildingId,
                            target_tile_def: &'tile_sets TileDef,
                            new_cell_range: CellRange) {

    debug_assert!(dest_id.is_valid());
    debug_assert!(target_tile_def.is_valid());
    debug_assert!(new_cell_range.is_valid());

    let dest_house = house_for_id_mut(context, dest_id);
    let tile_map = context.query.tile_map();

    // We'll have to restore the game_state on the new tile.
    let (prev_game_state, prev_cell_range) = {
        let prev_tile = tile_map.find_tile_mut(dest_house.base_cell(), TileMapLayerKind::Objects, TileKind::Building)
            .expect("House building should have an associated Tile in the TileMap!");

        let game_state = prev_tile.game_state_handle();
        let cell_range = prev_tile.cell_range();

        debug_assert!(game_state.is_valid(), "House tile doesn't have a valid associated TileGameStateHandle!");
        debug_assert!(dest_house.kind() == BuildingKind::from_game_state_handle(game_state));
        debug_assert!(dest_house.id().index() == game_state.index());

        (game_state, cell_range)
    };

    // Clear the previous tile:
    tile_map.try_clear_tile_from_layer(prev_cell_range.start, TileMapLayerKind::Objects)
        .expect("Failed to clear previous house tile! This is unexpected...");

    // And place the new one:
    let new_tile = tile_map.try_place_tile_in_layer(
        new_cell_range.start,
        TileMapLayerKind::Objects,
        target_tile_def)
        .expect("Failed to place new house tile! This is unexpected...");

    debug_assert!(new_tile.cell_range() == new_cell_range);

    // Update game state handle:
    new_tile.set_game_state_handle(prev_game_state);

    if new_cell_range != prev_cell_range {
        // Update cell range cached in the building & context.
        dest_house.map_cells = new_cell_range;
        *context.map_cells.as_mut() = new_cell_range;

        // Update path finding graph:
        let graph = context.query.graph();
        for cell in &prev_cell_range {
            graph.set_node_kind(Node::new(cell), PathNodeKind::Dirt); // Traversable
        }
        for cell in &new_cell_range {
            graph.set_node_kind(Node::new(cell), PathNodeKind::Building); // Not Traversable
        }  
    }
}

// Merges `ids_to_merge` houses into `dest_id` house and destroys all
// `ids_to_merge` houses. Ids are assumed to be all valid house buildings.
fn merge_houses(context: &BuildingContext,
                target_level_config: &HouseLevelConfig,
                dest_id: BuildingId,
                ids_to_merge: &[BuildingId]) {

    debug_assert!(dest_id.is_valid());
    debug_assert!(!ids_to_merge.is_empty());

    let dest_building = house_for_id_mut(context, dest_id);

    for merge_id in ids_to_merge {
        debug_assert!(*merge_id != dest_id);

        let building_to_merge = house_for_id_mut(context, *merge_id);

        merge_house(context, target_level_config, dest_building, building_to_merge);
        destroy_house(context, building_to_merge);
    }
}

// Merge resources, population and workers.
fn merge_house(context: &BuildingContext,
               target_level_config: &HouseLevelConfig,
               dest_building: &mut Building,
               building_to_merge: &Building) {

    debug_assert!(!std::ptr::eq(dest_building, building_to_merge));

    let dest_building_kind_and_id = dest_building.kind_and_id();
    let building_to_merge_kind_and_id = building_to_merge.kind_and_id();

    let dest_house = dest_building.as_house_mut();
    let house_to_merge = building_to_merge.as_house();

    dest_house.stock_mut().update_capacities(target_level_config.stock_capacity);
    dest_house.stock_mut().merge(house_to_merge.stock_ref());

    dest_house.population_mut().set_max(target_level_config.max_population);
    dest_house.population_mut().merge(house_to_merge.population_ref());

    dest_house.adjust_workers_available(context);
    dest_house.worker_pool_mut().merge(house_to_merge.worker_pool_ref());

    // Employers of `house_to_merge` workers must now point to `dest_house`.
    dest_house.worker_pool_mut()
        .for_each_employer(context.query.world(), |employer, employed_count| {
            let prev_popups = employer.archetype.debug_options().set_show_popups(false);

            let removed_count = employer.remove_workers(employed_count, building_to_merge_kind_and_id);
            debug_assert!(removed_count == employed_count);

            let added_count = employer.add_workers(employed_count, dest_building_kind_and_id);
            debug_assert!(added_count == employed_count);

            employer.archetype.debug_options().set_show_popups(prev_popups);
            true
        });
}

fn destroy_house<'config>(context: &BuildingContext<'config, '_, '_>,
                          merged_building: &mut Building<'config>) {

    let world = context.query.world();
    let query = context.query;

    // First reset all population/workers so we won't try to evict any residents
    // or notify employers when the building is destroyed.
    let merged_house = merged_building.as_house_mut();
    merged_house.stock_mut().clear();
    merged_house.population_mut().clear();
    merged_house.worker_pool_mut().clear();

    if let Err(err) = world.despawn_building(query, merged_building) {
        log::error!(log::channel!("house"), "Failed to despawn merged house building '{}', cell:{}, id:{} => {err}",
                    merged_building.name(), merged_building.base_cell(), merged_building.id());
    }
}

// ----------------------------------------------
// Utilities
// ----------------------------------------------

// SmallMap starts with a fixed-size buffer but can expand into the heap.
// This allows us to mostly stay on the stack and avoid any allocations.
// We only care about the key being present or not, so value is an empty type.
struct HouseIdsSet(small_map::SmallMap<32, BuildingId, ()>);

impl HouseIdsSet {
    #[inline]
    fn new() -> Self {
        Self(small_map::SmallMap::new())
    }

    #[inline]
    fn contains(&self, id: BuildingId) -> bool {
        self.0.get(&id).is_some()
    }

    #[inline]
    fn insert(&mut self, id: BuildingId) {
        self.0.insert(id, ());
    }

    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    fn iter(&self) -> small_map::Iter<'_, 32, BuildingId, ()> {
        self.0.iter()
    }
}

fn house_for_id<'config>(context: &'config BuildingContext<'config, '_, '_>,
                         id: BuildingId) -> &'config Building<'config> {

    let world = context.query.world();

    world.find_building(BuildingKind::House, id)
        .expect("Invalid Building id! Expected to have a valid House Building.")
}

fn house_for_id_mut<'config>(context: &'config BuildingContext<'config, '_, '_>,
                             id: BuildingId) -> &'config mut Building<'config> {

    let world = context.query.world();

    world.find_building_mut(BuildingKind::House, id)
        .expect("Invalid Building id! Expected to have a valid House Building.")
}

fn find_house_for_cell<'config>(context: &'config BuildingContext<'config, '_, '_>,
                                cell: Cell) -> Option<&'config Building<'config>> {

    let world = context.query.world();
    let tile_map = context.query.tile_map();

    if let Some(building) = world.find_building_for_cell(cell, tile_map) {
        if building.is(BuildingKind::House) {
            return Some(building);
        }
    }

    None
}

fn find_tile_def_for_level<'tile_sets>(context: &BuildingContext<'_, 'tile_sets, '_>,
                                       level: HouseLevel) -> Option<&'tile_sets TileDef> {

    let level_config = context.query.building_configs().find_house_level_config(level);
    context.find_tile_def(level_config.tile_def_name_hash)
}

// ----------------------------------------------
// Debug UI
// ----------------------------------------------

pub fn draw_debug_ui(context: &BuildingContext, ui_sys: &UiSystem) {
    let ui = ui_sys.builder();
    if !ui.collapsing_header("Merge Debug", imgui::TreeNodeFlags::empty()) {
        return; // collapsed.
    }

    #[allow(static_mut_refs)]
    let (current_level, level_idx_mut_ref) = unsafe {
        let mut house_level_names: SmallVec<[String; HouseLevel::count()]> = SmallVec::new();
        for level in HouseLevel::iter() {
            house_level_names.push(level.to_string());
        }

        static mut LEVEL_IDX: usize = 0;
        ui.combo_simple_string("Level", &mut LEVEL_IDX, &house_level_names);

        (HouseLevel::try_from_primitive(LEVEL_IDX.try_into().unwrap()).unwrap(), &mut LEVEL_IDX)
    };

    if ui.button("Test Expand To Next Level") {
        let house_id = context.id;
        let house = house_for_id_mut(context, house_id).as_house_mut();

        if !house.level().is_max() && !current_level.is_max() {
            let target_level = current_level.next();

            let current_tile_def = find_tile_def_for_level(context, current_level);
            let target_tile_def  = find_tile_def_for_level(context, target_level);

            if let (Some(current_tile_def), Some(target_tile_def)) = (current_tile_def, target_tile_def) {
                log::info!("Expanding house from {current_level:?} to {target_level:?} ...");

                if target_tile_def.size_in_cells() == current_tile_def.size_in_cells() + 1 {
                    let current_cell_range = context.cell_range();

                    if target_tile_def.size_in_cells() == current_cell_range.size() + 1 &&
                       try_expand_house(context, house_id, current_level, target_level, current_cell_range) {
                        *level_idx_mut_ref += 1;
                    } else {
                        log::error!("Failed to expand house '{}' from {current_level:?} to {target_level:?}.", house.name());
                    }
                } else {
                    // One cell increase per level assumed.
                    if target_tile_def.size_in_cells() > current_tile_def.size_in_cells() + 1 {
                        log::error!("House level {target_level:?} increases house size by more than one cell: {} vs {}",
                                    current_tile_def.size_in_cells(), target_tile_def.size_in_cells());
                    } else {
                        log::error!("House level {target_level:?} does not increase house size by one cell: {} vs {}",
                                    current_tile_def.size_in_cells(), target_tile_def.size_in_cells());
                    }
                }
            } else {
                log::error!("Missing TileDefs for house levels {current_level:?} and {target_level:?}.");
            }
        } else {
            log::info!("House level already maxed!");
        }
    }

    ui.separator();

    #[allow(static_mut_refs)]
    let (mark_start_cell, rect_idx) = unsafe {
        static mut MARK_START_CELL: bool = false;
        static mut RECT_IDX: usize = 0;

        ui.checkbox("Mark Start Cell", &mut MARK_START_CELL);
        if ui.input_scalar("Candidate Rect", &mut RECT_IDX).step(1).build() {
            RECT_IDX = RECT_IDX.min(CANDIDATE_RECTS_COUNT - 1);
        }

        (MARK_START_CELL, RECT_IDX)
    };

    if ui.button("Visualize Candidate Cells") {
        let rects = candidate_target_rect(context.cell_range());
        let target_rect = rects[rect_idx];
        highlight_merge_rect(context, target_rect, mark_start_cell);
    }
}

fn highlight_merge_rect(context: &BuildingContext, target_rect: CellRect, mark_start_cell: bool) {
    let tile_map = context.query.tile_map();

    for cell in target_rect.iter_cells() {
        if let Some(tile) = tile_map.try_tile_from_layer_mut(cell, TileMapLayerKind::Terrain) {
            tile.set_flags(TileFlags::Highlighted, true);
        }

        if let Some(tile) = tile_map.find_tile_mut(cell, TileMapLayerKind::Objects, TileKind::Building) {
            tile.set_flags(TileFlags::Invalidated, true);
        }
    }

    if mark_start_cell {
        let start_cell = Cell::new(target_rect.min_x, target_rect.min_y);

        if let Some(tile) = tile_map.try_tile_from_layer_mut(start_cell, TileMapLayerKind::Terrain) {
            tile.set_flags(TileFlags::DrawDebugBounds, true);
        }

        if let Some(tile) = tile_map.find_tile_mut(start_cell, TileMapLayerKind::Objects, TileKind::Building) {
            tile.set_flags(TileFlags::DrawDebugBounds, true);
        }
    }
}
