use strum::EnumCount;
use strum_macros::EnumCount;
use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    imgui_ui::UiSystem,
    utils::{
        Color,
        hash::StringHash
    },
    tile::{
        map::TileMapLayerKind,
        sets::{TileDef, OBJECTS_BUILDINGS_CATEGORY}
    },
    game::sim::resources::{
        ConsumerGoodsList,
        ConsumerGoodsStock,
        ServicesList
    }
};

use super::{
    BuildingBehavior,
    BuildingUpdateContext,
    config::BuildingConfigs
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

const HOUSE_LEVEL_COUNT: usize = HouseLevel::COUNT;
const DEFAULT_HOUSE_UPGRADE_FREQUENCY_SECS: f32 = 10.0;

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
}

impl<'config> BuildingBehavior<'config> for HouseState<'config> {
    fn update(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>, delta_time_secs: f32) {
        self.upgrade_state.update(update_ctx, &self.goods_stock, delta_time_secs);
    }

    fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        let draw_level_requirements = |label: &str, level_requirements: &HouseLevelRequirements<'config>| {
            ui.separator();
            ui.text(label);

            ui.text(format!("  Goods avail.....: {} out of {}",
                level_requirements.goods_available.len(),
                level_requirements.level_config.goods_required.len()));
            ui.text(format!("  Services avail..: {} out of {}",
                level_requirements.services_available.len(),
                level_requirements.level_config.services_required.len()));

            if ui.collapsing_header(format!("Goods##_{}_goods", label), imgui::TreeNodeFlags::empty()) {
                ui.text("In stock:");
                for good in self.goods_stock.iter() {
                    ui.text(format!("  {:?}: {}", good.kind, good.count));
                }
                ui.text("Required:");
                if level_requirements.level_config.goods_required.is_empty() {
                    ui.text("  <none>");
                }
                for good in level_requirements.level_config.goods_required.iter() {
                    ui.text(format!("  {:?}", good));
                }
            }

            if ui.collapsing_header(format!("Services##_{}_services", label), imgui::TreeNodeFlags::empty()) {
                ui.text("Available:");
                if level_requirements.services_available.is_empty() {
                    ui.text("  <none>");
                }
                for service in level_requirements.services_available.iter() {
                    ui.text(format!("  {}", service));
                }

                ui.text("Required:");
                if level_requirements.level_config.services_required.is_empty() {
                    ui.text("  <none>");
                }
                for service in level_requirements.level_config.services_required.iter() {
                    ui.text(format!("  {}", service));
                }
            }
        };

        let upgrade_state = &self.upgrade_state;
        ui.text(format!("Level.............: {:?}", upgrade_state.level));

        ui.text("Upgrade:");
        ui.text(format!("  Upgrade freg....: {:.2}s", upgrade_state.upgrade_frequency_secs));
        ui.text(format!("  Time since last.: {:.2}s", upgrade_state.time_since_last_upgrade_secs));

        ui.text("  Has room........:");
        if upgrade_state.has_room_to_upgrade {
            ui.same_line();
            ui.text("true");
        } else {
            ui.same_line();
            ui.text_colored(Color::red().to_array(), "false");
        }

        draw_level_requirements(
            &format!("Curr level reqs ({:?}):", upgrade_state.level),
            &upgrade_state.curr_level_requirements);

        if !upgrade_state.level.is_max() {
            draw_level_requirements(
                &format!("Next level reqs ({:?}):", upgrade_state.level.next()),
                &upgrade_state.next_level_requirements);
        }
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
    Level2,
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
    pub tile_def_name_hash: StringHash,

    pub max_residents: u32,
    pub tax_generated: u32,

    // Types of services provided by these kinds of buildings for the house level to be obtained and maintained.
    pub services_required: ServicesList,

    // Kinds of goods required for the house level to be obtained and maintained.
    pub goods_required: ConsumerGoodsList,
}

// ----------------------------------------------
// HouseLevelRequirements
// ----------------------------------------------

