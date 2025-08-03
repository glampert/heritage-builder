use smallvec::SmallVec;
use strum_macros::Display;
use proc_macros::DrawDebugUi;

use crate::{
    game_object_debug_options,
    pathfind::{SearchResult, Path, NodeKind as PathNodeKind},
    debug::{self},
    imgui_ui::{
        self,
        UiSystem,
        DPadDirection
    },
    tile::{
        map::{self, Tile, TileMap, TileMapLayerKind},
        sets::{TileDef, TileKind},
    },
    utils::{
        self,
        Color,
        Seconds,
        coords::{
            Cell,
            CellRange,
            WorldToScreenTransform
        },
        hash::{
            StrHashPair,
            StringHash,
            PreHashedKeyMap
        }
    }
};

use super::{
    sim::{
        Query,
        world::GenerationalIndex,
        resources::{ResourceKind, StockItem}
    },
    building::{
        Building,
        BuildingKind,
        BuildingTileInfo
    }
};

pub mod config;
use config::UnitConfig;

// ----------------------------------------------
// Helper Macros
// ----------------------------------------------

macro_rules! find_unit_tile {
    (&$unit:ident, $query:ident) => {
        $query.find_tile($unit.map_cell, TileMapLayerKind::Objects, TileKind::Unit)
            .expect("Unit should have an associated Tile in the TileMap!")
    };
    (&mut $unit:ident, $query:ident) => {
        $query.find_tile_mut($unit.map_cell, TileMapLayerKind::Objects, TileKind::Unit)
            .expect("Unit should have an associated Tile in the TileMap!")
    };
}

// ----------------------------------------------
// UnitDebug
// ----------------------------------------------

game_object_debug_options! {
    UnitDebug,
}

// ----------------------------------------------
// Unit  
// ----------------------------------------------

/*
Common Unit Behavior:
 - Spawn and despawn dynamically.
 - Moves across the tile map, so map cell can change.
 - Transports resources from A to B (has a start point and a destination).
 - Patrols an area around its building to provide a service to households.
 - Most units will only walk on paved roads. Some units may go off-road.
 - Has an inventory that can cary a single ResourceKind at a time, any amount.
*/
#[derive(Clone, Default)]
pub struct Unit<'config> {
    config: Option<&'config UnitConfig>,
    map_cell: Cell,
    id: GenerationalIndex,
    direction: UnitDirection,
    anim_sets: UnitAnimSets,
    inventory: UnitInventory,
    navigation: UnitNavigation,
    next_task: UnitInternalTask,
    debug: UnitDebug,
}

impl<'config> Unit<'config> {
    // ----------------------
    // Spawning / Despawning:
    // ----------------------

    pub fn new(tile: &mut Tile, config: &'config UnitConfig, id: GenerationalIndex) -> Self {
        let mut unit = Unit::default();
        unit.spawned(tile, config, id);
        unit
    }

    pub fn spawned(&mut self, tile: &mut Tile, config: &'config UnitConfig, id: GenerationalIndex) {
        debug_assert!(!self.is_spawned());
        debug_assert!(tile.is_valid());
        debug_assert!(id.is_valid());

        self.config    = Some(config);
        self.map_cell  = tile.base_cell();
        self.id        = id;
        self.direction = UnitDirection::Idle;

        self.anim_sets.set_anim(tile, UnitAnimSets::IDLE);
    }

    pub fn despawned(&mut self) {
        debug_assert!(self.is_spawned());
        debug_assert!(self.inventory.is_empty()); // Should be empty, otherwise we might be losing resources!

        self.config     = None;
        self.map_cell   = Cell::default();
        self.id         = GenerationalIndex::default();
        self.direction  = UnitDirection::default();
        self.next_task  = UnitInternalTask::default();

        self.anim_sets.reset();
        self.navigation.reset(None, None);
        self.debug.clear_popups();
    }

    #[inline]
    pub fn is_spawned(&self) -> bool {
        self.id.is_valid()
    }

    #[inline]
    pub fn id(&self) -> GenerationalIndex {
        self.id
    }

    // ----------------------
    // Utilities:
    // ----------------------

    #[inline]
    pub fn name(&self) -> &str {
        debug_assert!(self.is_spawned());
        &self.config.unwrap().name
    }

    #[inline]
    pub fn cell(&self) -> Cell {
        debug_assert!(self.is_spawned());
        self.map_cell
    }

