use strum_macros::Display;
use proc_macros::DrawDebugUi;

use crate::{
    game_object_debug_options,
    pathfind::Path,
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
        resources::{ResourceKind, StockItem}
    }
};

pub mod config;
use config::UnitConfig;

// ----------------------------------------------
// Helper macros
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
*/
pub struct Unit<'config> {
    name: &'config str,
    config: &'config UnitConfig,
    map_cell: Cell,
    anim_sets: UnitAnimSets,
    inventory: UnitInventory,
    navigation: UnitNavigation,
    direction: UnitDirection,
    debug: UnitDebug,
}

impl<'config> Unit<'config> {
    pub fn new(name: &'config str, tile: &mut Tile, config: &'config UnitConfig) -> Self {
        Self {
            name,
            config,
            map_cell: tile.base_cell(),
            anim_sets: UnitAnimSets::new(tile, UnitAnimSets::IDLE),
            inventory: UnitInventory::default(),
            navigation: UnitNavigation::default(),
            direction: UnitDirection::default(),
            debug: UnitDebug::default(),
        }
    }

    #[inline]
    pub fn name(&self) -> &str {
        self.name
    }

    #[inline]
    pub fn cell(&self) -> Cell {
        self.map_cell
    }

    #[inline]
    pub fn follow_path(&mut self, path: Option<&Path>) {
        self.navigation.reset(path, None);
        if path.is_some() {
            self.debug.popup_msg("New Goal");
        }
    }

    #[inline]
    pub fn go_to_building(&mut self, path: &Path, start_building_cell: Cell, goal_building_cell: Cell) {
        self.navigation.reset(Some(path), Some((start_building_cell, goal_building_cell)));
        self.log_going_to(start_building_cell, goal_building_cell);
    }

    // Teleports to new tile cell and updates direction and animation.
    pub fn teleport(&mut self, tile_map: &mut TileMap, destination_cell: Cell) -> bool {
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

    pub fn update_movement(&mut self, query: &Query, delta_time_secs: Seconds) {
        // Path following and movement:
        match self.navigation.update(delta_time_secs) {
            UnitNavResult::ReachedGoal(cell, direction) => {
                let tile = find_unit_tile!(&mut self, query);

                let goal_building_cell = self.navigation.target_buildings.map_or(
                    Cell::invalid(), |(_, goal_building_cell)| goal_building_cell);

                debug_assert!(self.direction == direction);
                debug_assert!(self.map_cell == cell && tile.base_cell() == cell);

                self.follow_path(None); // Clear current path.
                self.update_direction_and_anim(tile, UnitDirection::Idle);
                self.debug.popup_msg("Reached Goal");

                if goal_building_cell.is_valid() {
                    let world = query.world();
                    let tile_map = query.tile_map();
                    if let Some(building) = world.find_building_for_cell_mut(goal_building_cell, tile_map) {
                        building.visited(self, query);
                    }
                }
            },
            UnitNavResult::AdvancedCell(cell, direction) => {
                let did_teleport = self.teleport(query.tile_map(), cell);
                let tile = find_unit_tile!(&mut self, query);
                self.update_direction_and_anim(tile, direction);
                debug_assert!(did_teleport && self.direction == direction);
                debug_assert!(self.map_cell == cell && tile.base_cell() == cell);
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
            UnitNavResult::None => {
                // Nothing.
            },
        }
    }

    pub fn update(&mut self, _query: &Query, _delta_time_secs: Seconds) {
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

        // Unit might have these methods:
        // 
        // fn ship_to_storage(storage_kinds, resource_kind, resource_count, from_building_cell, starting_cell)
        // fn ship_to_producer(producer_kind, resource_kind, resource_count, from_building_cell, starting_cell)
        // fn ship_to_service(service_kind, resource_kind, resource_count, from_building_cell, starting_cell)
    }

    pub fn receive_resources(&mut self, kind: ResourceKind, count: u32) {
        self.debug.log_resources_gained(kind, count);
        self.inventory.receive_resources(kind, count);
    }

    pub fn give_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        self.debug.log_resources_lost(kind, count);
        self.inventory.give_resources(kind, count)
    }

    pub fn peek_inventory(&self) -> Option<StockItem> {
        self.inventory.peek()
    }

    // ----------------------
    // Internal:
    // ----------------------

    fn update_direction_and_anim(&mut self, tile: &mut Tile, new_direction: UnitDirection) {
        if self.direction != new_direction {
            self.direction = new_direction;
            let new_anim_set_key = anim_set_for_direction(new_direction);
            self.anim_sets.set_anim(tile, new_anim_set_key);
        }
    }

    fn log_going_to(&mut self, start_building_cell: Cell, goal_building_cell: Cell) {
        if !self.debug.show_popups() {
            return;
        }
        let start_building_name = debug::tile_name_at(start_building_cell, TileMapLayerKind::Objects);
        let goal_building_name  = debug::tile_name_at(goal_building_cell,  TileMapLayerKind::Objects);
        self.debug.popup_msg(format!("Goto: {} -> {}", start_building_name, goal_building_name));
    }
}

// ----------------------------------------------
// UnitAnimSets
// ----------------------------------------------

type UnitAnimSetKey = StrHashPair;

#[derive(Default)]
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

#[derive(Default, DrawDebugUi)]
struct UnitNavigation {
    #[debug_ui(skip)]
    path: Path,
    path_index: usize,
    progress: f32, // 0.0 to 1.0 for the current segment.

