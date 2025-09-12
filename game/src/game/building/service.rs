use std::cmp::Reverse;
use proc_macros::DrawDebugUi;

use serde::{
    Serialize,
    Deserialize
};

use crate::{
    game_object_debug_options,
    building_config_impl,
    imgui_ui::UiSystem,
    save::PostLoadContext,
    tile::Tile,
    utils::{
        Color,
        Seconds,
        callback,
        hash::StringHash,
    },
    game::{
        cheats,
        unit::{
            Unit,
            UnitTaskHelper,
            runner::Runner,
            patrol::Patrol,
            task::{
                UnitTaskFetchFromStorage,
                UnitTaskRandomizedPatrol
            }
        },
        world::{
            stats::WorldStats,
            object::GameObject
        },
        sim::{
            Query,
            UpdateTimer,
            resources::{
                ShoppingList,
                ResourceKind,
                ResourceKinds,
                StockItem,
                Workers
            }
        }
    }
};

use super::{
    Building,
    BuildingKind,
    BuildingBehavior,
    BuildingContext,
    BuildingStock,
    config::BuildingConfig
};

// ----------------------------------------------
// ServiceConfig
// ----------------------------------------------

#[derive(DrawDebugUi)]
pub struct ServiceConfig {
    pub name: String,
    pub tile_def_name: String,

    #[debug_ui(skip)]
    pub tile_def_name_hash: StringHash,

    pub min_workers: u32,
    pub max_workers: u32,

    pub effect_radius: i32, // How far our patrol unit can go.
    pub requires_road_access: bool,
    pub has_patrol_unit: bool,
    pub patrol_frequency_secs: Seconds,

    pub stock_update_frequency_secs: Seconds,
    pub stock_capacity: u32, // Capacity for each resource kind it accepts.

    // Kinds of resources required for the service to run, if any.
    pub resources_required: ResourceKinds,
}

building_config_impl!(ServiceConfig);

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
pub struct ServiceBuilding<'config> {
    #[serde(skip)] config: Option<&'config ServiceConfig>,
    workers: Workers,

    stock_update_timer: UpdateTimer,
    stock: Option<BuildingStock>, // Current local stock of resources required (if any).

    runner: Runner, // Runner Unit we may send out to fetch resources from storage.
    patrol: Patrol, // Unit we may send out on patrol to provide the service.
    patrol_timer: UpdateTimer, // Min time before we can send out a new patrol unit.

    #[serde(skip)] debug: ServiceDebug,
}

// ----------------------------------------------
// BuildingBehavior for ServiceBuilding
// ----------------------------------------------

impl<'config> BuildingBehavior<'config> for ServiceBuilding<'config> {
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
        let has_stock_requirements = self.stock.as_ref().is_some_and(|stock| stock.accepts_any());

        // Procure resources from storage periodically if we need them.
        if has_stock_requirements &&
           self.stock_update_timer.tick(delta_time_secs).should_update() &&
           has_min_required_workers && !self.debug.freeze_stock_update() {
            self.stock_update(context);
        }

        if self.has_patrol_unit() &&
           self.patrol_timer.tick(delta_time_secs).should_update() &&
           has_min_required_workers && !self.debug.freeze_patrol() {
            self.send_out_patrol_unit(context);
        }
    }

    fn visited_by(&mut self, _unit: &mut Unit, _context: &BuildingContext) {
        // TODO: Do we need anything here? Deliveries are handled by the task completion callback...
        unimplemented!("ServiceBuilding::visited_by() not yet implemented!");
    }

    fn post_load(&mut self, context: &PostLoadContext<'_, '_, 'config>, kind: BuildingKind, _tile: &Tile) {
        debug_assert!(kind.intersects(BuildingKind::services()));
        self.config = Some(context.building_configs.find_service_config(kind));
        self.patrol.post_load();
    }

    // ----------------------
    // Resources/Stock:
    // ----------------------

    fn is_stock_full(&self) -> bool {
        self.stock.as_ref().is_none_or(|stock| stock.is_full())
    }

    fn has_min_required_resources(&self) -> bool {
        if let Some(stock) = &self.stock {
            if stock.accepts_any() {
                return !stock.is_empty();
            }
        }
        true
    }

    fn available_resources(&self, kind: ResourceKind) -> u32 {
        if self.has_min_required_workers() {
            if let Some(stock) = &self.stock {
                return stock.available_resources(kind);
            }
        }
        0
    }

    fn receivable_resources(&self, kind: ResourceKind) -> u32 {
        if self.has_min_required_workers() {
            if let Some(stock) = &self.stock {
                return stock.receivable_resources(kind);
            }
        }
        0
    }

    fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        if count != 0 && self.has_min_required_workers() {
            if let Some(stock) = &mut self.stock {
                let received_count = stock.receive_resources(kind, count);
                self.debug.log_resources_gained(kind, received_count);
                return received_count;
            }
        }
        0
    }

    fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        if count != 0 && self.has_min_required_workers() {
            if let Some(stock) = &mut self.stock {
                let removed_count = stock.remove_resources(kind, count);
                self.debug.log_resources_lost(kind, removed_count);
                return removed_count;
            }
        }
        0
    }

    fn tally(&self, stats: &mut WorldStats, kind: BuildingKind) {
        if let Some(stock) = &self.stock {
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
    }

    // ----------------------
    // Patrol/Runner/Workers:
    // ----------------------

    fn active_patrol(&mut self) -> Option<&mut Patrol> { Some(&mut self.patrol) }
    fn active_runner(&mut self) -> Option<&mut Runner> { Some(&mut self.runner) }

    fn workers(&self) -> Option<&Workers> { Some(&self.workers) }
    fn workers_mut(&mut self) -> Option<&mut Workers> { Some(&mut self.workers) }

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
    }
}