struct HouseLevelRequirements<'config> {
    level_config: &'config HouseLevelConfig,
    services_available: ServicesList,   // From the level requirements, how many we have access to.
    goods_available: ConsumerGoodsList, // From the level requirements, how many we have in stock.
}

impl<'config> HouseLevelRequirements<'config> {
    #[inline]
    fn has_all_required_services(&self) -> bool {
        self.services_available.len() >= self.level_config.services_required.len()
    }

    #[inline]
    fn has_all_required_consumer_goods(&self) -> bool {
        self.goods_available.len() >= self.level_config.goods_required.len()
    }

    fn update(&mut self,
              update_ctx: &BuildingUpdateContext<'config, '_, '_, '_, '_>,
              goods_stock: &ConsumerGoodsStock) {

        let config = self.level_config;

        self.services_available.clear();
        for service in config.services_required.iter() {
            // Break down services that are ORed together.
            for single_service in *service {
                if update_ctx.has_access_to_service(single_service) {
                    self.services_available.add(single_service);
                }
            }
        }

        self.goods_available.clear();
        for good in config.goods_required.iter() {
            if goods_stock.has(*good) {
                self.goods_available.add(*good);
            }
        }
    }
}

// ----------------------------------------------
// HouseUpgradeState
// ----------------------------------------------

struct HouseUpgradeState<'config> {
    level: HouseLevel,

    curr_level_requirements: HouseLevelRequirements<'config>,
    next_level_requirements: HouseLevelRequirements<'config>,

    upgrade_frequency_secs: f32,
    time_since_last_upgrade_secs: f32,
    has_room_to_upgrade: bool,
}

impl<'config> HouseUpgradeState<'config> {
    fn new(level: HouseLevel, configs: &'config BuildingConfigs) -> Self {
        Self {
            level: level,
            curr_level_requirements: HouseLevelRequirements {
                level_config: configs.find_house_level(level),
                services_available: ServicesList::new(),
                goods_available: ConsumerGoodsList::new(),
            },
            next_level_requirements: HouseLevelRequirements {
                level_config: configs.find_house_level(level.next()),
                services_available: ServicesList::new(),
                goods_available: ConsumerGoodsList::new(),
            },
            upgrade_frequency_secs: DEFAULT_HOUSE_UPGRADE_FREQUENCY_SECS,
            time_since_last_upgrade_secs: 0.0,
            has_room_to_upgrade: true,
        }
    }

    fn update(&mut self,
              update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>,
              goods_stock: &ConsumerGoodsStock,
              delta_time_secs: f32) {

        if self.can_upgrade(update_ctx, goods_stock) {
            self.try_upgrade(update_ctx);
        } else if self.can_downgrade(update_ctx, goods_stock) {
            self.try_downgrade(update_ctx);
        } else {
            self.time_since_last_upgrade_secs += delta_time_secs;
        }
    }

    fn can_upgrade(&mut self,
                   update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>,
                   goods_stock: &ConsumerGoodsStock) -> bool {
        if self.level.is_max() {
            return false;
        }

        if self.time_since_last_upgrade_secs < self.upgrade_frequency_secs {
            return false;
        }

        self.next_level_requirements.update(update_ctx, goods_stock);

        // Upgrade if we have the required goods and services for the next level.
        self.next_level_requirements.has_all_required_services() &&
        self.next_level_requirements.has_all_required_consumer_goods()
    }

    fn can_downgrade(&mut self,
                     update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>,
                     goods_stock: &ConsumerGoodsStock) -> bool {
        if self.level.is_min() {
            return false;
        }

        if self.time_since_last_upgrade_secs < self.upgrade_frequency_secs {
            return false;
        }

        self.curr_level_requirements.update(update_ctx, goods_stock);

        // Downgrade if we don't have the required goods and services for the current level.
        !self.curr_level_requirements.has_all_required_services() ||
        !self.curr_level_requirements.has_all_required_consumer_goods()
    }