    // Teleports to new tile cell and updates direction and animation.
    pub fn teleport(&mut self, tile_map: &mut TileMap, destination_cell: Cell) -> bool {
        debug_assert!(self.is_spawned());
        if tile_map.try_move_tile(self.map_cell, destination_cell, TileMapLayerKind::Objects) {
            let tile = tile_map.find_tile_mut(
                destination_cell,
                TileMapLayerKind::Objects,
                TileKind::Unit)
                .unwrap();

            let new_direction = direction_between(self.map_cell, destination_cell);    
            self.update_direction_and_anim(tile, new_direction);

            debug_assert!(destination_cell == tile.base_cell());
            self.map_cell = destination_cell;
            return true;
        }
        false
    }

    // ----------------------
    // Path Navigation:
    // ----------------------

    #[inline]
    pub fn follow_path(&mut self, path: Option<&Path>) {
        debug_assert!(self.is_spawned());
        self.navigation.reset(path, None);
        if path.is_some() {
            self.debug.popup_msg("New Goal");
        }
    }

    #[inline]
    pub fn go_to_building(&mut self, path: &Path, origin: Cell, destination: Cell) {
        debug_assert!(self.is_spawned());
        self.navigation.reset(Some(path), Some((origin, destination)));
        self.log_going_to(origin, destination);
    }

    pub fn update_navigation(&mut self, query: &Query, delta_time_secs: Seconds) {
        debug_assert!(self.is_spawned());

        // Path following and movement:
        match self.navigation.update(delta_time_secs) {
            UnitNavResult::Idle => {
                // Nothing.
            },
            UnitNavResult::Moving(from_cell, to_cell, progress, direction) => {
                let tile = find_unit_tile!(&mut self, query);

                let draw_size = tile.draw_size();
                let from_iso = map::calc_unit_iso_coords(from_cell, draw_size);
                let to_iso = map::calc_unit_iso_coords(to_cell, draw_size);

                let new_iso_coords = utils::lerp(from_iso, to_iso, progress);
                tile.set_iso_coords_f32(new_iso_coords);

                self.update_direction_and_anim(tile, direction);
            },
            UnitNavResult::AdvancedCell(cell, direction) => {
                let did_teleport = self.teleport(query.tile_map(), cell);
                debug_assert!(did_teleport, "Failed to advance unit tile cell!");

                let tile = find_unit_tile!(&mut self, query);
                debug_assert!(self.map_cell == cell && tile.base_cell() == cell);

                self.update_direction_and_anim(tile, direction);
            },
            UnitNavResult::ReachedGoal(cell, direction) => {
                let tile = find_unit_tile!(&mut self, query);

                debug_assert!(self.direction == direction);
                debug_assert!(self.map_cell == cell && tile.base_cell() == cell);

                self.debug.popup_msg("Reached Goal");

                // Clear current path.
                self.follow_path(None);

                // Go idle.
                self.update_direction_and_anim(tile, UnitDirection::Idle);
            }
        }
    }

    // ----------------------
    // Unit Behavior / Tasks:
    // ----------------------

