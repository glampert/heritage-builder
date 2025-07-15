use crate::{
    imgui_ui::UiSystem,
    utils::{
        Seconds,
        hash::StringHash
    },
    game::sim::resources::{
        ResourceKinds,
        ResourceStock,
        Workers
    }
};

use super::{
    BuildingKind,
    BuildingBehavior,
    BuildingContext,
    config::BuildingConfigs
};

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
// ServiceBuilding
// ----------------------------------------------

pub struct ServiceBuilding<'config> {
    config: &'config ServiceConfig,
    workers: Workers,

    // Current local stock of resources.
    stock: ResourceStock,
}

impl<'config> BuildingBehavior<'config> for ServiceBuilding<'config> {
    fn update(&mut self, _context: &mut BuildingContext, _delta_time_secs: Seconds) {
        // TODO
    }

    fn draw_debug_ui(&mut self, _context: &mut BuildingContext, ui_sys: &UiSystem) {
        self.draw_debug_ui_service_config(ui_sys);
        self.stock.draw_debug_ui("Resources In Stock", ui_sys);
    }
}

impl<'config> ServiceBuilding<'config> {
    pub fn new(kind: BuildingKind, configs: &'config BuildingConfigs) -> Self {
        let config = configs.find::<ServiceConfig>(kind);
        Self {
            config: config,
            workers: Workers::new(config.min_workers, config.max_workers),
            stock: ResourceStock::with_accepted_list(&config.resources_required),
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
}
