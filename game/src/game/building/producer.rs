use std::cmp::Reverse;
use smallvec::SmallVec;
use proc_macros::DrawDebugUi;

use crate::{
    game_object_debug_options,
    building_config_impl,
    imgui_ui::UiSystem,
    utils::{
        Color,
        Seconds,
        hash::StringHash
    },
    game::{
        unit::{
            Unit,
            UnitTaskHelper,
            runner::Runner,
            task::{
                UnitTaskDeliverToStorage,
                UnitTaskFetchFromStorage
            }
        },
        sim::{
            Query,
            UpdateTimer,
            world::WorldStats,
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
    config::BuildingConfig
};

// ----------------------------------------------
// ProducerConfig
// ----------------------------------------------

#[derive(DrawDebugUi)]
pub struct ProducerConfig {
    pub name: String,
    pub tile_def_name: String,

    #[debug_ui(skip)]
    pub tile_def_name_hash: StringHash,

    pub min_workers: u32,
    pub max_workers: u32,

    pub production_output_frequency_secs: Seconds,

    // Producer output: A raw material or a consumer good.
    pub production_output: ResourceKind,
    pub production_capacity: u32,

    // Kinds of raw materials required for production, if any.
    pub resources_required: ResourceKinds,
    pub resources_capacity: u32,

    // Where we can deliver our production to (Granary, StorageYard).
    pub deliver_to_storage_kinds: BuildingKind,

    // Where to find out production input raw materials.
    pub fetch_from_storage_kinds: BuildingKind,
}

building_config_impl!(ProducerConfig);

// ----------------------------------------------
// ProducerDebug
// ----------------------------------------------

game_object_debug_options! {
    ProducerDebug,

    // Stops goods from being produced and stock from being spent.
    freeze_production: bool,

    // Stop delivering production output from local stock to storage buildings.
    freeze_storage_delivery: bool,

    // Stop fetching raw materials from storage.
    freeze_storage_fetching: bool,
}

// ----------------------------------------------
// ProducerBuilding
// ----------------------------------------------

pub struct ProducerBuilding<'config> {
    config: &'config ProducerConfig,
    workers: Workers,

    production_update_timer: UpdateTimer,
    production_input_stock:  ProducerInputsLocalStock, // Local stock of required raw materials.
    production_output_stock: ProducerOutputLocalStock, // Local production output storage.

    // Runner Unit we may send out to deliver our production or fetch raw materials.
    runner: Runner,

    debug: ProducerDebug,
}

// ----------------------------------------------
// BuildingBehavior for ProducerBuilding
// ----------------------------------------------

impl<'config> BuildingBehavior<'config> for ProducerBuilding<'config> {
    // ----------------------
    // World Callbacks:
    // ----------------------

    fn name(&self) -> &str {
        &self.config.name
    }

    fn configs(&self) -> &dyn BuildingConfig {
        self.config
    }

    fn update(&mut self, context: &BuildingContext) {
        let delta_time_secs = context.query.delta_time_secs();

        // Update producer states:
        if self.production_update_timer.tick(delta_time_secs).should_update() {
            if !self.debug.freeze_production() {
                self.production_update();
            }
            if !self.debug.freeze_storage_delivery() {
                self.deliver_to_storage(context);
            }
            if !self.debug.freeze_storage_fetching() {
                self.fetch_from_storage(context);
            }
        }
    }

    fn visited_by(&mut self, unit: &mut Unit, context: &BuildingContext) {
        // We can only accept resource deliveries here.
        debug_assert!(unit.is_running_task::<UnitTaskDeliverToStorage>(context.query.task_manager()));

        if self.is_runner_fetching_resources(context.query) {
            // If we've already sent out a runner to fetch some resources we'll refuse deliveries.
            // Let this runner deliver the resources somewhere else.
            return;
        }

        // Try unload cargo:
        if let Some(item) = unit.peek_inventory() {
            debug_assert!(item.count != 0, "{item}");

            let received_count = self.receive_resources(item.kind, item.count);
            if received_count != 0 {
                let removed_count = unit.remove_resources(item.kind, received_count);
                debug_assert!(removed_count == received_count);

                self.debug.popup_msg(format!("{} received delivery -> {}", self.name(), unit.name()));
            }
        }
    }

    // ----------------------
    // Resources/Stock:
    // ----------------------

    fn available_resources(&self, kind: ResourceKind) -> u32 {
        self.production_output_stock.available_resources(kind)
    }

    fn receivable_resources(&self, kind: ResourceKind) -> u32 {
        // TODO: If we are not operating (no workers),
        // make this return zero so storage search will ignore it.
        self.production_input_stock.receivable_resources(kind)
    }

    // Returns number of resources it was able to accommodate, which can be less than `count`.
    fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        if count != 0 {
            let received_count = self.production_input_stock.receive_resources(kind, count);
            self.debug.log_resources_gained(kind, received_count);
            return received_count;
        }
        0
    }

    fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        if count != 0 {
            let removed_count = self.production_output_stock.remove_resources(kind, count);
            self.debug.log_resources_lost(kind, removed_count);
            return removed_count;
        }
        0
    }

    fn tally(&self, stats: &mut WorldStats, _kind: BuildingKind) {
        for item in &self.production_input_stock.slots {
            stats.add_producer_resources(item.kind, item.count);
        }

        let item = &self.production_output_stock.item;
        stats.add_producer_resources(item.kind, item.count);
    }

    // ----------------------
    // Runner Unit / Workers:
    // ----------------------

    fn active_runner(&mut self) -> Option<&mut Runner> {
        Some(&mut self.runner)
    }

    fn workers(&self) -> Option<Workers> {
        Some(self.workers)
    }

    // ----------------------
    // Debug:
    // ----------------------

    fn debug_options(&mut self) -> &mut dyn GameObjectDebugOptions {
        &mut self.debug
    }

    fn draw_debug_ui(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        self.draw_debug_ui_input_stock(ui_sys);
        self.draw_debug_ui_production_output(context, ui_sys);
    }
}

