// Session commands: game state, new games, preset maps, saves, menus mode.

use serde::Deserialize;
use serde_json::{Value, json};
use strum::{Display, EnumString};

use common::hash;
use engine::{file_sys::paths::PathRef, save};

use super::{SizeParam, validate_save_name, world::find_tile_def};
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params, pending},
    campaign::{self, config::{CampaignConfigs, MissionMap, MissionRef}},
    custom_game,
    config::LoadMapSetting,
    debug::preset_maps,
    launch,
    menu::GameMenusMode,
    profile,
    session::{self, GameSession},
    tile::TileMapLayerKind,
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "session.state",
        help: "Menus mode, map size, simulation state, date, seed, campaign mission and custom game.",
        params: &[],
        run: cmd_state,
    },
    CommandDef {
        name: "session.new_map",
        help: "Start a new game on an empty map, seeded like --map: the same seed gives the same game. Ends any campaign.",
        params: &[
            ParamDef { name: "size", kind: "size", required: true, help: "[width, height] or \"WxH\", from 8 to 512 cells." },
            ParamDef { name: "terrain", kind: "string", required: false, help: "Terrain tile: grass (default), dirt or water." },
        ],
        run: cmd_new_map,
    },
    CommandDef { name: "session.list_presets", help: "List the debug preset maps.", params: &[], run: cmd_list_presets },
    CommandDef {
        name: "session.load_preset",
        help: "Start a new game on a debug preset map. Ends any campaign.",
        params: &[ParamDef { name: "preset", kind: "int", required: true, help: "Preset number, see session.list_presets." }],
        run: cmd_load_preset,
    },
    CommandDef { name: "session.list_saves", help: "List the save games in saves/.", params: &[], run: cmd_list_saves },
    CommandDef {
        name: "session.load_save",
        help: "Load a save game.",
        params: &[ParamDef { name: "name", kind: "string", required: true, help: "Save name, without folder or extension." }],
        run: cmd_load_save,
    },
    CommandDef {
        name: "session.save_info",
        help: "What a save says about itself without loading it: whether it exists, whether this game can load it, its header (campaign, mission, realm, date, population, treasury, time played, time saved; empty for an old save) and its thumbnail, if any (also for a missing save, so a thumbnail left behind shows).",
        params: &[ParamDef { name: "name", kind: "string", required: true, help: "Save name, without folder or extension." }],
        run: cmd_save_info,
    },
    CommandDef {
        name: "session.delete_save",
        help: "Delete a save game and its thumbnail.",
        params: &[ParamDef { name: "name", kind: "string", required: true, help: "Save name, without folder or extension." }],
        run: cmd_delete_save,
    },
    CommandDef {
        name: "session.save",
        help: "Save the game.",
        params: &[ParamDef { name: "name", kind: "string", required: true, help: "Save name, without folder or extension." }],
        run: cmd_save,
    },
    CommandDef {
        name: "session.checksum",
        help: "Hash of the game state as a save would store it. Equal across runs means the runs matched.",
        params: &[],
        run: cmd_checksum,
    },
    CommandDef {
        name: "session.start_mission",
        help: "Start a campaign mission, as the home menu's Campaign does: any campaign, hidden ones included (test_choices). The mission is the profile's next one unless given.",
        params: &[
            ParamDef { name: "campaign", kind: "string", required: true, help: "The campaign's id, e.g. dragon_legacy." },
            ParamDef { name: "mission", kind: "any", required: false, help: "Its index from 0 or its id; the profile's next mission if left out." },
        ],
        run: cmd_start_mission,
    },
    CommandDef {
        name: "session.quit_to_menu",
        help: "Leave the game for the home menu. Ends any campaign.",
        params: &[],
        run: cmd_quit_to_menu,
    },
    CommandDef {
        name: "menus.mode",
        help: "Get or set the menus: InGame (the player's HUD) or DevEditor.",
        params: &[ParamDef { name: "mode", kind: "string", required: false, help: "InGame or DevEditor." }],
        run: cmd_menus_mode,
    },
];

