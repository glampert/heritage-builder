use rand::Rng;
use proc_macros::DrawDebugUi;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumCount, EnumIter};
use strum::EnumCount;

use super::{
    config::{BuildingConfig, BuildingConfigs},
    house_upgrade, Building, BuildingBehavior, BuildingContext, BuildingKind, BuildingKindAndId,
    BuildingStock,
};
use crate::{
    building_config,
    game_object_debug_options,
    engine::time::{Seconds, UpdateTimer},
    game::{
        sim::resources::{
            Population, ResourceKind, ResourceKinds, ServiceKind, ServiceKinds, Workers,
        },
        system::settlers::Settler,
        unit::Unit,
        world::stats::WorldStats,
    },
    log,
    imgui_ui::UiSystem,
    save::PostLoadContext,
    tile::{
        sets::{TileDef, TileSets, PresetTiles, TERRAIN_GROUND_CATEGORY},
        Tile, TileMapLayerKind,
    },
    utils::{
        hash::{self, StringHash},
        Color,
    },
};

// ----------------------------------------------
// TODO List For Houses / Buildings:
// ----------------------------------------------

// - Resources should have individual rates of consumption. Some kinds of
//   resources are consumed slower/faster than others.
//
// - Resources consumption rate should be expressed in units per day. The house
//   occupancy should also influence the resources consumption rate.
//
// - Allow houses to stock up on more than 1 unit of each kind of resources?
//   Could allow stocking up to a maximum number of units.
//
// - If houses stay without access to basic resources for too long (food/water),
//   settlers may decide to leave (house may downgrade back to vacant lot).
//
// - Should we make house access to services depend on it being visited by the
//   service patrol unit? Right now access to a service is simply based on
//   proximity to the building, measured from the house's road link tile.
//
// - Buildings that require workers should run slower if they are below max
//   workers.

// ----------------------------------------------
// HouseConfig
// ----------------------------------------------

#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)] // Default all fields.
pub struct HouseConfig {
    #[serde(skip)] // BuildingKind::House implied.
    pub kind: BuildingKind,

    pub name: String,
    pub tile_def_name: String,

    #[debug_ui(skip)]
    #[serde(skip)] // Not serialized. Computed on post_load.
    pub tile_def_name_hash: StringHash,

    // General configuration parameters for all house buildings & levels.
    pub population_update_frequency_secs: Seconds,
    pub stock_update_frequency_secs: Seconds,
    pub upgrade_update_frequency_secs: Seconds,
    pub generate_tax_frequency_secs: Seconds,
}

impl Default for HouseConfig {
    #[inline]
    fn default() -> Self {
        Self { kind: BuildingKind::House,
               name: "House".into(),
               tile_def_name: "house".into(),
               tile_def_name_hash: hash::fnv1a_from_str("house"),
               population_update_frequency_secs: 60.0,
               stock_update_frequency_secs: 60.0,
               upgrade_update_frequency_secs: 10.0,
               generate_tax_frequency_secs: 60.0 }
    }
}

building_config! {
    HouseConfig
}

// ----------------------------------------------
// HouseLevelConfig
// ----------------------------------------------

#[derive(DrawDebugUi, Serialize, Deserialize)]
pub struct HouseLevelConfig {
    #[serde(skip)] // BuildingKind::House implied.
    pub kind: BuildingKind,
    pub level: HouseLevel,

    pub name: String,
    pub tile_def_name: String,

    #[debug_ui(skip)]
    #[serde(skip)] // Not serialized. Computed on post_load.
    pub tile_def_name_hash: StringHash,

    pub max_population: u32,

    // Base tax generated per employed resident.
    pub base_tax_generated: u32,

    // Bonus tax percentage added if the house has full employment; [0,100].
    #[serde(default)]
    pub tax_bonus: u32,

    // What percentage of the house population is available as workers; [0,100].
    pub worker_percentage: u32,

    // Percentage of chance that the house population will increase on population updates; [0,100].
    pub population_increase_chance: u32,

    // Types of services provided by these kinds of buildings for the house level to be obtained
    // and maintained.
    #[serde(default)]
    pub services_required: ServiceKinds,

    // Kinds of resources required for the house level to be obtained and maintained.
    #[serde(default)]
    pub resources_required: ResourceKinds,

    #[serde(default)]
    pub stock_capacity: u32,
}

impl Default for HouseLevelConfig {
    #[inline]
    fn default() -> Self {
        Self { kind: BuildingKind::House,
               level: HouseLevel::Level0,
               name: "House Level 0".into(),
               tile_def_name: "house0".into(),
               tile_def_name_hash: hash::fnv1a_from_str("house0"),
               max_population: 2,
               base_tax_generated: 0,
               tax_bonus: 0,
               worker_percentage: 100,
               population_increase_chance: 80,
               services_required: ServiceKinds::none(),
               resources_required: ResourceKinds::none(),
               stock_capacity: 5 }
    }
}

building_config! {
    HouseLevelConfig
}

