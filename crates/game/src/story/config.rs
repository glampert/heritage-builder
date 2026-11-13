// The story comics' data (docs/plans/realm-ui.md, Phase 4: Data): one file per campaign,
// assets/configs/story/<campaign id>.json, for the campaigns that have one. Loaded once at boot,
// after campaigns.json (StoryConfigs::load).
//
// A file holds the campaign's sequences (docs/design/story-script.md): each an id (unique in its
// campaign; the profile records the sequences seen by it), a title, the trigger that plays it
// (before a mission's briefing, after its debrief, or the campaign's epilogue) and its panels.
// A panel is a picture (a path under assets/ui/, e.g. "story/dragon_legacy/i_a_1.jpg", drawn by
// crates/tools/ui-art until real art replaces the file), a narration caption, speech bubbles, a
// sepia flag for flashbacks, and `stele_lines` for a panel that lists the player's stele lines
// (the epilogue's).
//
// Bubbles are placed in the panel's own coordinates: points on the mockup's 1800 × 870 panel
// (PANEL_SIZE), from its top-left corner, whatever the picture's size.
//
// Text is keys (en.json): titles, narration and lines in the Story category, speakers' names in
// the Realm category.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use strum::Display;

use engine::{
    config::configs_path,
    file_sys::{self, CollectFlags, paths::PathRef},
    log,
    ui::text::{self, UiTextCategory},
};

use crate::campaign::config::{CampaignConfigs, CampaignDef, MissionRef};

// The story files' folder, under assets/configs/.
const STORY_DIR: &str = "story";

// The panel's coordinate space: the intro mockup's panel, in points (screens.py's intro).
pub const PANEL_SIZE: [f32; 2] = [1800.0, 870.0];

// ----------------------------------------------
// Story file
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryDef {
    pub sequences: Vec<SequenceDef>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SequenceDef {
    // Unique in its campaign, [a-z0-9_]: the profile records it once seen.
    pub id: String,
    // Its title, the viewer's bottom bar's (a Story key).
    pub title: String,
    pub trigger: StoryTrigger,
    // The developers' note (where the script has it, what it waits for). Not shown.
    #[serde(default)]
    pub note: String,
    pub panels: Vec<PanelDef>,
}

// When a sequence plays. A mission is named as in campaigns.json (MissionRef: its index in the
// campaign, or its id); a sequence named for a step also plays for the variants its choice starts.
// A mission index past the campaign's missions isn't an error: the sequence waits for that mission
// to be written (StoryConfigs::pending), as I-B waits for Campaign I's mission 5.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoryTrigger {
    // Before the mission's briefing, as it starts.
    BeforeMission(MissionRef),
    // After the mission's debrief.
    AfterMission(MissionRef),
    // The campaign's epilogue: the campaign-complete screen's Watch the epilogue.
    Epilogue,
}

impl StoryTrigger {
    // Its kind, as story.state reports it.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::BeforeMission(_) => "before_mission",
            Self::AfterMission(_) => "after_mission",
            Self::Epilogue => "epilogue",
        }
    }

    pub fn mission(&self) -> Option<&MissionRef> {
        match self {
            Self::BeforeMission(mission) | Self::AfterMission(mission) => Some(mission),
            Self::Epilogue => None,
        }
    }
}

// A trigger resolved against its campaign's missions.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TriggerPoint {
    Before(usize),
    After(usize),
    Epilogue,
    // A mission index past the campaign's missions: it waits for that mission.
    Pending,
    // A mission id no mission has.
    Unknown,
}

