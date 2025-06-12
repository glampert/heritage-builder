use strum::EnumCount;
use strum_macros::EnumCount;
use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    tile::{
        map::TileMapLayerKind,
        sets::TileDef 
    },
    game::sim::resources::{
        ConsumerGoodsList,
        ConsumerGoodsStock,
        ServicesList
    }
};

use super::{
    BuildingUpdateContext,
    config::{
        BuildingConfigs
    }
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const HOUSE_TILE_CATEGORY_NAME: &str = "houses";
pub const HOUSE_LEVEL_COUNT: usize = HouseLevel::COUNT;
pub const DEFAULT_HOUSE_UPGRADE_FREQUENCY_SECS: f32 = 10.0;

// ----------------------------------------------
// HouseState
// ----------------------------------------------

pub struct HouseState<'config> {
    upgrade_state: HouseUpgradeState<'config>,
    goods_stock: ConsumerGoodsStock,
}

impl<'config> HouseState<'config> {
    pub fn new(level: HouseLevel, configs: &'config BuildingConfigs) -> Self {
        Self {
            upgrade_state: HouseUpgradeState::new(level, configs),
            goods_stock: ConsumerGoodsStock::new(),
        }
    }

    pub fn update(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>, delta_time_secs: f32) {
        self.upgrade_state.update(update_ctx, &self.goods_stock, delta_time_secs);
    }
}

// ----------------------------------------------
// HouseUpgradeState
// ----------------------------------------------

struct HouseUpgradeState<'config> {
    level: HouseLevel,
    level_config: &'config HouseLevelConfig,
    frequency_secs: f32,
    time_since_last_secs: f32,
    services_available: u32, // From the level requirements, how many we have access to.
    goods_available: u32,    // From the level requirements, how many we have in stock.
}

impl<'config> HouseUpgradeState<'config> {
    fn new(level: HouseLevel, configs: &'config BuildingConfigs) -> Self {
        Self {
            level: level,
            level_config: configs.find_house_level(level),
            frequency_secs: DEFAULT_HOUSE_UPGRADE_FREQUENCY_SECS,
            time_since_last_secs: 0.0,
            services_available: 0,
            goods_available: 0,
        }
    }

    fn update(&mut self,
              update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>,
              goods_stock: &ConsumerGoodsStock,
              delta_time_secs: f32) {

        self.refresh_requirements(update_ctx, goods_stock);

        if self.can_upgrade() {
            self.try_upgrade(update_ctx);
        } else if self.can_downgrade() {
            self.try_downgrade(update_ctx);
        } else {
            self.time_since_last_secs += delta_time_secs;
        }
    }

    fn refresh_requirements(&mut self,
                            update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>,
                            goods_stock: &ConsumerGoodsStock) {

        let mut services_available: u32 = 0;
        for service in self.level_config.services_required.iter() {
            if update_ctx.has_access_to_service(*service) {
                services_available += 1;
            }
        }
        self.services_available = services_available;

        let mut goods_available: u32 = 0;
        for good in self.level_config.goods_required.iter() {
            if goods_stock.has(*good) {
                goods_available += 1;
            }
        }
        self.goods_available = goods_available;
    }

    #[inline]
    fn has_all_required_services(&self) -> bool {
        (self.services_available as usize) == self.level_config.services_required.len()
    }

    #[inline]
    fn has_all_required_consumer_goods(&self) -> bool {
        (self.goods_available as usize) == self.level_config.goods_required.len()
    }

    fn can_upgrade(&self) -> bool {
        if self.level.is_max() {
            return false;
        }

        if self.time_since_last_secs < self.frequency_secs {
            return false;
        }

        self.has_all_required_services() && self.has_all_required_consumer_goods()
    }

    fn can_downgrade(&self) -> bool {
        if self.level.is_min() {
            return false;
        }

        if self.time_since_last_secs < self.frequency_secs {
            return false;
        }

        !self.has_all_required_services() || !self.has_all_required_consumer_goods()
    }

    // TODO: Merge neighboring houses into larger ones when upgrading.
    fn try_upgrade(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>) {
        let mut tile_placed_successfully = false;
        let level_config = update_ctx.configs.find_house_level(self.level.next());

        if let Some(new_tile_def) = 
            update_ctx.find_tile_def(HOUSE_TILE_CATEGORY_NAME, &level_config.tile_def_name) {

            // Try placing new. Might fail if there isn't enough space.
            if Self::try_replace_tile(update_ctx, new_tile_def) {

                self.level.upgrade();
                self.level_config = level_config;
                tile_placed_successfully = true;

                // Set a random variation for the new building:
                update_ctx.set_random_building_variation();

                println!("{}: upgraded to {:?}.", update_ctx, self.level);
            }
        }

        if !tile_placed_successfully {
            println!("{}: Failed to place new tile for upgrade. Building cannot upgrade.", update_ctx);
        }

        self.time_since_last_secs = 0.0;
    }