// ----------------------------------------------
// HouseDebug
// ----------------------------------------------

game_object_debug_options! {
    HouseDebug,

    // Stops population increase when requirements are met.
    freeze_population_update: bool,

    // Stops any resources from being consumed.
    // Also stops refreshing resources stock from a market.
    freeze_stock_update: bool,

    // Stops any upgrade/downgrade when true.
    freeze_upgrade_update: bool,

    // Stops tax income from being generated.
    freeze_tax_generation: bool,
}

// ----------------------------------------------
// HouseBuilding
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct HouseBuilding {
    workers: Workers, // Workers this household provides (employed + unemployed).

    population_update_timer: UpdateTimer,
    population: Population,

    stock_update_timer: UpdateTimer,
    stock: BuildingStock,

    upgrade_update_timer: UpdateTimer,
    upgrade_state: HouseUpgradeState,

    generate_tax_timer: UpdateTimer,
    tax_available: u32,

    #[serde(skip)]
    debug: HouseDebug,
}

// ----------------------------------------------
// BuildingBehavior for HouseBuilding
// ----------------------------------------------

impl BuildingBehavior for HouseBuilding {
    // ----------------------
    // World Callbacks:
    // ----------------------

    fn name(&self) -> &str {
        &self.current_level_config().name
    }

    fn configs(&self) -> &dyn BuildingConfig {
        self.current_level_config()
    }

    fn spawned(&mut self, context: &BuildingContext) {
        debug_assert!(context.base_cell().is_valid());

        let cell = context.base_cell();
        let tile_map = context.query.tile_map();

        const LAYER_KIND: TileMapLayerKind = TileMapLayerKind::Terrain;
        const TILE_DEF: PresetTiles = PresetTiles::Grass;

        // Clear the vacant lot tile this house was placed over.
        if let Some(tile) = tile_map.try_tile_from_layer(cell, LAYER_KIND) {
            if tile.path_kind().is_vacant_lot() {
                match tile_map.try_clear_tile_from_layer(cell, LAYER_KIND) {
                    Ok(_) => {
                        if let Some(tile_def_to_place) =
                            TileSets::get().find_tile_def_by_hash(LAYER_KIND,
                                                                  TERRAIN_GROUND_CATEGORY.hash,
                                                                  TILE_DEF.hash())
                        {
                            let _ = tile_map.try_place_tile_in_layer(cell,
                                                                     LAYER_KIND,
                                                                     tile_def_to_place)
                                            .inspect_err(|err| {
                                                log::error!(log::channel!("house"),
                                                            "Failed to place new tile: {err}")
                                            });
                        } else {
                            log::error!(log::channel!("house"),
                                        "Couldn't find '{TILE_DEF}' TileDef!");
                        }
                    }
                    Err(err) => {
                        log::error!(log::channel!("house"),
                                    "Failed to clear VacantLot tile: {err}");
                    }
                }
            }
        }
    }

    fn update(&mut self, context: &BuildingContext) {
        let delta_time_secs = context.query.delta_time_secs();

        // Update house states:
        if self.stock_update_timer.tick(delta_time_secs).should_update()
           && !self.debug.freeze_stock_update()
        {
            self.stock_update();
        }

        if self.upgrade_update_timer.tick(delta_time_secs).should_update()
           && !self.debug.freeze_upgrade_update()
        {
            self.upgrade_update(context);
        }

        if self.population_update_timer.tick(delta_time_secs).should_update()
           && !self.debug.freeze_population_update()
        {
            self.population_update(context);
        }

        if self.generate_tax_timer.tick(delta_time_secs).should_update()
           && !self.debug.freeze_tax_generation()
        {
            self.generate_tax();
        }
    }

    fn visited_by(&mut self, unit: &mut Unit, context: &BuildingContext) {
        if unit.is_settler() {
            self.visited_by_settler(unit, context);
        } else if unit.is_market_vendor(context.query) {
            self.visited_by_market_vendor(unit, context);
        } else if unit.is_tax_collector(context.query) {
            self.visited_by_tax_collector(unit, context);
        }
    }

    fn post_load(&mut self, _context: &PostLoadContext, kind: BuildingKind, _tile: &Tile) {
        debug_assert!(kind == BuildingKind::House);

        let config = BuildingConfigs::get().find_house_config();

        self.population_update_timer.post_load(config.population_update_frequency_secs);
        self.stock_update_timer.post_load(config.stock_update_frequency_secs);
        self.upgrade_update_timer.post_load(config.upgrade_update_frequency_secs);
        self.generate_tax_timer.post_load(config.generate_tax_frequency_secs);

        self.upgrade_state.post_load();
    }

    // ----------------------
    // Resources/Stock:
    // ----------------------

    fn has_stock(&self) -> bool {
        true
    }

    fn is_stock_full(&self) -> bool {
        self.stock.is_full()
    }

    fn available_resources(&self, kind: ResourceKind) -> u32 {
        self.stock.available_resources(kind)
    }

