use smallvec::SmallVec;
use proc_macros::DrawDebugUi;

use crate::{
    game_object_debug_options,
    imgui_ui::UiSystem,
    utils::{
        Color,
        Seconds,
        hash::StringHash,
        coords::{CellRange, WorldToScreenTransform},
    },
    game::sim::{
        UpdateTimer,
        resources::{
            ResourceKind,
            ResourceKinds,
            StockItem,
            Workers
        }
    }
};

use super::{
    BuildingKind,
    BuildingBehavior,
    BuildingContext,
    unit::{Unit, UnitTask}
};

// ----------------------------------------------
// TODO List
// ----------------------------------------------

// - Ship production to nearest storage (send unit out with cargo).
// - Get raw materials from storage OR from other producers directly.

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

    // Where we can ship our production to (Granary, StorageYard).
    pub storage_buildings_accepted: BuildingKind,
}

// ----------------------------------------------
// ProducerDebug
// ----------------------------------------------

game_object_debug_options! {
    ProducerDebug,

    // Stops goods from being produced and stock from being spent.
    freeze_production: bool,

    // Stop shipping production output from local stock to storage buildings.
    freeze_shipping: bool,
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

    debug: ProducerDebug,
}

impl<'config> BuildingBehavior<'config> for ProducerBuilding<'config> {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn update(&mut self, context: &BuildingContext, delta_time_secs: Seconds) {
        // Update producer states:
        if self.production_update_timer.tick(delta_time_secs).should_update() {
            if !self.debug.freeze_production() {
                self.production_update();
            }
            if !self.debug.freeze_shipping() {
                self.ship_to_storage(context);
            }
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

    fn draw_debug_ui(&mut self, _context: &BuildingContext, ui_sys: &UiSystem) {
        if ui_sys.builder().collapsing_header("Config", imgui::TreeNodeFlags::empty()) {
            self.config.draw_debug_ui(ui_sys);
        }
        self.debug.draw_debug_ui(ui_sys);
        self.draw_debug_ui_input_stock(ui_sys);
        self.draw_debug_ui_production_output(ui_sys);
    }

    fn draw_debug_popups(&mut self,
                         context: &BuildingContext,
                         ui_sys: &UiSystem,
                         transform: &WorldToScreenTransform,
                         visible_range: CellRange,
                         delta_time_secs: Seconds) {

        self.debug.draw_popup_messages(
            || context.find_tile(),
            ui_sys,
            transform,
            visible_range,
            delta_time_secs);
    }
}

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
                self.production_output_stock.store(1);
                self.debug.log_resources_gained(self.production_output_stock.kind(), 1);
            }
        }
    }

    fn ship_to_storage(&mut self, context: &BuildingContext) {
        if self.production_output_stock.is_empty() {
            return; // Nothing to ship.
        }

        // Unit spawns at the nearest road link.
        let unit_starting_cell = match context.find_nearest_road_link() {
            Some(road_link) => road_link,
            None => return, // We are not connected to a road. No shipping possible!
        };  

        let storage_buildings_accepted = self.config.storage_buildings_accepted;
        let resource_kind_to_deliver = self.production_output_stock.kind();
        let resource_count = self.production_output_stock.count();

        let result = Unit::try_spawn_with_task(
            context.query,
            context.id,
            UnitTask::DeliverToStorage {
                origin_building_kind: context.kind,
                origin_building_base_cell: context.base_cell(),
                unit_starting_cell,
                storage_buildings_accepted,
                resource_kind_to_deliver,
                resource_count,
                completion_callback: Some(|unit, building| {
                    // TODO notify us that cargo was delivered.
                    // building = self,
                    // unit = runner we've spawned
                    debug_assert!(building.is(BuildingKind::producers()));
                    println!("Unit {} finished delivering cargo from Producer {}", unit.name(), building.name());

                    // TODO: need to stop sending out runners while there is already one in flight.
                    // We need to be notified when our runner has completed the delivery!
                })
            });

        match result {
            Ok(spawned_unit) => {
                debug_assert!(spawned_unit.peek_inventory().unwrap().kind  == resource_kind_to_deliver);
                debug_assert!(spawned_unit.peek_inventory().unwrap().count == resource_count);

                // We've handed over our resources to the spawned unit, clear the stock.
                self.production_output_stock.clear();
                self.debug.log_resources_lost(resource_kind_to_deliver, resource_count);
            },
            Err(err) => {
                eprintln!("{} {}: Failed to ship production: {}", self.name(), context.base_cell(), err);
            }
        }

        // TODO: If we failed to ship to storage we could try shipping directly to another producer
        // that uses our output. E.g. farm -> factory.
        // Maybe DeliverToStorage task can have a fallback task?
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
        debug_assert!(output_kind.bits().count_ones() == 1); // One flag (kind) only.
        Self {
            item: StockItem { kind: output_kind, count: 0 },
            capacity,
        }
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
    fn kind(&self) -> ResourceKind {
        self.item.kind
    }

    #[inline]
    fn count(&self) -> u32 {
        self.item.count
    }

    #[inline]
    fn store(&mut self, count: u32) {
        debug_assert!(self.item.count + count <= self.capacity);
        self.item.count += count;
    }

    #[inline]
    fn clear(&mut self) {
        self.item.count = 0;
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
    fn is_resource_slot_full(&self, kind: ResourceKind) -> bool {
        for slot in &self.slots {
            if slot.kind.intersects(kind) && slot.count >= self.capacity {
                return true;
            }
        }
        false
    }

    #[inline]
    fn resource_slot_capacity_left(&self, kind: ResourceKind) -> u32 {
        for slot in &self.slots {
            if slot.kind.intersects(kind) {
                return self.capacity - slot.count;
            }
        }
        0
    }

    #[inline]
    fn count_resources(&self) -> u32 {
        let mut count = 0;
        for slot in &self.slots {
            count += slot.count;
        }
        count
    }

    #[inline]
    fn consume_resources(&mut self, debug: &mut ProducerDebug) {
        for slot in &mut self.slots {
            debug_assert!(slot.count != 0);
            slot.count -= 1;
            debug.log_resources_lost(slot.kind, 1);
        }
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
            }
        }
    }

    fn draw_debug_ui_production_output(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header("Production Output", imgui::TreeNodeFlags::empty()) {
            if self.is_production_halted() {
                ui.text_colored(Color::red().to_array(), "Production Halted!");
            }
            self.production_update_timer.draw_debug_ui("Update", 0, ui_sys);
            self.production_output_stock.draw_debug_ui(ui_sys);
        }
    }
}
