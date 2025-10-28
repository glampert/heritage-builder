use std::cmp::Reverse;
use proc_macros::DrawDebugUi;
use serde::{Deserialize, Serialize};

use super::{
    config::{BuildingConfig, BuildingConfigs},
    Building, BuildingBehavior, BuildingContext, BuildingKind, BuildingStock,
};
use crate::{
    building_config,
    game_object_debug_options,
    engine::time::{Seconds, UpdateTimer},
    game::{
        cheats,
        sim::{
            resources::{ResourceKind, ResourceKinds, ShoppingList, StockItem, Workers},
            Query,
        },
        unit::{
            patrol::*,
            runner::Runner,
            task::{
                UnitTaskFetchCompletionCallback, UnitTaskFetchFromStorage, UnitTaskRandomizedPatrol,
            },
            config::UnitConfigKey,
            Unit, UnitTaskHelper,
        },
        world::{object::GameObject, stats::WorldStats},
    },
    log,
    imgui_ui::UiSystem,
    save::PostLoadContext,
    tile::Tile,
    utils::{
        callback::{self, Callback},
        hash::{self, StringHash},
        Color,
    },
};

// ----------------------------------------------
// ServiceConfig
// ----------------------------------------------

#[derive(DrawDebugUi, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub kind: BuildingKind,

    pub name: String,
    pub tile_def_name: String,

    #[debug_ui(skip)]
    #[serde(skip)] // Not serialized. Computed on post_load.
    pub tile_def_name_hash: StringHash,

    pub min_workers: u32,
    pub max_workers: u32,

    pub effect_radius: i32, // How far our patrol unit can go.
    pub requires_road_access: bool,

    #[serde(default)]
    pub has_patrol_unit: bool,

    #[serde(default)]
    pub patrol_unit: UnitConfigKey,

    #[serde(default)]
    pub patrol_frequency_secs: Seconds,

    #[serde(default)] // Optional if no `resources_required`.
    pub stock_update_frequency_secs: Seconds,

    // Kinds of resources required for the service to run, if any.
    #[serde(default)]
    pub resources_required: ResourceKinds,

    // Capacity for each resource kind it accepts.
    #[serde(default)]
    pub stock_capacity: u32,
}

impl Default for ServiceConfig {
    #[inline]
    fn default() -> Self {
        Self { kind: BuildingKind::SmallWell,
               name: "Small Well".into(),
               tile_def_name: "small_well".into(),
               tile_def_name_hash: hash::fnv1a_from_str("small_well"),
               min_workers: 0,
               max_workers: 0,
               effect_radius: 5,
               requires_road_access: false,
               has_patrol_unit: false,
               patrol_unit: UnitConfigKey::default(),
               patrol_frequency_secs: 0.0,
               stock_update_frequency_secs: 0.0,
               resources_required: ResourceKinds::none(),
               stock_capacity: 0 }
    }
}

building_config! {
    ServiceConfig
}

// ----------------------------------------------
// ServiceDebug
// ----------------------------------------------

game_object_debug_options! {
    ServiceDebug,

    // Stops fetching resources from storage.
    freeze_stock_update: bool,

    // Stops sending out units on patrol.
    freeze_patrol: bool,
}

// ----------------------------------------------
// ServiceBuilding
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct ServiceBuilding {
    #[serde(skip)]
    config: Option<&'static ServiceConfig>,
    workers: Workers,

    // Stock of required resources for this service or a treasury for a TaxOffice.
    stock_or_treasury: StockOrTreasury,

    runner: Runner, // Runner Unit we may send out to fetch resources from storage.
    patrol: Patrol, // Unit we may send out on patrol to provide the service.
    patrol_timer: UpdateTimer, // Min time before we can send out a new patrol unit.

    #[serde(skip)]
    debug: ServiceDebug,
}

// ----------------------------------------------
// BuildingBehavior for ServiceBuilding
// ----------------------------------------------

impl BuildingBehavior for ServiceBuilding {
    // ----------------------
    // World Callbacks:
    // ----------------------

    fn name(&self) -> &str {
        &self.config.unwrap().name
    }

    fn configs(&self) -> &dyn BuildingConfig {
        self.config.unwrap()
    }