    // TODO: Merge neighboring houses into larger ones when upgrading.
    fn try_upgrade(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>) {
        let mut tile_placed_successfully = false;

        let next_level = self.level.next();
        let next_level_config = update_ctx.configs.find_house_level(next_level);

        if let Some(new_tile_def) = 
            update_ctx.find_tile_def(OBJECTS_BUILDINGS_CATEGORY.hash, next_level_config.tile_def_name_hash) {

            // Try placing new. Might fail if there isn't enough space.
            if Self::try_replace_tile(update_ctx, new_tile_def) {
                self.level.upgrade();
                debug_assert!(self.level == next_level);

                self.curr_level_requirements.level_config = next_level_config;
                if !next_level.is_max() {
                    self.next_level_requirements.level_config = update_ctx.configs.find_house_level(next_level.next());
                }

                // Set a random variation for the new building tile:
                update_ctx.set_random_building_variation();

                tile_placed_successfully = true;
                println!("{update_ctx}: upgraded to {:?}.", self.level);
            }
        }

        if !tile_placed_successfully {
            println!("{update_ctx}: Failed to place new tile for upgrade. Building cannot upgrade.");
        }

        self.has_room_to_upgrade = tile_placed_successfully;
        self.time_since_last_upgrade_secs = 0.0;
    }

    fn try_downgrade(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>) {
        let mut tile_placed_successfully = false;

        let prev_level = self.level.prev();
        let prev_level_config = update_ctx.configs.find_house_level(prev_level);

        if let Some(new_tile_def) = 
            update_ctx.find_tile_def(OBJECTS_BUILDINGS_CATEGORY.hash, prev_level_config.tile_def_name_hash) {

            // Try placing new. Should always be able to place a lower-tier (smaller or same size) house tile.
            if Self::try_replace_tile(update_ctx, new_tile_def) {
                self.level.downgrade();
                debug_assert!(self.level == prev_level);

                self.curr_level_requirements.level_config = prev_level_config;
                self.next_level_requirements.level_config = update_ctx.configs.find_house_level(prev_level.next());

                // Set a random variation for the new building:
                update_ctx.set_random_building_variation();

                tile_placed_successfully = true;
                println!("{update_ctx}: downgraded to {:?}.", self.level);
            }
        }

        if !tile_placed_successfully {
            eprintln!("{update_ctx}: Failed to place new tile for downgrade. Building cannot downgrade.");
        }

        self.time_since_last_upgrade_secs = 0.0;
    }

    fn try_replace_tile<'tile_sets>(update_ctx: &mut BuildingUpdateContext<'_, '_, '_, '_, 'tile_sets>,
                                    tile_def_to_place: &'tile_sets TileDef) -> bool {

        // Replaces the give tile if the placement is valid,
        // fails and leaves the map unchanged otherwise.

        // First check if we have space to place this tile.
        let cell_range = tile_def_to_place.calc_footprint_cells(update_ctx.map_cells.start);
        for cell in &cell_range {
            if let Some(tile) =
                update_ctx.query.tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects) {
                let is_self = tile.base_cell() == update_ctx.map_cells.start;
                if !is_self {
                    // Cannot expand here.
                    return false;
                }
            }
        }

        // We'll need to restore this to the new tile.
        let prev_tile_game_state = {
            let prev_tile = update_ctx.find_tile();
            let game_state = prev_tile.game_state_handle();
            debug_assert!(game_state.is_valid(), "Building tile doesn't have a valid associated GameStateHandle!");
            game_state
        };

        // Now we must clear the previous tile.
        if !update_ctx.query.tile_map.try_clear_tile_from_layer(
            update_ctx.map_cells.start, TileMapLayerKind::Objects) {
            panic!("Failed to clear previous tile! This is unexpected...");
        }

        // And place the new one.
        let place_result = update_ctx.query.tile_map.try_place_tile_in_layer(
            update_ctx.map_cells.start,
            TileMapLayerKind::Objects,
            tile_def_to_place);

        if place_result.is_none() {
            panic!("Failed to place new tile! This is unexpected...");
        }

        // Update game state handle:
        let new_tile = place_result.unwrap();
        new_tile.set_game_state_handle(prev_tile_game_state);

        true
    }
}
