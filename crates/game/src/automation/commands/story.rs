// Story commands (docs/plans/realm-ui.md, Phase 4): a campaign's story sequences, read from the
// configs and the player's profile.

use serde::Deserialize;
use serde_json::{Value, json};

use engine::ui::text::{self, UiTextCategory};

use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ErrorCode, ParamDef, done, parse_params},
    campaign::config::{CampaignConfigs, MissionRef},
    menu::dialog::{self, DialogMenuKind, StoryRequest},
    profile,
    story::{self, PANEL_SIZE, PanelDef, SequenceDef, StoryConfigs, TriggerPoint, config::resolve_trigger},
    world_map,
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "story.state",
        help: "A campaign's story sequences (assets/configs/story/<campaign>.json): each with its id, title, trigger \
           (before_mission or after_mission and the mission, or epilogue; pending while its mission isn't written \
           yet), whether the profile has seen it and whether it plays on its own (unseen and its mission there: \
           before the mission's briefing, on its debrief's Continue, or an epilogue on the campaign-complete \
           screen's Return to the menu), and its panels (picture, narration, bubbles with their speaker, kind, \
           anchor and tail in panel points, sepia, stele_lines). Also: the panel's size in points, what plays \
           before and after the mission being played, the campaign's epilogue, and every sequence the profile has \
           seen, in Replay the story's order.",
        params: &[ParamDef {
            name: "campaign",
            kind: "string",
            required: false,
            help: "A campaign id (campaigns.json), hidden ones included. Default: the mission being played's \
               campaign, else the custom game's realm's, else the latest the player can play.",
        }],
        run: cmd_state,
    },
    CommandDef {
        name: "story.play",
        help: "Play a story sequence in the comic viewer (StoryComic), over the dialogs open, in the UI realm's look \
           (ui.realm), the simulation paused while it plays. A sequence playing is ended first. It doesn't change \
           the profile unless seen=true: then ending it (Skip, Escape, Continue) records it seen, as the campaign \
           flow does. Its panels are StoryComic/panel (kind canvas) in ui.tree; StoryComic/bar/Next, Back and Skip \
           step through it, and so do the keys (Space, Enter, Right, Left, Escape).",
        params: &[
            ParamDef {
                name: "campaign",
                kind: "string",
                required: false,
                help: "A campaign id (campaigns.json), hidden ones included. Default: as story.state's.",
            },
            ParamDef {
                name: "sequence",
                kind: "string",
                required: false,
                help: "The sequence's id (story.state). Default: the campaign's first, in the order they play.",
            },
            ParamDef { name: "panel", kind: "int", required: false, help: "The panel to start on, from 1. Default: 1." },
            ParamDef {
                name: "seen",
                kind: "bool",
                required: false,
                help: "Record it seen in the profile once it ends. Default: false.",
            },
        ],
        run: cmd_play,
    },
];

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
                    // Plays on its own in the campaign flow: before the briefing, on the debrief's
                    // Continue, or an epilogue on the campaign-complete screen's Return to the menu.
                    "plays": !seen && !matches!(point, TriggerPoint::Pending | TriggerPoint::Unknown),
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
        "playing": playing_json(),
    }))
}

// The sequence the comic viewer shows, if it's open.
fn playing_json() -> Value {
    let Some((campaign_index, sequence, panel)) = crate::menu::dialog::story_playing() else {
        return Value::Null;
    };
    let campaign = CampaignConfigs::get().campaign(campaign_index).map(|campaign| campaign.id.clone());
    json!({ "campaign": campaign, "sequence": sequence, "panel": panel + 1 })
}

fn campaign_param(campaign: Option<&str>) -> Result<usize, CommandError> {
    let campaigns = CampaignConfigs::get();
    match campaign {
        Some(id) => campaigns.find_by_id(id).ok_or_else(|| {
            let ids: Vec<&str> = campaigns.campaigns.iter().map(|campaign| campaign.id.as_str()).collect();
            CommandError::not_found(format!("No campaign has the id '{id}'. Campaigns: {}.", ids.join(", ")))
        }),
        None => Ok(world_map::default_campaign()),
    }
}

fn cmd_play(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        campaign: Option<String>,
        sequence: Option<String>,
        panel: Option<usize>,
        #[serde(default)]
        seen: bool,
    }
    let params: Params = parse_params(params)?;
    if !dialog::is_initialized() {
        return Err(CommandError::new(ErrorCode::NotInGame, "No dialogs here: they come with the home and in-game menus."));
    }
    let campaigns = CampaignConfigs::get();
    let campaign_index = campaign_param(params.campaign.as_deref())?;
    let campaign = campaigns.campaign(campaign_index).ok_or_else(|| CommandError::not_found("There are no campaigns."))?;
    let stories = StoryConfigs::get();
    let story = stories.stories.get(&campaign.id);
    let ids: Vec<String> =
        story.map_or_else(Vec::new, |story| story.sequences.iter().map(|sequence| sequence.id.clone()).collect());
    let sequence = match &params.sequence {
        Some(id) => story.and_then(|story| story.sequence(id)).ok_or_else(|| {
            CommandError::not_found(format!(
                "Campaign '{}' has no story sequence '{id}'. Its sequences: {}.",
                campaign.id,
                if ids.is_empty() { "none".to_string() } else { ids.join(", ") }
            ))
        })?,
        None => {
            let first = stories.sequences_in_order(campaigns, campaign_index).into_iter().next();
            first
                .map(|sequence| sequence.sequence)
                .ok_or_else(|| CommandError::not_found(format!("Campaign '{}' has no story sequences.", campaign.id)))?
        }
    };
    let panel = params.panel.unwrap_or(1);
    if panel == 0 || panel > sequence.panels.len() {
        return Err(CommandError::invalid_params(format!(
            "Sequence '{}' has panels 1 to {}.",
            sequence.id,
            sequence.panels.len()
        )));
    }

    let request = StoryRequest {
        campaign_index,
        sequence_id: sequence.id.clone(),
        panel: panel - 1,
        records_seen: params.seen,
        on_end: None,
    };
    let opened = context.session.with_ui_context(context.engine, |ui_context| {
        if dialog::is_in_stack(DialogMenuKind::StoryComic) {
            dialog::close(DialogMenuKind::StoryComic, ui_context);
        }
        dialog::play_story(request, ui_context)
    });
    if !opened {
        return Err(CommandError::failed(format!("Sequence '{}' didn't play.", sequence.id)));
    }
    done(json!({
        "campaign": campaign.id,
        "sequence": sequence.id,
        "panel": panel,
        "panels": sequence.panels.len(),
        "seen": params.seen,
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