    fn update(&mut self, context: &BuildingContext) {
        debug_assert!(self.config.is_some());

        let delta_time_secs = context.query.delta_time_secs();
        let has_min_required_workers = self.has_min_required_workers();
        let has_stock_requirements = self.stock_or_treasury.is_stock_and_requires_resources();
        let has_patrol_unit = self.has_patrol_unit();

        // Procure resources from storage periodically if we need them.
        if has_stock_requirements && has_min_required_workers && !self.debug.freeze_stock_update() {
            if let StockOrTreasury::Stock { update_timer, .. } = &mut self.stock_or_treasury {
                if update_timer.tick(delta_time_secs).should_update() {
                    self.stock_update(context);
                }
            }
        }

        if has_patrol_unit
           && has_min_required_workers
           && !self.debug.freeze_patrol()
           && self.patrol_timer.tick(delta_time_secs).should_update()
        {
            self.send_out_patrol_unit(context);
        }
    }

    fn visited_by(&mut self, _unit: &mut Unit, _context: &BuildingContext) {
        // TODO: Do we need anything here? Deliveries are handled by the task completion
        // callback...
        unimplemented!("ServiceBuilding::visited_by() not yet implemented!");
    }

    fn post_load(&mut self, _context: &PostLoadContext, kind: BuildingKind, _tile: &Tile) {
        debug_assert!(kind.intersects(BuildingKind::services()));
        self.patrol.post_load();

        let configs = BuildingConfigs::get();
        let config = configs.find_service_config(kind);

        if let StockOrTreasury::Stock { update_timer, .. } = &mut self.stock_or_treasury {
            update_timer.post_load(config.stock_update_frequency_secs);
        }

        self.patrol_timer.post_load(config.patrol_frequency_secs);
        self.config = Some(config);
    }

    // ----------------------
    // Resources/Stock:
    // ----------------------

    fn has_stock(&self) -> bool {
        matches!(&self.stock_or_treasury, StockOrTreasury::Stock { .. })
    }

    fn is_stock_full(&self) -> bool {
        match &self.stock_or_treasury {
            StockOrTreasury::Stock { stock, .. } => stock.is_full(),
            StockOrTreasury::Treasury { .. } => false,
            StockOrTreasury::None => false,
        }
    }

    fn has_min_required_resources(&self) -> bool {
        match &self.stock_or_treasury {
            StockOrTreasury::Stock { stock, .. } => stock.accepts_any() && !stock.is_empty(),
            StockOrTreasury::Treasury { .. } => true,
            StockOrTreasury::None => true,
        }
    }

    fn available_resources(&self, kind: ResourceKind) -> u32 {
        match &self.stock_or_treasury {
            StockOrTreasury::Stock { stock, .. } => {
                if self.has_min_required_workers() {
                    return stock.available_resources(kind);
                }
            }
            StockOrTreasury::Treasury { gold_units } => {
                // NOTE: Treasury can receive gold even if !has_min_required_workers.
                if kind == ResourceKind::Gold {
                    return *gold_units;
                }
            }
            StockOrTreasury::None => {}
        }
        0
    }

    fn receivable_resources(&self, kind: ResourceKind) -> u32 {
        match &self.stock_or_treasury {
            StockOrTreasury::Stock { stock, .. } => {
                if self.has_min_required_workers() {
                    return stock.receivable_resources(kind);
                }
            }
            StockOrTreasury::Treasury { .. } => {
                // NOTE: Treasury can receive gold even if !has_min_required_workers.
                // No max limit on the amount it can receive.
                if kind == ResourceKind::Gold {
                    return u32::MAX;
                }
            }
            StockOrTreasury::None => {}
        }
        0
    }

    fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        if count != 0 {
            let has_min_required_workers = self.has_min_required_workers();
            match &mut self.stock_or_treasury {
                StockOrTreasury::Stock { stock, .. } => {
                    if has_min_required_workers {
                        let received_count = stock.receive_resources(kind, count);
                        self.debug.log_resources_gained(kind, received_count);
                        return received_count;
                    }
                }
                StockOrTreasury::Treasury { gold_units } => {
                    // NOTE: Treasury can receive gold even if !has_min_required_workers.
                    if kind == ResourceKind::Gold {
                        *gold_units += count;
                        self.debug.log_resources_gained(kind, count);
                        return count;
                    }
                }
                StockOrTreasury::None => {}
            }
        }
        0
    }

    fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        if count != 0 {
            let has_min_required_workers = self.has_min_required_workers();
            match &mut self.stock_or_treasury {
                StockOrTreasury::Stock { stock, .. } => {
                    if has_min_required_workers {
                        let removed_count = stock.remove_resources(kind, count);
                        self.debug.log_resources_lost(kind, removed_count);
                        return removed_count;
                    }
                }
                StockOrTreasury::Treasury { gold_units } => {
                    // NOTE: Can withdraw from the treasury even if !has_min_required_workers.
                    if kind == ResourceKind::Gold {
                        let prev_count = *gold_units;
                        *gold_units = gold_units.saturating_sub(count);
                        let removed_count = prev_count - *gold_units;
                        self.debug.log_resources_lost(kind, removed_count);
                        return removed_count;
                    }
                }
                StockOrTreasury::None => {}
            }
        }
        0
    }

    fn tally(&self, stats: &mut WorldStats, kind: BuildingKind) {
        match &self.stock_or_treasury {
            StockOrTreasury::Stock { stock, .. } => {
                if kind.intersects(BuildingKind::Market) {
                    stock.for_each(|_, item| {
                             stats.add_market_resources(item.kind, item.count);
                         });
                } else {
                    stock.for_each(|_, item| {
                             stats.add_service_resources(item.kind, item.count);
                         });
                }
            }
            StockOrTreasury::Treasury { gold_units } => {
                stats.treasury.gold_units_total += gold_units;
                stats.treasury.gold_units_in_buildings += gold_units;
            }
            StockOrTreasury::None => {}
        }
    }

    // ----------------------
    // Patrol/Runner/Workers:
    // ----------------------

    fn active_patrol(&mut self) -> Option<&mut Patrol> {
        Some(&mut self.patrol)
    }

    fn active_runner(&mut self) -> Option<&mut Runner> {
        Some(&mut self.runner)
    }

    fn workers(&self) -> Option<&Workers> {
        Some(&self.workers)
    }

    fn workers_mut(&mut self) -> Option<&mut Workers> {
        Some(&mut self.workers)
    }

    #[inline]
    fn has_min_required_workers(&self) -> bool {
        if cheats::get().ignore_worker_requirements {
            return true;
        }
        self.workers.as_employer().unwrap().has_min_required()
    }

    // ----------------------
    // Debug:
    // ----------------------

    fn debug_options(&mut self) -> &mut dyn GameObjectDebugOptions {
        &mut self.debug
    }

    fn draw_debug_ui(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        self.draw_debug_ui_resources_stock(context, ui_sys);
        self.draw_debug_ui_patrol(ui_sys);
        self.draw_debug_ui_treasury(ui_sys);
    }
}

// ----------------------------------------------
// ServiceBuilding
// ----------------------------------------------

impl ServiceBuilding {
    pub fn new(config: &'static ServiceConfig) -> Self {
        Self { config: Some(config),
               workers: Workers::employer(config.min_workers, config.max_workers),
               stock_or_treasury: StockOrTreasury::new(config),
               runner: Runner::default(),
               patrol: Patrol::default(),
               patrol_timer: UpdateTimer::new(config.patrol_frequency_secs),
               debug: ServiceDebug::default() }
    }

    pub fn register_callbacks() {
        let _: Callback<UnitTaskFetchCompletionCallback> =
            callback::register!(ServiceBuilding::on_resources_fetched);
        let _: Callback<PatrolCompletionCallback> =
            callback::register!(ServiceBuilding::on_patrol_completed);
    }

    // ----------------------
    // Stock Update:
    // ----------------------

    fn stock_update(&mut self, context: &BuildingContext) {
        if self.is_waiting_on_runner() {
            return; // A runner is already out fetching resources. Try again
                    // later.
        }

        // Unit spawns at the nearest road link.
        let unit_origin = match context.road_link {
            Some(road_link) => road_link,
            None => return, // We are not connected to a road. No stock update possible!
        };

        // Send out a runner:
        let storage_buildings_accepted = BuildingKind::storage(); // Search all storage building kinds.
        let resources_to_fetch = self.resource_fetch_list();
        if resources_to_fetch.is_empty() {
            return;
        }

        self.runner
            .try_fetch_from_storage(context,
                                    unit_origin,
                                    storage_buildings_accepted,
                                    resources_to_fetch,
                                    callback::create!(ServiceBuilding::on_resources_fetched));
    }

