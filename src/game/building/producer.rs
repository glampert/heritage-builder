use crate::{
    imgui_ui::UiSystem,
    game::sim::resources::{
        ConsumerGoodKind,
        RawMaterialKind,
        RawMaterialsList,
        RawMaterialsStock,
        Workers
    }
};

use super::{
    BuildingKind,
    BuildingUpdateContext,
    config::{BuildingConfigs}
};

// ----------------------------------------------
// ProducerState
// ----------------------------------------------

pub struct ProducerState<'config> {
    config: &'config ProducerConfig,
    workers: Workers,

    // Current local stock of raw materials.
    raw_materials_stock: RawMaterialsStock,
}

impl<'config> ProducerState<'config> {
    pub fn new(kind: BuildingKind, configs: &'config BuildingConfigs) -> Self {
        let config = configs.find::<ProducerConfig>(kind);
        Self {
            config: config,
            workers: Workers::new(config.min_workers, config.max_workers),
            raw_materials_stock: RawMaterialsStock::new(),
        }
    }

    pub fn update(&mut self, _update_ctx: &mut BuildingUpdateContext, _delta_time_secs: f32) {
    }

    pub fn draw_debug_ui(&self, _ui_sys: &UiSystem) {
    }
}

// ----------------------------------------------
// ProducerOutputKind
// ----------------------------------------------

pub enum ProducerOutputKind {
    RawMaterial(RawMaterialKind),
    ConsumerGood(ConsumerGoodKind),
}

// ----------------------------------------------
// ProducerConfig
// ----------------------------------------------

pub struct ProducerConfig {
    pub tile_def_name: String,

    pub min_workers: u32,
    pub max_workers: u32,

    // Producer output: A raw material or a consumer good.
    pub production_output: ProducerOutputKind,

    // Kinds of raw materials required for production, if any.
    pub raw_materials_required: RawMaterialsList,
}