    pub fn update(&mut self, query: &Query, _delta_time_secs: Seconds) {
        debug_assert!(self.is_spawned());

        // FIXME: borrow issues...
        let current_task = self.next_task.clone();

        match &current_task {
            UnitInternalTask::Retry { task } => {
                match task.as_ref() {
                    UnitInternalTask::DeliverToStorage {
                        unit_starting_cell,
                        storage_buildings_accepted,
                        resource_kind_to_deliver,
                        ..
                    } => {
                        match find_storage(query, *unit_starting_cell, *storage_buildings_accepted, *resource_kind_to_deliver) {
                            Some((destination, path)) => {

                                // TODO: unit_starting_cell should be same as self.cell(), we also don't use resource_count.
                                // should push a different enum for retry (e.g. RetryDeliverToStorage).
                                self.go_to_building(path, *unit_starting_cell, destination.base_cell);

                                self.assign_next_task(UnitInternalTask::VisitBuilding { root_task: Box::new(*task.clone()), destination });
                            },
                            None => {
                                // Stay in the Retry state.
                            },
                        }
                    },
                    _ => panic!("Invalid retry task!"),
                }
            },
            UnitInternalTask::VisitBuilding { root_task, destination } => {
                if self.has_reached_building(destination) {
                    let mut completed_task = false;

                    let world = query.world();
                    let tile_map = query.tile_map();

                    if let Some(building) = world.find_building_for_cell_mut(destination.base_cell, tile_map) {
                        // NOTE: No need to check for a generation match here. If the destination building
                        // is still the same kind of building we where looking for, it doesn't matter if it
                        // was destroyed and recreated since we started the task.
                        if building.kind() == destination.kind {
                            building.visited_by(self, query);
    
                            // If we've delivered our goods, we're done.
                            // Otherwise we were not able to offload everything, so reroute.
                            if self.is_inventory_empty() {

                                // TODO: this termination condition actually depends on what the root_task was!
                                completed_task = true;
                            }
                        }
                    }

                    if completed_task {
                        match root_task.as_ref() {
                            UnitInternalTask::DeliverToStorage {
                                origin_building_kind,
                                origin_building_id,
                                origin_building_base_cell,
                                completion_callback,
                                ..
                            } => {
                                // Notify source building of task completion.
                                if let Some(on_completion) = completion_callback {
                                    if let Some(building) = world.find_building_for_cell_mut(*origin_building_base_cell, tile_map) {
                                        debug_assert!(building.id().is_valid());
                                        debug_assert!(origin_building_id.is_valid());
                                        // NOTE: Only invoke the completion callback if the original base cell still contains the
                                        // exact same building that initiated this task. We don't want to accidentally invoke the
                                        // callback on a different building, even if the type of building there is the same.
                                        if building.kind() == *origin_building_kind &&
                                           building.id() == *origin_building_id {
                                            on_completion(self, building);
                                        }
                                    }
                                }
                            },
                            _ => {},
                        }

                        // TODO Make despawn the completion followup task instead.
                        query.despawn_unit(self);
                    } else {
                        let retry_task = match root_task.as_ref() {
                            UnitInternalTask::DeliverToStorage {
                                origin_building_kind,
                                origin_building_id,
                                origin_building_base_cell,
                                storage_buildings_accepted,
                                resource_kind_to_deliver,
                                completion_callback,
                                ..
                            } => {
                                debug_assert!(!self.is_inventory_empty());
                                Box::new(UnitInternalTask::DeliverToStorage {
                                    origin_building_kind: *origin_building_kind,
                                    origin_building_id: *origin_building_id,
                                    origin_building_base_cell: *origin_building_base_cell,
                                    unit_starting_cell: self.cell(),
                                    storage_buildings_accepted: *storage_buildings_accepted,
                                    resource_kind_to_deliver: *resource_kind_to_deliver,
                                    resource_count: self.inventory.count(),
                                    completion_callback: *completion_callback
                                })
                            },
                            _ => panic!("Invalid root task!"),
                        };
                        self.assign_next_task(UnitInternalTask::Retry { task: retry_task });
                    }
                }
            },
            _ => {},
        }

        // DeliverToStorage -> VisitBuilding -> Despawn | Retry
        // NOTE: Retry with a possibly decremented resource_count

        // TODO
        // Unit behavior should be here:
        // - If it has a goal to deliver goods to a building, go to the building and deliver.
        //   - If the building we are delivering to is a storage building:
        //     - If it can accept all our goods, unload them and despawn.
        //     - Else, move to another storage building and try again.
        //     - If no building can be found that will accept our goods, stop and wait, retry next update.
        //   - Else if we are delivering directly to a service or producer:
        //     - If it cannot accept our goods, try another building of the same kind.
        //     - If no building can be found that will accept our goods, stop and wait, retry next update.
        // Despawn only when all goods are delivered.
    }

