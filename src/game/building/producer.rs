use smallvec::SmallVec;

use crate::{
    declare_building_debug_options,
    imgui_ui::UiSystem,
    utils::{
        Color,
        Seconds,
        hash::StringHash
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
// Constants
// ----------------------------------------------

const PRODUCTION_OUTPUT_FREQUENCY_SECS: Seconds = 20.0;

// ----------------------------------------------
// ProducerConfig
// ----------------------------------------------

pub struct ProducerConfig {
    pub tile_def_name: String,
    pub tile_def_name_hash: StringHash,

    pub min_workers: u32,
    pub max_workers: u32,

    // Producer output: A raw material or a consumer good.
    pub production_output_kind: ResourceKind,
    pub production_capacity: u32,

    // Kinds of raw materials required for production, if any.
    pub resources_required: ResourceKinds,
    pub resources_capacity: u32,
}

// ----------------------------------------------
// ProducerDebug
// ----------------------------------------------

declare_building_debug_options!(
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
            if !self.debug.freeze_production {
                self.production_update();
            }
            if !self.debug.freeze_shipping {
                self.ship_to_storage(context);
            }
        };
    }

    fn draw_debug_ui(&mut self, _context: &mut BuildingContext, ui_sys: &UiSystem) {
        self.draw_debug_ui_producer_config(ui_sys);
        self.draw_debug_ui_input_stock(ui_sys);
        self.draw_debug_ui_production_output(ui_sys);
    }
}

impl<'config> ProducerBuilding<'config> {
    pub fn new(kind: BuildingKind, configs: &'config BuildingConfigs) -> Self {
        let config = configs.find::<ProducerConfig>(kind);
        Self {
            config: config,
            workers: Workers::new(config.min_workers, config.max_workers),
            production_update_timer: UpdateTimer::new(PRODUCTION_OUTPUT_FREQUENCY_SECS),
            production_input_stock: ProducerInputsLocalStock::new(
                &config.resources_required,
                config.resources_capacity
            ),
            production_output_stock: ProducerOutputLocalStock::new(
                config.production_output_kind,
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
                if self.production_input_stock.has_all_required_resources() {
                    // Consume our raw materials (one of each).
                    self.production_input_stock.consume_all_resources();
                } else {
                    // We are missing one or more raw materials, halt production.
                    produce_one_item = false;
                }
            }

            // Produce one item and store it locally:
            if produce_one_item {
                self.production_output_stock.add_item();
            }
        }
    }

    fn ship_to_storage(&mut self, context: &mut BuildingContext) {
        // Try to find a storage yard that can accept our goods.
        context.for_each_storage(BuildingKind::StorageYard, |storage| {
            let mut continue_search = true;

            if !storage.is_full() {
                if self.production_output_stock.try_ship_to_storage(storage) {
                    // Storage accepted at least some of our items, stop.
                    continue_search = false;
                }
            }
            // Else we couldn't find a single free slot in this storage, try again with another one.

            continue_search
        });
    }

    fn is_production_halted(&self) -> bool {
        if self.debug.freeze_production {
            return true;
        }
        if self.production_output_stock.is_full() {
            return true;
        }
        if self.production_input_stock.requires_any_resource() &&
          !self.production_input_stock.has_all_required_resources() {
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
    fn try_ship_to_storage(&mut self, storage: &mut StorageBuilding) -> bool {
        storage.try_receive_resources(&mut self.item)
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
    fn has_all_required_resources(&self) -> bool {
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
    fn count_all_resources(&self) -> u32 {
        let mut count = 0;
        for slot in &self.slots {
            count += slot.count;
        }
        count
    }

    #[inline]
    fn consume_all_resources(&mut self) {
        for slot in &mut self.slots {
            debug_assert!(slot.count != 0);
            slot.count -= 1;
        }
    }
}

// ----------------------------------------------
// Debug UI
// ----------------------------------------------

impl ProducerConfig {
    fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        ui.text(format!("Tile def name.......: '{}'", self.tile_def_name));
        ui.text(format!("Min workers.........: {}", self.min_workers));
        ui.text(format!("Max workers.........: {}", self.max_workers));
        ui.text(format!("Production output...: {}", self.production_output_kind));
        ui.text(format!("Production capacity.: {}", self.production_capacity));
        ui.text(format!("Resources required..: {}", self.resources_required));
        ui.text(format!("Resources capacity..: {}", self.resources_capacity));
    }
}

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
    fn draw_debug_ui_producer_config(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header("Config##_building_config", imgui::TreeNodeFlags::empty()) {
            self.config.draw_debug_ui(ui_sys);
        }
    }

    fn draw_debug_ui_input_stock(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header("Raw Materials In Stock##_building_input_stock", imgui::TreeNodeFlags::empty()) {
            self.production_input_stock.draw_debug_ui(ui_sys);
        }
    }

    fn draw_debug_ui_production_output(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header("Production Output##_building_prod_output", imgui::TreeNodeFlags::empty()) {
            self.debug.draw_debug_ui(ui_sys);

            if self.is_production_halted() {
                ui.text_colored(Color::red().to_array(), "Production Halted!");
            }

            ui.text(format!("Frequency.....: {:.2}s", self.production_update_timer.frequency_secs()));
            ui.text(format!("Time since....: {:.2}s", self.production_update_timer.time_since_last_secs()));

            self.production_output_stock.draw_debug_ui(ui_sys);
        }
    }
}
