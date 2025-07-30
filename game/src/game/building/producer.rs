use smallvec::SmallVec;
use proc_macros::DrawDebugUi;

use crate::{
    game_object_debug_options,
    imgui_ui::UiSystem,
    pathfind::{SearchResult, NodeKind as PathNodeKind},
    utils::{
        Color,
        Seconds,
        hash::StringHash,
        coords::{Cell, CellRange, WorldToScreenTransform}
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
    unit::{self, Unit}
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

    fn visited(&mut self, _unit: &mut Unit, _context: &BuildingContext) {
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
                         delta_time_secs: Seconds,
                         show_popup_messages: bool) {

        self.debug.draw_popup_messages(
            || context.find_tile(),
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
                self.production_output_stock.store_item();
                self.debug.log_resources_gained(self.production_output_stock.resource_kind(), 1);
            }
        }
    }

    fn ship_to_storage(&mut self, context: &BuildingContext) {
        if self.production_output_stock.is_empty() {
            return; // Nothing to ship.
        }

        let this_building_road_link = context.find_nearest_road_link();
        if !this_building_road_link.is_valid() {
            return; // We are not connected to a road. No shipping possible!
        }

        let storage_kinds = self.config.storage_buildings_accepted;
        let resource_kind = self.production_output_stock.resource_kind();

        const MAX_CANDIDATES: usize = 4;
        let mut available_storages: SmallVec<[(Cell, Cell, i32, u32); MAX_CANDIDATES]> = SmallVec::new();

        // Try to find storage buildings that can accept our goods.
        context.for_each_storage(storage_kinds, |building| {
            let storage = building.as_storage();
            let slots_available = storage.how_many_can_fit(resource_kind);
            if slots_available != 0 {
                let storage_building_road_link = context.query.find_nearest_road_link(building.cell_range());
                if storage_building_road_link.is_valid() {
                    let storage_building_base_cell = building.base_cell();
                    let distance = this_building_road_link.manhattan_distance(storage_building_road_link);
                    available_storages.push((storage_building_road_link, storage_building_base_cell, distance, slots_available));
                    if available_storages.len() == MAX_CANDIDATES {
                        // We've collected enough candidate storage buildings, stop the search.
                        return false;
                    }
                }
            }
            // Else we couldn't find a single free slot in this storage, try again with another one.
            true
        });

        if available_storages.is_empty() {
            // Couldn't find any suitable storage building.
            return;
        }

        // Sort by closest storage buildings first. Tie breaker is the number of slots available, highest first.
        available_storages.sort_by_key(|(_, _, distance, slots_available)| {
            (*distance, std::cmp::Reverse(*slots_available))
        });

        // Try our best candidates first:
        for (storage_building_road_link, storage_building_base_cell, _, _) in available_storages {
            match context.query.find_path(PathNodeKind::Road, this_building_road_link, storage_building_road_link) {
                SearchResult::PathFound(path) => {
                    // If found a path, spawn a unit, give it the resources and make it follow the path to a storage building.
                    if let Some(unit) = context.try_spawn_unit_at(
                        this_building_road_link,
                        unit::config::UNIT_RUNNER) {

                        let items_to_ship_count = self.production_output_stock.remove_items();
                        self.debug.log_resources_lost(resource_kind, items_to_ship_count);

                        let start_building_cell = context.base_cell();
                        let goal_building_cell  = storage_building_base_cell;

                        unit.receive_resources(resource_kind, items_to_ship_count);
                        unit.go_to_building(path, start_building_cell, goal_building_cell);
                    }
                    break;
                },
                SearchResult::PathNotFound => {
                    // Building is not reachable (lacks road access?).
                    // Try another candidate.
                    continue;
                },
            }
        }
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
    fn resource_kind(&self) -> ResourceKind {
        self.item.kind
    }

    #[inline]
    fn store_item(&mut self) {
        debug_assert!(self.item.count < self.capacity);
        self.item.count += 1;
    }

    #[inline]
    fn remove_items(&mut self) -> u32 {
        let prev_count = self.item.count;
        self.item.count = 0;
        prev_count
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
