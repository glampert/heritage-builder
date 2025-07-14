use strum::EnumCount;
use strum_macros::EnumCount;
use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    declare_building_debug_options,
    imgui_ui::UiSystem,
    utils::{
        Color,
        Seconds,
        hash::StringHash
    },
    tile::{
        sets::TileDef,
        map::TileMapLayerKind
    },
    game::{
        building::BuildingKind,
        sim::{
            UpdateTimer,
            resources::{
                ConsumerGoodsList,
                ConsumerGoodsStock,
                ServicesList
            }
        }
    }
};

use super::{
    BuildingBehavior,
    BuildingUpdateContext,
    config::BuildingConfigs
};

// ----------------------------------------------
// TODO List
// ----------------------------------------------

// - Implement house population & tax income.
//
// - Merge neighboring houses into larger ones when upgrading.
//   Also have to update is_upgrade_available() to handle this!
//
// - Goods should have individual rates of consumption. Some
//   kinds of goods are consumed slower/faster than others.
//
// - Goods consumption rate should be expressed in units per day.
// - The house occupancy should also influence the goods consumption rate.
//
// - Allow houses to stock up on more than 1 unit of each kind of goods?
//   Could allow stocking up to a maximum number of units.

// ----------------------------------------------
// Constants
// ----------------------------------------------

const STOCK_UPDATE_FREQUENCY_SECS: Seconds = 20.0;
const UPGRADE_UPDATE_FREQUENCY_SECS: Seconds = 10.0;

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
// HouseDebug
// ----------------------------------------------

declare_building_debug_options!(
    HouseDebug,

    // Stops any goods from being consumed.
    // Also stops refreshing goods stock from a market.
    freeze_stock_update: bool,

    // Stops any upgrade/downgrade when true.
    freeze_upgrade_update: bool,
);

// ----------------------------------------------
// HouseBuilding
// ----------------------------------------------

pub struct HouseBuilding<'config> {
    stock_update_timer: UpdateTimer,
    upgrade_update_timer: UpdateTimer,

    upgrade_state: HouseUpgradeState<'config>,
    goods_stock: ConsumerGoodsStock,

    debug: HouseDebug,
}

impl<'config> BuildingBehavior<'config> for HouseBuilding<'config> {
    fn update(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>, delta_time_secs: Seconds) {
        // Update house states:
        if self.stock_update_timer.tick(delta_time_secs).should_update() {
            if !self.debug.freeze_stock_update {
                self.stock_update(update_ctx);
            }
        }

        if self.upgrade_update_timer.tick(delta_time_secs).should_update() {
            if !self.debug.freeze_upgrade_update {
                self.upgrade_update(update_ctx);
            }
        }
    }

    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        self.draw_debug_ui_level_config(ui_sys);
        self.draw_debug_ui_upgrade_state(ui_sys);
    }
}

impl<'config> HouseBuilding<'config> {
    pub fn new(level: HouseLevel, configs: &'config BuildingConfigs) -> Self {
        Self {
            stock_update_timer: UpdateTimer::new(STOCK_UPDATE_FREQUENCY_SECS),
            upgrade_update_timer: UpdateTimer::new(UPGRADE_UPDATE_FREQUENCY_SECS),
            upgrade_state: HouseUpgradeState::new(level, configs),
            goods_stock: ConsumerGoodsStock::accept_all_items(),
            debug: HouseDebug::default(),
        }
    }

    fn is_upgrade_available(&self, update_ctx: &BuildingUpdateContext) -> bool {
        if self.debug.freeze_upgrade_update {
            return false;
        }
        self.upgrade_state.is_upgrade_available(update_ctx)
    }

