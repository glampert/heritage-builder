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
        sets::TileDef
    },
    game::{
        building::BuildingKind,
        sim::resources::{
            ConsumerGoodsList,
            ConsumerGoodsStock,
            ServicesList
        }
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

const UPGRADE_FREQUENCY_SECS: f32 = 10.0;
const GOODS_CONSUMPTION_FREQUENCY_SECS: f32 = 20.0;

// ----------------------------------------------
// HouseBuilding
// ----------------------------------------------

pub struct HouseBuilding<'config> {
    upgrade_state: HouseUpgradeState<'config>,
    goods_stock: ConsumerGoodsStock,
    time_since_last_goods_consumed_secs: f32,

    // ----------------------
    // Debug flags:
    // ----------------------

    // Stops any goods from being consumed.
    // Also stops shopping from the closest market.
    freeze_goods_consumption: bool,
}

impl<'config> HouseBuilding<'config> {
    pub fn new(level: HouseLevel, configs: &'config BuildingConfigs) -> Self {
        Self {
            upgrade_state: HouseUpgradeState::new(level, configs),
            goods_stock: ConsumerGoodsStock::new(),
            time_since_last_goods_consumed_secs: 0.0,
            freeze_goods_consumption: false,
        }
    }

    fn update_goods_stock(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>, delta_time_secs: f32) {
        // Consume goods from the stock periodically.
        if self.time_since_last_goods_consumed_secs < GOODS_CONSUMPTION_FREQUENCY_SECS || self.freeze_goods_consumption {
            self.time_since_last_goods_consumed_secs += delta_time_secs;
            return;
        }

        let curr_level_goods_required =
            &self.upgrade_state.curr_level_requirements.level_config.goods_required;

        let next_level_goods_required =
            &self.upgrade_state.next_level_requirements.level_config.goods_required;

        if !curr_level_goods_required.is_empty() || !next_level_goods_required.is_empty() {
            // Consume one of each goods this level uses.
            for goods in curr_level_goods_required.iter() {
                // Break down goods that are ORed together.
                for wanted_good in *goods {
                    if self.goods_stock.consume(wanted_good).is_some() {
                        // We consumed one, done.
                        // E.g.: goods = Meat|Fish, consume one of either.
                        break;
                    }
                }
            }

            let upgrade_available = self.upgrade_state.is_upgrade_available(update_ctx);

            // Go shopping:
            if let Some(market) =
                update_ctx.find_nearest_service_mut(BuildingKind::Market) {

                // Shop for goods needed for this level.
                market.shop(&mut self.goods_stock, &curr_level_goods_required, false);

                // And if we have space to upgrade, shop for goods needed for the next level, so we can advance.
                // But only take any if we have the whole shopping list. No point in shopping partially since we
                // wouldn't be able to upgrade and would wasted those goods.
                if upgrade_available {
                    let mut next_level_shopping_list = ConsumerGoodsList::new();

                    // We've already shopped for goods in the current level list,
                    // so take only the ones that are exclusive to the next level.
                    for &goods in next_level_goods_required.iter() {
                        if !self.goods_stock.has(goods) {
                            next_level_shopping_list.add(goods);
                        }
                    }

                    market.shop(&mut self.goods_stock, &next_level_shopping_list, true);
                }
            }
        }

        self.time_since_last_goods_consumed_secs = 0.0;
    }
}