    // TODO
    // Unit::try_spawn_with_task could have a fallback:
    // e.g., if we fail to deliver to storage, fallback to DeliverToProducer?
    //
    // fallback_task: Option<UnitTask>,
    //
    // also a completion task, no need to hardcode it to despawn.
    // 
    // completion_task: Option<UnitTask>
    //
    pub fn try_spawn_with_task(query: &Query, owner_id: GenerationalIndex, task: UnitTask) -> Result<GenerationalIndex, String> {
        debug_assert!(owner_id.is_valid());

        // Handle root tasks here. These will start the task chain and might take some time to complete.
        match task {
            UnitTask::DeliverToStorage {
                origin_building_kind,
                origin_building_base_cell,
                unit_starting_cell,
                storage_buildings_accepted,
                resource_kind_to_deliver,
                resource_count,
                completion_callback
            } => {
                let (destination, path) =
                    match find_storage(query, unit_starting_cell, storage_buildings_accepted, resource_kind_to_deliver) {
                    Some(result) => result,
                    None => return Err("Couldn't find a storage building!".into()),
                };

                let unit =
                    match query.try_spawn_unit(unit_starting_cell, config::UNIT_RUNNER) {
                    Some(unit) => unit,
                    None => return Err("Couldn't spawn new unit!".into()),
                };

                unit.receive_resources(resource_kind_to_deliver, resource_count);
                unit.go_to_building(path, origin_building_base_cell, destination.base_cell);

                unit.assign_next_task(UnitInternalTask::VisitBuilding {
                    root_task: Box::new(UnitInternalTask::DeliverToStorage {
                        origin_building_kind,
                        origin_building_id: owner_id,
                        origin_building_base_cell,
                        unit_starting_cell,
                        storage_buildings_accepted,
                        resource_kind_to_deliver,
                        resource_count,
                        completion_callback
                    }),
                    destination
                });

                Ok(unit.id())
            }
        }
    }

    // ----------------------
    // Inventory / Resources:
    // ----------------------

    pub fn peek_inventory(&self) -> Option<StockItem> {
        debug_assert!(self.is_spawned());
        self.inventory.peek()
    }

    pub fn is_inventory_empty(&self) -> bool {
        debug_assert!(self.is_spawned());
        self.inventory.is_empty()
    }

    // Returns number of resources it was able to accommodate.
    // Unit inventories can always accommodate all resources received.
    pub fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(self.is_spawned());
        debug_assert!(kind.bits().count_ones() == 1);

        self.debug.log_resources_gained(kind, count);
        self.inventory.receive_resources(kind, count)
    }

    // Tries to gives away up to `count` resources. Returns the number
    // of resources it was able to give, which can be less or equal to `count`.
    pub fn give_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(self.is_spawned());
        debug_assert!(kind.bits().count_ones() == 1);

        self.debug.log_resources_lost(kind, count);
        self.inventory.give_resources(kind, count)
    }

    // ----------------------
    // Internal helpers:
    // ----------------------

    #[inline]
    fn assign_next_task(&mut self, task: UnitInternalTask) {
        self.next_task = task;
    }

    #[inline]
    fn has_reached_building(&self, destination: &BuildingTileInfo) -> bool {
        self.map_cell == destination.road_link
    }

    fn update_direction_and_anim(&mut self, tile: &mut Tile, new_direction: UnitDirection) {
        if self.direction != new_direction {
            self.direction = new_direction;
            let new_anim_set_key = anim_set_for_direction(new_direction);
            self.anim_sets.set_anim(tile, new_anim_set_key);
        }
    }

    fn log_going_to(&mut self, origin: Cell, destination: Cell) {
        if !self.debug.show_popups() {
            return;
        }
        let origin_building_name = debug::tile_name_at(origin, TileMapLayerKind::Objects);
        let destination_building_name = debug::tile_name_at(destination, TileMapLayerKind::Objects);
        self.debug.popup_msg(format!("Goto: {} -> {}", origin_building_name, destination_building_name));
    }
}

// ----------------------------------------------
// UnitInventory
// ----------------------------------------------

#[derive(Clone, Default)]
struct UnitInventory {
    // Unit can carry only one resource kind at a time.
    item: Option<StockItem>,
}

impl UnitInventory {
    #[inline]
    fn peek(&self) -> Option<StockItem> {
        self.item
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.item.is_none()
    }

    #[inline]
    fn count(&self) -> u32 {
        match &self.item {
            Some(item) => item.count,
            None => 0,
        }
    }

    #[inline]
    fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        if let Some(item) = &mut self.item {
            debug_assert!(item.kind == kind && item.count != 0);
            item.count += count;
        } else {
            self.item = Some(StockItem { kind, count });
        }
        count
    }

    // Returns number of items decremented, which can be <= `count`.
    #[inline]
    fn give_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        if let Some(item) = &mut self.item {
            debug_assert!(item.kind == kind && item.count != 0);

            let given_count = {
                if count <= item.count {
                    item.count -= count;
                    count
                } else {
                    let prev_count = item.count;
                    item.count = 0;
                    prev_count
                }
            };

            if item.count == 0 {
                self.item = None; // Gave away everything.
            }

            given_count
        } else {
            0
        }
    }
}

// ----------------------------------------------
// UnitAnimSets
// ----------------------------------------------

