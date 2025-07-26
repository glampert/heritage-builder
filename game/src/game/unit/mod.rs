use strum_macros::Display;
use proc_macros::DrawDebugUi;

use crate::{
    game_object_debug_options,
    pathfind::Path,
    imgui_ui::{
        self,
        UiSystem,
        DPadDirection
    },
    tile::{
        map::{Tile, TileMapLayerKind},
        sets::{TileDef, TileKind},
    },
    utils::{
        self,
        Vec2,
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
        resources::ResourceStock
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
    map_cell: Cell,
    config: &'config UnitConfig,
    inventory: UnitInventory,
    navigation: UnitNavigation,
    direction: UnitDirection,
    anim_sets: UnitAnimSets,
    debug: UnitDebug,
}

impl<'config> Unit<'config> {
    pub fn new(name: &'config str, map_cell: Cell, config: &'config UnitConfig) -> Self {
        Self {
            name,
            map_cell,
            config,
            inventory: UnitInventory::default(),
            navigation: UnitNavigation::default(),
            direction: UnitDirection::default(),
            anim_sets: UnitAnimSets::default(),
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
    pub fn set_cell(&mut self, new_map_cell: Cell) {
        debug_assert!(new_map_cell.is_valid());
        self.map_cell = new_map_cell;
    }

    #[inline]
    pub fn follow_path(&mut self, path: Option<&Path>) {
        self.navigation.reset(path);
    }

    pub fn update(&mut self, query: &mut Query, delta_time_secs: Seconds) {

        //TODO: need a separate update for movement that runs more frequently since simulation time step is fixed.

        // Path following and movement:
        match self.navigation.update(delta_time_secs) {
            UnitNavResult::NoPath => {
                // Nothing.
            },
            UnitNavResult::ReachedGoal(cell, direction) => {
                self.move_to_cell(query, cell);
                self.set_direction(query, direction);
                self.set_anim(query, UnitAnimSets::IDLE);
                self.navigation.reset(None);
            },
            UnitNavResult::AdvancedCell(cell, direction) => {
                self.move_to_cell(query, cell);
                self.set_direction(query, direction);
            },
            UnitNavResult::Moving => {
                // TODO: interpolate within tile position.
            },
        }
    }

    pub fn draw_debug_ui(&mut self, query: &mut Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        /* TODO
        if ui.collapsing_header("Config", imgui::TreeNodeFlags::empty()) {
            self.config.draw_debug_ui(ui_sys);
        }
        */

        self.debug.draw_debug_ui(ui_sys);

        if ui.collapsing_header("Movement", imgui::TreeNodeFlags::empty()) {
            if let Some(dir) = imgui_ui::dpad_buttons(ui) {
                let current_cell = self.map_cell;
                match dir {
                    DPadDirection::NE => self.move_to_cell(query, Cell::new(current_cell.x + 1, current_cell.y)),
                    DPadDirection::NW => self.move_to_cell(query, Cell::new(current_cell.x, current_cell.y + 1)),
                    DPadDirection::SE => self.move_to_cell(query, Cell::new(current_cell.x, current_cell.y - 1)),
                    DPadDirection::SW => self.move_to_cell(query, Cell::new(current_cell.x - 1, current_cell.y)),
                }
            }

            ui.separator();
            ui.text(format!("Cell {}:", self.map_cell));
            ui.indent_by(5.0);
            self.navigation.draw_debug_ui(ui_sys);
            ui.unindent_by(5.0);
        }
    }

    pub fn draw_debug_popups(&mut self,
                             query: &mut Query,
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

    // ----------------------
    // Internal:
    // ----------------------

    fn set_anim(&mut self, query: &mut Query, anim_set_name_hash: StrHashPair) {
        if self.anim_sets.current_anim_set_hash != anim_set_name_hash.hash {
            self.anim_sets.current_anim_set_hash = anim_set_name_hash.hash;

            let tile = find_unit_tile!(&mut self, query);
            if let Some(index) = self.anim_sets.find_index(tile, anim_set_name_hash) {
                tile.set_anim_set_index(index);
            }
        }
    }

    fn set_direction(&mut self, query: &mut Query, new_direction: UnitDirection) {
        if self.direction != new_direction {
            self.direction = new_direction;

            let new_anim_set = match new_direction {
                UnitDirection::NE => UnitAnimSets::WALK_NE,
                UnitDirection::NW => UnitAnimSets::WALK_NW,
                UnitDirection::SE => UnitAnimSets::WALK_SE,
                UnitDirection::SW => UnitAnimSets::WALK_SW,
                UnitDirection::None => StrHashPair::empty(),
            };

            self.set_anim(query, new_anim_set);
        }
    }

    // Moves to new tile and updates direction and animation.
    fn move_to_cell(&mut self, query: &mut Query, destination_cell: Cell) {
        if query.tile_map.try_move_tile(self.map_cell, destination_cell, TileMapLayerKind::Objects) {
            let new_direction = direction_between(self.map_cell, destination_cell);
            self.set_cell(destination_cell);
            self.set_direction(query, new_direction);
        }
    }
}

// ----------------------------------------------
// UnitAnimSets
// ----------------------------------------------

#[derive(Default)]
struct UnitAnimSets {
    // Hash of current anim set we're playing.
    current_anim_set_hash: StringHash,

    // Maps from anim set name hash to anim set index.
    anim_set_index_map: PreHashedKeyMap<StringHash, usize>,
}

impl UnitAnimSets {
    const IDLE:    StrHashPair = StrHashPair::from_str("idle");
    const WALK_NE: StrHashPair = StrHashPair::from_str("walk_ne");
    const WALK_NW: StrHashPair = StrHashPair::from_str("walk_nw");
    const WALK_SE: StrHashPair = StrHashPair::from_str("walk_se");
    const WALK_SW: StrHashPair = StrHashPair::from_str("walk_sw");

    fn find_index(&mut self, tile: &Tile, anim_set_name_hash: StrHashPair) -> Option<usize> {
        if self.anim_set_index_map.is_empty() {
            // Lazily init on demand.
            self.build_mapping(tile.tile_def(), tile.variation_index());
        }

        self.anim_set_index_map.get(&anim_set_name_hash.hash).copied()
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
    None,
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
        _ => UnitDirection::None,
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
    progress: f32, // 0.0 to 1.0 for the current segment
    position: Vec2, // f32 position for rendering
    direction: UnitDirection,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum UnitNavResult {
    NoPath,
    Moving,
    ReachedGoal(Cell, UnitDirection),  // Goal Cell, current direction.
    AdvancedCell(Cell, UnitDirection), // Cell we've just entered, new direction to turn.
}

impl UnitNavigation {
    // TODO: Make this part of UnitConfig:
    //  config.speed = 2.5; // tiles per second
    //  config.segment_duration = 1.0 / config.speed;
    const SEGMENT_DURATION: f32 = 0.4;

    fn update(&mut self, delta_time_secs: Seconds) -> UnitNavResult {
        if self.path.is_empty() {
            // No path to follow.
            return UnitNavResult::NoPath;
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
            self.position = to.cell.to_vec2(); // Snap to cell center.

            // Look ahead for next turn:
            if self.path_index + 1 < self.path.len() {
                let dir = direction_between(to.cell, self.path[self.path_index + 1].cell);
                if dir != self.direction {
                    self.direction = dir;
                }
            }

            return UnitNavResult::AdvancedCell(to.cell, self.direction);
        }

        // Smooth interpolation:
        let from_pos = from.cell.to_vec2();
        let to_pos = to.cell.to_vec2();
        self.position = utils::lerp(from_pos, to_pos, self.progress);

        UnitNavResult::Moving
    }

    fn reset(&mut self, path: Option<&Path>) {
        self.path.clear();
        self.path_index = 0;

        if let Some(new_path) = path {
            // Use extend() instead of direct assignment so
            // we can reuse the previous allocation of `self.path`.
            self.path.extend(new_path.iter().copied());
        }
    }
}

// ----------------------------------------------
// UnitInventory
// ----------------------------------------------

#[derive(Default)]
struct UnitInventory {
    resources: Option<ResourceStock>,
    // WIP
}
