// The world map (docs/plans/realm-ui.md, Phase 2): its data per campaign (config), and what the
// screen reads of it (provider). The screen itself comes in Phase 2's later steps.

pub mod config;
pub mod provider;

pub use config::WorldMapConfigs;
pub use provider::{ConfigWorldMapProvider, WorldMapProvider};

use crate::{
    campaign::{self, config::CampaignConfigs},
    custom_game,
    game_loop::GameLoop,
    profile,
};

// The provider of the campaign at `campaign_index` for this run: the configs loaded, the player's
// profile and the mission being played. None before the configs load, or for a campaign without a
// world map.
pub fn provider(campaign_index: usize) -> Option<ConfigWorldMapProvider<'static>> {
    if !CampaignConfigs::is_initialized() || !WorldMapConfigs::is_initialized() {
        return None;
    }
    let active = if campaign::CampaignManager::is_initialized() { campaign::active_mission() } else { None };
    // Any game but the home menu's: a custom game, another campaign's mission or a preset.
    let in_game = GameLoop::is_initialized() && GameLoop::get().is_in_game();
    ConfigWorldMapProvider::new(CampaignConfigs::get(), WorldMapConfigs::get(), campaign_index, profile::get(), active)
        .map(|provider| provider.with_other_game(in_game))
}

// The campaign whose map the world map shows when none is named: the mission being played's, else
// the first visible campaign of the custom game's realm, else the latest the player can play
// (hidden campaigns aside), else the first.
pub fn default_campaign() -> usize {
    let configs = CampaignConfigs::get();
    if campaign::CampaignManager::is_initialized()
        && let Some(mission) = campaign::active_mission()
    {
        return mission.campaign_id;
    }
    if let Some(game) = custom_game::active()
        && let Some((index, _)) = configs.visible().find(|(_, campaign)| campaign.realm == game.realm)
    {
        return index;
    }
    profile::get().playable_campaigns(configs).last().copied().unwrap_or(0)
}
