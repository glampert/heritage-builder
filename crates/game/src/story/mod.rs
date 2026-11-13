// The story comics (docs/plans/realm-ui.md, Phase 4): the sequences' data (config) and what the
// campaign flow and the menus ask of it: which sequence plays before or after a mission, the
// campaign's epilogue, and which sequences the player's profile has seen (each plays once per
// profile; Replay the story plays them again).
//
// The comic viewer (menu::dialog's StoryComic, dialog::play_story) shows a sequence. The campaign
// flow plays them: a mission's before its briefing (dialog::open_mission_start: to_play_before),
// its after-sequence on its debrief's Continue (to_play_after), the epilogue from the
// campaign-complete screen.
// TODO(realm-ui): the other sequences of docs/design/story-script.md wait for their missions (game
// Phases 2-4), and its story events (a sequence on a city's event, not a mission's start or end)
// for game Phase 2.

pub mod config;

pub use config::{
    BubbleDef,
    BubbleKind,
    PANEL_SIZE,
    PanelDef,
    SequenceDef,
    StoryConfigs,
    StoryDef,
    StoryTrigger,
    TriggerPoint,
};

use crate::{
    campaign::config::{CampaignConfigs, CampaignDef},
    profile::{self, Profile},
};

// A sequence and the campaign it belongs to.
#[derive(Copy, Clone)]
pub struct SequenceRef<'a> {
    pub campaign_index: usize,
    pub campaign: &'a CampaignDef,
    pub sequence: &'a SequenceDef,
}

impl SequenceRef<'_> {
    // Whether the profile has seen it.
    pub fn is_seen(&self, profile: &Profile) -> bool {
        profile.has_seen_sequence(&self.campaign.id, &self.sequence.id)
    }
}

impl StoryConfigs {
    // The campaign's sequences that can play (their triggers resolve), in the order they play:
    // step by step (a variant's with its choice point's), before a mission, after it, then the
    // epilogue (TriggerPoint::order).
    pub fn sequences_in_order<'a>(&'a self, campaigns: &'a CampaignConfigs, campaign_index: usize) -> Vec<SequenceRef<'a>> {
        let Some(campaign) = campaigns.campaign(campaign_index) else {
            return Vec::new();
        };
        let Some(story) = self.stories.get(&campaign.id) else {
            return Vec::new();
        };
        let mut sequences: Vec<((usize, u8, usize), SequenceRef<'a>)> = story
            .sequences
            .iter()
            .filter_map(|sequence| {
                let order = config::resolve_trigger(campaign, &sequence.trigger).order(campaign)?;
                Some((order, SequenceRef { campaign_index, campaign, sequence }))
            })
            .collect();
        sequences.sort_by_key(|(order, _)| *order);
        sequences.into_iter().map(|(_, sequence)| sequence).collect()
    }

    // The sequence that plays before the mission at `mission_index` (before its briefing): the one
    // named for it, else the one named for its step (a variant takes its choice point's).
    pub fn before_mission<'a>(
        &'a self,
        campaigns: &'a CampaignConfigs,
        campaign_index: usize,
        mission_index: usize,
    ) -> Option<SequenceRef<'a>> {
        self.at_mission(campaigns, campaign_index, mission_index, TriggerPoint::Before)
    }

    // The sequence that plays after the mission at `mission_index` (after its debrief), as
    // before_mission finds it.
    pub fn after_mission<'a>(
        &'a self,
        campaigns: &'a CampaignConfigs,
        campaign_index: usize,
        mission_index: usize,
    ) -> Option<SequenceRef<'a>> {
        self.at_mission(campaigns, campaign_index, mission_index, TriggerPoint::After)
    }

    // The campaign's epilogue (the campaign-complete screen's Watch the epilogue). None: it has none.
    pub fn epilogue<'a>(&'a self, campaigns: &'a CampaignConfigs, campaign_index: usize) -> Option<SequenceRef<'a>> {
        self.sequences_in_order(campaigns, campaign_index)
            .into_iter()
            .find(|sequence| sequence.sequence.trigger == StoryTrigger::Epilogue)
    }

    fn at_mission<'a>(
        &'a self,
        campaigns: &'a CampaignConfigs,
        campaign_index: usize,
        mission_index: usize,
        point: fn(usize) -> TriggerPoint,
    ) -> Option<SequenceRef<'a>> {
        let campaign = campaigns.campaign(campaign_index)?;
        if mission_index >= campaign.missions.len() {
            return None;
        }
        let sequences = self.sequences_in_order(campaigns, campaign_index);
        let find = |mission: usize| {
            sequences
                .iter()
                .find(|sequence| config::resolve_trigger(campaign, &sequence.sequence.trigger) == point(mission))
                .copied()
        };
        find(mission_index).or_else(|| find(campaign.step_of(mission_index)))
    }

    // The sequence of the campaign with this id. None: no such campaign or sequence.
    pub fn find<'a>(
        &'a self,
        campaigns: &'a CampaignConfigs,
        campaign_id: &str,
        sequence_id: &str,
    ) -> Option<SequenceRef<'a>> {
        let campaign_index = campaigns.find_by_id(campaign_id)?;
        let campaign = campaigns.campaign(campaign_index)?;
        let sequence = self.stories.get(campaign_id)?.sequence(sequence_id)?;
        Some(SequenceRef { campaign_index, campaign, sequence })
    }

    // The sequences the profile has seen that can play, campaign by campaign (in campaigns.json
    // order), each campaign's in the order they play: Replay the story's list.
    pub fn seen_sequences<'a>(&'a self, campaigns: &'a CampaignConfigs, profile: &Profile) -> Vec<SequenceRef<'a>> {
        (0..campaigns.campaigns.len())
            .flat_map(|campaign_index| self.sequences_in_order(campaigns, campaign_index))
            .filter(|sequence| sequence.is_seen(profile))
            .collect()
    }
}