// Map sizes session.new_map accepts, in cells per side.
const NEW_MAP_SIZE_LIMITS: (i32, i32) = (8, 512);

// Frames session changes wait, once the session command queue has run, before they finish. None:
// the new menus take input on their first frame.
const SESSION_SETTLE_FRAMES: u64 = 0;

// Finishes `settle` frames after the session command queue has run (it runs at the start of the
// next frame), with `reply_fn`'s result.
fn after_session_settles<F>(context: &CommandContext, queued: bool, mut reply_fn: F) -> CommandResult
where
    F: FnMut(&mut CommandContext) -> Value + 'static,
{
    let done_frame = context.frame() + u64::from(queued) + SESSION_SETTLE_FRAMES;
    pending(move |context| (context.frame() >= done_frame).then(|| Ok(reply_fn(context))))
}
const DEFAULT_NEW_MAP_TERRAIN: &str = "grass";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveNameParams {
    name: String,
}

fn cmd_state(context: &mut CommandContext, _params: Value) -> CommandResult {
    super::refresh_world_stats(context);

    let session = &*context.session;
    let sim = session.sim();
    let stats = session.world().stats();
    let map_size = session.tile_map().size_in_cells();

    let campaign = campaign::active_mission().map(|mission| {
        let configs = CampaignConfigs::get();
        json!({
            "campaign": configs.campaign(mission.campaign_id).map(|campaign| campaign.name.clone()),
            "mission": configs.mission(mission.campaign_id, mission.mission_index).map(|mission| mission.name.clone()),
            "mission_index": mission.mission_index,
            "campaign_id": configs.campaign(mission.campaign_id).map(|campaign| campaign.id.clone()),
            // None for a mission without an id in campaigns.json.
            "mission_id": configs.mission(mission.campaign_id, mission.mission_index).map(|mission| mission.id.clone()).filter(|id| !id.is_empty()),
            "completed": mission.completed,
        })
    });

    done(json!({
        "menus_mode": session.current_menus_mode().map(|mode| format!("{mode:?}")),
        "map_size": [map_size.width, map_size.height],
        "paused": sim.is_paused(),
        "speed": sim.speed(),
        "sim_time": sim.elapsed_secs(),
        "date": super::sim::date_json(sim.calendar()),
        "gold": stats.treasury.gold_units_total,
        "population": stats.population.total,
        "seed": launch::get().random_seed(context.configs),
        "frame": context.frame(),
        "campaign": campaign,
        "custom_game": custom_game_json(),
    }))
}

// The custom game being played, if any: its realm, map and options, and the construction time
// scale its fast construction gives (SimConfigs::construction.time_scale times its own).
fn custom_game_json() -> Value {
    let Some(game) = custom_game::active() else {
        return Value::Null;
    };
    let record = &game.record;
    let construction = &crate::config::GameConfigs::get().sim.construction;
    json!({
        "realm": game.realm,
        "map": record.map,
        "difficulty": record.difficulty,
        "fast_construction": record.fast_construction,
        "construction_time_scale": construction.time_scale * custom_game::construction_time_scale(),
        "seasons_and_floods": record.seasons_and_floods,
        "raids": record.raids,
    })
}

fn cmd_new_map(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        size: SizeParam,
        terrain: Option<String>,
    }

    let params: Params = parse_params(params)?;
    let size = params.size.0;

    let (min_size, max_size) = NEW_MAP_SIZE_LIMITS;
    if !(min_size..=max_size).contains(&size.width) || !(min_size..=max_size).contains(&size.height) {
        return Err(CommandError::invalid_params(format!(
            "Map sides go from {min_size} to {max_size} cells, got {}x{}.",
            size.width, size.height
        )));
    }

    let terrain_name = params.terrain.as_deref().unwrap_or(DEFAULT_NEW_MAP_TERRAIN);
    let (terrain_tile_def, terrain_category) = find_tile_def(terrain_name, Some(TileMapLayerKind::Terrain))?;

    if terrain_tile_def.is_road() || terrain_tile_def.is_vacant_lot() {
        return Err(CommandError::invalid_params(format!("'{terrain_name}' can't fill a map. Use grass, dirt or water.")));
    }

    let load_map_setting = LoadMapSetting::EmptyMap {
        size_in_cells: size,
        terrain_tile_category: terrain_category.name.clone(),
        terrain_tile_name: terrain_tile_def.name.clone(),
    };

    campaign::reset();
    session::replace_with_new_game(context.session, context.engine, context.configs, &load_map_setting);

    let reply = json!({ "map_size": [size.width, size.height], "terrain": terrain_tile_def.name });
    after_session_settles(context, false, move |_| reply.clone())
}