    #[inline]
    fn is_stock_empty(&self) -> bool {
        match &self.stock_or_treasury {
            StockOrTreasury::Stock { stock, .. } => stock.is_empty(),
            StockOrTreasury::Treasury { gold_units } => *gold_units == 0,
            StockOrTreasury::None => true,
        }
    }

    #[inline]
    fn is_waiting_on_runner(&self) -> bool {
        self.runner.is_spawned()
    }

    #[inline]
    fn is_runner_fetching_resources(&self, query: &Query) -> bool {
        self.runner.is_running_task::<UnitTaskFetchFromStorage>(query)
    }

    fn on_resources_fetched(this_building: &mut Building, runner_unit: &mut Unit, query: &Query) {
        let this_building_kind = this_building.kind();
        let this_service = this_building.as_service_mut();

        debug_assert!(!runner_unit.inventory_is_empty(),
                      "Runner Unit inventory shouldn't be empty!");
        debug_assert!(this_service.is_runner_fetching_resources(query),
                      "No Runner was sent out by this building!");
        debug_assert!(this_service.runner.unit_id() == runner_unit.id());

        // Try unload cargo:
        if let Some(item) = runner_unit.peek_inventory() {
            debug_assert!(item.count
                          <= this_service.stock_or_treasury.as_stock().capacity_for(item.kind));

            let received_count = this_service.receive_resources(item.kind, item.count);
            if received_count != 0 {
                let removed_count = runner_unit.remove_resources(item.kind, received_count);
                debug_assert!(removed_count == received_count);
            }

            if !runner_unit.inventory_is_empty() {
                // TODO: We have to ship back to storage if we couldn't receive everything!
                log::error!(log::channel!("TODO"),
                            "{} - '{}': Couldn't receive all resources from runner. Implement fallback task for this!",
                            this_building_kind, this_service.name());

                runner_unit.clear_inventory();
            }
        }

        this_service.runner.reset();
        this_service.debug.popup_msg_color(Color::cyan(), "Fetch Task complete");
    }

    fn resource_fetch_list(&self) -> ShoppingList {
        let mut list = ShoppingList::default();
        let stock = self.stock_or_treasury.as_stock();

        stock.for_each(|index, item| {
                 debug_assert!(item.count <= stock.capacity_at(index), "{item}");
                 list.push(StockItem { kind: item.kind,
                                       count: stock.capacity_at(index) - item.count });
             });

        // Items with the highest capacity first.
        list.sort_by_key(|item: &StockItem| Reverse(item.count));
        list
    }

    // ----------------------
    // Patrol Update:
    // ----------------------

    fn send_out_patrol_unit(&mut self, context: &BuildingContext) {
        if self.is_waiting_on_patrol() {
            return; // A patrol unit is already out. Try again later.
        }

        if context.kind == BuildingKind::Market && self.is_stock_empty() {
            // Markets will only send out a patrol if there are goods in stock.
            return;
        }

        // Unit spawns at the nearest road link.
        let unit_origin = match context.road_link {
            Some(road_link) => road_link,
            None => return, // We are not connected to a road!
        };

        let unit_config = self.config.unwrap().patrol_unit;
        let max_patrol_distance = self.config.unwrap().effect_radius;

        // Look for houses to visit:
        self.patrol
            .start_randomized_patrol(context,
                                     unit_origin,
                                     unit_config,
                                     max_patrol_distance,
                                     Some(BuildingKind::House),
                                     callback::create!(ServiceBuilding::on_patrol_completed));
    }

    #[inline]
    fn has_patrol_unit(&self) -> bool {
        self.config.unwrap().has_patrol_unit
    }

    #[inline]
    fn is_waiting_on_patrol(&self) -> bool {
        self.patrol.is_spawned()
    }

    fn on_patrol_completed(this_building: &mut Building,
                           patrol_unit: &mut Unit,
                           query: &Query)
                           -> bool {
        let this_building_kind = this_building.kind();
        let this_service = this_building.as_service_mut();

        debug_assert!(this_service.patrol.unit_id() == patrol_unit.id());
        debug_assert!(this_service.patrol.is_running_task::<UnitTaskRandomizedPatrol>(query),
                      "No Patrol was sent out by this building!");

        if let Some(item) = patrol_unit.peek_inventory() {
            // Only tax collector patrols will bring back resources (Gold).
            if let StockOrTreasury::Treasury { gold_units } = &mut this_service.stock_or_treasury {
                debug_assert!(item.kind == ResourceKind::Gold,
                              "ServiceBuilding Treasury: Expected Gold but got: {item}");

                let tax_collected = patrol_unit.remove_resources(item.kind, item.count);
                *gold_units += tax_collected;

                this_service.debug.popup_msg_color(Color::yellow(),
                                                   format!("Tax collected +{tax_collected}"));
            } else {
                log::error!(log::channel!("unit"),
                            "Patrol unit inventory has {} which {} - '{}' cannot receive!",
                            item,
                            this_building_kind,
                            this_service.name());
            }

            patrol_unit.clear_inventory();
        }

        this_service.patrol.reset();
        this_service.debug.popup_msg_color(Color::magenta(), "Patrol complete");

        true
    }
}