// ----------------------------------------------
// ProducerBuilding
// ----------------------------------------------

impl<'config> ProducerBuilding<'config> {
    pub fn new(config: &'config ProducerConfig) -> Self {
        Self {
            config,
            workers: Workers::new(config.min_workers, config.max_workers),
            production_update_timer: UpdateTimer::new(config.production_output_frequency_secs),
            production_input_stock: ProducerInputsLocalStock::new(
                &config.resources_required,
                config.resources_capacity
            ),
            production_output_stock: ProducerOutputLocalStock::new(
                config.production_output,
                config.production_capacity
            ),
            runner: Runner::default(),
            debug: ProducerDebug::default(),
        }
    }

    fn production_update(&mut self) {
        // Production halts if the local stock is full.
        if !self.production_output_stock.is_full() {
            let mut produce_one_item = true;

            // If we have raw material requirements, first check if they are available in stock.
            if self.production_input_stock.requires_any_resource() {
                if self.production_input_stock.has_required_resources() {
                    // Consume our raw materials (one of each).
                    self.production_input_stock.consume_resources(&mut self.debug);
                } else {
                    // We are missing one or more raw materials, halt production.
                    produce_one_item = false;
                }
            }

            // Produce one item and store it locally:
            if produce_one_item {
                self.production_output_stock.store_resources(1);
                self.debug.log_resources_gained(self.production_output_stock.resource_kind(), 1);
            }
        }
    }

