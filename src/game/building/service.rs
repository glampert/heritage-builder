use crate::{
    building_debug_options,
    imgui_ui::UiSystem,
    utils::{
        Seconds,
        hash::StringHash,
        coords::{CellRange, WorldToScreenTransform}
    },
    game::sim::{
        UpdateTimer,
        resources::{
            ResourceKind,
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
// ServiceConfig
// ----------------------------------------------

pub struct ServiceConfig {
    pub tile_def_name: String,
    pub tile_def_name_hash: StringHash,

    pub min_workers: u32,
    pub max_workers: u32,

    pub stock_update_frequency_secs: Seconds,
    pub effect_radius: i32,

    // Kinds of resources required for the service to run, if any.
    pub resources_required: ResourceKinds,
}

// ----------------------------------------------
// ServiceDebug
// ----------------------------------------------

building_debug_options!(
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
                if !self.debug.freeze_stock_update() {
                    self.stock_update(context);
                }
            }
        }
    }

    fn draw_debug_ui(&mut self, _context: &mut BuildingContext, ui_sys: &UiSystem) {
        self.config.draw_debug_ui(ui_sys);
        self.debug.draw_debug_ui(ui_sys);
        self.draw_debug_ui_resources_stock(ui_sys);
    }

    fn draw_debug_popups(&mut self,
                         context: &BuildingContext,
                         ui_sys: &UiSystem,
                         transform: &WorldToScreenTransform,
                         visible_range: CellRange,
                         delta_time_secs: Seconds,
                         show_popup_messages: bool) {

        self.debug.draw_popup_messages(context, ui_sys, transform, visible_range, delta_time_secs, show_popup_messages);
    }
}

impl<'config> ServiceBuilding<'config> {
    pub fn new(kind: BuildingKind, configs: &'config BuildingConfigs) -> Self {
        let config = configs.find_service_config(kind);
        Self {
            config: config,
            workers: Workers::new(config.min_workers, config.max_workers),
            stock_update_timer: UpdateTimer::new(config.stock_update_frequency_secs),
            stock: ResourceStock::with_accepted_list(&config.resources_required),
            debug: ServiceDebug::new(),
        }
    }

    pub fn shop(&mut self,
                shopping_basket: &mut ResourceStock,
                shopping_list: &ResourceKinds,
                all_or_nothing: bool) -> ResourceKind {

        if all_or_nothing {
            for wanted_resource in shopping_list.iter() {
                if !self.stock.has(*wanted_resource) {
                    return ResourceKind::empty(); // If any item is missing we take nothing.
                }
            }      
        }

        let mut kinds_added_to_basked = ResourceKind::empty();

        for wanted_resource in shopping_list.iter() {
            if let Some(resource) = self.stock.remove(*wanted_resource) {
                shopping_basket.add(resource);
                kinds_added_to_basked.insert(resource);
                self.debug.log_resources_lost(resource, 1);
            }
        }

        kinds_added_to_basked
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
            let resource_kinds_got =
                storage.shop(&mut self.stock, &shopping_list, all_or_nothing);
            self.debug.log_resources_gained(resource_kinds_got, 1);

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
        if ui.collapsing_header("Config", imgui::TreeNodeFlags::empty()) {
            ui.text(format!("Tile def name......: '{}'", self.tile_def_name));
            ui.text(format!("Min workers........: {}", self.min_workers));
            ui.text(format!("Max workers........: {}", self.max_workers));
            ui.text(format!("Effect radius......: {}", self.effect_radius));
            ui.text(format!("Resources required.: {}", self.resources_required));
        }
    }
}

impl<'config> ServiceBuilding<'config> {
    fn draw_debug_ui_resources_stock(&mut self, ui_sys: &UiSystem) {
        if self.stock.accepts_any() {
            let ui = ui_sys.builder();
            if ui.collapsing_header("Stock", imgui::TreeNodeFlags::empty()) {
                self.stock_update_timer.draw_debug_ui("Update:", ui_sys);
                self.stock.draw_debug_ui("Resources", ui_sys);
            }
        }
    }
}
