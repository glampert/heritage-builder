use serde::{Deserialize, Serialize};
use proc_macros::DrawDebugUi;

use super::{
    sim::{
        debug::DebugUiMode,
        resources::{ResourceKind, StockItem},
        Query,
    },
    world::{
        object::{GameObject, GenerationalIndex},
        stats::WorldStats,
    },
    undo_redo::GameObjectSavedState,
    unit::UnitId,
};
use crate::{
    game_object_debug_options,
    game_object_undo_redo_state,
    imgui_ui::UiSystem,
    save::PostLoadContext,
    engine::time::{CountdownTimer, Seconds},
    tile::{Tile, TileKind, TileMapLayerKind},
    utils::{
        coords::{Cell, CellRange, WorldToScreenTransform},
        hash::{self, StringHash, StrHashPair},
        Color,
    },
};

use config::{PropConfig, PropConfigs};
pub mod config;

// ----------------------------------------------
// Constants
// ----------------------------------------------

const PROP_VARIATION_DEPLETED: StrHashPair = StrHashPair::from_str("depleted");

// ----------------------------------------------
// PropDebug
// ----------------------------------------------

game_object_debug_options! {
    PropDebug,
}

// ----------------------------------------------
// HarvestablePropState
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
struct HarvestablePropState {
    resource: ResourceKind,
    amount: u32,
    respawn_timer: CountdownTimer,
    harvester_unit: UnitId,
    initial_variation: u32,
}

// ----------------------------------------------
// UndoRedoPropSavedState
// ----------------------------------------------

struct UndoRedoPropSavedState {
    harvestable_amount: u32,
    respawn_countdown: Seconds,
    initial_variation: u32,
}

game_object_undo_redo_state! {
    UndoRedoPropSavedState
}

// ----------------------------------------------
// Prop
// ----------------------------------------------

pub type PropId = GenerationalIndex;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Prop {
    id: PropId,
    map_cell: Cell,
    config_key: StringHash,

    // Resource harvesting state.
    harvestable: HarvestablePropState,

    #[serde(skip)]
    config: Option<&'static PropConfig>, // patched on post_load.

    #[serde(skip)]
    debug: PropDebug,
}

impl GameObject for Prop {
    // ----------------------
    // GameObject Interface:
    // ----------------------

    #[inline]
    fn id(&self) -> PropId {
        self.id
    }

    #[inline]
    fn update(&mut self, query: &Query) {
        debug_assert!(self.is_spawned());
        debug_assert!(self.config.is_some());

        if self.is_harvestable()
            && self.harvestable.amount == 0
            && self.harvestable.respawn_timer.tick(query.delta_time_secs())
        {
            self.respawn_harvestable(query);
        }
    }

    #[inline]
    fn tally(&self, _stats: &mut WorldStats) {
        // Nothing to tally.
    }

    fn post_load(&mut self, _context: &PostLoadContext) {
        debug_assert!(self.is_spawned());
        debug_assert!(self.config_key != hash::NULL_HASH);

        let config = PropConfigs::get().find_config_by_hash(self.config_key, "<prop>");

        self.config = Some(config);
    }

    fn undo_redo_record(&self) -> Option<Box<dyn GameObjectSavedState>> {
        UndoRedoPropSavedState::new_state(UndoRedoPropSavedState {
            harvestable_amount: self.harvestable.amount,
            respawn_countdown: self.harvestable.respawn_timer.remaining_secs(),
            initial_variation: self.harvestable.initial_variation,
        })
    }

    fn undo_redo_apply(&mut self, state: &dyn GameObjectSavedState) {
        let saved_state = UndoRedoPropSavedState::downcast(state);
        self.harvestable.amount = saved_state.harvestable_amount;
        self.harvestable.respawn_timer.reset(saved_state.respawn_countdown);
        self.harvestable.initial_variation = saved_state.initial_variation;
    }