    fn receivable_resources(&self, kind: ResourceKind) -> u32 {
        self.stock.receivable_resources(kind)
    }

    fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        let received_count = self.stock.receive_resources(kind, count);
        self.debug.log_resources_gained(kind, received_count);
        received_count
    }

    fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        let removed_count = self.stock.remove_resources(kind, count);
        self.debug.log_resources_lost(kind, removed_count);
        removed_count
    }

    fn tally(&self, stats: &mut WorldStats, _kind: BuildingKind) {
        stats.update_housing_stats(self.level());

        self.stock.for_each(|_, item| {
                      stats.add_house_resources(item.kind, item.count);
                  });

        stats.treasury.tax_generated += self.tax_generated();
        stats.treasury.tax_available += self.tax_available();
    }

    // ----------------------
    // Population/Workers:
    // ----------------------

    fn population(&self) -> Option<Population> {
        Some(self.population)
    }

    fn add_population(&mut self, context: &BuildingContext, count: u32) -> u32 {
        if count != 0 && !self.population.is_max() {
            let amount_added = self.population.add(count);
            self.debug.popup_msg_color(Color::green(), format!("+{amount_added} Population"));
            self.adjust_workers_available(context);
            return amount_added;
        }
        0
    }

    fn remove_population(&mut self, context: &BuildingContext, count: u32) -> u32 {
        if count != 0 && self.population.count() != 0 {
            let amount_removed = self.population.remove(count);
            self.evict_population(context, amount_removed);
            self.adjust_workers_available(context);
            return amount_removed;
        }
        0
    }

    // ----------------------
    // Workers:
    // ----------------------

    fn workers(&self) -> Option<&Workers> {
        Some(&self.workers)
    }
    fn workers_mut(&mut self) -> Option<&mut Workers> {
        Some(&mut self.workers)
    }

    // ----------------------
    // Debug:
    // ----------------------

    fn debug_options(&mut self) -> &mut dyn GameObjectDebugOptions {
        &mut self.debug
    }

    fn draw_debug_ui(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        house_upgrade::draw_debug_ui(context, ui_sys);
        self.draw_debug_ui_timers(ui_sys);
        self.draw_debug_ui_stock(ui_sys);
        self.draw_debug_ui_upgrade_state(context, ui_sys);
    }
}

// ----------------------------------------------
// HouseBuilding
// ----------------------------------------------

impl HouseBuilding {
    pub fn new(level: HouseLevel,
               house_config: &'static HouseConfig,
               configs: &'static BuildingConfigs)
               -> Self {
        let upgrade_state = HouseUpgradeState::new(level, configs);

        let stock =
            BuildingStock::with_accepted_kinds_and_capacity(ResourceKind::foods()
                                                            | ResourceKind::consumer_goods(),
                                                            upgrade_state.curr_level_config
                                                                         .unwrap()
                                                                         .stock_capacity);

        Self { workers: Workers::household_worker_pool(0, 0),
               population_update_timer:
                   UpdateTimer::new(house_config.population_update_frequency_secs),
               population: Population::new(0,
                                           upgrade_state.curr_level_config
                                                        .unwrap()
                                                        .max_population),
               stock_update_timer: UpdateTimer::new(house_config.stock_update_frequency_secs),
               stock,
               upgrade_update_timer: UpdateTimer::new(house_config.upgrade_update_frequency_secs),
               upgrade_state,
               generate_tax_timer: UpdateTimer::new(house_config.generate_tax_frequency_secs),
               tax_available: 0,
               debug: HouseDebug::default() }
    }

    pub fn register_callbacks() {}

    #[inline]
    fn current_level_config(&self) -> &'static HouseLevelConfig {
        self.upgrade_state.curr_level_config.unwrap()
    }