    fn deliver_to_storage(&mut self, context: &BuildingContext) {
        if self.production_output_stock.is_empty() {
            return; // Nothing to deliver.
        }

        if self.is_waiting_on_runner() {
            return; // A runner is already out fetching/delivering resources. Try again later.
        }

        // Unit spawns at the nearest road link.
        let unit_origin = match context.road_link {
            Some(road_link) => road_link,
            None => return, // We are not connected to a road. No delivery possible!
        };

        // Send out a runner:
        let storage_buildings_accepted = self.config.deliver_to_storage_kinds;
        let resource_kind_to_deliver = self.production_output_stock.resource_kind();
        let resource_count = self.production_output_stock.resource_count();

        if self.runner.try_deliver_to_storage(
            context,
            unit_origin,
            storage_buildings_accepted,
            resource_kind_to_deliver,
            resource_count,
            Some(Self::on_resources_delivered)) {
            // We've handed over our resources to the spawned unit, clear the stock.
            let removed_count = self.remove_resources(resource_kind_to_deliver, resource_count);
            debug_assert!(removed_count == resource_count);
        }
    }

    fn fetch_from_storage(&mut self, context: &BuildingContext) {
        if !self.production_input_stock.requires_any_resource() {
            return; // We don't require any raw materials.
        }

        if self.production_input_stock.is_full() {
            return; // No room.
        }

        if self.is_waiting_on_runner() {
            return; // A runner is already out fetching/delivering resources. Try again later.
        }

        // Unit spawns at the nearest road link.
        let unit_origin = match context.road_link {
            Some(road_link) => road_link,
            None => return, // We are not connected to a road. No fetching possible!
        };

        // Send out a runner:
        let storage_buildings_accepted = self.config.fetch_from_storage_kinds;
        let resources_to_fetch = self.production_input_stock.resource_fetch_list();
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

    fn on_resources_delivered(this_building: &mut Building, runner_unit: &mut Unit, query: &Query) {
        let this_producer = this_building.as_producer_mut();

        debug_assert!(runner_unit.inventory_is_empty(), "Runner Unit should have delivered all resourced by now!");
        debug_assert!(this_producer.is_runner_delivering_resources(query), "No Runner was sent out by this building!");
        debug_assert!(this_producer.runner.unit_id() == runner_unit.id());

        this_producer.runner.reset();
        this_producer.debug.popup_msg_color(Color::cyan(), "Delivery Task complete");
    }

    fn on_resources_fetched(this_building: &mut Building, runner_unit: &mut Unit, query: &Query) {
        let this_producer = this_building.as_producer_mut();

        debug_assert!(!runner_unit.inventory_is_empty(), "Runner Unit inventory shouldn't be empty!");
        debug_assert!(this_producer.is_runner_fetching_resources(query), "No Runner was sent out by this building!");
        debug_assert!(this_producer.runner.unit_id() == runner_unit.id());

        // Try unload cargo:
        if let Some(item) = runner_unit.peek_inventory() {
            debug_assert!(item.count != 0, "{item}");
            debug_assert!(item.count <= this_producer.production_input_stock.capacity(), "{item}");

            let received_count = this_producer.receive_resources(item.kind, item.count);
            if received_count != 0 {
                let removed_count = runner_unit.remove_resources(item.kind, received_count);
                debug_assert!(removed_count == received_count);
            }

            if !runner_unit.inventory_is_empty() {
                // TODO: We have to ship back to storage if we couldn't receive everything!
                todo!("Couldn't receive all resources. Implement fallback task for this!");
            }
        }

        this_producer.runner.reset();
        this_producer.debug.popup_msg_color(Color::cyan(), "Fetch Task complete");
    }

    #[inline]
    fn is_waiting_on_runner(&self) -> bool {
        self.runner.is_spawned()
    }

    #[inline]
    fn is_runner_delivering_resources(&self, query: &Query) -> bool {
        self.runner.is_running_task::<UnitTaskDeliverToStorage>(query)
    }

    #[inline]
    fn is_runner_fetching_resources(&self, query: &Query) -> bool {
        self.runner.is_running_task::<UnitTaskFetchFromStorage>(query)
    }

    fn is_production_halted(&self) -> bool {
        if self.debug.freeze_production() {
            return true;
        }
        if self.production_output_stock.is_full() {
            return true;
        }
        if self.production_input_stock.requires_any_resource() &&
          !self.production_input_stock.has_required_resources() {
            return true;
        }
        false
    }
}

// ----------------------------------------------
// ProducerOutputLocalStock
// ----------------------------------------------

struct ProducerOutputLocalStock {
    item: StockItem,
    capacity: u32,
}

impl ProducerOutputLocalStock {
    fn new(output_kind: ResourceKind, capacity: u32) -> Self {
        debug_assert!(output_kind.is_single_resource()); // One flag (kind) only.
        Self {
            item: StockItem { kind: output_kind, count: 0 },
            capacity,
        }
    }