    fn draw_debug_ui(&mut self, query: &Query, ui_sys: &UiSystem, mode: DebugUiMode) {
        debug_assert!(self.is_spawned());

        match mode {
            DebugUiMode::Overview => {
                self.draw_debug_ui_overview(query, ui_sys);
            }
            DebugUiMode::Detailed => {
                let ui = ui_sys.builder();
                if ui.collapsing_header("Prop", imgui::TreeNodeFlags::empty()) {
                    ui.indent_by(10.0);
                    self.draw_debug_ui_detailed(query, ui_sys);
                    ui.unindent_by(10.0);
                }
            }
        }
    }

    fn draw_debug_popups(&mut self,
                         query: &Query,
                         ui_sys: &UiSystem,
                         transform: WorldToScreenTransform,
                         visible_range: CellRange) {
        debug_assert!(self.is_spawned());

        self.debug.draw_popup_messages(self.find_tile(query),
                                       ui_sys,
                                       transform,
                                       visible_range,
                                       query.delta_time_secs());
    }
}

impl Prop {
    // ----------------------
    // Spawning / Despawning:
    // ----------------------

    pub fn spawned(&mut self, tile: &mut Tile, config: &'static PropConfig, id: PropId) {
        debug_assert!(!self.is_spawned());
        debug_assert!(tile.is_valid());
        debug_assert!(id.is_valid());
        debug_assert!(config.key_hash() != hash::NULL_HASH);
        debug_assert!(config.harvestable_resource.is_empty() || config.harvestable_resource.is_single_resource());

        self.id = id;
        self.map_cell = tile.base_cell();
        self.config = Some(config);
        self.config_key = config.key_hash();

        self.harvestable.resource = config.harvestable_resource;
        self.harvestable.amount = config.harvestable_amount;
        self.harvestable.respawn_timer.reset(config.respawn_time_secs);
        self.harvestable.initial_variation = tile.variation_index().try_into().unwrap();
    }