    #[inline]
    fn next_level_config(&self) -> &'static HouseLevelConfig {
        self.upgrade_state.next_level_config.unwrap()
    }

    // ----------------------
    // Stock Update:
    // ----------------------

    fn stock_update(&mut self) {
        // Consume resources from the stock periodically:
        let curr_level_resources_required =
            &self.upgrade_state.curr_level_config.unwrap().resources_required;

        // Consume one of each resources this level uses.
        curr_level_resources_required.for_each(|resource| {
            if self.remove_resources(resource, 1) != 0 {
                // We consumed one, done.
                // E.g.: resource = Meat|Fish, consume one of either.
                return false;
            }
            true
        });
    }

    fn visited_by_market_vendor(&mut self, unit: &mut Unit, context: &BuildingContext) {
        if self.debug.freeze_stock_update() {
            return;
        }

        if let Some(market) = unit.patrol_task_origin_building(context.query) {
            self.shop_from_market(market, context);
            self.debug.popup_msg_color(Color::green(), "Visited by market vendor");
        }
    }

    fn shop_from_market(&mut self, market: &mut Building, context: &BuildingContext) {
        debug_assert!(market.is(BuildingKind::Market));

        // Shop for resources needed for this level.
        let current_level_shopping_list =
            &self.upgrade_state.curr_level_config.unwrap().resources_required;

        const ALL_OR_NOTHING: bool = false;
        self.shop_items(market, current_level_shopping_list, ALL_OR_NOTHING);

        // And if we have space to upgrade, shop for resources needed for the next
        // level, so we can advance. But only take any if we have the whole
        // shopping list. No point in shopping partially since we wouldn't be
        // able to upgrade and would wasted those resources.
        if self.is_upgrade_available(context) {
            let mut next_level_shopping_list = ResourceKinds::none();

            // We've already shopped for resources in the current level list,
            // so take only the ones that are exclusive to the next level.
            for &resource in self.next_level_config().resources_required.iter() {
                if !self.stock.has_any_of(resource) {
                    next_level_shopping_list.add(resource);
                }
            }

            // Only succeed if we can shop all required items.
            const ALL_OR_NOTHING: bool = true;
            self.shop_items(market, &next_level_shopping_list, ALL_OR_NOTHING);
        }
    }

    fn shop_items(&mut self,
                  market: &mut Building,
                  shopping_list: &ResourceKinds,
                  all_or_nothing: bool) {
        if all_or_nothing {
            for wanted_resources in shopping_list.iter() {
                let mut has_any = false;

                for single_resource in wanted_resources.iter() {
                    if market.available_resources(single_resource) != 0 {
                        has_any = true;
                        break;
                    }
                }

                // If any resource is unavailable we take nothing.
                if !has_any {
                    return;
                }
            }
        }

        shopping_list.for_each(|resource| {
            let removed_count = market.remove_resources(resource, 1);
            self.receive_resources(resource, removed_count);
            true
        });
    }

    // ----------------------
    // Upgrade Update:
    // ----------------------

    fn upgrade_update(&mut self, context: &BuildingContext) {
        let mut upgraded = false;
        let mut downgraded = false;

        // Attempt to upgrade or downgrade based on services and resources availability.
        let upgrade = &mut self.upgrade_state;

        debug_assert!(upgrade.curr_level_config.is_some());
        debug_assert!(upgrade.next_level_config.is_some());

        if upgrade.can_upgrade(context, &self.stock) {
            upgraded = upgrade.try_upgrade(context, &mut self.debug);
        } else if upgrade.can_downgrade(context, &self.stock) {
            downgraded = upgrade.try_downgrade(context, &mut self.debug);
        }

        if upgraded || downgraded {
            self.stock.update_capacities(self.current_level_config().stock_capacity);
            self.adjust_population(context,
                                   self.population.count(),
                                   self.current_level_config().max_population);
        }
    }

    pub fn is_upgrade_available(&self, context: &BuildingContext) -> bool {
        if self.debug.freeze_upgrade_update() {
            return false;
        }
        self.upgrade_state.is_upgrade_available(context)
    }

    pub fn upgrade_requirements(&self, context: &BuildingContext) -> HouseLevelRequirements {
        HouseLevelRequirements::new(context, self.next_level_config(), &self.stock)
    }

    // ----------------------
    // Upgrade Helpers:
    // ----------------------

    #[inline]
    pub fn level(&self) -> HouseLevel {
        self.upgrade_state.level
    }

    #[inline]
    pub fn merge(&mut self,
                 context: &BuildingContext,
                 house_to_merge: &mut HouseBuilding,
                 house_to_merge_kind_and_id: BuildingKindAndId,
                 target_level_config: &HouseLevelConfig) {
        self.merge_resources(context, house_to_merge, target_level_config);
        self.merge_population(context, house_to_merge, target_level_config);
        self.merge_workers(context, house_to_merge, house_to_merge_kind_and_id);
    }

    fn merge_resources(&mut self,
                       context: &BuildingContext,
                       house_to_merge: &mut HouseBuilding,
                       target_level_config: &HouseLevelConfig) {
        self.stock.update_capacities(target_level_config.stock_capacity);

        if !self.stock.merge(&house_to_merge.stock) {
            log::error!(log::channel!("house"),
                        "Failed to fully merge house stocks: {} - {} and {}.",
                        self.name(),
                        context.id,
                        house_to_merge.name());
        }

        // Resources moved into this house.
        house_to_merge.stock.clear();
    }

    fn merge_population(&mut self,
                        context: &BuildingContext,
                        house_to_merge: &mut HouseBuilding,
                        target_level_config: &HouseLevelConfig) {
        let new_max_population = target_level_config.max_population;
        let mut new_population = self.population.count() + house_to_merge.population.count();

        // NOTE: If the merge exceeds new house population capacity we will evict some
        // residents first.
        if new_population > new_max_population {
            let amount_to_evict = new_population - new_max_population;
            self.evict_population(context, amount_to_evict);
            new_population -= amount_to_evict;
        }

        // Should always succeed since we've made enough room.
        self.population.set_max_and_count(new_max_population, new_population);
        self.adjust_workers_available(context);

        // NOTE: Reset all population so we won't try to evict any residents when
        // the building is destroyed. Population has been moved into this household.
        house_to_merge.population.clear();
    }

    fn merge_workers(&mut self,
                     context: &BuildingContext,
                     house_to_merge: &mut HouseBuilding,
                     house_to_merge_kind_and_id: BuildingKindAndId) {
        if !self.workers.merge(&house_to_merge.workers) {
            log::error!(log::channel!("house"),
                        "Failed to fully merge house worker pools: {} - {} and {}.",
                        self.name(),
                        context.id,
                        house_to_merge.name());
        }

        // Employers of `house_to_merge` workers must now point to this house.
        house_to_merge.workers.as_household_worker_pool().unwrap()
            .for_each_employer(context.query.world(), |employer, employed_count| {
                let prev_popups = employer.archetype_mut().debug_options().set_show_popups(false);

                let removed_count = employer.remove_workers(employed_count, house_to_merge_kind_and_id);
                if removed_count != employed_count {
                    log::error!(log::channel!("house"), "House merge between {} - {} and {}: Failed to remove {} workers from {}.",
                                self.name(), context.id, house_to_merge.name(), employed_count, employer.name());
                }

                let added_count = employer.add_workers(employed_count, context.kind_and_id());
                if added_count != employed_count {
                    log::error!(log::channel!("house"), "House merge between {} - {} and {}: Failed to add {} workers to {}.",
                                self.name(), context.id, house_to_merge.name(), employed_count, employer.name());
                }

                employer.archetype_mut().debug_options().set_show_popups(prev_popups);
                true
            });

        // NOTE: Reset all workers so we won't try to notify
        // employers when the merged building is destroyed.
        house_to_merge.workers.clear();
    }

    // ----------------------
    // Population Update:
    // ----------------------

    fn population_update(&mut self, context: &BuildingContext) {
        if self.population.is_max() {
            return;
        }

        let rng = context.query.rng();
        let chance = self.current_level_config().population_increase_chance.min(100);
        let increase_population = rng.random_ratio(chance, 100);

        if increase_population {
            let population_added = self.add_population(context, 1);
            debug_assert!(population_added == 1);
        }
    }

    fn visited_by_settler(&mut self, unit: &mut Unit, context: &BuildingContext) {
        let population_to_add = unit.settler_population(context.query);
        let population_added = self.add_population(context, population_to_add);
        if population_added == 0 {
            self.debug.popup_msg_color(Color::red(), "Refused settler");
        }
    }

    fn evict_population(&mut self, context: &BuildingContext, amount_to_evict: u32) {
        let unit_origin = context.road_link_or_building_access_tile();
        if !unit_origin.is_valid() {
            log::error!(log::channel!("house"),
                        "Failed to find a vacant cell to spawn evicted unit!");
            return;
        }

        let mut settler = Settler::default();

        settler.try_spawn(context.query, unit_origin, amount_to_evict);

        self.debug.popup_msg_color(Color::red(), format!("Evicted {amount_to_evict} residents"));
    }

    fn adjust_population(&mut self, context: &BuildingContext, new_population: u32, new_max: u32) {
        let prev_population = self.population.count();
        let curr_population = self.population.set_max_and_count(new_max, new_population);

        if curr_population != prev_population {
            self.adjust_workers_available(context);

            if curr_population < prev_population {
                let amount_to_evict = prev_population - curr_population;
                self.evict_population(context, amount_to_evict);
            }
        }
    }

    fn adjust_workers_available(&mut self, context: &BuildingContext) {
        // Percentage of current household residents that are workers: [0,100].
        let worker_percentage = (self.current_level_config().worker_percentage as f32) / 100.0;
        let curr_population = self.population.count() as f32;
        let workers_available = (curr_population * worker_percentage).round() as u32;

        let workers = self.workers.as_household_worker_pool_mut().unwrap();

        let curr_employed = workers.employed_count();
        let new_employed = curr_employed.min(workers_available);
        let new_unemployed = workers_available - new_employed;

        if new_employed < curr_employed {
            let mut difference = curr_employed - new_employed;
            self.debug.popup_msg_color(Color::magenta(), format!("-{difference} workers"));

            workers.for_each_employer_mut(context.query.world(), |employer, employed_count| {
                       let removed_count =
                           employer.remove_workers((*employed_count).min(difference),
                                                   context.kind_and_id());

                       *employed_count -= removed_count;

                       difference = difference.saturating_sub(removed_count);
                       if difference == 0 {
                           return false; // stop
                       }
                       true // continue
                   });
        }

        workers.set_counts(new_employed, new_unemployed);
    }

    // ----------------------
    // Household Tax:
    // ----------------------

    #[inline]
    pub fn tax_available(&self) -> u32 {
        self.tax_available
    }

    pub fn tax_generated(&self) -> u32 {
        let workers = self.workers.as_household_worker_pool().unwrap();
        let level_config = self.current_level_config();

        let employed_residents = workers.employed_count();
        let total_residents = self.population.count();

        let base_tax_generated = level_config.base_tax_generated;
        let tax_bonus = level_config.tax_bonus.min(100); // 0-100%

        Self::calc_household_tax(employed_residents, total_residents, base_tax_generated, tax_bonus)
    }

    // - `base_tax_generated`: base tax per employed resident
    // - `tax_bonus`: percentage bonus applied if all residents are employed (0–100)
    fn calc_household_tax(employed_residents: u32,
                          total_residents: u32,
                          base_tax_generated: u32,
                          tax_bonus: u32)
                          -> u32 {
        if employed_residents == 0 || total_residents == 0 || base_tax_generated == 0 {
            // If we have no working population we can't generate any tax income!
            return 0;
        }

        let mut tax = (employed_residents * base_tax_generated) as f32;

        // Apply bonus if the household is fully employed.
        if employed_residents == total_residents && tax_bonus != 0 {
            tax += tax * (tax_bonus as f32 / 100.0);
        }

        tax.round() as u32
    }

    fn generate_tax(&mut self) {
        let tax_generated = self.tax_generated();
        if tax_generated != 0 {
            self.tax_available += tax_generated;
            self.debug.popup_msg_color(Color::yellow(), format!("Tax available +{tax_generated}"));
        }
    }

    fn visited_by_tax_collector(&mut self, unit: &mut Unit, _context: &BuildingContext) {
        if self.tax_available != 0 {
            let tax_collected = self.tax_available;
            self.tax_available = 0;

            // Tax collected in units of gold.
            let received_amount = unit.receive_resources(ResourceKind::Gold, tax_collected);
            debug_assert!(received_amount == tax_collected);

            self.debug.popup_msg_color(Color::red(), format!("Tax collected -{tax_collected}"));
        }
    }
}

