use super::{
    BuildingUpdateContext
};

pub struct ServiceState {
}

impl ServiceState {
    pub fn new() -> Self {
        Self {}
    }

    pub fn update(&mut self, _update_ctx: &mut BuildingUpdateContext, _delta_time_secs: f32) {
    }
}