type UnitAnimSetKey = StrHashPair;

#[derive(Clone, Default)]
struct UnitAnimSets {
    // Hash of current anim set we're playing.
    current_anim_set_key: UnitAnimSetKey,

    // Maps from anim set name hash to anim set index.
    anim_set_index_map: PreHashedKeyMap<StringHash, usize>,
}

impl UnitAnimSets {
    const IDLE:    UnitAnimSetKey = UnitAnimSetKey::from_str("idle");
    const WALK_NE: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_ne");
    const WALK_NW: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_nw");
    const WALK_SE: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_se");
    const WALK_SW: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_sw");

    fn new(tile: &mut Tile, new_anim_set_key: UnitAnimSetKey) -> Self {
        let mut anim_set = Self::default();
        anim_set.set_anim(tile, new_anim_set_key);
        anim_set
    }

    fn reset(&mut self) {
        self.current_anim_set_key = UnitAnimSetKey::default();
        self.anim_set_index_map.clear();
    }

    fn set_anim(&mut self, tile: &mut Tile, new_anim_set_key: UnitAnimSetKey) {
        if self.current_anim_set_key.hash != new_anim_set_key.hash {
            self.current_anim_set_key = new_anim_set_key;
            if let Some(index) = self.find_index(tile, new_anim_set_key) {
                tile.set_anim_set_index(index);
            }
        }
    }

    fn find_index(&mut self, tile: &Tile, anim_set_key: UnitAnimSetKey) -> Option<usize> {
        if self.anim_set_index_map.is_empty() {
            // Lazily init on demand.
            self.build_mapping(tile.tile_def(), tile.variation_index());
        }

        self.anim_set_index_map.get(&anim_set_key.hash).copied()
    }

    fn build_mapping(&mut self, tile_def: &TileDef, variation_index: usize) {
        debug_assert!(self.anim_set_index_map.is_empty());

        if variation_index >= tile_def.variations.len() {
            return;
        }

        let variation = &tile_def.variations[variation_index];
        for (index, anim_set) in variation.anim_sets.iter().enumerate() {
            if self.anim_set_index_map.insert(anim_set.hash, index).is_some() {
                eprintln!("Unit '{}': An entry for anim set '{}' ({:#X}) already exists at index: {index}!",
                          tile_def.name,
                          anim_set.name,
                          anim_set.hash);
            }
        }
    }
}

// ----------------------------------------------
// UnitDirection
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Default, Display)]
enum UnitDirection {
    #[default]
    Idle,
    NE,
    NW,
    SE,
    SW,
}

#[inline]
fn direction_between(a: Cell, b: Cell) -> UnitDirection {
    match (b.x - a.x, b.y - a.y) {
        ( 1,  0 ) => UnitDirection::NE,
        ( 0,  1 ) => UnitDirection::NW,
        ( 0, -1 ) => UnitDirection::SE,
        (-1,  0 ) => UnitDirection::SW,
        _ => UnitDirection::Idle,
    }
}