    fn stock_update(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>) {
        // Consume goods from the stock periodically and shop for more as needed.

        let curr_level_goods_required =
            &self.upgrade_state.curr_level_requirements.level_config.goods_required;

        let next_level_goods_required =
            &self.upgrade_state.next_level_requirements.level_config.goods_required;

        if !curr_level_goods_required.is_empty() || !next_level_goods_required.is_empty() {
            // Consume one of each goods this level uses.
            curr_level_goods_required.for_each(|good| {
                if self.goods_stock.remove(good).is_some() {
                    // We consumed one, done.
                    // E.g.: goods = Meat|Fish, consume one of either.
                    return false;
                }
                true
            });

            let upgrade_available = self.is_upgrade_available(update_ctx);

            // Go shopping:
            if let Some(market) =
                update_ctx.find_nearest_service(BuildingKind::Market) {

                // Shop for goods needed for this level.
                let all_or_nothing = false;
                market.shop(&mut self.goods_stock, &curr_level_goods_required, all_or_nothing);

                // And if we have space to upgrade, shop for goods needed for the next level, so we can advance.
                // But only take any if we have the whole shopping list. No point in shopping partially since we
                // wouldn't be able to upgrade and would wasted those goods.
                if upgrade_available {
                    let mut next_level_shopping_list = ConsumerGoodsList::empty();

                    // We've already shopped for goods in the current level list,
                    // so take only the ones that are exclusive to the next level.
                    for &goods in next_level_goods_required.iter() {
                        if !self.goods_stock.has(goods) {
                            next_level_shopping_list.add(goods);
                        }
                    }

                    let all_or_nothing = true;
                    market.shop(&mut self.goods_stock, &next_level_shopping_list, all_or_nothing);
                }
            }
        }
    }

    fn upgrade_update(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>) {
        // Attempt to upgrade or downgrade based on service and goods availability.
        self.upgrade_state.update(update_ctx, &self.goods_stock);
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
        self.level_config.services_required.for_each(|service| {
            if update_ctx.has_access_to_service(service) {
                self.services_available.add(service);
            }
            true
        });

        self.goods_available.clear();
        self.level_config.goods_required.for_each(|good| {
            if goods_stock.has(good) {
                self.goods_available.add(good);
            }
            true
        });
    }
}

// ----------------------------------------------
// HouseUpgradeState
// ----------------------------------------------

struct HouseUpgradeState<'config> {
    level: HouseLevel,
    curr_level_requirements: HouseLevelRequirements<'config>,
    next_level_requirements: HouseLevelRequirements<'config>,
    has_room_to_upgrade: bool, // Result of last attempt to expand the house.
}

impl<'config> HouseUpgradeState<'config> {
    fn new(level: HouseLevel, configs: &'config BuildingConfigs) -> Self {
        Self {
            level: level,
            curr_level_requirements: HouseLevelRequirements {
                level_config: configs.find_house_level(level),
                services_available: ServicesList::empty(),
                goods_available: ConsumerGoodsList::empty(),
            },
            next_level_requirements: HouseLevelRequirements {
                level_config: configs.find_house_level(level.next()),
                services_available: ServicesList::empty(),
                goods_available: ConsumerGoodsList::empty(),
            },
            has_room_to_upgrade: true,
        }
    }

    fn update(&mut self,
              update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>,
              goods_stock: &ConsumerGoodsStock) {

        if self.can_upgrade(update_ctx, goods_stock) {
            self.try_upgrade(update_ctx);
        } else if self.can_downgrade(update_ctx, goods_stock) {
            self.try_downgrade(update_ctx);
        }
    }

