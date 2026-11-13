// The developer editor's Screens menu: opens the retail screens that are hard to reach in play,
// for a campaign and mission picked in it (every campaign, the hidden test campaigns included): a
// mission's briefing (also as it opens at the mission's start, offering a choice point's sites),
// its debrief, the campaign-complete screen, the world map after or during the mission (its next
// mission's box or choice) and its popups, and every story sequence of the campaign.
//
// Each is a preview (menu::dialog::preview): the in-game menus show it over the city, it records
// nothing in the profile and starts no session (its buttons that would only close it), and the
// editor comes back once it has closed. The realm is the campaign's, unless the Realm choice (the
// ui.realm override) picks another.
//
// Paths: Screens/Realm (dropdown: auto or a realm), Screens/Campaign (dropdown: campaign ids),
// Screens/Mission (dropdown: "N · name"), a button per screen (Screens/Mission briefing, ...),
// Screens/Sequence (dropdown: sequence ids) and Screens/Play sequence.

use engine::ui;
use strum::IntoEnumIterator;

use crate::{
    campaign::config::CampaignConfigs,
    menu::dialog::{
        preview::{self, PreviewRequest, PreviewScreen},
        world_map::MapPopup,
    },
    realm::{self, Realm},
    story::StoryConfigs,
    ui_context::GameUiContext,
    world_map::provider::step_choice,
};

#[derive(Default)]
pub struct ScreensDevMenu {
    campaign: usize,
    mission: usize,
    sequence: usize,
}

impl ScreensDevMenu {
    pub fn draw(&mut self, context: &mut GameUiContext) {
        let ui_sys = context.ui_sys;
        if !CampaignConfigs::is_initialized() {
            ui::text(ui_sys, "No campaigns loaded.");
            return;
        }
        let configs = CampaignConfigs::get();

        // The realm: the campaign's (auto), or the ui.realm override.
        let shown = realm::ui_realm_for(false);
        ui::text(ui_sys, format!("UI realm: {} ({})", shown.realm.display_name(), shown.source));
        let realms: Vec<String> = std::iter::once("auto".to_string()).chain(Realm::names()).collect();
        let mut realm_choice =
            realm::override_realm().and_then(|chosen| Realm::iter().position(|realm| realm == chosen)).map_or(0, |index| index + 1);
        if ui::combo(ui_sys, "Realm", &mut realm_choice, &realms) {
            realm::set_override(realm_choice.checked_sub(1).and_then(|index| Realm::iter().nth(index)));
        }

        // The campaign and its mission.
        ui::separator(ui_sys);
        let campaign_ids: Vec<&str> = configs.campaigns.iter().map(|campaign| campaign.id.as_str()).collect();
        if ui::combo(ui_sys, "Campaign", &mut self.campaign, &campaign_ids) {
            self.mission = 0;
            self.sequence = 0;
        }
        self.campaign = self.campaign.min(configs.campaigns.len().saturating_sub(1));
        let Some(campaign) = configs.campaign(self.campaign) else {
            return;
        };
        let hidden = if campaign.hidden { ", hidden" } else { "" };
        ui::text(ui_sys, format!("{} ({}{hidden})", campaign.display_name(), campaign.realm.display_name()));

        if campaign.missions.is_empty() {
            ui::text(ui_sys, "No missions yet.");
        } else {
            let missions: Vec<String> = (0..campaign.missions.len())
                .map(|index| {
                    let variant = if campaign.is_variant(index) { " (variant)" } else { "" };
                    format!("{} · {}{variant}", campaign.mission_number(index), campaign.mission_display_name(index).unwrap_or_default())
                })
                .collect();
            ui::combo(ui_sys, "Mission", &mut self.mission, &missions);
            self.mission = self.mission.min(campaign.missions.len() - 1);
            let mission = self.mission;

            ui::text(ui_sys, "Previews: nothing is recorded, no session starts.");
            let mut screen = None;
            if ui::button(ui_sys, "Mission briefing") {
                screen = Some(PreviewScreen::Briefing);
            }
            if step_choice(campaign, mission).is_some() && ui::button(ui_sys, "Briefing with its sites") {
                screen = Some(PreviewScreen::BriefingAtStart);
            }
            if ui::button(ui_sys, "Debrief") {
                screen = Some(PreviewScreen::Debrief);
            }
            if ui::button(ui_sys, "Campaign complete") {
                screen = Some(PreviewScreen::CampaignComplete);
            }
            if ui::button(ui_sys, "World map after the mission") {
                screen = Some(PreviewScreen::WorldMapAfter);
            }
            if ui::button(ui_sys, "World map during the mission") {
                screen = Some(PreviewScreen::WorldMapDuring);
            }
            // The world map's popups the next step offers: Begin without a choice, the site
            // popups with one.
            if let Some(next) = campaign.next_step_after(mission) {
                if step_choice(campaign, next).is_some() {
                    if ui::button(ui_sys, "Choose Site Popup") {
                        screen = Some(PreviewScreen::Popup(MapPopup::ChooseSite));
                    }
                    if ui::button(ui_sys, "Finish Mission Popup") {
                        screen = Some(PreviewScreen::Popup(MapPopup::FinishFirst));
                    }
                } else if ui::button(ui_sys, "Begin Mission Popup") {
                    screen = Some(PreviewScreen::Popup(MapPopup::BeginMission));
                }
            }
            if let Some(screen) = screen {
                // The campaign-complete screen is the last step's.
                let mission = if screen == PreviewScreen::CampaignComplete { campaign.steps().last().copied().unwrap_or(mission) } else { mission };
                preview::request(PreviewRequest { campaign_index: self.campaign, mission_index: mission, screen }, context);
            }
        }

        // The campaign's story sequences, every one (those waiting for their mission included).
        ui::separator(ui_sys);
        let sequences = StoryConfigs::is_initialized()
            .then(|| StoryConfigs::get().stories.get(&campaign.id))
            .flatten()
            .map_or_else(Vec::new, |story| story.sequences.iter().map(|sequence| (sequence.id.clone(), sequence.trigger.kind())).collect::<Vec<_>>());
        if sequences.is_empty() {
            ui::text(ui_sys, "No story sequences.");
            return;
        }
        let ids: Vec<String> = sequences.iter().map(|(id, _)| id.clone()).collect();
        ui::combo(ui_sys, "Sequence", &mut self.sequence, &ids);
        self.sequence = self.sequence.min(sequences.len() - 1);
        ui::text(ui_sys, format!("Plays: {}", sequences[self.sequence].1));
        if ui::button(ui_sys, "Play sequence") {
            let screen = PreviewScreen::Story(sequences[self.sequence].0.clone());
            preview::request(PreviewRequest { campaign_index: self.campaign, mission_index: self.mission, screen }, context);
        }
    }
}
