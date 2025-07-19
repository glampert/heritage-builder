use smallvec::SmallVec;
use proc_macros::DrawDebugUi;

use crate::{
    building_debug_options,
    imgui_ui::UiSystem,
    utils::{
        Color,
        Seconds,
        hash::StringHash,
        coords::{CellRange, WorldToScreenTransform}
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
    config::BuildingConfigs,
    storage::StorageBuilding,
};

// ----------------------------------------------
// TODO List
// ----------------------------------------------

// - Get raw materials from storage OR from other producers directly.

// ----------------------------------------------
// ProducerConfig
// ----------------------------------------------

#[derive(DrawDebugUi)]
pub struct ProducerConfig {
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

building_debug_options!(
    ProducerDebug,

    // Stops goods from being produced and stock from being spent.
    freeze_production: bool,

    // Stop shipping production output from local stock to storage buildings.
    freeze_shipping: bool,
);

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
    fn update(&mut self, context: &mut BuildingContext, delta_time_secs: Seconds) {
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

    fn draw_debug_ui(&mut self, _context: &mut BuildingContext, ui_sys: &UiSystem) {
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
                         delta_time_secs: Seconds,
                         show_popup_messages: bool) {

        self.debug.draw_popup_messages(
            context,
            ui_sys,
            transform,
            visible_range,
            delta_time_secs,
            show_popup_messages);
    }
}

impl<'config> ProducerBuilding<'config> {
    pub fn new(kind: BuildingKind, tile_name: &str, tile_name_hash: StringHash, configs: &'config BuildingConfigs) -> Self {
        let config = configs.find_producer_config(kind, tile_name, tile_name_hash);
        Self {
            config: config,
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
                self.production_output_stock.add_item();
                self.debug.log_resources_gained(self.production_output_stock.item.kind, 1);
            }
        }
    }

    fn ship_to_storage(&mut self, context: &mut BuildingContext) {
        let storage_kinds = self.config.storage_buildings_accepted;

        // Try to find a storage building that can accept our goods.
        context.for_each_storage(storage_kinds, |storage| {
            let mut continue_search = true;

            if !storage.is_full() {
                let shipped = self.production_output_stock.ship_to_storage(storage);
                if shipped != 0 {
                    // Storage accepted at least some of our items, stop.
                    continue_search = false;
                    self.debug.log_resources_lost(self.production_output_stock.item.kind, shipped);
                }
            }
            // Else we couldn't find a single free slot in this storage, try again with another one.

            continue_search
        });
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
        Self {
            item: StockItem { kind: output_kind, count: 0 },
            capacity: capacity,
        }
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.item.count >= self.capacity
    }

    #[inline]
    fn add_item(&mut self) {
        debug_assert!(self.item.count < self.capacity);
        self.item.count += 1;
    }

    #[inline]
    fn ship_to_storage(&mut self, storage: &mut StorageBuilding) -> u32 {
        let received = storage.receive_resources(self.item);
        self.item.count -= received;
        received
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
            slots.push(StockItem { kind: kind, count: 0 });
            true
        });
        Self {
            slots: slots,
            capacity: capacity,
        }
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
            if slot.kind.intersects(kind) {
                if slot.count >= self.capacity {
                    return true;
                }
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

impl<'config> ProducerBuilding<'config> {
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