    fn can_upgrade(&mut self,
                   update_ctx: &BuildingUpdateContext<'config, '_, '_, '_, '_>,
                   goods_stock: &ConsumerGoodsStock) -> bool {
        if self.level.is_max() {
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
        if self.level.is_min() {
            return false;
        }

        self.curr_level_requirements.update(update_ctx, goods_stock);

        // Downgrade if we don't have the required goods and services for the current level.
        !self.curr_level_requirements.has_all_required_services() ||
        !self.curr_level_requirements.has_all_required_consumer_goods()
    }

    fn try_upgrade(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>) {
        let mut tile_placed_successfully = false;

        let next_level = self.level.next();
        let next_level_config = update_ctx.configs.find_house_level(next_level);

        if let Some(new_tile_def) = update_ctx.find_tile_def(next_level_config.tile_def_name_hash) {
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
    }

    fn try_downgrade(&mut self, update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>) {
        let mut tile_placed_successfully = false;

        let prev_level = self.level.prev();
        let prev_level_config = update_ctx.configs.find_house_level(prev_level);

        if let Some(new_tile_def) = update_ctx.find_tile_def(prev_level_config.tile_def_name_hash) {
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

    // Check if we can increment the level and if there's enough space to expand the house.
    fn is_upgrade_available(&self, update_ctx: &BuildingUpdateContext) -> bool {
        if self.level.is_max() {
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
            if let Some(tile) = update_ctx.query.tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects) {
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

// ----------------------------------------------
// Debug UI
// ----------------------------------------------

impl HouseLevelConfig {
    fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        ui.text(format!("Tile def name.....: '{}'", self.tile_def_name));
        ui.text(format!("Max residents.....: {}", self.max_residents));
        ui.text(format!("Tax generated.....: {}", self.tax_generated));
        ui.text(format!("Services required.: {}", self.services_required));
        ui.text(format!("Goods required....: {}", self.goods_required));
    }
}

impl<'config> HouseBuilding<'config> {
    fn draw_debug_ui_level_config(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header(format!("Config ({:?})##_building_config", self.upgrade_state.level), imgui::TreeNodeFlags::empty()) {
            self.upgrade_state.curr_level_requirements.level_config.draw_debug_ui(ui_sys);
        }
    }

    fn draw_debug_ui_upgrade_state(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header(format!("Upgrade##_building_upgrade"), imgui::TreeNodeFlags::empty()) {
            self.draw_debug_ui_upgrade_state_internal(ui_sys);
        }
    }

    fn draw_debug_ui_upgrade_state_internal(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        let draw_level_requirements = 
            |label: &str, level_requirements: &mut HouseLevelRequirements<'config>, imgui_id: u32| {

            ui.separator();
            ui.text(label);

            ui.text(format!("  Goods avail....: {} (req: {})",
                level_requirements.goods_available.len(),
                level_requirements.level_config.goods_required.len()));
            ui.text(format!("  Services avail.: {} (req: {})",
                level_requirements.services_available.len(),
                level_requirements.level_config.services_required.len()));

            if ui.collapsing_header(format!("Goods##_building_goods_{}", imgui_id), imgui::TreeNodeFlags::empty()) {
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

            if ui.collapsing_header(format!("Services##_building_services_{}", imgui_id), imgui::TreeNodeFlags::empty()) {
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

        self.debug.draw_debug_ui(ui_sys);

        let upgrade_state = &mut self.upgrade_state;
        ui.text(format!("Level...........: {:?}", upgrade_state.level));

        ui.text("Upgrade:");
        ui.text(format!("  Frequency.....: {:.2}s", self.upgrade_update_timer.frequency_secs()));
        ui.text(format!("  Time since....: {:.2}s", self.upgrade_update_timer.time_since_last_secs()));
        color_text("  Has room......:", upgrade_state.has_room_to_upgrade);
        color_text("  Has services..:", upgrade_state.next_level_requirements.has_all_required_services());
        color_text("  Has goods.....:", upgrade_state.next_level_requirements.has_all_required_consumer_goods());

        ui.text("Stock:");
        ui.text(format!("  Frequency.....: {:.2}s", self.stock_update_timer.frequency_secs()));
        ui.text(format!("  Time since....: {:.2}s", self.stock_update_timer.time_since_last_secs()));
        self.goods_stock.draw_debug_ui("Goods In Stock", ui_sys);

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
