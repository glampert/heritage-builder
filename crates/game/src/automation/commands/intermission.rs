// Intermission commands (docs/plans/realm-ui.md, Phase 3): what the debrief, the campaign-complete
// screen and the briefing show, read from the configs and the player's profile.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    campaign::{
        config::{CampaignConfigs, MissionRef},
        stele,
    },
    profile, world_map,
};

pub(super) const COMMANDS: &[CommandDef] = &[CommandDef {
    name: "intermission.state",
    help: "What a campaign's stele shows, as the debrief and the campaign-complete screen carve it: its title, \
           one line per finished step (the mission's stele line, or its choice point's for a variant without \
           one; each with its mission index, number, Story key and text) followed by a line per distinction \
           earned, the line freshly carved (newest), and how many steps are still to come (empty rows). \
           full: the stele as it would stand with every step finished, at their first missions (no site \
           picked, no distinctions), the last line freshly carved.",
    params: &[
        ParamDef {
            name: "campaign",
            kind: "string",
            required: false,
            help: "A campaign id (campaigns.json), hidden ones included. Default: the mission being played's \
                   campaign, else the custom game's realm's, else the latest the player can play.",
        },
        ParamDef {
            name: "newest",
            kind: "any",
            required: false,
            help: "The mission whose lines are freshly carved, by index or id (the one just finished). Default, \
                   or a mission with no line yet: the last line's. Not with full, which carves the last line \
                   freshly.",
        },
        ParamDef {
            name: "full",
            kind: "bool",
            required: false,
            help: "The stele with every step finished, at their first missions (default false).",
        },
    ],
    run: cmd_state,
}];

fn cmd_state(_context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        campaign: Option<String>,
        newest: Option<MissionRef>,
        #[serde(default)]
        full: bool,
    }
    let params: Params = parse_params(params)?;
    if params.full && params.newest.is_some() {
        return Err(CommandError::invalid_params("newest can't be given with full: a full stele carves its last line freshly."));
    }
    let configs = CampaignConfigs::get();
    let campaign_index = match &params.campaign {
        Some(id) => configs.find_by_id(id).ok_or_else(|| {
            let ids: Vec<&str> = configs.campaigns.iter().map(|campaign| campaign.id.as_str()).collect();
            CommandError::not_found(format!("No campaign has the id '{id}'. Campaigns: {}.", ids.join(", ")))
        })?,
        None => world_map::default_campaign(),
    };
    let campaign = configs.campaign(campaign_index).ok_or_else(|| CommandError::not_found("There are no campaigns."))?;
    let newest =
        match &params.newest {
            Some(mission) => Some(campaign.resolve_mission(mission).ok_or_else(|| {
                CommandError::not_found(format!("Campaign '{}' has no mission {mission:?}.", campaign.id))
            })?),
            None => None,
        };
    let stele = if params.full {
        stele::carved_in_full(configs, campaign_index)
    } else {
        stele::stele(configs, profile::get(), campaign_index, newest)
    };
    let stele = stele.ok_or_else(|| CommandError::not_found("There are no campaigns."))?;
    done(json!({ "campaign": campaign.id, "stele": stele.state_json() }))
}
