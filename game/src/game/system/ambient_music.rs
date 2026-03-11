use std::any::Any;
use serde::{Deserialize, Serialize};

use super::GameSystem;
use crate::{
    game::{
        sim::Query,
    },
};

// ----------------------------------------------
// AmbientMusicSystem
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct AmbientMusicSystem {
}

impl GameSystem for AmbientMusicSystem {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update(&mut self, _query: &Query) {
    }
}

impl Default for AmbientMusicSystem {
    fn default() -> Self {
        Self {
        }
    }
}