// ----------------------------------------------
// HouseLevel
// ----------------------------------------------

#[repr(u8)]
#[derive(Copy,
         Clone,
         Display,
         PartialOrd,
         Ord,
         PartialEq,
         Eq,
         IntoPrimitive,
         TryFromPrimitive,
         EnumCount,
         EnumIter,
         Serialize,
         Deserialize)]
pub enum HouseLevel {
    Level0,
    Level1,
    Level2,
    Level3,
    Level4,
}

impl HouseLevel {
    #[inline]
    #[must_use]
    pub const fn count() -> usize {
        Self::COUNT
    }

    #[inline]
    #[must_use]
    pub fn min() -> Self {
        Self::try_from_primitive(0).unwrap()
    }

    #[inline]
    #[must_use]
    pub fn max() -> Self {
        Self::try_from_primitive((Self::COUNT - 1) as u8).unwrap()
    }

    #[inline]
    #[must_use]
    pub fn is_min(self) -> bool {
        self == Self::min()
    }

    #[inline]
    #[must_use]
    pub fn is_max(self) -> bool {
        self == Self::max()
    }

    #[inline]
    #[must_use]
    pub fn next(self) -> Self {
        let curr: u8 = self.into();
        let next = curr + 1;
        Self::try_from(next).expect("Max HouseLevel exceeded!")
    }

