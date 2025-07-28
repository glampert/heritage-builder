use slab::Slab;

use crate::{
    imgui_ui::UiSystem,
    tile::sets::TileKind,
    utils::{
        Seconds,
        coords::{
            Cell,
            CellRange,
            WorldToScreenTransform
        }
    },
    tile::map::{
        Tile,
        TileMapLayerKind,
        GameStateHandle
    },
    game:: {
        building::{
            Building,
            BuildingKind,
            BuildingArchetypeKind,
            BUILDING_ARCHETYPE_COUNT
        },
        unit::{
            Unit,
        }
    }
};

use super::{
    Query
};

// ----------------------------------------------
// World
// ----------------------------------------------

// Holds the world state and provides queries.
pub struct World<'config> {
    // One list per building archetype.
    buildings_list: [BuildingList<'config>; BUILDING_ARCHETYPE_COUNT],

    // All active spawned units.
    units_list: UnitList<'config>,
}

impl<'config> World<'config> {
    pub fn new() -> Self {
        Self {
            buildings_list: [
                BuildingList::new(BuildingArchetypeKind::Producer),
                BuildingList::new(BuildingArchetypeKind::Storage),
                BuildingList::new(BuildingArchetypeKind::Service),
                BuildingList::new(BuildingArchetypeKind::House)
            ],
            units_list: UnitList::new(),
        }
    }

    pub fn reset(&mut self) {
        for buildings in &mut self.buildings_list {
            buildings.clear();
        }

        self.units_list.clear();
    }

    pub fn update(&mut self, query: &mut Query<'config, '_, '_, '_>, delta_time_secs: Seconds) {
        for buildings in &mut self.buildings_list {
            let list_archetype = buildings.archetype_kind();
            for (_, building) in buildings.iter_mut() {
                debug_assert!(building.archetype_kind() == list_archetype);
                building.update(query, delta_time_secs);
            }
        }

        for (_, unit) in self.units_list.iter_mut() {
            unit.update(query, delta_time_secs);
        }
    }

    pub fn update_unit_movement(&mut self, query: &mut Query<'config, '_, '_, '_>, delta_time_secs: Seconds) {
        for (_, unit) in self.units_list.iter_mut() {
            unit.update_movement(query, delta_time_secs);
        } 
    }

    // ----------------------
    // Buildings API:
    // ----------------------

    pub fn add_building(&mut self, tile: &mut Tile, building: Building<'config>) {
        let building_kind = building.kind();
        let archetype_kind = building.archetype_kind();

        let buildings = self.buildings_list_mut(archetype_kind);
        let index = buildings.add(building);

        tile.set_game_state_handle(GameStateHandle::new(index, building_kind.bits()));
    }

    pub fn remove_building(&mut self, tile: &Tile) {
        let game_state = tile.game_state_handle();
        if !game_state.is_valid() {
            eprintln!("Building tile '{}' {} should have a valid game state!",
                      tile.name(), tile.base_cell());
            return;
        }

        let list_index = game_state.index();
        let building_kind = BuildingKind::from_game_state_handle(game_state);
        let archetype_kind = building_kind.archetype_kind();
        let buildings = self.buildings_list_mut(archetype_kind);
        debug_assert!(buildings.archetype_kind() == archetype_kind);

        if !buildings.remove(list_index) {
            panic!("Failed to remove building '{}' {}! This is unexpected...",
                   tile.name(), tile.base_cell());
        }
    }

