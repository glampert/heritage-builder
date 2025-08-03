use proc_macros::DrawDebugUi;

use crate::{
    game_object_debug_options,
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
    unit::Unit
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

    pub stock_update_frequency_secs: Seconds,
    pub effect_radius: i32,

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
}

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
    fn name(&self) -> &str {
        &self.config.name
    }

    fn update(&mut self, context: &BuildingContext, delta_time_secs: Seconds) {
        // Procure resources from storage periodically if we need them.
        if self.stock.accepts_any() &&
           self.stock_update_timer.tick(delta_time_secs).should_update() &&
          !self.debug.freeze_stock_update() {
            self.stock_update(context);
        }
    }

    fn visited_by(&mut self, _unit: &mut Unit, _context: &BuildingContext) {
        todo!()
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
        self.draw_debug_ui_resources_stock(ui_sys);
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

impl<'config> ServiceBuilding<'config> {
    pub fn new(config: &'config ServiceConfig) -> Self {
        Self {
            config,
            workers: Workers::new(config.min_workers, config.max_workers),
            stock_update_timer: UpdateTimer::new(config.stock_update_frequency_secs),
            stock: ResourceStock::with_accepted_list(&config.resources_required),
            debug: ServiceDebug::default(),
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

    fn stock_update(&mut self, context: &BuildingContext) {
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

        context.for_each_storage_mut(storage_kinds, |building| {
            let all_or_nothing = false;
            let resource_kinds_got =
                building.as_storage_mut().shop(&mut self.stock, &shopping_list, all_or_nothing);
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

impl ServiceBuilding<'_> {
    fn draw_debug_ui_resources_stock(&mut self, ui_sys: &UiSystem) {
        if self.stock.accepts_any() {
            let ui = ui_sys.builder();
            if ui.collapsing_header("Stock", imgui::TreeNodeFlags::empty()) {
                self.stock_update_timer.draw_debug_ui("Update", 0, ui_sys);
                self.stock.draw_debug_ui("Resources", ui_sys);
            }
        }
    }
}