fn cmd_list_presets(_context: &mut CommandContext, _params: Value) -> CommandResult {
    let presets: Vec<Value> = preset_maps::preset_tile_maps_list()
        .iter()
        .enumerate()
        .map(|(preset, name)| json!({ "preset": preset, "name": name }))
        .collect();

    done(json!({ "presets": presets }))
}

fn cmd_load_preset(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        preset: usize,
    }

    let params: Params = parse_params(params)?;

    // create_preset_tile_map clamps out-of-range numbers instead of failing.
    let presets = preset_maps::preset_tile_maps_list();
    let Some(preset_name) = presets.get(params.preset).copied() else {
        return Err(CommandError::invalid_params(format!(
            "No preset {}. Presets go from 0 to {}.",
            params.preset,
            presets.len() - 1
        )));
    };

    // Loading a preset keeps the active campaign mission otherwise.
    campaign::reset();
    context.session_cmds.push_load_preset_map(params.preset);

    after_session_settles(context, true, move |context| {
        let map_size = context.session.tile_map().size_in_cells();
        json!({ "preset": params.preset, "name": preset_name, "map_size": [map_size.width, map_size.height] })
    })
}

fn cmd_start_mission(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        campaign: String,
        mission: Option<MissionRef>,
    }

    let params: Params = parse_params(params)?;
    let configs = CampaignConfigs::get();
    let Some(campaign_index) = configs.find_by_id(&params.campaign) else {
        let ids: Vec<&str> = configs.campaigns.iter().map(|campaign| campaign.id.as_str()).collect();
        return Err(CommandError::not_found(format!("No campaign has the id '{}'. Campaigns: {}.", params.campaign, ids.join(", "))));
    };
    let campaign = &configs.campaigns[campaign_index];
    let mission_index = match &params.mission {
        Some(mission) => campaign.resolve_mission(mission),
        None => profile::get().next_mission(configs, campaign_index),
    };
    let Some(mission_index) = mission_index else {
        return Err(CommandError::not_found(format!("Campaign '{}' has no such mission ({} missions).", campaign.id, campaign.missions.len())));
    };
    // A save map this game can't load would start nothing: refused before the campaign changes.
    let map = campaign::start_mission_if_loadable(campaign_index, mission_index)
        .map_err(|err| CommandError::failed(format!("{err} Extract saves/sample_saves.zip.")))?;
    match map {
        MissionMap::SaveGame { save_file } => context.session_cmds.push_load_save_game(PathRef::from_str(&save_file)),
        MissionMap::Preset { preset_number } => context.session_cmds.push_load_preset_map(preset_number),
    }

    after_session_settles(context, true, move |_| {
        let active = campaign::active_mission();
        json!({
            "started": active.as_ref().is_some_and(|active| active.campaign_id == campaign_index && active.mission_index == mission_index),
            "campaign_index": campaign_index,
            "mission_index": mission_index,
        })
    })
}

fn cmd_list_saves(_context: &mut CommandContext, _params: Value) -> CommandResult {
    let saves: Vec<String> =
        save::storage::list_save_files().iter().map(|save| save.to_string_lossy().into_owned()).collect();

    done(json!({ "saves": saves }))
}