#[inline]
fn anim_set_for_direction(direction: UnitDirection) -> UnitAnimSetKey {
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

#[derive(Clone, Default, DrawDebugUi)]
struct UnitNavigation {
    #[debug_ui(skip)]
    path: Path,
    path_index: usize,
    progress: f32, // 0.0 to 1.0 for the current segment.

    #[debug_ui(separator)]
    direction: UnitDirection,

    // Debug:
    #[debug_ui(edit)]
    pause_current_path: bool,
    #[debug_ui(edit)]
    single_step: bool,
    #[debug_ui(edit, step = "0.01")]
    step_size: f32,
    #[debug_ui(edit, widget = "button")]
    advance_one_step: bool,
    #[debug_ui(skip)]
    goals: Option<(Cell, Cell)>, // (origin_cell, destination_cell), may be different from path start/end.
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum UnitNavStatus {
    Idle,
    Moving,
    Paused,
}

#[derive(Copy, Clone)]
enum UnitNavResult {
    Idle,                                   // Do nothing (also returned when no path).
    Moving(Cell, Cell, f32, UnitDirection), // From -> To cells and progress between them.
    AdvancedCell(Cell, UnitDirection),      // Cell we've just entered, new direction to turn.
    ReachedGoal(Cell, UnitDirection),       // Goal Cell, current direction.
}

impl UnitNavigation {
    // TODO: Make this part of UnitConfig:
    //  config.speed = 1.5; // tiles per second
    //  config.segment_duration = 1.0 / config.speed;
    const SEGMENT_DURATION: f32 = 0.6;

    fn update(&mut self, mut delta_time_secs: Seconds) -> UnitNavResult {
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

    fn reset(&mut self, new_path: Option<&Path>, optional_goals: Option<(Cell, Cell)>) {
        self.path.clear();
        self.path_index = 0;
        self.progress   = 0.0;
        self.direction  = UnitDirection::default();
        self.goals      = optional_goals;

        if let Some(new_path) = new_path {
            debug_assert!(!new_path.is_empty());
            // NOTE: Use extend() instead of direct assignment so
            // we can reuse the previous allocation of `self.path`.
            self.path.extend(new_path.iter().copied());
        }
    }

    fn status(&self) -> UnitNavStatus {
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
}

// ----------------------------------------------
// UnitTasks
// ----------------------------------------------

// Root tasks.
pub enum UnitTask {
    // Producer -> Storage
    DeliverToStorage {
        origin_building_kind: BuildingKind,
        origin_building_base_cell: Cell,
        unit_starting_cell: Cell,
        storage_buildings_accepted: BuildingKind,
        resource_kind_to_deliver: ResourceKind,
        resource_count: u32,
        completion_callback: Option<fn(&mut Unit, &mut Building)>,
    },

    // TODO: Other tasks

    // Storage -> Producer | Producer -> Producer
    //DeliverToProducer {},

    // Producer|Service -> Fetch Storage -> Producer|Service
    //FetchFromStorage {},
}

// Expanded internal tasks.
#[derive(Clone, Default)]
enum UnitInternalTask {
    #[default]
    Idle,

    Retry {
        task: Box<UnitInternalTask>,
    },

    VisitBuilding { 
        destination: BuildingTileInfo,
        root_task: Box<UnitInternalTask>,
    },

    // TODO: maybe just one generic delivery task if possible?
    DeliverToStorage {
        // TODO use BuildingKindAndId
        origin_building_kind: BuildingKind,
        origin_building_id: GenerationalIndex,
        origin_building_base_cell: Cell,
        unit_starting_cell: Cell, // TODO probably dont need this
        storage_buildings_accepted: BuildingKind,
        resource_kind_to_deliver: ResourceKind,
        resource_count: u32, // TODO probably dont need this

        completion_callback: Option<fn(&mut Unit, &mut Building)>,
    }
}

struct DeliverToStorageTaskData {
}   //data

impl DeliverToStorageTaskData {
    fn is_completed(&self) {}
    fn execute(&self) {}
    fn retry(&self) {}
    fn terminate(&self) {}
}

// etc...

impl UnitInternalTask {
    // TODO
    // - should have a member .execute() and a .is_completed() to tidy up the match code.
    // - can we maybe just keep a vector with all the current tasks running, to avoid the 
    //   Box<> for the root/retry? Then just hold an index to the vector. Vector gets cleared
    //   once all tasks completed.

    //fn is_completed() {}
    //fn execute() {}
    //fn retry() {}
    //fn terminate() {}
}

// ----------------------------------------------
// Path finding helpers:
// ----------------------------------------------

fn find_storage<'search>(query: &'search Query,
                         origin: Cell,
                         storage_buildings_accepted: BuildingKind,
                         resource_kind_to_deliver: ResourceKind) -> Option<(BuildingTileInfo, &'search Path)> {
    // Only one resource kind at a time.
    debug_assert!(resource_kind_to_deliver.bits().count_ones() == 1);

    struct StorageInfo {
        kind: BuildingKind,
        road_link: Cell,
        base_cell: Cell,
        distance: i32,
        slots_available: u32,
    }

    const MAX_CANDIDATES: usize = 4;
    let mut storage_candidates: SmallVec<[StorageInfo; MAX_CANDIDATES]> = SmallVec::new();

    // Try to find storage buildings that can accept our delivery.
    query.for_each_storage_building(storage_buildings_accepted, |storage| {
        let slots_available = storage.receivable_amount(resource_kind_to_deliver);
        if slots_available != 0 {
            if let Some(storage_road_link) = query.find_nearest_road_link(storage.cell_range()) {
                storage_candidates.push(StorageInfo {
                    kind: storage.kind(),
                    road_link: storage_road_link,
                    base_cell: storage.base_cell(),
                    distance: origin.manhattan_distance(storage_road_link),
                    slots_available,
                });
                if storage_candidates.len() == MAX_CANDIDATES {
                    // We've collected enough candidate storage buildings, stop the search.
                    return false;
                }
            }
        }
        // Else we couldn't find a single free slot in this storage, try again with another one.
        true
    });

    if storage_candidates.is_empty() {
        // Couldn't find any suitable storage building.
        return None;
    }

    // Sort by closest storage buildings first. Tie breaker is the number of slots available, highest first.
    storage_candidates.sort_by_key(|storage| {
        (storage.distance, std::cmp::Reverse(storage.slots_available))
    });

    // Find a road path to a storage building. Try our best candidates first.
    for storage in &storage_candidates {
        match query.find_path(PathNodeKind::Road, origin, storage.road_link) {
            SearchResult::PathFound(path) => {
                let destination = BuildingTileInfo {
                    kind: storage.kind,
                    road_link: storage.road_link,
                    base_cell: storage.base_cell,
                };
                return Some((destination, path));
            },
            SearchResult::PathNotFound => {
                // Building is not reachable (lacks road access?).
                // Try another candidate.
                continue;
            },
        }
    }

    None
}

// ----------------------------------------------
// Debug UI
// ----------------------------------------------

impl Unit<'_> {
    pub fn draw_debug_ui(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        /* TODO
        if ui.collapsing_header("Config", imgui::TreeNodeFlags::empty()) {
            self.config.draw_debug_ui(ui_sys);
        }

        if ui.collapsing_header("Inventory", imgui::TreeNodeFlags::empty()) {
            self.inventory.draw_debug_ui(ui_sys);
        }

        if ui.collapsing_header("Tasks", imgui::TreeNodeFlags::empty()) {
            self.tasks.draw_debug_ui(ui_sys);
        }
        */

        self.debug.draw_debug_ui(ui_sys);

        if ui.collapsing_header("Navigation", imgui::TreeNodeFlags::empty()) {
            if let Some(dir) = imgui_ui::dpad_buttons(ui) {
                let tile_map = query.tile_map();
                match dir {
                    DPadDirection::NE => {
                        self.teleport(tile_map, Cell::new(self.map_cell.x + 1, self.map_cell.y));
                    },
                    DPadDirection::NW => {
                        self.teleport(tile_map, Cell::new(self.map_cell.x, self.map_cell.y + 1));
                    },
                    DPadDirection::SE => {
                        self.teleport(tile_map, Cell::new(self.map_cell.x, self.map_cell.y - 1));
                    },
                    DPadDirection::SW => {
                        self.teleport(tile_map, Cell::new(self.map_cell.x - 1, self.map_cell.y));
                    },
                }
            }

            ui.separator();

            ui.text(format!("Cell       : {}", self.map_cell));
            ui.text(format!("Iso Coords : {}", find_unit_tile!(&self, query).iso_coords()));
            ui.text(format!("Direction  : {}", self.direction));
            ui.text(format!("Anim       : {}", self.anim_sets.current_anim_set_key.string));

            if ui.button("Force Idle Anim") {
                self.update_direction_and_anim(find_unit_tile!(&mut self, query), UnitDirection::Idle);
            }

            ui.separator();

            let color = match self.navigation.status() {
                UnitNavStatus::Idle   => Color::yellow(),
                UnitNavStatus::Paused => Color::red(),
                UnitNavStatus::Moving => Color::green(),
            };

            ui.text_colored(color.to_array(), format!("Path Navigation Status: {:?}", self.navigation.status()));

            if let Some((origin, destination)) = self.navigation.goals {
                let origin_building_name = debug::tile_name_at(origin, TileMapLayerKind::Objects);
                let destination_building_name = debug::tile_name_at(destination, TileMapLayerKind::Objects);
                ui.text(format!("Start Building : {}, {}", origin, origin_building_name));
                ui.text(format!("Dest  Building : {}, {}", destination, destination_building_name));
            }

            self.navigation.draw_debug_ui(ui_sys);
        }
    }

    pub fn draw_debug_popups(&mut self,
                             query: &Query,
                             ui_sys: &UiSystem,
                             transform: &WorldToScreenTransform,
                             visible_range: CellRange,
                             delta_time_secs: Seconds) {

        self.debug.draw_popup_messages(
            || find_unit_tile!(&self, query),
            ui_sys,
            transform,
            visible_range,
            delta_time_secs);
    }
}
