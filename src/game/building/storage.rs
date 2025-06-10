use super::{
    BuildingUpdateContext
};

pub struct StorageState {
}

impl StorageState {
    pub fn new() -> Self {
        Self {}
    }

    pub fn update(&mut self, _update_ctx: &mut BuildingUpdateContext, _delta_time_secs: f32) {
    }
}
