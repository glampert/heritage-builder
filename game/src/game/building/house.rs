use strum::EnumCount;
use strum_macros::EnumCount;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use proc_macros::DrawDebugUi;

use crate::{
    game_object_debug_options,
    imgui_ui::UiSystem,
    pathfind::{Node, NodeKind as PathNodeKind},
    utils::{
        Color,
        Seconds,
        hash::StringHash,
        coords::{CellRange, WorldToScreenTransform}
    },
    tile::{
        TileMapLayerKind,
        sets::TileDef
    },
    game::{
        unit::Unit,
        sim::{
            UpdateTimer,
            resources::{
                ResourceStock,
                ResourceKind,
                ResourceKinds,
                ServiceKind,
                ServiceKinds
            }
        }
    }
};

use super::{
    BuildingKind,
    BuildingBehavior,
    BuildingContext,
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
// - Resources should have individual rates of consumption. Some
//   kinds of resources are consumed slower/faster than others.
//
// - Resources consumption rate should be expressed in units per day.
// - The house occupancy should also influence the resources consumption rate.
//
// - Allow houses to stock up on more than 1 unit of each kind of resources?
//   Could allow stocking up to a maximum number of units.

// ----------------------------------------------
// HouseConfig & HouseLevelConfig
// ----------------------------------------------

pub struct HouseConfig {
    pub stock_update_frequency_secs: Seconds,
    pub upgrade_update_frequency_secs: Seconds,
}

#[derive(DrawDebugUi)]
pub struct HouseLevelConfig {
    pub name: String,
    pub tile_def_name: String,

    #[debug_ui(skip)]
    pub tile_def_name_hash: StringHash,

    pub max_residents: u32,
    pub tax_generated: u32,

    // Types of services provided by these kinds of buildings for the house level to be obtained and maintained.
    pub services_required: ServiceKinds,

    // Kinds of resources required for the house level to be obtained and maintained.
    pub resources_required: ResourceKinds,
}

// ----------------------------------------------
// HouseDebug
// ----------------------------------------------

game_object_debug_options! {
    HouseDebug,

    // Stops any resources from being consumed.
    // Also stops refreshing resources stock from a market.
    freeze_stock_update: bool,

    // Stops any upgrade/downgrade when true.
    freeze_upgrade_update: bool,
}

// ----------------------------------------------
// HouseBuilding
// ----------------------------------------------

pub struct HouseBuilding<'config> {
    stock_update_timer: UpdateTimer,
    upgrade_update_timer: UpdateTimer,

    upgrade_state: HouseUpgradeState<'config>,
    stock: ResourceStock,

    debug: HouseDebug,
}

impl<'config> BuildingBehavior<'config> for HouseBuilding<'config> {
    fn name(&self) -> &str {
        &self.upgrade_state.curr_level_config.name
    }

    fn update(&mut self, context: &BuildingContext<'config, '_, '_>) {
        let delta_time_secs = context.query.delta_time_secs();

        // Update house states:
        if self.stock_update_timer.tick(delta_time_secs).should_update() && !self.debug.freeze_stock_update() {
            self.stock_update(context);
        }

        if self.upgrade_update_timer.tick(delta_time_secs).should_update() && !self.debug.freeze_upgrade_update() {
            self.upgrade_update(context);
        }
    }

    fn visited_by(&mut self, _unit: &mut Unit, _context: &BuildingContext) {
        todo!();
    }

    fn receivable_amount(&self, _kind: ResourceKind) -> u32 {
        todo!();
    }

    fn receive_resources(&mut self, _kind: ResourceKind, _count: u32) -> u32 {
        todo!();
    }

    fn give_resources(&mut self, _kind: ResourceKind, _count: u32) -> u32 {
        todo!();
    }

    fn draw_debug_ui(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        self.draw_debug_ui_level_config(ui_sys);
        self.debug.draw_debug_ui(ui_sys);
        self.draw_debug_ui_upgrade_state(context, ui_sys);
    }

    fn draw_debug_popups(&mut self,
                         context: &BuildingContext,
                         ui_sys: &UiSystem,
                         transform: &WorldToScreenTransform,
                         visible_range: CellRange) {

        self.debug.draw_popup_messages(
            || context.find_tile(),
            ui_sys,
            transform,
            visible_range,
            context.query.delta_time_secs());
    }
}

impl<'config> HouseBuilding<'config> {
    pub fn new(level: HouseLevel, house_config: &'config HouseConfig, configs: &'config BuildingConfigs) -> Self {
        Self {
            stock_update_timer: UpdateTimer::new(house_config.stock_update_frequency_secs),
            upgrade_update_timer: UpdateTimer::new(house_config.upgrade_update_frequency_secs),
            upgrade_state: HouseUpgradeState::new(level, configs),
            stock: ResourceStock::with_accepted_kinds(ResourceKind::foods()),
            debug: HouseDebug::default(),
        }
    }

