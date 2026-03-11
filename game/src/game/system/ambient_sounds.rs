use std::any::Any;
use serde::{Deserialize, Serialize};

use super::GameSystem;
use crate::{
    game::{
        sim::Query,
    },
};

// ----------------------------------------------
// AmbientSoundsSystem
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct AmbientSoundsSystem {
}

impl GameSystem for AmbientSoundsSystem {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update(&mut self, _query: &Query) {
    }
}

impl Default for AmbientSoundsSystem {
    fn default() -> Self {
        Self {
        }
    }
}