// ----------------------------------------------
// This run's story (the configs loaded, the profile)
// ----------------------------------------------

fn configs() -> Option<(&'static StoryConfigs, &'static CampaignConfigs)> {
    (StoryConfigs::is_initialized() && CampaignConfigs::is_initialized())
        .then(|| (StoryConfigs::get(), CampaignConfigs::get()))
}

// The sequence to play before the mission's briefing: its sequence, unless the profile has seen it.
pub fn to_play_before(campaign_index: usize, mission_index: usize) -> Option<SequenceRef<'static>> {
    let (stories, campaigns) = configs()?;
    stories.before_mission(campaigns, campaign_index, mission_index).filter(|sequence| !sequence.is_seen(profile::get()))
}

// The sequence to play after the mission's debrief: its sequence, unless the profile has seen it.
pub fn to_play_after(campaign_index: usize, mission_index: usize) -> Option<SequenceRef<'static>> {
    let (stories, campaigns) = configs()?;
    stories.after_mission(campaigns, campaign_index, mission_index).filter(|sequence| !sequence.is_seen(profile::get()))
}

// The sequence that plays before the mission, seen or not (the briefing's Replay the story).
pub fn before(campaign_index: usize, mission_index: usize) -> Option<SequenceRef<'static>> {
    let (stories, campaigns) = configs()?;
    stories.before_mission(campaigns, campaign_index, mission_index)
}

// The sequence that plays after the mission, seen or not (the debrief's preview: debug::screens).
pub fn after(campaign_index: usize, mission_index: usize) -> Option<SequenceRef<'static>> {
    let (stories, campaigns) = configs()?;
    stories.after_mission(campaigns, campaign_index, mission_index)
}

// The campaign's epilogue (the campaign-complete screen's Watch the epilogue).
pub fn epilogue(campaign_index: usize) -> Option<SequenceRef<'static>> {
    let (stories, campaigns) = configs()?;
    stories.epilogue(campaigns, campaign_index)
}

// The sequences the profile has seen that can still play, as Replay the story lists them: the
// home menu offers it while there's one (home.rs, replay_subline).
pub fn seen() -> Vec<SequenceRef<'static>> {
    configs().map_or_else(Vec::new, |(stories, campaigns)| stories.seen_sequences(campaigns, profile::get()))
}

// Whether every sequence each campaign record of `profile` has seen is one of its campaign's (for
// profile.set, whose `campaigns` replace every record: so it checks exactly the records given).
// Err names the first that isn't.
pub fn check_seen_sequences(profile: &Profile) -> Result<(), String> {
    let Some((stories, _)) = configs() else {
        return Ok(());
    };
    for record in &profile.campaigns {
        let story = stories.stories.get(&record.id);
        for id in &record.sequences_seen {
            if story.and_then(|story| story.sequence(id)).is_none() {
                let known: Vec<&str> = story
                    .map_or_else(Vec::new, |story| story.sequences.iter().map(|sequence| sequence.id.as_str()).collect());
                return Err(format!(
                    "Campaign '{}' has no story sequence '{id}'. Its sequences: {}.",
                    record.id,
                    if known.is_empty() { "none".to_string() } else { known.join(", ") }
                ));
            }
        }
    }
    Ok(())
}