    fn is_upgrade_available(&self, context: &BuildingContext) -> bool {
        if self.debug.freeze_upgrade_update() {
            return false;
        }
        self.upgrade_state.is_upgrade_available(context)
    }

    fn stock_update(&mut self, context: &BuildingContext) {
        // Consume resources from the stock periodically and shop for more as needed.

        let curr_level_resources_required =
            &self.upgrade_state.curr_level_config.resources_required;

        let next_level_resources_required =
            &self.upgrade_state.next_level_config.resources_required;

        if !curr_level_resources_required.is_empty() || !next_level_resources_required.is_empty() {
            // Consume one of each resources this level uses.
            curr_level_resources_required.for_each(|resource| {
                if let Some(resource_consumed) = self.stock.remove(resource) {
                    // We consumed one, done.
                    // E.g.: resource = Meat|Fish, consume one of either.
                    self.debug.log_resources_lost(resource_consumed, 1);
                    return false;
                }
                true
            });

            // Go shopping:
            if let Some(building) =
                context.find_nearest_service_mut(BuildingKind::Market) {

                let market = building.as_service_mut();

                // Shop for resources needed for this level.
                let all_or_nothing = false;
                let resource_kinds_got =
                    market.shop(&mut self.stock, curr_level_resources_required, all_or_nothing);
                self.debug.log_resources_gained(resource_kinds_got, 1);

                // And if we have space to upgrade, shop for resources needed for the next level, so we can advance.
                // But only take any if we have the whole shopping list. No point in shopping partially since we
                // wouldn't be able to upgrade and would wasted those resources.
                if self.is_upgrade_available(context) {
                    let mut next_level_shopping_list = ResourceKinds::none();

                    // We've already shopped for resources in the current level list,
                    // so take only the ones that are exclusive to the next level.
                    for &resources in next_level_resources_required.iter() {
                        if !self.stock.has(resources) {
                            next_level_shopping_list.add(resources);
                        }
                    }

                    let all_or_nothing = true;
                    let resource_kinds_got =
                        market.shop(&mut self.stock, &next_level_shopping_list, all_or_nothing);
                    self.debug.log_resources_gained(resource_kinds_got, 1);
                }
            }
        }
    }

    fn upgrade_update(&mut self, context: &BuildingContext<'config, '_, '_>) {
        // Attempt to upgrade or downgrade based on services and resources availability.
        self.upgrade_state.update(context, &self.stock, &mut self.debug);
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
        (curr as usize) == (HouseLevel::COUNT - 1)
    }

    #[inline]
    fn is_min(self) -> bool {
        let curr: u32 = self.into();
        curr == 0
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
    services_available: ServiceKind,   // From the level requirements, which ones we have access to.
    resources_available: ResourceKind, // From the level requirements, which ones we have in stock.
}

impl<'config> HouseLevelRequirements<'config> {
    fn new(context: &BuildingContext,
           level_config: &'config HouseLevelConfig,
           stock: &ResourceStock) -> Self {

        let mut reqs = Self {
            level_config,
            services_available: ServiceKind::empty(),
            resources_available: ResourceKind::empty(),
        };

        level_config.services_required.for_each(|service| {
            if context.has_access_to_service(service) {
                reqs.services_available.insert(service);
            }
            true
        });

        level_config.resources_required.for_each(|resource| {
            if stock.has(resource) {
                reqs.resources_available.insert(resource);
            }
            true
        });

        reqs
    }

    #[inline]
    fn services_available_count(&self) -> usize {
        self.services_available.bits().count_ones() as usize
    }

    #[inline]
    fn resources_available_count(&self) -> usize {
        self.resources_available.bits().count_ones() as usize
    }

    #[inline]
    fn has_required_services(&self) -> bool {
        if self.services_available_count() < self.level_config.services_required.len() {
            return false;
        }

        for service in self.level_config.services_required.iter() {
            if !self.services_available.intersects(*service) {
                return false;
            }
        }
        true
    }

    #[inline]
    fn has_required_resources(&self) -> bool {
        if self.resources_available_count() < self.level_config.resources_required.len() {
            return false;
        }

        for resource in self.level_config.resources_required.iter() {
            if !self.resources_available.intersects(*resource) {
                return false;
            }
        }
        true
    }
}

// ----------------------------------------------
// HouseUpgradeState
// ----------------------------------------------

struct HouseUpgradeState<'config> {
    level: HouseLevel,
    curr_level_config: &'config HouseLevelConfig,
    next_level_config: &'config HouseLevelConfig,
    has_room_to_upgrade: bool, // Result of last attempt to expand the house.
}