// ----------------------------------------------
// ServiceBuilding
// ----------------------------------------------

impl<'config> ServiceBuilding<'config> {
    pub fn new(config: &'config ServiceConfig) -> Self {
        let stock = {
            if config.resources_required.is_empty() || config.stock_capacity == 0 {
                None
            } else {
                Some(BuildingStock::with_accepted_list_and_capacity(
                    &config.resources_required,
                    config.stock_capacity
                ))
            }
        };

        Self {
            config: Some(config),
            workers: Workers::employer(config.min_workers, config.max_workers),
            stock_update_timer: UpdateTimer::new(config.stock_update_frequency_secs),
            stock,
            runner: Runner::default(),
            patrol: Patrol::default(),
            patrol_timer: UpdateTimer::new(config.patrol_frequency_secs),
            debug: ServiceDebug::default(),
        }
    }

    // ----------------------
    // Stock Update:
    // ----------------------

    fn stock_update(&mut self, context: &BuildingContext) {
        if self.is_waiting_on_runner() {
            return; // A runner is already out fetching resources. Try again later.
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

        self.runner.try_fetch_from_storage(
            context,
            unit_origin,
            storage_buildings_accepted,
            resources_to_fetch,
            callback::create!(ServiceBuilding::on_resources_fetched));
    }

    #[inline]
    fn is_stock_empty(&self) -> bool {
        self.stock.as_ref().is_none_or(|stock| stock.is_empty())
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
        let this_service = this_building.as_service_mut();

        debug_assert!(!runner_unit.inventory_is_empty(), "Runner Unit inventory shouldn't be empty!");
        debug_assert!(this_service.is_runner_fetching_resources(query), "No Runner was sent out by this building!");
        debug_assert!(this_service.runner.unit_id() == runner_unit.id());

        // Try unload cargo:
        if let Some(item) = runner_unit.peek_inventory() {
            debug_assert!(item.count <= this_service.stock.as_ref().unwrap().capacity_for(item.kind));

            let received_count = this_service.receive_resources(item.kind, item.count);
            if received_count != 0 {
                let removed_count = runner_unit.remove_resources(item.kind, received_count);
                debug_assert!(removed_count == received_count);
            }

            if !runner_unit.inventory_is_empty() {
                // TODO: We have to ship back to storage if we couldn't receive everything!
                todo!("Couldn't receive all resources. Implement fallback task for this!");
            }
        }

        this_service.runner.reset();
        this_service.debug.popup_msg_color(Color::cyan(), "Fetch Task complete");
    }

    fn resource_fetch_list(&self) -> ShoppingList {
        let mut list = ShoppingList::default();
        let stock = self.stock.as_ref().unwrap();

        stock.for_each(|index, item| {
            debug_assert!(item.count <= stock.capacity_at(index), "{item}");
            list.push(StockItem { kind: item.kind, count: stock.capacity_at(index) - item.count });
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

        // Look for houses to visit:
        self.patrol.start_randomized_patrol(
            context,
            unit_origin,
            self.config.unwrap().effect_radius,
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

    fn on_patrol_completed(this_building: &mut Building, patrol_unit: &mut Unit, query: &Query) -> bool {
        let this_service = this_building.as_service_mut();

        debug_assert!(this_service.patrol.unit_id() == patrol_unit.id());
        debug_assert!(this_service.patrol.is_running_task::<UnitTaskRandomizedPatrol>(query),
                      "No Patrol was sent out by this building!");

        this_service.patrol.reset();
        this_service.debug.popup_msg_color(Color::magenta(), "Patrol complete");

        true
    }
}

// ----------------------------------------------
// Debug UI
// ----------------------------------------------

impl ServiceBuilding<'_> {
    fn draw_debug_ui_resources_stock(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        if self.stock.as_ref().is_none_or(|stock| !stock.accepts_any()) {
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

        self.stock_update_timer.draw_debug_ui("Update", 0, ui_sys);

        let stock = self.stock.as_mut().unwrap();
        if ui.button("Fill Stock") {
            // Set all to capacity.
            stock.fill();
        }
        if ui.button("Clear Stock") {
            stock.clear();
        }

        stock.draw_debug_ui("Resources", ui_sys);
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
}
