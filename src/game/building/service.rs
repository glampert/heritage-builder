use crate::{
    imgui_ui::UiSystem,
    utils::hash::StringHash,
    game::sim::resources::{
        ConsumerGoodsList,
        ConsumerGoodsStock,
        Workers
    }
};

use super::{
    BuildingKind,
    BuildingBehavior,
    BuildingUpdateContext,
    config::BuildingConfigs
};

// ----------------------------------------------
// ServiceBuilding
// ----------------------------------------------

pub struct ServiceBuilding<'config> {
    config: &'config ServiceConfig,
    workers: Workers,

    // Current local stock of goods.
    goods_stock: ConsumerGoodsStock,
}

impl<'config> ServiceBuilding<'config> {
    pub fn new(kind: BuildingKind, configs: &'config BuildingConfigs) -> Self {
        let config = configs.find::<ServiceConfig>(kind);
        Self {
            config: config,
            workers: Workers::new(config.min_workers, config.max_workers),
            goods_stock: ConsumerGoodsStock::new(&config.goods_required),
        }
    }

    pub fn shop(&mut self, shopping_basket: &mut ConsumerGoodsStock, shopping_list: &ConsumerGoodsList, all_or_nothing: bool) {
        if all_or_nothing {
            for wanted_item in shopping_list.iter() {
                if !self.goods_stock.has(*wanted_item) {
                    return; // If any item is missing we take nothing.
                }
            }      
        }

        for wanted_item in shopping_list.iter() {
            if let Some(stock_item) = self.goods_stock.remove(*wanted_item) {
                shopping_basket.add(stock_item);
            }
        }
    }
}

impl<'config> BuildingBehavior<'config> for ServiceBuilding<'config> {
    fn update(&mut self, _update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>, _delta_time_secs: f32) {
    }

    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        if ui.collapsing_header("Config##_building_config", imgui::TreeNodeFlags::empty()) {
            self.config.draw_debug_ui(ui_sys);
        }

        self.goods_stock.draw_debug_ui("Stock", ui_sys);
    }
}

// ----------------------------------------------
// ServiceConfig
// ----------------------------------------------

pub struct ServiceConfig {
    pub tile_def_name: String,
    pub tile_def_name_hash: StringHash,

    pub min_workers: u32,
    pub max_workers: u32,

    pub effect_radius: i32,

    // Kinds of goods required for the service to run, if any.
    pub goods_required: ConsumerGoodsList,
}

impl ServiceConfig {
    fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        ui.text(format!("Tile def name..: '{}'", self.tile_def_name));
        ui.text(format!("Min workers....: {}", self.min_workers));
        ui.text(format!("Max workers....: {}", self.max_workers));
        ui.text(format!("Effect radius..: {}", self.effect_radius));
        ui.text(format!("Goods required.: {}", self.goods_required));
    }
}