impl<'config> HouseUpgradeState<'config> {
    fn new(level: HouseLevel, configs: &'config BuildingConfigs) -> Self {
        Self {
            level,
            curr_level_config: configs.find_house_level_config(level),
            next_level_config: configs.find_house_level_config(level.next()),
            has_room_to_upgrade: true,
        }
    }

    fn update(&mut self,
              context: &BuildingContext<'config, '_, '_>,
              stock: &ResourceStock,
              debug: &mut HouseDebug) {

        if self.can_upgrade(context, stock) {
            self.try_upgrade(context, debug);
        } else if self.can_downgrade(context, stock) {
            self.try_downgrade(context, debug);
        }
    }

    fn can_upgrade(&mut self,
                   context: &BuildingContext,
                   stock: &ResourceStock) -> bool {
        if self.level.is_max() {
            return false;
        }

        let next_level_requirements =
            HouseLevelRequirements::new(context, self.next_level_config, stock);

        // Upgrade if we have the required services and resources for the next level.
        next_level_requirements.has_required_services() &&
        next_level_requirements.has_required_resources()
    }

    fn can_downgrade(&mut self,
                     context: &BuildingContext,
                     stock: &ResourceStock) -> bool {
        if self.level.is_min() {
            return false;
        }

        let curr_level_requirements =
            HouseLevelRequirements::new(context, self.curr_level_config, stock);

        // Downgrade if we don't have the required services and resources for the current level.
        !curr_level_requirements.has_required_services() ||
        !curr_level_requirements.has_required_resources()
    }

    fn try_upgrade(&mut self, context: &BuildingContext<'config, '_, '_>, debug: &mut HouseDebug) {
        let mut tile_placed_successfully = false;

        let next_level = self.level.next();
        let next_level_config = context.query.building_configs().find_house_level_config(next_level);

        if let Some(new_tile_def) = context.find_tile_def(next_level_config.tile_def_name_hash) {
            // Try placing new. Might fail if there isn't enough space.
            if Self::try_replace_tile(context, new_tile_def) {
                self.level.upgrade();
                debug_assert!(self.level == next_level);

                self.curr_level_config = next_level_config;
                if !next_level.is_max() {
                    self.next_level_config = context.query.building_configs().find_house_level_config(next_level.next());
                }

                // Set a random variation for the new building tile:
                context.set_random_building_variation();

                tile_placed_successfully = true;
                debug.popup_msg(format!("[U] {} -> {:?}", self.curr_level_config.tile_def_name, self.level));
            }
        }

        if !tile_placed_successfully {
            debug.popup_msg_color(Color::yellow(), format!("[U] {}: No space", self.curr_level_config.tile_def_name));
        }

        self.has_room_to_upgrade = tile_placed_successfully;
    }

    fn try_downgrade(&mut self, context: &BuildingContext<'config, '_, '_>, debug: &mut HouseDebug) {
        let mut tile_placed_successfully = false;

        let prev_level = self.level.prev();
        let prev_level_config = context.query.building_configs().find_house_level_config(prev_level);

        if let Some(new_tile_def) = context.find_tile_def(prev_level_config.tile_def_name_hash) {
            // Try placing new. Should always be able to place a lower-tier (smaller or same size) house tile.
            if Self::try_replace_tile(context, new_tile_def) {
                self.level.downgrade();
                debug_assert!(self.level == prev_level);

                self.curr_level_config = prev_level_config;
                self.next_level_config = context.query.building_configs().find_house_level_config(prev_level.next());

                // Set a random variation for the new building:
                context.set_random_building_variation();

                tile_placed_successfully = true;
                debug.popup_msg(format!("[D] {} -> {:?}", self.curr_level_config.tile_def_name, self.level));
            }
        }

        if !tile_placed_successfully {
            debug.popup_msg_color(Color::red(), format!("[D] {}: Failed!", self.curr_level_config.tile_def_name));
        }
    }

    fn try_replace_tile<'tile_sets>(context: &BuildingContext<'config, 'tile_sets, '_>,
                                    tile_def_to_place: &'tile_sets TileDef) -> bool {

        // Replaces the give tile if the placement is valid,
        // fails and leaves the map unchanged otherwise.
        let tile_map = context.query.tile_map();