    fn try_downgrade(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>) {
        let mut tile_placed_successfully = false;
        let level_config = update_ctx.configs.find_house_level(self.level.prev());

        if let Some(new_tile_def) = 
            update_ctx.find_tile_def(HOUSE_TILE_CATEGORY_NAME, &level_config.tile_def_name) {

            // Try placing new. Should always be able to place a lower-tier (smaller or same size) house tile.
            if Self::try_replace_tile(update_ctx, new_tile_def) {

                self.level.downgrade();
                self.level_config = level_config;
                tile_placed_successfully = true;

                // Set a random variation for the new building:
                update_ctx.set_random_building_variation();

                println!("{}: downgraded to {:?}.", update_ctx, self.level);
            }
        }

        if !tile_placed_successfully {
            eprintln!("{}: Failed to place new tile for downgrade. Building cannot downgrade.", update_ctx);
        }

        self.time_since_last_secs = 0.0;
    }

    fn try_replace_tile<'tile_sets>(update_ctx: &mut BuildingUpdateContext<'_, '_, '_, '_, 'tile_sets>,
                                    tile_to_place: &'tile_sets TileDef) -> bool {

        // Replaces the give tile if the placement is valid,
        // fails and leaves the map unchanged otherwise.

        // First check if we have space to place this tile.
        let footprint = tile_to_place.calc_footprint_cells(update_ctx.map_cell);
        for footprint_cell in footprint {
            if footprint_cell == update_ctx.map_cell {
                continue;
            }

            if let Some(tile) =
                update_ctx.query.tile_map.try_tile_from_layer(footprint_cell, TileMapLayerKind::Buildings) {
                if tile.is_building() || tile.is_blocker() {
                    // Cannot expand here.
                    return false;
                }
            }
        }

        // Now we must clear the previous tile.
        if !update_ctx.query.tile_map.try_place_tile_in_layer(
            update_ctx.map_cell, TileMapLayerKind::Buildings, TileDef::empty()) {
            eprintln!("Failed to clear previous tile! This is unexpected...");
            return false;
        }

        // And place the new one.
        if !update_ctx.query.tile_map.try_place_tile_in_layer(
            update_ctx.map_cell, TileMapLayerKind::Buildings, tile_to_place) {
            eprintln!("Failed to place new tile! This is unexpected...");
            return false;
        }

        true
    }
}

// ----------------------------------------------
// HouseLevel
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, IntoPrimitive, TryFromPrimitive)]
pub enum HouseLevel {
    Level0,
    Level1,
}

impl HouseLevel {
    #[inline]
    fn is_max(self) -> bool {
        let curr: u32 = self.into();
        if (curr as usize) == (HOUSE_LEVEL_COUNT - 1) {
            true
        } else {
            false
        }
    }

    #[inline]
    fn is_min(self) -> bool {
        let curr: u32 = self.into();
        if curr == 0 {
            true
        } else {
            false
        }
    }

    #[inline]
    #[must_use]
    fn next(self) -> HouseLevel {
        let curr: u32 = self.into();
        let next = curr + 1;
        HouseLevel::try_from(next).expect("Max HouseLevel exceeded!")
    }

    #[inline]
    #[must_use]
    fn prev(self) -> HouseLevel {
        let curr: u32 = self.into();
        let next = curr - 1;
        HouseLevel::try_from(next).expect("Min HouseLevel exceeded!")
    }

    #[inline]
    fn upgrade(&mut self) {
        *self = self.next();
    }

    #[inline]
    fn downgrade(&mut self) {
        *self = self.prev();
    }
}

// ----------------------------------------------
// HouseLevelConfig
// ----------------------------------------------

pub struct HouseLevelConfig {
    pub tile_def_name: String,
    pub upgrade_frequency_secs: f32,

    pub max_residents: u32,
    pub tax_generated: u32,

    // Types of services provided by these kinds of buildings for the house level to be obtained and maintained.
    pub services_required: ServicesList,

    // Kinds of goods required for the house level to be obtained and maintained.
    pub goods_required: ConsumerGoodsList,
}
