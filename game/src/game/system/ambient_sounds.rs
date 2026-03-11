use std::any::Any;
use serde::{Deserialize, Serialize};

use super::GameSystem;
use crate::{
    engine::Engine,
    game::sim::Query,
};

// ----------------------------------------------
// AmbientSoundsSystem
// ----------------------------------------------

#[derive(Default, Serialize, Deserialize)]
pub struct AmbientSoundsSystem {
}

impl GameSystem for AmbientSoundsSystem {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update(&mut self, _engine: &mut dyn Engine, _query: &Query) {
    }

    // TODO WIP
}