impl TriggerPoint {
    // Where it plays in the campaign, for the order sequences are listed and replayed in: before a
    // mission, then after it, the epilogue last. None for one that can't play.
    pub fn order(self) -> Option<(usize, u8)> {
        match self {
            Self::Before(mission) => Some((mission, 0)),
            Self::After(mission) => Some((mission, 1)),
            Self::Epilogue => Some((usize::MAX, 2)),
            Self::Pending | Self::Unknown => None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelDef {
    // The picture, a path under assets/ui/ (a JPEG or PNG), drawn to fill the panel.
    pub image: String,
    // What the picture shows, from the story script: the art brief, and the placeholder art's dim
    // caption (crates/tools/ui-art draws it there). Not shown by the game.
    #[serde(default)]
    pub visual: String,
    // The narration caption (a Story key). None: none.
    #[serde(default)]
    pub narration: Option<String>,
    // Where the caption's top-left corner goes, in panel coordinates. None: the skin's place for it.
    #[serde(default)]
    pub narration_anchor: Option<[f32; 2]>,
    #[serde(default)]
    pub bubbles: Vec<BubbleDef>,
    // A flashback: the viewer draws the picture in sepia.
    #[serde(default)]
    pub sepia: bool,
    // A dynamic panel: the viewer lists the player's stele lines over it (the epilogue's).
    #[serde(default)]
    pub stele_lines: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BubbleDef {
    // The speaker's name over the line (a Realm key, e.g. "speaker.khai_son"). None: no name.
    #[serde(default)]
    pub speaker: Option<String>,
    // The line (a Story key).
    pub text: String,
    #[serde(default)]
    pub kind: BubbleKind,
    // The bubble's top-left corner, in panel coordinates.
    pub anchor: [f32; 2],
    // Its width in panel points. None: the skin's.
    #[serde(default)]
    pub width: Option<f32>,
    // The tip of its tail, in panel coordinates: the speaker.
    pub tail: [f32; 2],
}

// How a bubble is drawn (story-script.md's ink bubbles).
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Display, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum BubbleKind {
    #[default]
    Speech,
    Whisper,
    Group,
    // Bold ink: a declaration.
    Bold,
}

impl SequenceDef {
    // Every text key it names, with its category.
    pub fn text_keys(&self) -> Vec<(UiTextCategory, String)> {
        let mut keys = vec![(UiTextCategory::Story, self.title.clone())];
        for panel in &self.panels {
            keys.extend(panel.narration.iter().map(|key| (UiTextCategory::Story, key.clone())));
            for bubble in &panel.bubbles {
                keys.extend(bubble.speaker.iter().map(|key| (UiTextCategory::Realm, key.clone())));
                keys.push((UiTextCategory::Story, bubble.text.clone()));
            }
        }
        keys
    }
}

impl StoryDef {
    pub fn sequence(&self, id: &str) -> Option<&SequenceDef> {
        self.sequences.iter().find(|sequence| sequence.id == id)
    }

    // What's wrong with the file, against its campaign: a sequence id empty, given twice or not
    // [a-z0-9_]; a title or a panel's picture or line left empty; a sequence without panels; a
    // trigger naming a mission id the campaign lacks; two sequences on one trigger (those waiting
    // for a mission not written yet included); a bubble or a caption off the panel. Empty when all
    // is well. (Text keys and pictures: StoryConfigs.)
    pub fn validate(&self, campaign: &CampaignDef) -> Vec<String> {
        let mut errors = Vec::new();
        let mut points: Vec<(TriggerPoint, &SequenceDef)> = Vec::new();
        for (index, sequence) in self.sequences.iter().enumerate() {
            let id = &sequence.id;
            if id.is_empty() || !id.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_') {
                errors.push(format!("The sequence id '{id}' is empty or not [a-z0-9_]."));
            }
            if self.sequences[..index].iter().any(|other| other.id == *id) {
                errors.push(format!("The sequence id '{id}' is given twice."));
            }
            if sequence.title.is_empty() {
                errors.push(format!("Sequence '{id}' has no title."));
            }
            if sequence.panels.is_empty() {
                errors.push(format!("Sequence '{id}' has no panels."));
            }

            let point = resolve_trigger(campaign, &sequence.trigger);
            if point == TriggerPoint::Unknown {
                errors.push(format!("Sequence '{id}' plays {:?}, which isn't a mission of the campaign.", sequence.trigger));
            }
            // Two sequences on one point; for those that wait for their mission, on one trigger.
            if point.order().is_some() || point == TriggerPoint::Pending {
                let same = |(other_point, other_sequence): &&(TriggerPoint, &SequenceDef)| {
                    *other_point == point && (point != TriggerPoint::Pending || other_sequence.trigger == sequence.trigger)
                };
                if let Some((_, other)) = points.iter().find(same) {
                    let other = &other.id;
                    errors.push(format!("Sequences '{other}' and '{id}' both play {point:?} ({:?}).", sequence.trigger));
                }
                points.push((point, sequence));
            }

            for (number, panel) in sequence.panels.iter().enumerate().map(|(index, panel)| (index + 1, panel)) {
                if panel.image.is_empty() {
                    errors.push(format!("Sequence '{id}', panel {number} has no picture."));
                }
                if panel.narration.as_deref().is_some_and(str::is_empty) {
                    errors.push(format!("Sequence '{id}', panel {number} has an empty narration key."));
                }
                if panel.narration_anchor.is_some_and(|point| !on_panel(point)) {
                    errors.push(format!("Sequence '{id}', panel {number}: the caption is off the panel."));
                }
                for (bubble_number, bubble) in panel.bubbles.iter().enumerate().map(|(index, bubble)| (index + 1, bubble)) {
                    let where_ = format!("Sequence '{id}', panel {number}, bubble {bubble_number}");
                    if bubble.text.is_empty() || bubble.speaker.as_deref().is_some_and(str::is_empty) {
                        errors.push(format!("{where_} has an empty key."));
                    }
                    if !on_panel(bubble.anchor) || !on_panel(bubble.tail) {
                        errors.push(format!("{where_}: its anchor or tail is off the panel."));
                    }
                    if bubble.width.is_some_and(|width| width <= 0.0 || bubble.anchor[0] + width > PANEL_SIZE[0]) {
                        errors.push(format!("{where_}: its width runs off the panel."));
                    }
                }
            }
        }
        errors
    }
}

fn on_panel([x, y]: [f32; 2]) -> bool {
    (0.0..=PANEL_SIZE[0]).contains(&x) && (0.0..=PANEL_SIZE[1]).contains(&y)
}

// The trigger resolved against the campaign's missions.
pub fn resolve_trigger(campaign: &CampaignDef, trigger: &StoryTrigger) -> TriggerPoint {
    let resolve = |mission: &MissionRef| match campaign.resolve_mission(mission) {
        Some(index) => Ok(index),
        None => Err(match mission {
            MissionRef::Index(_) => TriggerPoint::Pending,
            MissionRef::Id(_) => TriggerPoint::Unknown,
        }),
    };
    match trigger {
        StoryTrigger::BeforeMission(mission) => resolve(mission).map_or_else(|point| point, TriggerPoint::Before),
        StoryTrigger::AfterMission(mission) => resolve(mission).map_or_else(|point| point, TriggerPoint::After),
        StoryTrigger::Epilogue => TriggerPoint::Epilogue,
    }
}

// ----------------------------------------------
// StoryConfigs
// ----------------------------------------------

// Every campaign's story file, by campaign id. A campaign without a file has no sequences.
#[derive(Default)]
pub struct StoryConfigs {
    pub stories: BTreeMap<String, StoryDef>,
}

impl StoryConfigs {
    // The file `assets/configs/story/<campaign id>.json` would be.
    pub fn file_path(campaign_id: &str) -> String {
        format!("{STORY_DIR}/{campaign_id}")
    }

    // Loads the file of every campaign that has one and checks them; errors are logged on the
    // campaign channel, and a file that doesn't load leaves its campaign without sequences. A file
    // is named by its campaign's id: one whose name no campaign has is an error, and isn't loaded.
    pub fn load_for(campaigns: &CampaignConfigs) -> Self {
        let dir = configs_path().join(PathRef::from_str(STORY_DIR));
        let files = file_sys::collect_files(&dir, CollectFlags::FilenamesOnly, Some("json")).unwrap_or_else(|err| {
            log::error!(log::channel!("campaign"), "Can't list the story files in {dir}: {err}");
            Vec::new()
        });
        let names: Vec<String> =
            files.iter().filter_map(|file| file.file_stem().and_then(|stem| stem.to_str()).map(str::to_string)).collect();
        for name in names.iter().filter(|name| campaigns.find_by_id(name).is_none()) {
            log::error!(
                log::channel!("campaign"),
                "The story file {STORY_DIR}/{name}.json names no campaign: its name must be a campaign id (campaigns.json)."
            );
        }

        let mut stories = BTreeMap::new();
        for campaign in campaigns.campaigns.iter().filter(|campaign| names.contains(&campaign.id)) {
            let path = Self::file_path(&campaign.id);
            let story: StoryDef = engine::config::load_asset_file(PathRef::from_str(&path));
            if story.sequences.is_empty() {
                log::error!(log::channel!("campaign"), "The story file {path}.json has no sequences.");
                continue;
            }
            stories.insert(campaign.id.clone(), story);
        }
        let configs = Self { stories };
        let errors =
            configs.validate(campaigns).into_iter().chain(configs.missing_text_keys()).chain(configs.missing_images());
        for error in errors {
            log::error!(log::channel!("campaign"), "Story {error}");
        }
        for (campaign, sequence, trigger) in configs.pending(campaigns) {
            log::info!(
                log::channel!("campaign"),
                "Story '{campaign}': sequence '{sequence}' waits for its mission ({trigger:?}), which isn't written yet."
            );
        }
        configs
    }

    // Each file's errors (StoryDef::validate), prefixed with its campaign's id.
    pub fn validate(&self, campaigns: &CampaignConfigs) -> Vec<String> {
        let mut errors = Vec::new();
        for (id, story) in &self.stories {
            match campaigns.find_by_id(id).and_then(|index| campaigns.campaign(index)) {
                Some(campaign) => {
                    errors.extend(story.validate(campaign).into_iter().map(|error| format!("'{id}': {error}")))
                }
                None => errors.push(format!("'{id}': no campaign has this id.")),
            }
        }
        errors
    }

    // Every text key the files name, with its category.
    pub fn text_keys(&self) -> Vec<(UiTextCategory, String)> {
        self.stories.values().flat_map(|story| &story.sequences).flat_map(SequenceDef::text_keys).collect()
    }

    // The keys en.json (or the current language) lacks, as errors.
    pub fn missing_text_keys(&self) -> Vec<String> {
        self.text_keys()
            .into_iter()
            .filter(|(category, key)| text::peek(*category, key, None).1.is_none())
            .map(|(category, key)| format!("text: no {category} '{key}' in the strings."))
            .collect()
    }

    // The pictures that aren't under assets/ui/, as errors.
    pub fn missing_images(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for (id, story) in &self.stories {
            for sequence in &story.sequences {
                for panel in sequence.panels.iter().filter(|panel| !panel.image.is_empty()) {
                    if !file_sys::exists(engine::ui::assets_path().join(PathRef::from_str(&panel.image))) {
                        errors.push(format!(
                            "'{id}': sequence '{}' names the picture ui/{}, which isn't there.",
                            sequence.id, panel.image
                        ));
                    }
                }
            }
        }
        errors
    }

    // The sequences that wait for a mission not written yet: (campaign id, sequence id, trigger).
    pub fn pending(&self, campaigns: &CampaignConfigs) -> Vec<(String, String, StoryTrigger)> {
        let mut pending = Vec::new();
        for (id, story) in &self.stories {
            let Some(campaign) = campaigns.find_by_id(id).and_then(|index| campaigns.campaign(index)) else {
                continue;
            };
            for sequence in &story.sequences {
                if resolve_trigger(campaign, &sequence.trigger) == TriggerPoint::Pending {
                    pending.push((id.clone(), sequence.id.clone(), sequence.trigger.clone()));
                }
            }
        }
        pending
    }

    // The story of the campaign at `campaign_index`. None: it has none.
    pub fn for_campaign<'a>(&'a self, campaigns: &CampaignConfigs, campaign_index: usize) -> Option<&'a StoryDef> {
        self.stories.get(&campaigns.campaign(campaign_index)?.id)
    }

    // Loads them for the campaigns loaded (CampaignConfigs::load first).
    pub fn load() -> &'static Self {
        Self::initialize(Self::load_for(CampaignConfigs::get()));
        Self::get()
    }
}

common::singleton_late_init! { STORY_CONFIGS_SINGLETON, StoryConfigs }
