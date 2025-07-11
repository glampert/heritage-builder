use smallvec::SmallVec;

use crate::{
    imgui_ui::UiSystem,
    utils::{
        Color,
        hash::StringHash
    },
    game::sim::{
        UpdateTimer,
        resources::{
            ConsumerGoodKind,
            RawMaterialKind,
            RawMaterialsList,
            StockItem,
            Workers
        }
    }
};

use super::{
    BuildingKind,
    BuildingBehavior,
    BuildingUpdateContext,
    config::BuildingConfigs
};

// ----------------------------------------------
// TODO List
// ----------------------------------------------

// - Move production output from local stock to storage.
// - Get raw materials from storage OR from other producers directly.

// ----------------------------------------------
// Constants
// ----------------------------------------------

const PRODUCTION_OUTPUT_FREQUENCY_SECS: f32 = 20.0;

// ----------------------------------------------
// ProducerConfig
// ----------------------------------------------

pub struct ProducerConfig {
    pub tile_def_name: String,
    pub tile_def_name_hash: StringHash,

    pub min_workers: u32,
    pub max_workers: u32,

    // Producer output: A raw material or a consumer good.
    pub production_output: ProducerOutputKind,
    pub production_capacity: u32,

    // Kinds of raw materials required for production, if any.
    pub raw_materials_required: RawMaterialsList,
    pub raw_materials_capacity: u32,
}

// ----------------------------------------------
// ProducerDebug
// ----------------------------------------------

#[derive(Default)]
struct ProducerDebug {
    // Stops goods from being produced and stock from being spent.
    freeze_production: bool,
}

// ----------------------------------------------
// ProducerBuilding
// ----------------------------------------------

pub struct ProducerBuilding<'config> {
    config: &'config ProducerConfig,
    workers: Workers,

    production_update_timer: UpdateTimer,
    production_output_stock: ProducerOutputLocalStock, // Local production output storage.
    raw_materials_required_stock: ProducerRawMaterialsLocalStock, // Current local stock of required raw materials.

    debug: ProducerDebug,
}

impl<'config> BuildingBehavior<'config> for ProducerBuilding<'config> {
    fn update(&mut self, _update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>, delta_time_secs: f32) {
        // Update producer states:
        if self.production_update_timer.tick(delta_time_secs).should_update() {
            if !self.debug.freeze_production {
                self.production_update();
            }
        };
    }

    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
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
            production_output_stock: ProducerOutputLocalStock::new(
                &config.production_output,
                config.production_capacity
            ),
            raw_materials_required_stock: ProducerRawMaterialsLocalStock::new(
                &config.raw_materials_required,
                config.raw_materials_capacity
            ),
            debug: ProducerDebug::default(),
        }
    }

    fn production_update(&mut self) {
        // Production halts if the local stock is full.
        if !self.production_output_stock.is_full() {
            let mut produce_one_item = true;

            // If we have raw material requirements, first check if they are available in stock.
            if self.raw_materials_required_stock.has_any_entry() {
                if self.raw_materials_required_stock.has_all_required_items() {
                    // Consume our raw materials (one of each).
                    self.raw_materials_required_stock.consume_all_items();
                } else {
                    // We are missing one or more, halt production.
                    produce_one_item = false;
                }
            }

            // Produce one item and store it locally:
            if produce_one_item {
                self.production_output_stock.add_item();
            }
        }
    }

    fn is_production_halted(&self) -> bool {
        if self.debug.freeze_production {
            return true;
        }
        if self.production_output_stock.is_full() {
            return true;
        }
        if self.raw_materials_required_stock.has_any_entry() && !self.raw_materials_required_stock.has_all_required_items() {
            return true;
        }
        false
    }
}

// ----------------------------------------------
// ProducerOutputLocalStock
// ----------------------------------------------

enum ProducerOutputLocalStock {
    RawMaterial {
        item: StockItem<RawMaterialKind>,
        capacity: u32
    },
    ConsumerGood {
        item: StockItem<ConsumerGoodKind>,
        capacity: u32,
    },
}

impl ProducerOutputLocalStock {
    fn new(output: &ProducerOutputKind, capacity: u32) -> Self {
        match output {
            ProducerOutputKind::RawMaterial(material) => {
                Self::RawMaterial {
                    item: StockItem { kind: *material, count: 0 },
                    capacity: capacity,
                }
            },
            ProducerOutputKind::ConsumerGood(good) => {
                Self::ConsumerGood {
                    item: StockItem { kind: *good, count: 0 },
                    capacity: capacity,
                }
            }
        }
    }