// ----------------------------------------------
// StockOrTreasury
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
enum StockOrTreasury {
    None,
    Stock {
        update_timer: UpdateTimer,
        stock: BuildingStock, // Current local stock of resources required (if any).
    },
    Treasury {
        // Local treasury for a TaxOffice.
        gold_units: u32,
    },
}

impl StockOrTreasury {
    fn new(config: &'static ServiceConfig) -> Self {
        if config.kind.intersects(BuildingKind::treasury()) {
            Self::Treasury { gold_units: 0 }
        } else if !config.resources_required.is_empty() && config.stock_capacity != 0 {
            Self::Stock {
                update_timer: UpdateTimer::new(config.stock_update_frequency_secs),
                stock: BuildingStock::with_accepted_list_and_capacity(&config.resources_required, config.stock_capacity)
            }
        } else {
            Self::None
        }
    }

    fn is_stock_and_requires_resources(&self) -> bool {
        match self {
            Self::Stock { stock, .. } => stock.accepts_any(),
            _ => false,
        }
    }

    fn as_stock(&self) -> &BuildingStock {
        match self {
            Self::Stock { stock, .. } => stock,
            _ => panic!("Not a BuildingStock!"),
        }
    }
}

// ----------------------------------------------
// Debug UI
// ----------------------------------------------

impl ServiceBuilding {
    fn draw_debug_ui_resources_stock(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        if !self.stock_or_treasury.is_stock_and_requires_resources() {
            return;
        }

        let ui = ui_sys.builder();
        if !ui.collapsing_header("Stock", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if self.runner.failed_to_spawn() {
            ui.text_colored(Color::red().to_array(), "Failed to spawn last Runner!");
        }

        if self.is_waiting_on_runner() {
            if self.is_runner_fetching_resources(context.query) {
                ui.text_colored(Color::yellow().to_array(), "Runner sent on Fetch Task.");
            } else {
                ui.text_colored(Color::yellow().to_array(), "Runner sent out. Waiting...");
            }

            if ui.button("Forget Runner") {
                self.runner.reset();
            }
        }

        if let StockOrTreasury::Stock { update_timer, stock } = &mut self.stock_or_treasury {
            update_timer.draw_debug_ui("Update", 0, ui_sys);

            if ui.button("Fill Stock") {
                // Set all to capacity.
                stock.fill();
            }
            if ui.button("Clear Stock") {
                stock.clear();
            }

            stock.draw_debug_ui("Resources", ui_sys);
        }
    }

    fn draw_debug_ui_patrol(&mut self, ui_sys: &UiSystem) {
        if !self.has_patrol_unit() {
            return;
        }

        let ui = ui_sys.builder();
        if !ui.collapsing_header("Patrol", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if self.patrol.failed_to_spawn() {
            ui.text_colored(Color::red().to_array(), "Failed to spawn last Patrol!");
        }

        if self.is_waiting_on_patrol() {
            ui.text_colored(Color::yellow().to_array(), "Patrol sent out. Waiting...");
            if ui.button("Forget Patrol") {
                self.patrol.reset();
            }
        }

        self.patrol_timer.draw_debug_ui("Patrol", 0, ui_sys);
        self.patrol.draw_debug_ui("Patrol Params", ui_sys);
    }

    fn draw_debug_ui_treasury(&mut self, ui_sys: &UiSystem) {
        if let StockOrTreasury::Treasury { gold_units } = &mut self.stock_or_treasury {
            let ui = ui_sys.builder();
            if ui.collapsing_header("Treasury", imgui::TreeNodeFlags::empty()) {
                ui.input_scalar("Gold Units", gold_units).step(1).build();
            }
        }
    }
}
