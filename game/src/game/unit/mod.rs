use slab::Slab;

use crate::{
    game_object_debug_options,
    imgui_ui::UiSystem,
    tile::sets::TileKind,
    tile::map::TileMapLayerKind,
    utils::{
        Seconds,
        coords::{
            Cell,
            CellRange,
            WorldToScreenTransform
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
 - Moves across the tile map, so cell can change.
 - Transports resources from A to B (has a start point and a destination).
 - Patrols an area around its building to provide a service to households.
 - Most units will only walk on paved roads. Some units may go off-road.
*/

pub struct Unit<'config> {
    cell: Cell,
    config: &'config UnitConfig,
    resources: Option<ResourceStock>,
    debug: UnitDebug,
}

impl<'config> Unit<'config> {
    pub fn new(cell: Cell, config: &'config UnitConfig) -> Self {
        Self {
            cell,
            config,
            resources: None,
            debug: UnitDebug::default(),
        }
    }

    pub fn update(&mut self,
                  _query: &mut Query<'config, '_, '_, '_,>,
                  _delta_time_secs: Seconds) {
        // TODO
    }

    pub fn draw_debug_ui(&mut self,
                         _query: &mut Query<'config, '_, '_, '_,>,
                         ui_sys: &UiSystem) {

        let ui = ui_sys.builder();
        //if ui.collapsing_header("Config", imgui::TreeNodeFlags::empty()) {
            //self.config.draw_debug_ui(ui_sys);
        //}

        self.debug.draw_debug_ui(ui_sys);

        ui.text("Testing unit debug callback.");
        if ui.button("Test popup") {
            self.debug.popup_msg("hello");
        }
    }

    pub fn draw_debug_popups(&mut self,
                             query: &mut Query<'config, '_, '_, '_>,
                             ui_sys: &UiSystem,
                             transform: &WorldToScreenTransform,
                             visible_range: CellRange,
                             delta_time_secs: Seconds,
                             show_popup_messages: bool) {

        self.debug.draw_popup_messages(
            || {
                query.find_tile(self.cell, TileMapLayerKind::Objects, TileKind::Unit)
                    .expect("Unit should have an associated Tile in the TileMap!")
            },
            ui_sys,
            transform,
            visible_range,
            delta_time_secs,
            show_popup_messages);
    }
}

// ----------------------------------------------
// UnitList
// ----------------------------------------------

pub struct UnitList<'config> {
    units: Slab<Unit<'config>>,
}

impl<'config> UnitList<'config> {
    #[inline]
    pub fn new() -> Self {
        Self {
            units: Slab::new(),
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.units.clear();
    }

    #[inline]
    pub fn try_get(&self, index: usize) -> Option<&Unit<'config>> {
        self.units.get(index)
    }

    #[inline]
    pub fn try_get_mut(&mut self, index: usize) -> Option<&mut Unit<'config>> {
        self.units.get_mut(index)
    }

    #[inline]
    pub fn add(&mut self, unit: Unit<'config>) -> usize {
        self.units.insert(unit)
    }

    #[inline]
    pub fn remove(&mut self, index: usize) -> bool {
        if self.units.try_remove(index).is_none() {
            return false;
        }
        true
    }

    #[inline]
    pub fn for_each<F>(&self, mut visitor_fn: F)
        where F: FnMut(usize, &Unit<'config>) -> bool
    {
        for (index, unit) in &self.units {
            let should_continue = visitor_fn(index, unit);
            if !should_continue {
                break;
            }
        }
    }

    #[inline]
    pub fn for_each_mut<F>(&mut self, mut visitor_fn: F)
        where F: FnMut(usize, &mut Unit<'config>) -> bool
    {
        for (index, unit) in &mut self.units {
            let should_continue = visitor_fn(index, unit);
            if !should_continue {
                break;
            }
        }
    }

    #[inline]
    pub fn update(&mut self, query: &mut Query<'config, '_, '_, '_>, delta_time_secs: Seconds) {
        for (_, unit) in &mut self.units {
            unit.update(query, delta_time_secs);
        }
    }
}