    pub fn despawned(&mut self, _query: &Query) {
        debug_assert!(self.is_spawned());

        self.id = PropId::default();
        self.map_cell = Cell::default();
        self.config = None;
        self.config_key = hash::NULL_HASH;
        self.harvestable = HarvestablePropState::default();
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

    #[inline]
    pub fn cell_range(&self) -> CellRange {
        debug_assert!(self.is_spawned());
        CellRange::new(self.map_cell, self.map_cell)
    }

    // ----------------------
    // Resource harvesting:
    // ----------------------

    #[inline]
    pub fn is_harvestable(&self) -> bool {
        debug_assert!(self.is_spawned());
        !self.harvestable.resource.is_empty()
    }

    #[inline]
    pub fn harvestable_resource(&self) -> ResourceKind {
        debug_assert!(self.is_spawned());
        self.harvestable.resource
    }

    #[inline]
    pub fn harvestable_amount(&self) -> u32 {
        debug_assert!(self.is_spawned());
        self.harvestable.amount
    }

    #[inline]
    pub fn is_being_harvested(&self) -> bool {
        debug_assert!(self.is_spawned());
        self.harvestable.harvester_unit.is_valid()
    }

    #[inline]
    pub fn harvester_unit(&self) -> UnitId {
        debug_assert!(self.is_spawned());
        self.harvestable.harvester_unit
    }

    #[inline]
    pub fn set_harvester_unit(&mut self, unit_id: UnitId) {
        debug_assert!(self.is_spawned());
        self.harvestable.harvester_unit = unit_id;
    }

    // Tries to harvest `amount` units of the available resource.
    // Result may be <= `amount`.
    pub fn harvest(&mut self, query: &Query, amount: u32) -> StockItem {
        debug_assert!(self.is_spawned());

        let resource = self.harvestable.resource;
        debug_assert!(resource.is_empty() || resource.is_single_resource());

        let amount_harvested = self.harvestable.amount.min(amount);
        self.harvestable.amount = self.harvestable.amount.saturating_sub(amount);

        if self.harvestable.amount == 0 {
            self.update_variation(query);
        }

        if amount_harvested != 0 {
            self.debug.popup_msg_color(Color::red(), format!("-{amount_harvested} {resource}"));
        }

        StockItem { kind: resource, count: amount_harvested }
    }

    fn respawn_harvestable(&mut self, query: &Query) {
        let config = self.config.unwrap();
        self.harvestable.amount = config.harvestable_amount;
        self.harvestable.respawn_timer.reset(config.respawn_time_secs);
        self.update_variation(query);
    }

    // ----------------------
    // Callbacks:
    // ----------------------

    pub fn register_callbacks() {
    }

    // ----------------------
    // Internal helpers:
    // ----------------------

    fn update_variation(&mut self, query: &Query) {
        if self.is_harvestable() {
            let tile = self.find_tile_mut(query);
            if self.harvestable.amount == 0 {
                if let Some(index) = self.find_variation(query, PROP_VARIATION_DEPLETED.hash) {
                    tile.set_variation_index(index);
                    self.debug.popup_msg_color(Color::red(), "Depleted");
                }
            } else {
                tile.set_variation_index(self.harvestable.initial_variation as usize);
                self.debug.popup_msg_color(Color::green(), "Respawned");
            }
        }
    }

    fn find_variation(&self, query: &Query, hash: StringHash) -> Option<usize> {
        let tile = self.find_tile(query);
        for (index, var) in tile.tile_def().variations.iter().enumerate() {
            if var.hash == hash {
                return Some(index);
            }
        }
        None
    }

    #[inline]
    fn find_tile<'world>(&self, query: &'world Query) -> &'world Tile {
        query.find_tile(self.cell(), TileMapLayerKind::Objects, TileKind::Prop)
             .expect("Prop should have an associated Tile in the TileMap!")
    }

    #[inline]
    fn find_tile_mut<'world>(&self, query: &'world Query) -> &'world mut Tile {
        query.find_tile_mut(self.cell(), TileMapLayerKind::Objects, TileKind::Prop)
             .expect("Prop should have an associated Tile in the TileMap!")
    }

    // ----------------------
    // Debug UI:
    // ----------------------

    fn draw_debug_ui_overview(&mut self, _query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        let font = ui.push_font(ui_sys.fonts().large);
        ui.text(format!("{} | ID{} @{}", self.name(), self.id(), self.cell()));
        font.pop();

        let color_bullet_text = |label: &str, value: u32| {
            ui.bullet_text(format!("{label}:"));
            ui.same_line();
            if value == 0 {
                ui.text_colored(Color::red().to_array(), format!("{value}"));
            } else {
                ui.text(format!("{value}"));
            }
        };

        color_bullet_text(&format!("Harvestable {}", self.harvestable.resource),
                          self.harvestable.amount);
    }

    fn draw_debug_ui_detailed(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        self.config.unwrap().draw_debug_ui_with_header("Config", ui_sys);

        // NOTE: Use the special ##id here so we don't collide with Tile/Properties.
        if !ui.collapsing_header("Properties##_prop_properties", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        #[derive(DrawDebugUi)]
        struct DrawDebugUiVariables<'a> {
            name: &'a str,
            cell: Cell,
            id: PropId,
            harvestable_resource: ResourceKind,
            harvestable_amount: u32,
            is_being_harvested: bool,
        }
        let debug_vars = DrawDebugUiVariables {
            name: self.name(),
            cell: self.cell(),
            id: self.id(),
            harvestable_resource: self.harvestable.resource,
            harvestable_amount: self.harvestable.amount,
            is_being_harvested: self.is_being_harvested(),
        };
        debug_vars.draw_debug_ui(ui_sys);

        if self.is_harvestable() {
            if self.harvestable.amount == 0 {
                ui.text(format!("Time Until Respawn   : {:.2}", self.harvestable.respawn_timer.remaining_secs()));
            }

            #[allow(static_mut_refs)]
            unsafe {
                static mut HARVEST_AMOUNT: u32 = 1;
                ui.input_scalar("Harvest Amount", &mut HARVEST_AMOUNT)
                    .step(1)
                    .build();

                if ui.button("Harvest") {
                    self.harvest(query, HARVEST_AMOUNT);
                }
            }

            if ui.button("Respawn Now") {
                self.respawn_harvestable(query);
            }
        }
    }
}