    pub fn find_building_for_tile(&self, tile: &Tile) -> Option<&Building<'config>> {
        let game_state = tile.game_state_handle();
        if game_state.is_valid() {
            let list_index = game_state.index();
            let building_kind = BuildingKind::from_game_state_handle(game_state);
            let archetype_kind = building_kind.archetype_kind();
            let buildings = self.buildings_list(archetype_kind);
            debug_assert!(buildings.archetype_kind() == archetype_kind);
            return buildings.try_get(list_index);
        }
        None
    }

    pub fn find_building_for_tile_mut(&mut self, tile: &Tile) -> Option<&mut Building<'config>> {
        let game_state = tile.game_state_handle();
        if game_state.is_valid() {
            let list_index = game_state.index();
            let building_kind = BuildingKind::from_game_state_handle(game_state);
            let archetype_kind = building_kind.archetype_kind();
            let buildings = self.buildings_list_mut(archetype_kind);
            debug_assert!(buildings.archetype_kind() == archetype_kind);
            return buildings.try_get_mut(list_index);
        }
        None
    }

    pub fn find_building_by_name(&self, name: &str, archetype_kind: BuildingArchetypeKind) -> Option<&Building<'config>> {
        self.buildings_list(archetype_kind)
            .iter()
            .find(|(_, building)| building.name() == name)
            .map(|(_, building)| building)
    }

    pub fn find_building_by_name_mut(&mut self, name: &str, archetype_kind: BuildingArchetypeKind) -> Option<&mut Building<'config>> {
        self.buildings_list_mut(archetype_kind)
            .iter_mut()
            .find(|(_, building)| building.name() == name)
            .map(|(_, building)| building)
    }

    #[inline]
    pub fn buildings_list(&self, archetype_kind: BuildingArchetypeKind) -> &BuildingList<'config> {
        &self.buildings_list[archetype_kind as usize]
    }

    #[inline]
    pub fn buildings_list_mut(&mut self, archetype_kind: BuildingArchetypeKind) -> &mut BuildingList<'config> {
        &mut self.buildings_list[archetype_kind as usize]
    }

    // ----------------------
    // Buildings debug:
    // ----------------------

    pub fn draw_building_debug_popups(&mut self,
                                      query: &mut Query<'config, '_, '_, '_>,
                                      ui_sys: &UiSystem,
                                      transform: &WorldToScreenTransform,
                                      visible_range: CellRange,
                                      delta_time_secs: Seconds,
                                      show_popup_messages: bool) {

        for buildings in &mut self.buildings_list {
            let list_archetype = buildings.archetype_kind();
            for (_, building) in buildings.iter_mut() {
                debug_assert!(building.archetype_kind() == list_archetype);
                building.draw_debug_popups(
                    query,
                    ui_sys,
                    transform,
                    visible_range,
                    delta_time_secs,
                    show_popup_messages);
            };
        }
    }

    pub fn draw_building_debug_ui(&mut self,
                                  query: &mut Query<'config, '_, '_, '_>,
                                  ui_sys: &UiSystem,
                                  selected_cell: Cell) {

        let tile = match query.tile_map.find_tile(selected_cell,
                                                  TileMapLayerKind::Objects,
                                                  TileKind::Building) {
            Some(tile) => tile,
            None => return,
        };

        if let Some(building) = self.find_building_for_tile_mut(tile) {
            building.draw_debug_ui(query, ui_sys);
        }
    }

    // ----------------------
    // Units API:
    // ----------------------

    // TODO: Store anything more useful in the GameStateHandle `kind` field for Units?
    const UNIT_GAME_STATE_KIND: u32 = 0xABCD1234;

    pub fn add_unit(&mut self, tile: &mut Tile, unit: Unit<'config>) {
        let index = self.units_list.add(unit);
        tile.set_game_state_handle(GameStateHandle::new(index, Self::UNIT_GAME_STATE_KIND));
    }

    pub fn remove_unit(&mut self, tile: &Tile) {
        let game_state = tile.game_state_handle();
        if !game_state.is_valid() {
            eprintln!("Unit tile '{}' {} should have a valid game state!",
                      tile.name(), tile.base_cell());
            return;
        }

        debug_assert!(game_state.kind() == Self::UNIT_GAME_STATE_KIND);
        let list_index = game_state.index();
        if !self.units_list.remove(list_index) {
            panic!("Failed to remove unit '{}' {}! This is unexpected...",
                   tile.name(), tile.base_cell());
        }
    }

    pub fn find_unit_for_tile(&self, tile: &Tile) -> Option<&Unit<'config>> {
        let game_state = tile.game_state_handle();
        if game_state.is_valid() {
            debug_assert!(game_state.kind() == Self::UNIT_GAME_STATE_KIND);
            let list_index = game_state.index();
            return self.units_list.try_get(list_index);
        }
        None
    }

    pub fn find_unit_for_tile_mut(&mut self, tile: &Tile) -> Option<&mut Unit<'config>> {
        let game_state = tile.game_state_handle();
        if game_state.is_valid() {
            debug_assert!(game_state.kind() == Self::UNIT_GAME_STATE_KIND);
            let list_index = game_state.index();
            return self.units_list.try_get_mut(list_index);
        }
        None
    }

    pub fn find_unit_by_name(&self, name: &str) -> Option<&Unit<'config>> {
        self.units_list
            .iter()
            .find(|(_, unit)| unit.name() == name)
            .map(|(_, unit)| unit)
    }

    pub fn find_unit_by_name_mut(&mut self, name: &str) -> Option<&mut Unit<'config>> {
        self.units_list
            .iter_mut()
            .find(|(_, unit)| unit.name() == name)
            .map(|(_, unit)| unit)
    }

    // ----------------------
    // Units debug:
    // ----------------------

    pub fn draw_unit_debug_popups(&mut self,
                                  query: &mut Query<'config, '_, '_, '_>,
                                  ui_sys: &UiSystem,
                                  transform: &WorldToScreenTransform,
                                  visible_range: CellRange,
                                  delta_time_secs: Seconds,
                                  show_popup_messages: bool) {

        for (_, unit) in self.units_list.iter_mut() {
            unit.draw_debug_popups(
                query,
                ui_sys,
                transform,
                visible_range,
                delta_time_secs,
                show_popup_messages);
        };
    }

    pub fn draw_unit_debug_ui(&mut self,
                              query: &mut Query<'config, '_, '_, '_>,
                              ui_sys: &UiSystem,
                              selected_cell: Cell) {

        let tile = match query.tile_map.find_tile(selected_cell,
                                                  TileMapLayerKind::Objects,
                                                  TileKind::Unit) {
            Some(tile) => tile,
            None => return,
        };

        if let Some(unit) = self.find_unit_for_tile_mut(tile) {
            unit.draw_debug_ui(query, ui_sys);
        }
    }
}