    #[debug_ui(separator)]
    direction: UnitDirection,

    #[debug_ui(skip)]
    target_buildings: Option<(Cell, Cell)>,

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

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum UnitNavStatus {
    Idle,
    Moving,
    Paused,
}

#[derive(Copy, Clone)]
enum UnitNavResult {
    None,                                   // Do nothing (also returned when no path).
    Moving(Cell, Cell, f32, UnitDirection), // From -> To cells and progress between them.
    ReachedGoal(Cell, UnitDirection),       // Goal Cell, current direction.
    AdvancedCell(Cell, UnitDirection),      // Cell we've just entered, new direction to turn.
}

impl UnitNavigation {
    // TODO: Make this part of UnitConfig:
    //  config.speed = 1.5; // tiles per second
    //  config.segment_duration = 1.0 / config.speed;
    const SEGMENT_DURATION: f32 = 0.6;

    fn update(&mut self, mut delta_time_secs: Seconds) -> UnitNavResult {
        if self.pause_current_path || self.path.is_empty() {
            // No path to follow.
            return UnitNavResult::None;
        }

        // Single step debug:
        if self.single_step {
            if !self.advance_one_step {
                return UnitNavResult::None;
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

    fn reset(&mut self, new_path: Option<&Path>, target_buildings: Option<(Cell, Cell)>) {
        self.path.clear();
        self.path_index = 0;
        self.progress   = 0.0;
        self.direction  = UnitDirection::default();
        self.target_buildings = target_buildings;

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
// UnitInventory
// ----------------------------------------------

#[derive(Default)]
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
    fn receive_resources(&mut self, kind: ResourceKind, count: u32) {
        if let Some(item) = &mut self.item {
            debug_assert!(item.kind == kind && item.count != 0);
            item.count += count;
        } else {
            self.item = Some(StockItem { kind, count });
        }
    }

    // Returns number of items decremented, which can be <= count.
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

            if let Some((start_building_cell, goal_building_cell)) = self.navigation.target_buildings {
                let start_building_name = debug::tile_name_at(start_building_cell, TileMapLayerKind::Objects);
                let goal_building_name  = debug::tile_name_at(goal_building_cell,  TileMapLayerKind::Objects);
                ui.text(format!("Start Building : {}, {}", start_building_cell, start_building_name));
                ui.text(format!("Goal Building  : {}, {}", goal_building_cell, goal_building_name));
            }

            self.navigation.draw_debug_ui(ui_sys);
        }
    }

    pub fn draw_debug_popups(&mut self,
                             query: &Query,
                             ui_sys: &UiSystem,
                             transform: &WorldToScreenTransform,
                             visible_range: CellRange,
                             delta_time_secs: Seconds,
                             show_popup_messages: bool) {

        self.debug.draw_popup_messages(
            || find_unit_tile!(&self, query),
            ui_sys,
            transform,
            visible_range,
            delta_time_secs,
            show_popup_messages);
    }
}
