// Story commands (docs/plans/realm-ui.md, Phase 4): a campaign's story sequences, read from the
// configs and the player's profile.

use serde::Deserialize;
use serde_json::{Value, json};

use engine::ui::text::{self, UiTextCategory};

use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    campaign::config::{CampaignConfigs, MissionRef},
    profile,
    story::{self, PANEL_SIZE, PanelDef, SequenceDef, StoryConfigs, TriggerPoint, config::resolve_trigger},
    world_map,
};

pub(super) const COMMANDS: &[CommandDef] = &[CommandDef {
    name: "story.state",
    help: "A campaign's story sequences (assets/configs/story/<campaign>.json): each with its id, title, trigger \
           (before_mission or after_mission and the mission, or epilogue; pending while its mission isn't written \
           yet), whether the profile has seen it and whether it plays on its own (unseen, its mission there; never an \
           epilogue, which only Watch the epilogue plays), and its panels \
           (picture, narration, bubbles with their speaker, kind, anchor and tail in panel points, sepia, \
           stele_lines). Also: the panel's size in points, what plays before and after the mission being played, \
           the campaign's epilogue, and every sequence the profile has seen, in Replay the story's order.",
    params: &[ParamDef {
        name: "campaign",
        kind: "string",
        required: false,
        help: "A campaign id (campaigns.json), hidden ones included. Default: the mission being played's \
               campaign, else the custom game's realm's, else the latest the player can play.",
    }],
    run: cmd_state,
}];

fn cmd_state(_context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        campaign: Option<String>,
    }
    let params: Params = parse_params(params)?;
    let campaigns = CampaignConfigs::get();
    let campaign_index = match &params.campaign {
        Some(id) => campaigns.find_by_id(id).ok_or_else(|| {
            let ids: Vec<&str> = campaigns.campaigns.iter().map(|campaign| campaign.id.as_str()).collect();
            CommandError::not_found(format!("No campaign has the id '{id}'. Campaigns: {}.", ids.join(", ")))
        })?,
        None => world_map::default_campaign(),
    };
    let campaign = campaigns.campaign(campaign_index).ok_or_else(|| CommandError::not_found("There are no campaigns."))?;
    let stories = StoryConfigs::get();
    let profile = profile::get();

    let story = stories.stories.get(&campaign.id);
    let sequences: Vec<Value> = story.map_or_else(Vec::new, |story| {
        story
            .sequences
            .iter()
            .map(|sequence| {
                let point = resolve_trigger(campaign, &sequence.trigger);
                let seen = profile.has_seen_sequence(&campaign.id, &sequence.id);
                json!({
                    "id": sequence.id,
                    "title": sequence.title,
                    "title_text": text::get(UiTextCategory::Story, &sequence.title),
                    "trigger": trigger_json(campaign, sequence, point),
                    "seen": seen,
                    // Plays on its own in the campaign flow: an epilogue never does (Watch the
                    // epilogue plays it).
                    "plays": !seen && matches!(point, TriggerPoint::Before(_) | TriggerPoint::After(_)),
                    "panel_count": sequence.panels.len(),
                    "panels": sequence.panels.iter().map(panel_json).collect::<Vec<_>>(),
                })
            })
            .collect()
    });

    let active = crate::campaign::CampaignManager::is_initialized()
        .then(crate::campaign::active_mission)
        .flatten()
        .filter(|mission| mission.campaign_id == campaign_index);
    let id_of = |sequence: Option<story::SequenceRef>| sequence.map(|sequence| sequence.sequence.id.clone());
    let playing = active.map(|mission| {
        json!({
            "mission": mission.mission_index,
            "before": id_of(stories.before_mission(campaigns, campaign_index, mission.mission_index)),
            "after": id_of(stories.after_mission(campaigns, campaign_index, mission.mission_index)),
        })
    });
    let replay: Vec<Value> = stories
        .seen_sequences(campaigns, profile)
        .iter()
        .map(|sequence| json!({ "campaign": sequence.campaign.id, "sequence": sequence.sequence.id }))
        .collect();

    done(json!({
        "campaign": campaign.id,
        "file": story.map(|_| format!("assets/configs/{}.json", StoryConfigs::file_path(&campaign.id))),
        "panel_size": PANEL_SIZE,
        "sequences": sequences,
        "seen": profile.record(&campaign.id).map_or_else(Vec::new, |record| record.sequences_seen.clone()),
        "active_mission": playing,
        "epilogue": id_of(stories.epilogue(campaigns, campaign_index)),
        "replay": replay,
    }))
}

fn trigger_json(campaign: &crate::campaign::config::CampaignDef, sequence: &SequenceDef, point: TriggerPoint) -> Value {
    let mission = match point {
        TriggerPoint::Before(index) | TriggerPoint::After(index) => Some(index),
        _ => None,
    };
    json!({
        "kind": sequence.trigger.kind(),
        // As the file names it: an index or an id.
        "names": sequence.trigger.mission().map(|mission| match mission {
            MissionRef::Index(index) => json!(index),
            MissionRef::Id(id) => json!(id),
        }),
        "mission": mission,
        "number": mission.map(|index| campaign.mission_number(index)),
        "pending": point == TriggerPoint::Pending,
    })
}

fn panel_json(panel: &PanelDef) -> Value {
    json!({
        "image": panel.image,
        "narration": panel.narration,
        "narration_text": panel.narration.as_deref().map(|key| text::get(UiTextCategory::Story, key)),
        "narration_anchor": panel.narration_anchor,
        "bubbles": panel.bubbles.iter().map(|bubble| json!({
            "speaker": bubble.speaker,
            "speaker_text": bubble.speaker.as_deref().map(|key| text::get(UiTextCategory::Realm, key)),
            "text": bubble.text,
            "text_text": text::get(UiTextCategory::Story, &bubble.text),
            "kind": bubble.kind.to_string(),
            "anchor": bubble.anchor,
            "width": bubble.width,
            "tail": bubble.tail,
        })).collect::<Vec<_>>(),
        "sepia": panel.sepia,
        "stele_lines": panel.stele_lines,
    })
}
