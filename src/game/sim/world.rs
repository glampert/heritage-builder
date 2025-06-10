use arrayvec::ArrayVec;
use strum::{IntoEnumIterator};

use crate::{
    tile::{
        map::{Tile, TileMap, GameStateHandle}
    },
    game::building::{
        self,
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
pub struct World {
    // One list per archetype.
    building_lists: ArrayVec<BuildingList, BUILDING_ARCHETYPE_COUNT>,
}

impl World {
    pub fn new(tile_map: &mut TileMap) -> Self {
        let mut world = Self {
            building_lists: ArrayVec::new(),
        };

        for archetype_kind in BuildingArchetypeKind::iter() {
            world.building_lists.push(BuildingList::new(archetype_kind));
        }

        tile_map.for_each_building_tile_mut(|tile| {
            if tile.name() == "well" {
                world.add_building(tile, building::create::new_well(tile.cell));
            } else if tile.name() == "market" {
                world.add_building(tile, building::create::new_market(tile.cell));
            } else if tile.name() == "house_0" {
                world.add_building(tile, building::create::new_household(tile.cell));
            } else {
                panic!("Unknown building tile!")
            };
        });

        world
    }

    pub fn update(&mut self, query: &mut Query, delta_time_secs: f32) {
        for list in &mut self.building_lists {
            list.update(query, delta_time_secs);
        }
    }

    fn add_building(&mut self, tile: &mut Tile, building: Building) {
        let building_kind = building.kind();
        let archetype_kind = building.archetype_kind();

        let list = self.building_list_mut(archetype_kind);
        let index = list.add(building);

        tile.game_state = GameStateHandle::new(index, building_kind.into());
    }

    #[inline]
    fn building_list(&self, kind: BuildingArchetypeKind) -> &BuildingList {
        &self.building_lists[kind as usize]
    }

    #[inline]
    fn building_list_mut(&mut self, kind: BuildingArchetypeKind) -> &mut BuildingList {
        &mut self.building_lists[kind as usize]
    }
}
