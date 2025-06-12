use arrayvec::ArrayVec;
use strum::IntoEnumIterator;

use crate::{
    tile::map::{
        Tile,
        GameStateHandle
    },
    game::building::{
        Building,
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
    // One list per archetype.
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

        tile.game_state = GameStateHandle::new(index, building_kind.into());
    }

    #[inline]
    pub fn building_list(&self, kind: BuildingArchetypeKind) -> &BuildingList {
        &self.building_lists[kind as usize]
    }

    #[inline]
    pub fn building_list_mut(&mut self, kind: BuildingArchetypeKind) -> &mut BuildingList<'config> {
        &mut self.building_lists[kind as usize]
    }
}
