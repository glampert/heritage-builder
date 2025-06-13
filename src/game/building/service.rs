use crate::{
    imgui_ui::UiSystem,
    game::sim::resources::{
        ConsumerGoodsList,
        ConsumerGoodsStock,
        Workers
    }
};

use super::{
    BuildingKind,
    BuildingUpdateContext,
    config::{BuildingConfigs}
};

// ----------------------------------------------
// ServiceState
// ----------------------------------------------

pub struct ServiceState<'config> {
    config: &'config ServiceConfig,
    workers: Workers,

    // Current local stock of goods.
    goods_stock: ConsumerGoodsStock,
}

impl<'config> ServiceState<'config> {
    pub fn new(kind: BuildingKind, configs: &'config BuildingConfigs) -> Self {
        let config = configs.find::<ServiceConfig>(kind);
        Self {
            config: config,
            workers: Workers::new(config.min_workers, config.max_workers),
            goods_stock: ConsumerGoodsStock::new(),
        }
    }

    pub fn update(&mut self, _update_ctx: &mut BuildingUpdateContext, _delta_time_secs: f32) {
    }

    pub fn draw_debug_ui(&self, _ui_sys: &UiSystem) {
    }
}

// ----------------------------------------------
// ServiceConfig
// ----------------------------------------------

pub struct ServiceConfig {
    pub tile_def_name: String,

    pub min_workers: u32,
    pub max_workers: u32,

    pub effect_radius: i32,

    // Kinds of goods required for the service to run, if any.
    pub goods_required: ConsumerGoodsList,
}
