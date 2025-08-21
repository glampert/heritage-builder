use std::cmp::Reverse;
use proc_macros::DrawDebugUi;

use crate::{
    game_object_debug_options,
    imgui_ui::UiSystem,
    utils::{
        Color,
        Seconds,
        hash::StringHash,
        coords::{CellRange, WorldToScreenTransform}
    },
    game::{
        unit::{
            Unit,
            UnitTaskHelper,
            patrol::Patrol,
            runner::Runner,
            task::{UnitTaskFetchFromStorage, UnitTaskRandomizedPatrol}
        },
        sim::{
            Query,
            UpdateTimer,
            resources::{
                ShoppingList,
                ResourceKind,
                ResourceKinds,
                ResourceStock,
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
    BuildingContext
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
    pub has_patrol_unit: bool,
    pub patrol_frequency_secs: Seconds,

    pub stock_update_frequency_secs: Seconds,
    pub stock_capacity: u32, // Capacity for each resource kind it accepts.

    // Kinds of resources required for the service to run, if any.
    pub resources_required: ResourceKinds,
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

pub struct ServiceBuilding<'config> {
    config: &'config ServiceConfig,
    workers: Workers,

    stock_update_timer: UpdateTimer,
    stock_capacity: u32,
    stock: ResourceStock, // Current local stock of resources.

    runner: Runner, // Runner Unit we may send out to fetch resources from storage.
    patrol: Patrol, // Unit we may send out on patrol to provide the service.
    patrol_timer: UpdateTimer, // Min time before we can send out a new patrol unit.

    debug: ServiceDebug,
}

// ----------------------------------------------
// BuildingBehavior for ServiceBuilding
// ----------------------------------------------

impl<'config> BuildingBehavior<'config> for ServiceBuilding<'config> {
    // ----------------------
    // World Callbacks:
    // ----------------------

    fn name(&self) -> &str {
        &self.config.name
    }

    fn update(&mut self, context: &BuildingContext) {
        let delta_time_secs = context.query.delta_time_secs();

        // Procure resources from storage periodically if we need them.
        if self.stock.accepts_any() &&
           self.stock_update_timer.tick(delta_time_secs).should_update() &&
          !self.debug.freeze_stock_update() {
            self.stock_update(context);
        }

        if self.has_patrol_unit() &&
           self.patrol_timer.tick(delta_time_secs).should_update() &&
          !self.debug.freeze_patrol() {
            self.send_out_patrol_unit(context);
        }
    }

    fn visited_by(&mut self, _unit: &mut Unit, _context: &BuildingContext) {
        // TODO: Do we need anything here? Deliveries are handled by the task completion callback...
        unimplemented!("ServiceBuilding::visited_by() not yet implemented!");
    }

    // ----------------------
    // Resources/Stock:
    // ----------------------

    fn available_resources(&self, kind: ResourceKind) -> u32 {
        debug_assert!(kind.bits().count_ones() == 1);
        self.stock.count(kind)
    }

    fn receivable_resources(&self, kind: ResourceKind) -> u32 {
        debug_assert!(kind.bits().count_ones() == 1);
        let mut capacity_left = 0;
        if let Some((_, item)) = self.stock.find(kind) {
            capacity_left = self.stock_capacity - item.count;
        }
        capacity_left
    }

    fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(kind.bits().count_ones() == 1);
        if count != 0 {
            let capacity_left = self.receivable_resources(kind);
            if capacity_left != 0 {
                let add_count = count.min(capacity_left);
                self.stock.add(kind, add_count);
                self.debug.log_resources_gained(kind, add_count);
                return add_count;
            }
        }
        0
    }

    fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(kind.bits().count_ones() == 1);
        if count != 0 {
            let available_count = self.available_resources(kind);
            if available_count != 0 {
                let remove_count = count.min(available_count);
                if let Some(removed) = self.stock.remove(kind, remove_count) {
                    self.debug.log_resources_lost(removed, remove_count);
                    return remove_count;
                }
            }
        }
        0
    }

    // ----------------------
    // Patrol/Runner Units:
    // ----------------------

    fn active_patrol(&mut self) -> Option<&mut Patrol> {
        Some(&mut self.patrol)
    }

    fn active_runner(&mut self) -> Option<&mut Runner> {
        Some(&mut self.runner)
    }

    // ----------------------
    // Debug:
    // ----------------------

    fn draw_debug_ui(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        self.draw_debug_ui_config(ui_sys);
        self.debug.draw_debug_ui(ui_sys);
        self.draw_debug_ui_resources_stock(context, ui_sys);
        self.draw_debug_ui_patrol(ui_sys);
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

// ----------------------------------------------
// ServiceBuilding
// ----------------------------------------------

impl<'config> ServiceBuilding<'config> {
    pub fn new(config: &'config ServiceConfig) -> Self {
        Self {
            config,
            workers: Workers::new(config.min_workers, config.max_workers),
            stock_update_timer: UpdateTimer::new(config.stock_update_frequency_secs),
            stock_capacity: config.stock_capacity,
            stock: ResourceStock::with_accepted_list(&config.resources_required),
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
            Some(Self::on_resources_fetched));
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
            debug_assert!(item.count <= this_service.stock_capacity);

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

        self.stock.for_each(|_, item| {
            list.push(StockItem { kind: item.kind, count: self.stock_capacity - item.count });
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

        // Unit spawns at the nearest road link.
        let unit_origin = match context.road_link {
            Some(road_link) => road_link,
            None => return, // We are not connected to a road!
        };

        // Look for houses to visit:
        self.patrol.start_randomized_patrol(
            context,
            unit_origin,
            self.config.effect_radius,
            Some(BuildingKind::House),
            Some(Self::on_patrol_completed));
    }

    #[inline]
    fn has_patrol_unit(&self) -> bool {
        self.config.has_patrol_unit
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
    fn draw_debug_ui_config(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header("Config", imgui::TreeNodeFlags::empty()) {
            self.config.draw_debug_ui(ui_sys);
        }
    }

    fn draw_debug_ui_resources_stock(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        if !self.stock.accepts_any() {
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

        if ui.button("Fill Stock") {
            // Set all to capacity.
            self.stock.for_each_mut(|_, item| item.count = self.stock_capacity);
        }
        if ui.button("Clear Stock") {
            self.stock.clear();
        }

        self.stock.draw_debug_ui_clamped_counts("Resources", 0, self.stock_capacity, ui_sys);
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
