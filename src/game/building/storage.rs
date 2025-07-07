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
    config::{BuildingConfigs}
};

// ----------------------------------------------
// StorageState
// ----------------------------------------------

pub struct StorageState<'config> {
    config: &'config StorageConfig,
    workers: Workers,

    // Stockpiles:
    goods_stock: ConsumerGoodsStock,
    raw_materials_stock: RawMaterialsStock,
}

impl<'config> StorageState<'config> {
    pub fn new(kind: BuildingKind, configs: &'config BuildingConfigs) -> Self {
        let config = configs.find::<StorageConfig>(kind);
        Self {
            config: config,
            workers: Workers::new(config.min_workers, config.max_workers),
            goods_stock: ConsumerGoodsStock::new(),
            raw_materials_stock: RawMaterialsStock::new(),
        }
    }
}

impl<'config> BuildingBehavior<'config> for StorageState<'config> {
    fn update(&mut self, _update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>, _delta_time_secs: f32) {
    }

    fn draw_debug_ui(&self, _ui_sys: &UiSystem) {
    }
}

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
