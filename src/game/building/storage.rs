use crate::{
    imgui_ui::UiSystem,
    utils::hash::StringHash,
    game::sim::resources::{
        ConsumerGoodsList,
        ConsumerGoodsStock,
        RawMaterialsList,
        RawMaterialsStock,
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
// StorageConfig
// ----------------------------------------------

pub struct StorageConfig {
    pub tile_def_name: String,
    pub tile_def_name_hash: StringHash,

    pub min_workers: u32,
    pub max_workers: u32,

    // Goods/raw materials it can store.
    pub goods_accepted: ConsumerGoodsList,
    pub raw_materials_accepted: RawMaterialsList,

    // TODO: How about storage capacity?
}

// ----------------------------------------------
// StorageBuilding
// ----------------------------------------------

pub struct StorageBuilding<'config> {
    config: &'config StorageConfig,
    workers: Workers,

    // Stockpiles:
    goods_stock: ConsumerGoodsStock,
    raw_materials_stock: RawMaterialsStock,
}

impl<'config> BuildingBehavior<'config> for StorageBuilding<'config> {
    fn update(&mut self, _update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>, _delta_time_secs: f32) {
        // TODO
    }

    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        self.draw_debug_ui_storage_config(ui_sys);

        if self.goods_stock.has_any_entry() {
            self.goods_stock.draw_debug_ui("Goods In Stock", ui_sys);
        }

        if self.raw_materials_stock.has_any_entry() {
            self.raw_materials_stock.draw_debug_ui("Raw Materials In Stock", ui_sys);
        }
    }
}

impl<'config> StorageBuilding<'config> {
    pub fn new(kind: BuildingKind, configs: &'config BuildingConfigs) -> Self {
        let config = configs.find::<StorageConfig>(kind);
        Self {
            config: config,
            workers: Workers::new(config.min_workers, config.max_workers),
            goods_stock: ConsumerGoodsStock::new(&config.goods_accepted),
            raw_materials_stock: RawMaterialsStock::new(&config.raw_materials_accepted),
        }
    }
}

// ----------------------------------------------
// Debug UI
// ----------------------------------------------

impl StorageConfig {
    fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        ui.text(format!("Tile def name.....: '{}'", self.tile_def_name));
        ui.text(format!("Min workers.......: {}", self.min_workers));
        ui.text(format!("Max workers.......: {}", self.max_workers));
        ui.text(format!("Goods accepted....: {}", self.goods_accepted));
        ui.text(format!("Material accepted.: {}", self.raw_materials_accepted));
    }
}

impl<'config> StorageBuilding<'config> {
    fn draw_debug_ui_storage_config(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header("Config##_building_config", imgui::TreeNodeFlags::empty()) {
            self.config.draw_debug_ui(ui_sys);
        }
    }
}