    fn fill(&mut self) {
        self.item.count = self.capacity;
    }

    fn clear(&mut self) {
        self.item.count = 0;
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.item.count >= self.capacity
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.item.count == 0
    }

    #[inline]
    fn resource_kind(&self) -> ResourceKind {
        self.item.kind
    }

    #[inline]
    fn resource_count(&self) -> u32 {
        self.item.count
    }

    #[inline]
    fn available_resources(&self, kind: ResourceKind) -> u32 {
        debug_assert!(kind.is_single_resource());
        if self.item.kind == kind {
            self.item.count
        } else {
            0
        }
    }

    #[inline]
    fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(kind.is_single_resource());
        if self.item.kind == kind {
            let prev_count = self.item.count;
            let new_count  = prev_count.saturating_sub(count);
            self.item.count = new_count;
            prev_count - new_count
        } else {
            0
        }
    }

    #[inline]
    fn store_resources(&mut self, count: u32) {
        debug_assert!(self.item.count + count <= self.capacity);
        self.item.count += count;
    }
}

// ----------------------------------------------
// ProducerInputsLocalStock
// ----------------------------------------------

struct ProducerInputsLocalStock {
    slots: SmallVec<[StockItem; 1]>,
    capacity: u32, // Capacity for each resource kind.
}

impl ProducerInputsLocalStock {
    fn new(resources_required: &ResourceKinds, capacity: u32) -> Self {
        let mut slots = SmallVec::new();
        resources_required.for_each(|kind| {
            slots.push(StockItem { kind, count: 0 });
            true
        });
        Self { slots, capacity }
    }

    fn fill(&mut self) {
        for slot in &mut self.slots {
            slot.count = self.capacity;
        }
    }

    fn clear(&mut self) {
        for slot in &mut self.slots {
            slot.count = 0;
        }
    }

    #[inline]
    fn is_full(&self) -> bool {
        for slot in &self.slots {
            if slot.count < self.capacity {
                return false;
            }
        }
        true
    }

    #[inline]
    fn is_empty(&self) -> bool {
        for slot in &self.slots {
            if slot.count != 0 {
                return false;
            }
        }
        true
    }

    #[inline]
    fn capacity(&self) -> u32 {
        self.capacity
    }

    #[inline]
    fn resource_fetch_list(&self) -> ShoppingList {
        let mut list = ShoppingList::default();

        for slot in &self.slots {
            list.push(StockItem { kind: slot.kind, count: self.capacity - slot.count });
        }

        // Items with the highest capacity first.
        list.sort_by_key(|item: &StockItem| Reverse(item.count));
        list
    }

    #[inline]
    fn requires_any_resource(&self) -> bool {
        !self.slots.is_empty()
    }

    #[inline]
    fn has_required_resources(&self) -> bool {
        for slot in &self.slots {
            if slot.count == 0 {
                return false;
            }
        }
        true
    }

    #[inline]
    fn consume_resources(&mut self, debug: &mut ProducerDebug) {
        for slot in &mut self.slots {
            debug_assert!(slot.count != 0);
            slot.count -= 1;
            debug.log_resources_lost(slot.kind, 1);
        }
    }

    #[inline]
    fn receivable_resources(&self, kind: ResourceKind) -> u32 {
        debug_assert!(kind.is_single_resource());
        for slot in &self.slots {
            if slot.kind == kind {
                return self.capacity - slot.count;
            }
        }
        0
    }

