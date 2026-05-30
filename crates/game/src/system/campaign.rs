use std::any::Any;

use engine::Engine;
use serde::{Deserialize, Serialize};

use super::GameSystem;
use crate::{
    campaign,
    sim::{SimCmds, SimContext},
};

// ----------------------------------------------
// CampaignSystem
// ----------------------------------------------

// Per-tick hook that drives campaign mission-completion checks. The system is
// stateless: all campaign progress lives in the `crate::campaign` manager
// singleton, which survives the mission map loads that replace the GameSession.
#[derive(Default, Serialize, Deserialize)]
pub struct CampaignSystem;

impl GameSystem for CampaignSystem {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update(&mut self, _engine: &mut Engine, _cmds: &mut SimCmds, context: &SimContext) {
        campaign::tick(context);
    }
}
