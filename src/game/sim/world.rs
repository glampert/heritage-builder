use arrayvec::ArrayVec;
use strum::IntoEnumIterator;

use crate::{
    tile::map::{
        Tile,
        GameStateHandle
    },
    game::building::{
        Building,
        BuildingKind,
        BuildingList,
        BuildingArchetypeKind,
        BUILDING_ARCHETYPE_COUNT
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
    building_lists: ArrayVec<BuildingList<'config>, BUILDING_ARCHETYPE_COUNT>,
}

impl<'config> World<'config> {
    pub fn new() -> Self {
        let mut world = Self {
            building_lists: ArrayVec::new(),
        };

        // Populate archetype lists:
        for archetype_kind in BuildingArchetypeKind::iter() {
            world.building_lists.push(BuildingList::new(archetype_kind));
        }

        world
    }

    pub fn update(&mut self, query: &mut Query, delta_time_secs: f32) {
        for list in &mut self.building_lists {
            list.update(query, delta_time_secs);
        }
    }

    pub fn add_building(&mut self, tile: &mut Tile, building: Building<'config>) {
        let building_kind = building.kind();
        let archetype_kind = building.archetype_kind();

        let list = self.building_list_mut(archetype_kind);
        let index = list.add(building);

        tile.set_game_state_handle(GameStateHandle::new(index, building_kind.bits()));
    }

    pub fn remove_building(&mut self, tile: &Tile) {
        let game_state = tile.game_state_handle();
        if !game_state.is_valid() {
            panic!("Building tile '{}' [{},{}] must have a valid game state!",
                   tile.name(), tile.base_cell().x, tile.base_cell().y);
        }

        let list_index = game_state.index();
        let building_kind = BuildingKind::from_game_state_handle(game_state);
        let archetype_kind = building_kind.archetype_kind();
        let list = self.building_list_mut(archetype_kind);

        if !list.remove(list_index, archetype_kind) {
            panic!("Failed to remove building '{}' [{},{}]! This is unexpected...",
                   tile.name(), tile.base_cell().x, tile.base_cell().y);
        }
    }

    #[inline]
    pub fn building_list(&self, archetype_kind: BuildingArchetypeKind) -> &BuildingList<'config> {
        &self.building_lists[archetype_kind as usize]
    }

    #[inline]
    pub fn building_list_mut(&mut self, archetype_kind: BuildingArchetypeKind) -> &mut BuildingList<'config> {
        &mut self.building_lists[archetype_kind as usize]
    }

    #[inline]
    pub fn find_building_for_tile(&self, tile: &Tile) -> Option<&Building<'config>> {
        let game_state = tile.game_state_handle();
        if game_state.is_valid() {
            let list_index = game_state.index();
            let building_kind = BuildingKind::from_game_state_handle(game_state);
            let archetype_kind = building_kind.archetype_kind();
            let list = self.building_list(archetype_kind);
            return list.try_get(list_index, archetype_kind);
        }
        None
    }
}
