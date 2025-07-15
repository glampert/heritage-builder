use crate::{
    declare_building_debug_options,
    imgui_ui::UiSystem,
    utils::{
        Seconds,
        hash::StringHash
    },
    game::sim::{
        UpdateTimer,
        resources::{
            ResourceKinds,
            ResourceStock,
            Workers
        }
    }
};

use super::{
    BuildingKind,
    BuildingBehavior,
    BuildingContext,
    config::BuildingConfigs
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

const STOCK_UPDATE_FREQUENCY_SECS: Seconds = 20.0;

// ----------------------------------------------
// ServiceConfig
// ----------------------------------------------

pub struct ServiceConfig {
    pub tile_def_name: String,
    pub tile_def_name_hash: StringHash,

    pub min_workers: u32,
    pub max_workers: u32,

    pub effect_radius: i32,

    // Kinds of resources required for the service to run, if any.
    pub resources_required: ResourceKinds,
}

// ----------------------------------------------
// ServiceDebug
// ----------------------------------------------

declare_building_debug_options!(
    ServiceDebug,

    // Stops fetching resources from storage.
    freeze_stock_update: bool,
);

// ----------------------------------------------
// ServiceBuilding
// ----------------------------------------------

pub struct ServiceBuilding<'config> {
    config: &'config ServiceConfig,
    workers: Workers,

    stock_update_timer: UpdateTimer,
    stock: ResourceStock, // Current local stock of resources.

    debug: ServiceDebug,
}

impl<'config> BuildingBehavior<'config> for ServiceBuilding<'config> {
    fn update(&mut self, context: &mut BuildingContext, delta_time_secs: Seconds) {
        // Procure resources from storage periodically if we need them.
        if self.stock.accepts_any() {
            if self.stock_update_timer.tick(delta_time_secs).should_update() {
                if !self.debug.freeze_stock_update {
                    self.stock_update(context);
                }
            }
        }
    }

    fn draw_debug_ui(&mut self, _context: &mut BuildingContext, ui_sys: &UiSystem) {
        self.draw_debug_ui_service_config(ui_sys);
        self.draw_debug_ui_resources_stock(ui_sys);
    }
}

impl<'config> ServiceBuilding<'config> {
    pub fn new(kind: BuildingKind, configs: &'config BuildingConfigs) -> Self {
        let config = configs.find_service_config(kind);
        Self {
            config: config,
            workers: Workers::new(config.min_workers, config.max_workers),
            stock_update_timer: UpdateTimer::new(STOCK_UPDATE_FREQUENCY_SECS),
            stock: ResourceStock::with_accepted_list(&config.resources_required),
            debug: ServiceDebug::default(),
        }
    }

    pub fn shop(&mut self, shopping_basket: &mut ResourceStock, shopping_list: &ResourceKinds, all_or_nothing: bool) {
        if all_or_nothing {
            for wanted_resource in shopping_list.iter() {
                if !self.stock.has(*wanted_resource) {
                    return; // If any item is missing we take nothing.
                }
            }      
        }

        for wanted_resource in shopping_list.iter() {
            if let Some(resource) = self.stock.remove(*wanted_resource) {
                shopping_basket.add(resource);
            }
        }
    }

    fn stock_update(&mut self, context: &mut BuildingContext) {
        let resources_required = &self.config.resources_required;
        let mut shopping_list = ResourceKinds::none();

        resources_required.for_each(|resource| {
            if !self.stock.has(resource) {
                shopping_list.add(resource);
            }
            true
        });

        let storage_kinds =
            BuildingKind::Granary |
            BuildingKind::StorageYard;

        context.for_each_storage(storage_kinds, |storage| {
            let all_or_nothing = false;
            storage.shop(&mut self.stock, &shopping_list, all_or_nothing);

            let mut continue_search = false;

            resources_required.for_each(|resource| {
                if !self.stock.has(resource) {
                    continue_search = true;
                } else {
                    shopping_list.remove(resource);
                }
                true
            });

            continue_search
        });
    }
}

// ----------------------------------------------
// Debug UI
// ----------------------------------------------

impl ServiceConfig {
    fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        ui.text(format!("Tile def name......: '{}'", self.tile_def_name));
        ui.text(format!("Min workers........: {}", self.min_workers));
        ui.text(format!("Max workers........: {}", self.max_workers));
        ui.text(format!("Effect radius......: {}", self.effect_radius));
        ui.text(format!("Resources required.: {}", self.resources_required));
    }
}

impl<'config> ServiceBuilding<'config> {
    fn draw_debug_ui_service_config(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header("Config##_building_config", imgui::TreeNodeFlags::empty()) {
            self.config.draw_debug_ui(ui_sys);
        }
    }

    fn draw_debug_ui_resources_stock(&mut self, ui_sys: &UiSystem) {
        self.debug.draw_debug_ui(ui_sys);

        if self.stock.accepts_any() {
            let ui = ui_sys.builder();

            ui.text("Stock Update:");
            ui.text(format!("  Frequency.....: {:.2}s", self.stock_update_timer.frequency_secs()));
            ui.text(format!("  Time since....: {:.2}s", self.stock_update_timer.time_since_last_secs()));

            self.stock.draw_debug_ui("Resources In Stock", ui_sys);
        }
    }
}