fn cmd_load_save(context: &mut CommandContext, params: Value) -> CommandResult {
    let params: SaveNameParams = parse_params(params)?;
    validate_save_name(&params.name)?;

    // Safe to load right away: this runs at the same point as the session command queue.
    context
        .session
        .load_save_game(context.engine, context.configs, PathRef::from_str(&params.name))
        .map_err(CommandError::failed)?;

    let map_size = context.session.tile_map().size_in_cells();
    let reply = json!({ "name": params.name, "map_size": [map_size.width, map_size.height] });
    after_session_settles(context, false, move |_| reply.clone())
}

fn cmd_save(context: &mut CommandContext, params: Value) -> CommandResult {
    let params: SaveNameParams = parse_params(params)?;
    validate_save_name(&params.name)?;
    context.require_map()?;

    context.session.save_game_with_thumbnail(context.engine, context.configs, PathRef::from_str(&params.name)).map_err(CommandError::failed)?;

    let path = save::storage::save_files_path().join(&params.name).with_extension("json");
    done(json!({ "name": params.name, "path": path.to_string() }))
}

fn require_save(name: &str) -> Result<(), CommandError> {
    validate_save_name(name)?;
    if !save::storage::list_save_files().iter().any(|save| save.as_os_str() == name) {
        return Err(CommandError::not_found(format!("No save named '{name}'.")));
    }
    Ok(())
}

fn cmd_save_info(_context: &mut CommandContext, params: Value) -> CommandResult {
    let params: SaveNameParams = parse_params(params)?;
    validate_save_name(&params.name)?;

    let thumbnail = save::storage::save_thumbnail_path(PathRef::from_str(&params.name)).filter(|path| engine::file_sys::exists(path));
    if require_save(&params.name).is_err() {
        // No save by that name: report a thumbnail it left behind, which a deletion should take too.
        return done(json!({
            "name": params.name,
            "exists": false,
            "loadable": false,
            "header": Value::Null,
            "has_thumbnail": thumbnail.is_some(),
            "thumbnail": thumbnail.map(|path| path.to_string()),
        }));
    }

    let summary = session::probe_save(PathRef::from_str(&params.name)).map_err(CommandError::failed)?;
    done(json!({
        "name": params.name,
        "exists": true,
        "loadable": summary.loadable,
        "header": summary.header,
        "has_thumbnail": thumbnail.is_some(),
        "thumbnail": thumbnail.map(|path| path.to_string()),
    }))
}

fn cmd_delete_save(_context: &mut CommandContext, params: Value) -> CommandResult {
    let params: SaveNameParams = parse_params(params)?;
    require_save(&params.name)?;

    session::delete_save(PathRef::from_str(&params.name)).map_err(CommandError::failed)?;
    done(json!({ "name": params.name }))
}

fn cmd_checksum(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;

    let (checksum, bytes) = state_checksum(context.session).map_err(CommandError::failed)?;
    done(json!({ "checksum": checksum, "bytes": bytes }))
}

// Hash of the game state as a save would store it, and the state's size in bytes.
pub fn state_checksum(session: &mut GameSession) -> Result<(String, usize), String> {
    let state = session.save_to_string()?;
    Ok((format!("{:016x}", hash::fnv1a_from_str(&state)), state.len()))
}

fn cmd_quit_to_menu(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.session_cmds.push_quit_to_main_menu();
    after_session_settles(context, true, |_| Value::Null)
}

#[derive(Copy, Clone, PartialEq, Eq, Display, EnumString, Deserialize)]
enum MenusModeParam {
    InGame,
    DevEditor,
}

fn cmd_menus_mode(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        mode: Option<MenusModeParam>,
    }

    let params: Params = parse_params(params)?;

    if let Some(mode) = params.mode {
        context.require_map()?;

        let wanted = match mode {
            MenusModeParam::InGame => GameMenusMode::InGame,
            MenusModeParam::DevEditor => GameMenusMode::DevEditor,
        };

        // The session can only toggle between the two.
        if context.session.current_menus_mode() != Some(wanted) {
            context.session.toggle_menus_mode(context.engine);
        }
    }

    done(json!({ "mode": context.session.current_menus_mode().map(|mode| format!("{mode:?}")) }))
}