impl Default for World<'_> {
    fn default() -> Self {
        Self::new()
    }
}

// ----------------------------------------------
// BuildingList
// ----------------------------------------------

pub struct BuildingList<'config> {
    archetype_kind: BuildingArchetypeKind,
    buildings: Slab<Building<'config>>, // All share the same archetype.
}

impl<'config> BuildingList<'config> {
    #[inline]
    pub fn new(archetype_kind: BuildingArchetypeKind) -> Self {
        Self {
            archetype_kind,
            buildings: Slab::new(),
        }
    }

    #[inline]
    pub fn iter(&self) -> slab::Iter<'_, Building<'config>> {
        self.buildings.iter()
    }

    #[inline]
    pub fn iter_mut(&mut self) -> slab::IterMut<'_, Building<'config>> {
        self.buildings.iter_mut()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.buildings.clear();
    }

    #[inline]
    pub fn archetype_kind(&self) -> BuildingArchetypeKind {
        self.archetype_kind
    }

    #[inline]
    pub fn try_get(&self, index: usize) -> Option<&Building<'config>> {
        self.buildings.get(index)
    }

    #[inline]
    pub fn try_get_mut(&mut self, index: usize) -> Option<&mut Building<'config>> {
        self.buildings.get_mut(index)
    }

    #[inline]
    pub fn add(&mut self, building: Building<'config>) -> usize {
        debug_assert!(building.archetype_kind() == self.archetype_kind);
        self.buildings.insert(building)
    }

    #[inline]
    pub fn remove(&mut self, index: usize) -> bool {
        if self.buildings.try_remove(index).is_none() {
            return false;
        }
        true
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
        Self { units: Slab::new() }
    }

    #[inline]
    pub fn iter(&self) -> slab::Iter<'_, Unit<'config>> {
        self.units.iter()
    }

    #[inline]
    pub fn iter_mut(&mut self) -> slab::IterMut<'_, Unit<'config>> {
        self.units.iter_mut()
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
}