        // First check if we have space to place this tile.
        let new_cell_range = tile_def_to_place.cell_range(context.base_cell());
        for cell in &new_cell_range {
            if let Some(tile) =
                tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects) {
                let is_self = tile.base_cell() == context.base_cell();
                if !is_self {
                    // Cannot expand here.
                    return false;
                }
            }
        }

        // We'll need to restore this to the new tile.
        let (prev_game_state, prev_cell_range) = {
            let prev_tile = context.find_tile();

            let cell_range = prev_tile.cell_range();
            debug_assert!(context.map_cells == cell_range);

            let game_state = prev_tile.game_state_handle();
            debug_assert!(game_state.is_valid(), "Building tile doesn't have a valid associated GameStateHandle!");

            (game_state, cell_range)
        };

        debug_assert!(prev_cell_range.start == new_cell_range.start);

        // Now we must clear the previous tile.
        tile_map.try_clear_tile_from_layer(prev_cell_range.start, TileMapLayerKind::Objects)
            .expect("Failed to clear previous tile! This is unexpected...");

        // And place the new one.
        let new_tile = tile_map.try_place_tile_in_layer(
            new_cell_range.start,
            TileMapLayerKind::Objects,
            tile_def_to_place)
            .expect("Failed to place new tile! This is unexpected...");

        debug_assert!(new_tile.cell_range() == new_cell_range);

        // Update game state handle:
        new_tile.set_game_state_handle(prev_game_state);

        if new_cell_range != prev_cell_range {
            let world = context.query.world();
            let graph = context.query.graph();

            // Update cell range cached in the building.
            let this_building = world.find_building_for_tile_mut(new_tile).unwrap();
            this_building.map_cells = new_cell_range;

            // Update path finding graph:
            for cell in &prev_cell_range {
                graph.set_node_kind(Node::new(cell), PathNodeKind::Ground); // Traversable
            }
            for cell in &new_cell_range {
                graph.set_node_kind(Node::new(cell), PathNodeKind::empty()); // Not Traversable
            }  
        }

        true
    }

    // Check if we can increment the level and if there's enough space to expand the house.
    fn is_upgrade_available(&self, context: &BuildingContext) -> bool {
        if self.level.is_max() {
            return false;
        }

        let next_level = self.level.next();
        let next_level_config = context.query.building_configs().find_house_level_config(next_level);

        let tile_def = match context.find_tile_def(next_level_config.tile_def_name_hash) {
            Some(tile_def) => tile_def,
            None => return false,
        };

        let tile_map = context.query.tile_map();
        let cell_range = tile_def.cell_range(context.base_cell());

        for cell in &cell_range {
            if let Some(tile) = tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects) {
                let is_self = tile.base_cell() == context.base_cell();
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

impl HouseBuilding<'_> {
    fn draw_debug_ui_level_config(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header(format!("Config ({:?})", self.upgrade_state.level), imgui::TreeNodeFlags::empty()) {
            self.upgrade_state.curr_level_config.draw_debug_ui(ui_sys);
        }
    }

    fn draw_debug_ui_upgrade_state(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if !ui.collapsing_header("Upgrade", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let draw_level_requirements = 
            |label: &str, level_requirements: &HouseLevelRequirements, imgui_id: u32| {

            ui.separator();
            ui.text(label);

            ui.text(format!("  Resources avail : {} (req: {})",
                level_requirements.resources_available_count(),
                level_requirements.level_config.resources_required.len()));
            ui.text(format!("  Services avail  : {} (req: {})",
                level_requirements.services_available_count(),
                level_requirements.level_config.services_required.len()));

            if ui.collapsing_header(format!("Resources##_building_resources_{}", imgui_id), imgui::TreeNodeFlags::empty()) {
                if !level_requirements.level_config.resources_required.is_empty() {
                    ui.text("Available:");
                    if level_requirements.resources_available.is_empty() {
                        ui.text("  <none>");
                    }
                    for resource in level_requirements.resources_available.iter() {
                        ui.text(format!("  {}", resource));
                    }
                }

                ui.text("Required:");
                if level_requirements.level_config.resources_required.is_empty() {
                    ui.text("  <none>");
                }
                for resource in level_requirements.level_config.resources_required.iter() {
                    ui.text(format!("  {}", resource));
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

        let upgrade_state = &self.upgrade_state;

        let curr_level_requirements =
            HouseLevelRequirements::new(context, upgrade_state.curr_level_config, &self.stock);

        let next_level_requirements =
            HouseLevelRequirements::new(context, upgrade_state.next_level_config, &self.stock);

        ui.text(format!("Level: {:?}", upgrade_state.level));
        color_text(" - Has room      :", upgrade_state.has_room_to_upgrade);
        color_text(" - Has services  :", next_level_requirements.has_required_services());
        color_text(" - Has resources :", next_level_requirements.has_required_resources());
        ui.separator();

        self.upgrade_update_timer.draw_debug_ui("Upgrade", 0, ui_sys);
        self.stock_update_timer.draw_debug_ui("Stock Update", 1, ui_sys);
        self.stock.draw_debug_ui("Resources In Stock", ui_sys);

        draw_level_requirements(
            &format!("Curr level reqs ({:?}):", upgrade_state.level),
            &curr_level_requirements, 0);

        if !upgrade_state.level.is_max() {
            draw_level_requirements(
                &format!("Next level reqs ({:?}):", upgrade_state.level.next()),
                &next_level_requirements, 1);
        }
    }
}