    #[inline]
    fn is_full(&self) -> bool {
        match self {
            ProducerOutputLocalStock::RawMaterial { item, capacity } => {
                item.count >= *capacity
            },
            ProducerOutputLocalStock::ConsumerGood { item, capacity } => {
                item.count >= *capacity
            }
        }
    }

    #[inline]
    fn add_item(&mut self) {
        match self {
            ProducerOutputLocalStock::RawMaterial { item, capacity } => {
                debug_assert!(item.count < *capacity);
                item.count += 1
            },
            ProducerOutputLocalStock::ConsumerGood { item, capacity } => {
                debug_assert!(item.count < *capacity);
                item.count += 1
            }
        }
    }
}

// ----------------------------------------------
// ProducerRawMaterialsLocalStock
// ----------------------------------------------

struct ProducerRawMaterialsLocalStock {
    items: SmallVec<[StockItem<RawMaterialKind>; 1]>,
    capacity: u32, // Same capacity for each item.
}

impl ProducerRawMaterialsLocalStock {
    fn new(raw_materials_required: &RawMaterialsList, capacity: u32) -> Self {
        let mut items = SmallVec::new();
        for material in raw_materials_required.iter() {
            items.push(StockItem { kind: *material, count: 0 });
        }
        Self {
            items: items,
            capacity: capacity,
        }
    }

    #[inline]
    fn has_any_entry(&self) -> bool {
        !self.items.is_empty()
    }

    #[inline]
    fn is_entry_full(&self, wanted: RawMaterialKind) -> bool {
        for item in &self.items {
            if item.kind.intersects(wanted) && item.count >= self.capacity {
                return true;
            }
        }
        false
    }

    #[inline]
    fn has_all_required_items(&self) -> bool {
        for item in &self.items {
            if item.count == 0 {
                return false;
            }
        }
        true
    }

    #[inline]
    fn consume_all_items(&mut self) {
        for item in &mut self.items {
            debug_assert!(item.count != 0);
            item.count -= 1;
        }
    }
}

// ----------------------------------------------
// ProducerOutputKind
// ----------------------------------------------

pub enum ProducerOutputKind {
    RawMaterial(RawMaterialKind),
    ConsumerGood(ConsumerGoodKind),
}

impl std::fmt::Display for ProducerOutputKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ProducerOutputKind::RawMaterial(material) => {
                write!(f, "{}", material)
            },
            ProducerOutputKind::ConsumerGood(good) => {
                write!(f, "{}", good)
            }
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
        ui.text(format!("Production output...: {}", self.production_output));
        ui.text(format!("Production capacity.: {}", self.production_capacity));
        ui.text(format!("Materials required..: {}", self.raw_materials_required));
        ui.text(format!("Materials capacity..: {}", self.raw_materials_capacity));
    }
}

impl ProducerOutputLocalStock {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        ui.text("Local Stock:");

        match self {
            ProducerOutputLocalStock::RawMaterial { item, .. } => {
                ui.input_scalar(format!("{}", item.kind), &mut item.count)
                    .step(1)
                    .build();
            },
            ProducerOutputLocalStock::ConsumerGood { item, .. } => {
                ui.input_scalar(format!("{}", item.kind), &mut item.count)
                    .step(1)
                    .build();
            }
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

impl ProducerRawMaterialsLocalStock {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if self.items.is_empty() {
            ui.text("<none>");
        } else {
            for (index, item) in self.items.iter_mut().enumerate() {
                ui.input_scalar(format!("{}##_stock_item_{}", item.kind, index), &mut item.count)
                    .step(1)
                    .build();

                let is_full = item.count >= self.capacity;
                if is_full {
                    ui.same_line();
                    ui.text_colored(Color::red().to_array(), "(full)");
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
            self.raw_materials_required_stock.draw_debug_ui(ui_sys);
        }
    }

    fn draw_debug_ui_production_output(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header("Production Output##_building_prod_output", imgui::TreeNodeFlags::empty()) {
            ui.checkbox("Freeze production", &mut self.debug.freeze_production);
            if self.is_production_halted() {
                ui.text_colored(Color::red().to_array(), "Production Halted!");
            }
            ui.text(format!("Frequency.....: {:.2}s", self.production_update_timer.frequency_secs()));
            ui.text(format!("Time since....: {:.2}s", self.production_update_timer.time_since_last_secs()));
            self.production_output_stock.draw_debug_ui(ui_sys);
        }
    }
}