    #[inline]
    #[must_use]
    pub fn prev(self) -> Self {
        let curr: u8 = self.into();
        let next = curr - 1;
        Self::try_from(next).expect("Min HouseLevel exceeded!")
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

pub struct HouseLevelRequirements {
    level_config: &'static HouseLevelConfig,
    services_available: ServiceKind, // From the level requirements, which ones we have access to.
    resources_available: ResourceKind, // From the level requirements, which ones we have in stock.
}

impl HouseLevelRequirements {
    fn new(context: &BuildingContext,
           level_config: &'static HouseLevelConfig,
           stock: &BuildingStock)
           -> Self {
        let mut reqs = Self { level_config,
                              services_available: ServiceKind::empty(),
                              resources_available: ResourceKind::empty() };

        level_config.services_required.for_each(|service| {
            if context.has_access_to_service(service) {
               reqs.services_available.insert(service);
            }
            true
        });

        level_config.resources_required.for_each(|resource| {
            if stock.has_any_of(resource) {
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
    pub fn has_required_services(&self) -> bool {
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
    pub fn has_required_resources(&self) -> bool {
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

    pub fn services_missing(&self) -> ServiceKind {
        let mut missing = ServiceKind::empty();

        for service in self.level_config.services_required.iter() {
            if !self.services_available.intersects(*service) {
                missing.insert(*service);
            }
        }

        missing
    }

    pub fn resources_missing(&self) -> ResourceKind {
        let mut missing = ResourceKind::empty();

        for resource in self.level_config.resources_required.iter() {
            if !self.resources_available.intersects(*resource) {
                missing.insert(*resource);
            }
        }

        missing
    }
}

// ----------------------------------------------
// HouseUpgradeState
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
struct HouseUpgradeState {
    level: HouseLevel,
    #[serde(skip)]
    curr_level_config: Option<&'static HouseLevelConfig>,
    #[serde(skip)]
    next_level_config: Option<&'static HouseLevelConfig>,
    #[serde(skip)]
    has_room_to_upgrade: bool, // [Debug] Result of last attempt to expand the house.
}

impl HouseUpgradeState {
    fn new(level: HouseLevel, configs: &'static BuildingConfigs) -> Self {
        let curr_level_config = Some(configs.find_house_level_config(level));
        let next_level_config = if !level.is_max() {
            Some(configs.find_house_level_config(level.next()))
        } else {
            curr_level_config
        };

        Self { level, curr_level_config, next_level_config, has_room_to_upgrade: true }
    }

    fn post_load(&mut self) {
        let configs = BuildingConfigs::get();
        self.curr_level_config = Some(configs.find_house_level_config(self.level));

        if self.level.is_max() {
            self.next_level_config = self.curr_level_config;
        } else {
            self.next_level_config = Some(configs.find_house_level_config(self.level.next()));
        }
    }

    fn can_upgrade(&mut self, context: &BuildingContext, stock: &BuildingStock) -> bool {
        if self.level.is_max() {
            return false;
        }

        let next_level_requirements =
            HouseLevelRequirements::new(context, self.next_level_config.unwrap(), stock);

        // Upgrade if we have the required services and resources for the next level.
        next_level_requirements.has_required_services()
        && next_level_requirements.has_required_resources()
    }

    fn can_downgrade(&mut self, context: &BuildingContext, stock: &BuildingStock) -> bool {
        if self.level.is_min() {
            return false;
        }

        let curr_level_requirements =
            HouseLevelRequirements::new(context, self.curr_level_config.unwrap(), stock);

        // Downgrade if we don't have the required services and resources for the
        // current level.
        !curr_level_requirements.has_required_services()
        || !curr_level_requirements.has_required_resources()
    }

    fn try_upgrade(&mut self, context: &BuildingContext, debug: &mut HouseDebug) -> bool {
        let mut upgraded_successfully = false;

        let configs = BuildingConfigs::get();
        let next_level = self.level.next();
        let next_level_config = configs.find_house_level_config(next_level);

        if let Some(new_tile_def) = context.find_tile_def(next_level_config.tile_def_name_hash) {
            // Try placing new. Might fail if there isn't enough space.
            if self.try_replace_tile(context, self.level, next_level, new_tile_def) {
                self.level.upgrade();
                debug_assert!(self.level == next_level);

                self.curr_level_config = Some(next_level_config);
                if !next_level.is_max() {
                    self.next_level_config =
                        Some(configs.find_house_level_config(next_level.next()));
                }

                // Set a random variation for the new building tile:
                context.set_random_building_variation();

                debug.popup_msg(format!("[U] {} -> {}",
                                        self.curr_level_config.unwrap().tile_def_name,
                                        self.level));
                upgraded_successfully = true;
            }
        } else {
            log::error!(log::channel!("house"),
                        "Cannot find TileDef '{}' for house level {}.",
                        next_level_config.tile_def_name,
                        next_level);
        }

        if !upgraded_successfully {
            debug.popup_msg_color(Color::yellow(),
                                  format!("[U] {}: No space",
                                          self.curr_level_config.unwrap().tile_def_name));
        }

        self.has_room_to_upgrade = upgraded_successfully;
        upgraded_successfully
    }

    fn try_downgrade(&mut self, context: &BuildingContext, debug: &mut HouseDebug) -> bool {
        let mut downgraded_successfully = false;

        let configs = BuildingConfigs::get();
        let prev_level = self.level.prev();
        let prev_level_config = configs.find_house_level_config(prev_level);

        if let Some(new_tile_def) = context.find_tile_def(prev_level_config.tile_def_name_hash) {
            // Try placing new. Should always be able to place a lower-tier (smaller or same
            // size) house tile.
            if self.try_replace_tile(context, self.level, prev_level, new_tile_def) {
                self.level.downgrade();
                debug_assert!(self.level == prev_level);

                self.curr_level_config = Some(prev_level_config);
                self.next_level_config = Some(configs.find_house_level_config(prev_level.next()));

                // Set a random variation for the new building:
                context.set_random_building_variation();

                debug.popup_msg(format!("[D] {} -> {}",
                                        self.curr_level_config.unwrap().tile_def_name,
                                        self.level));
                downgraded_successfully = true;
            }
        } else {
            log::error!(log::channel!("house"),
                        "Cannot find TileDef '{}' for house level {}.",
                        prev_level_config.tile_def_name,
                        prev_level);
        }

        if !downgraded_successfully {
            debug.popup_msg_color(Color::red(),
                                  format!("[D] {}: Failed!",
                                          self.curr_level_config.unwrap().tile_def_name));
        }

        self.has_room_to_upgrade = downgraded_successfully;
        downgraded_successfully
    }

    // Replaces the give tile if the placement is valid, fails and leaves the map
    // unchanged otherwise.
    fn try_replace_tile(&self,
                        context: &BuildingContext,
                        current_level: HouseLevel,
                        target_level: HouseLevel,
                        target_tile_def: &'static TileDef)
                        -> bool {
        debug_assert!(current_level != target_level);
        debug_assert!(target_tile_def.is_valid());

        let house_id = context.id;

        let wants_to_expand =
            target_level > current_level
            && house_upgrade::requires_expansion(context, current_level, target_level);

        if wants_to_expand {
            // Upgrade to larger tile:
            house_upgrade::try_expand_house(context, house_id, current_level, target_level)
        } else {
            // Downgrade or upgrade to same size:
            // - No expansion required, but we still have to place a new tile.
            let new_cell_range = target_tile_def.cell_range(context.base_cell());
            house_upgrade::try_replace_tile(context, house_id, target_tile_def, new_cell_range)
        }
    }

    // Check if we can increment the level and if there's enough space to expand the
    // house.
    fn is_upgrade_available(&self, context: &BuildingContext) -> bool {
        if self.level.is_max() {
            return false;
        }

        let current_level = self.level;
        let target_level = current_level.next();

        if !house_upgrade::requires_expansion(context, current_level, target_level) {
            return true; // No expansion required, upgrade is possible.
        }

        // Check if we have enough space to expand this house to a larger tile size
        // (possibly merging with others).
        house_upgrade::can_expand_house(context, context.id, current_level, target_level)
    }
}

// ----------------------------------------------
// Debug UI
// ----------------------------------------------

impl HouseBuilding {
    fn draw_debug_ui_upgrade_state(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        if !ui.collapsing_header("Upgrade", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        let draw_level_requirements = |label: &str, level_requirements: &HouseLevelRequirements, imgui_id: u32| {
            ui.separator();
            ui.text(label);

            ui.text(format!("  Resources avail : {} (req: {})",
                            level_requirements.resources_available_count(),
                            level_requirements.level_config.resources_required.len()));
            ui.text(format!("  Services avail  : {} (req: {})",
                            level_requirements.services_available_count(),
                            level_requirements.level_config.services_required.len()));

            if ui.collapsing_header(format!("Resources##_building_resources_{}", imgui_id),
                                    imgui::TreeNodeFlags::empty())
            {
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

            if ui.collapsing_header(format!("Services##_building_services_{}", imgui_id),
                                    imgui::TreeNodeFlags::empty())
            {
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

        let mut level_num: u8 = self.upgrade_state.level.into();
        if ui.input_scalar("Level", &mut level_num).step(1).build() {
            if let Ok(level) = HouseLevel::try_from_primitive(level_num) {
                let mut upgraded = false;
                let mut downgraded = false;

                match level.cmp(&self.upgrade_state.level) {
                    std::cmp::Ordering::Greater => {
                        upgraded = self.upgrade_state.try_upgrade(context, &mut self.debug);
                    }
                    std::cmp::Ordering::Less => {
                        downgraded = self.upgrade_state.try_downgrade(context, &mut self.debug);
                    }
                    std::cmp::Ordering::Equal => {} // nothing
                }

                if upgraded || downgraded {
                    self.stock.update_capacities(self.current_level_config().stock_capacity);
                    self.adjust_population(context,
                                           self.population.count(),
                                           self.current_level_config().max_population);
                }
            }
        }

        let upgrade_state = &self.upgrade_state;

        let curr_level_requirements = HouseLevelRequirements::new(
            context,
            upgrade_state.curr_level_config.unwrap(),
            &self.stock
        );

        let next_level_requirements = HouseLevelRequirements::new(
            context,
            upgrade_state.next_level_config.unwrap(),
            &self.stock
        );

        color_text(" - Has room        :", upgrade_state.has_room_to_upgrade);
        color_text(" - Has services    :", next_level_requirements.has_required_services());
        color_text(" - Has resources   :", next_level_requirements.has_required_resources());
        color_text(" - Has road access :", context.is_linked_to_road());

        draw_level_requirements(&format!("Curr level reqs ({}):", upgrade_state.level),
                                &curr_level_requirements,
                                0);

        if !upgrade_state.level.is_max() {
            draw_level_requirements(&format!("Next level reqs ({}):", upgrade_state.level.next()),
                                    &next_level_requirements,
                                    1);
        }
    }

    fn draw_debug_ui_timers(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        if !ui.collapsing_header("Timers", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        self.population_update_timer.draw_debug_ui("Population Update", 0, ui_sys);
        self.upgrade_update_timer.draw_debug_ui("Upgrade Update", 1, ui_sys);
        self.stock_update_timer.draw_debug_ui("Stock Update", 2, ui_sys);
        self.generate_tax_timer.draw_debug_ui("Gen Tax", 3, ui_sys);
    }

    fn draw_debug_ui_stock(&mut self, ui_sys: &UiSystem) {
        self.stock.draw_debug_ui("Stock", ui_sys);
    }
}