impl<'config> BuildingBehavior<'config> for HouseBuilding<'config> {
    fn update(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>, delta_time_secs: f32) {
        self.update_goods_stock(update_ctx, delta_time_secs);
        self.upgrade_state.update(update_ctx, &self.goods_stock, delta_time_secs);
    }

    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        if !ui.collapsing_header(format!("Upgrade##_building_upgrade"), imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let draw_level_requirements = 
            |label: &str, level_requirements: &mut HouseLevelRequirements<'config>, next_imgui_id: u32| {

            ui.separator();
            ui.text(label);

            ui.text(format!("  Goods avail....: {} (req: {})",
                level_requirements.goods_available.len(),
                level_requirements.level_config.goods_required.len()));
            ui.text(format!("  Services avail.: {} (req: {})",
                level_requirements.services_available.len(),
                level_requirements.level_config.services_required.len()));

            if ui.collapsing_header(format!("Goods##_building_goods_{}", next_imgui_id), imgui::TreeNodeFlags::empty()) {
                if !level_requirements.level_config.goods_required.is_empty() {
                    ui.text("Available:");
                    if level_requirements.goods_available.is_empty() {
                        ui.text("  <none>");
                    }
                    for good in level_requirements.goods_available.iter() {
                        ui.text(format!("  {}", good));
                    }
                }

                ui.text("Required:");
                if level_requirements.level_config.goods_required.is_empty() {
                    ui.text("  <none>");
                }
                for good in level_requirements.level_config.goods_required.iter() {
                    ui.text(format!("  {}", good));
                }
            }

            if ui.collapsing_header(format!("Services##_building_services_{}", next_imgui_id), imgui::TreeNodeFlags::empty()) {
                if !level_requirements.level_config.services_required.is_empty() {
                    ui.text("Available:");
                    if level_requirements.services_available.is_empty() {
                        ui.text("  <none>");
                    }
                    for service in level_requirements.services_available.iter() {
                        ui.text(format!("  {}", service));
                    }
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

        let color_text = |text: &str, value: bool| {
            ui.text(text);
            ui.same_line();
            if value {
                ui.text("yes");
            } else {
                ui.text_colored(Color::red().to_array(), "no");
            }
        };

        let upgrade_state = &mut self.upgrade_state;

        ui.checkbox("Force refresh level reqs", &mut upgrade_state.force_refresh_level_requirements);
        ui.checkbox("Freeze level changes", &mut upgrade_state.freeze_level_change);
        ui.checkbox("Freeze goods consumption", &mut self.freeze_goods_consumption);
        ui.text(format!("Level...........: {:?}", upgrade_state.level));

        ui.text("Upgrade:");
        ui.text(format!("  Frequency.....: {:.2}s", UPGRADE_FREQUENCY_SECS));
        ui.text(format!("  Time since....: {:.2}s", upgrade_state.time_since_last_upgrade_secs));
        color_text("  Has room......:", upgrade_state.has_room_to_upgrade);
        color_text("  Has services..:", upgrade_state.next_level_requirements.has_all_required_services());
        color_text("  Has goods.....:", upgrade_state.next_level_requirements.has_all_required_consumer_goods());

        ui.text("Goods Consumption:");
        ui.text(format!("  Frequency.....: {:.2}s", GOODS_CONSUMPTION_FREQUENCY_SECS));
        ui.text(format!("  Time since....: {:.2}s", self.time_since_last_goods_consumed_secs));

        if ui.collapsing_header("Stock##_building_stock", imgui::TreeNodeFlags::empty()) {
            for (index, good) in self.goods_stock.iter_mut().enumerate() {
                ui.input_scalar(format!("{}##_stock_item_{}", good.kind, index), &mut good.count).step(1).build();
            }
        }

        draw_level_requirements(
            &format!("Curr level reqs ({:?}):", upgrade_state.level),
            &mut upgrade_state.curr_level_requirements, 0);

        if !upgrade_state.level.is_max() {
            draw_level_requirements(
                &format!("Next level reqs ({:?}):", upgrade_state.level.next()),
                &mut upgrade_state.next_level_requirements, 1);
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
        if (curr as usize) == (HouseLevel::COUNT - 1) {
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
        if self.services_available.len() < self.level_config.services_required.len() {
            return false;
        }

        for service in self.level_config.services_required.iter() {
            if !self.services_available.has(*service) {
                return false;
            }
        }

        true
    }

    #[inline]
    fn has_all_required_consumer_goods(&self) -> bool {
        if self.goods_available.len() < self.level_config.goods_required.len() {
            return false;
        }

        for good in self.level_config.goods_required.iter() {
            if !self.goods_available.has(*good) {
                return false;
            }
        }

        true
    }

    fn update(&mut self,
              update_ctx: &BuildingUpdateContext<'config, '_, '_, '_, '_>,
              goods_stock: &ConsumerGoodsStock) {

        self.services_available.clear();
        for services in self.level_config.services_required.iter() {
            // Break down services that are ORed together.
            for wanted_service in *services {
                if update_ctx.has_access_to_service(wanted_service) {
                    self.services_available.add(wanted_service);
                }
            }
        }

        self.goods_available.clear();
        for goods in self.level_config.goods_required.iter() {
            // Break down goods that are ORed together.
            for wanted_good in *goods {
                if goods_stock.has(wanted_good) {
                    self.goods_available.add(wanted_good);
                }
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

    time_since_last_upgrade_secs: f32,
    has_room_to_upgrade: bool,

    // ----------------------
    // Debug flags:
    // ----------------------

    // Stops any upgrade/downgrade when true.
    freeze_level_change: bool,

    // Refresh HouseLevelRequirements every update() rather than based on HOUSE_UPGRADE_FREQUENCY_SECS.
    force_refresh_level_requirements: bool,
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
            time_since_last_upgrade_secs: 0.0,
            has_room_to_upgrade: true,
            // Debug flags:
            freeze_level_change: false,
            force_refresh_level_requirements: false,
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

            if self.force_refresh_level_requirements {
                self.curr_level_requirements.update(update_ctx, goods_stock);
                self.next_level_requirements.update(update_ctx, goods_stock);
            }
        }
    }

    fn can_upgrade(&mut self,
                   update_ctx: &BuildingUpdateContext<'config, '_, '_, '_, '_>,
                   goods_stock: &ConsumerGoodsStock) -> bool {
        if self.level.is_max() || self.freeze_level_change {
            return false;
        }

        if self.time_since_last_upgrade_secs < UPGRADE_FREQUENCY_SECS {
            return false;
        }

        self.next_level_requirements.update(update_ctx, goods_stock);

        // Upgrade if we have the required goods and services for the next level.
        self.next_level_requirements.has_all_required_services() &&
        self.next_level_requirements.has_all_required_consumer_goods()
    }

    fn can_downgrade(&mut self,
                     update_ctx: &BuildingUpdateContext<'config, '_, '_, '_, '_>,
                     goods_stock: &ConsumerGoodsStock) -> bool {
        if self.level.is_min() || self.freeze_level_change {
            return false;
        }

        if self.time_since_last_upgrade_secs < UPGRADE_FREQUENCY_SECS {
            return false;
        }

        self.curr_level_requirements.update(update_ctx, goods_stock);

        // Downgrade if we don't have the required goods and services for the current level.
        !self.curr_level_requirements.has_all_required_services() ||
        !self.curr_level_requirements.has_all_required_consumer_goods()
    }

    // TODO: Merge neighboring houses into larger ones when upgrading.
    // Also have to update is_upgrade_available() to handle this!

    fn try_upgrade(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>) {
        let mut tile_placed_successfully = false;

        let next_level = self.level.next();
        let next_level_config = update_ctx.configs.find_house_level(next_level);

        if let Some(new_tile_def) = 
            update_ctx.find_tile_def(next_level_config.tile_def_name_hash) {

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
            update_ctx.find_tile_def(prev_level_config.tile_def_name_hash) {

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

    fn is_upgrade_available(&self, update_ctx: &BuildingUpdateContext) -> bool {
        if self.level.is_max() || self.freeze_level_change {
            return false;
        }

        let next_level = self.level.next();
        let next_level_config = update_ctx.configs.find_house_level(next_level);

        let result = update_ctx.find_tile_def(next_level_config.tile_def_name_hash);
        if result.is_none() {
            return false;
        }

        let tile_def = result.unwrap();
        let cell_range = tile_def.calc_footprint_cells(update_ctx.map_cells.start);

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

        true
    }
}