    #[inline]
    fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(kind.is_single_resource());
        for slot in &mut self.slots {
            if slot.kind == kind {
                let prev_count = slot.count;
                let new_count  = (prev_count + count).min(self.capacity);
                slot.count = new_count;
                return new_count - prev_count;
            }
        }
        0
    }
}

// ----------------------------------------------
// Debug UI
// ----------------------------------------------

impl ProducerOutputLocalStock {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        ui.text("Local Stock:");

        if ui.input_scalar(format!("{}", self.item.kind), &mut self.item.count).step(1).build() {
            self.item.count = self.item.count.min(self.capacity);
        }

        ui.text("Is full:");
        ui.same_line();
        if self.is_full() {
            ui.text_colored(Color::red().to_array(), "yes");
        } else {
            ui.text("no");
        }
    }
}

impl ProducerInputsLocalStock {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if self.slots.is_empty() {
            ui.text("<none>");
        } else {
            let capacity = self.capacity;

            for (index, item) in self.slots.iter_mut().enumerate() {
                let label = format!("{}##_stock_item_{}", item.kind, index);

                if ui.input_scalar(label, &mut item.count).step(1).build() {
                    item.count = item.count.min(capacity);
                }

                let capacity_left = capacity - item.count;
                let is_full = item.count >= capacity;

                ui.same_line();
                if is_full {
                    ui.text_colored(Color::red().to_array(), "(full)");
                } else {
                    ui.text(format!("({} left)", capacity_left));
                }
            }
        }
    }
}

impl ProducerBuilding<'_> {
    fn draw_debug_ui_input_stock(&mut self, ui_sys: &UiSystem) {
        if self.production_input_stock.requires_any_resource() {
            let ui = ui_sys.builder();
            if ui.collapsing_header("Raw Materials In Stock", imgui::TreeNodeFlags::empty()) {
                self.production_input_stock.draw_debug_ui(ui_sys);

                if ui.button("Fill Stock##_fill_input_stock") {
                    self.production_input_stock.fill();
                }
                if ui.button("Clear Stock##_clear_input_stock") {
                    self.production_input_stock.clear();
                }
            }
        }
    }

    fn draw_debug_ui_production_output(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header("Production Output", imgui::TreeNodeFlags::empty()) {
            if self.is_production_halted() {
                ui.text_colored(Color::red().to_array(), "Production Halted:");
                ui.same_line();
                if self.production_output_stock.is_full() {
                    ui.text_colored(Color::red().to_array(), "Local Stock Full!");
                } else if self.production_input_stock.requires_any_resource() &&
                         !self.production_input_stock.has_required_resources() {
                    ui.text_colored(Color::red().to_array(), "Missing Resources!");
                } else {
                    ui.text_colored(Color::red().to_array(), "Production Frozen.");
                }
            }

            if self.runner.failed_to_spawn() {
                ui.text_colored(Color::red().to_array(), "Failed to spawn last Runner!");
            }

            if self.is_waiting_on_runner() {
                if self.is_runner_delivering_resources(context.query) {
                    ui.text_colored(Color::yellow().to_array(), "Runner sent on Delivery Task.");
                } else if self.is_runner_fetching_resources(context.query) {
                    ui.text_colored(Color::yellow().to_array(), "Runner sent on Fetch Task.");
                } else {
                    ui.text_colored(Color::yellow().to_array(), "Runner sent out. Waiting...");
                }

                if ui.button("Forget Runner") {
                    self.runner.reset();
                }
            }

            self.production_update_timer.draw_debug_ui("Update", 0, ui_sys);
            self.production_output_stock.draw_debug_ui(ui_sys);

            if ui.button("Fill Stock##_fill_output_stock") {
                self.production_output_stock.fill();
            }
            if ui.button("Clear Stock##_clear_output_stock") {
                self.production_output_stock.clear();
            }
        }
    }
}
