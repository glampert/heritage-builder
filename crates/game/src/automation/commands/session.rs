// Session commands: game state, preset maps, saves.

use serde::Deserialize;
use serde_json::{Value, json};

use engine::{file_sys::paths::PathRef, save};

use super::validate_save_name;
use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params, pending},
    campaign::{self, config::CampaignConfigs},
    debug::preset_maps,
    launch,
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "session.state",
        help: "Menus mode, map size, simulation state, seed and campaign mission.",
        params: &[],
        run: cmd_state,
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
        name: "session.save",
        help: "Save the game.",
        params: &[ParamDef { name: "name", kind: "string", required: true, help: "Save name, without folder or extension." }],
        run: cmd_save,
    },
    CommandDef {
        name: "session.quit_to_menu",
        help: "Leave the game for the home menu. Ends any campaign.",
        params: &[],
        run: cmd_quit_to_menu,
    },
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveNameParams {
    name: String,
}

fn cmd_state(context: &mut CommandContext, _params: Value) -> CommandResult {
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
            "completed": mission.completed,
        })
    });

    done(json!({
        "menus_mode": session.current_menus_mode().map(|mode| format!("{mode:?}")),
        "map_size": [map_size.width, map_size.height],
        "paused": sim.is_paused(),
        "speed": sim.speed(),
        "gold": stats.treasury.gold_units_total,
        "population": stats.population.total,
        "seed": launch::get().random_seed(context.configs),
        "frame": context.frame(),
        "campaign": campaign,
    }))
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

    // The session command queue runs at the start of the next frame, before this is polled.
    let requested_frame = context.frame();
    pending(move |context| {
        (context.frame() > requested_frame).then(|| {
            let map_size = context.session.tile_map().size_in_cells();
            Ok(json!({ "preset": params.preset, "name": preset_name, "map_size": [map_size.width, map_size.height] }))
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
    if !context.session.load_save_game(context.engine, context.configs, PathRef::from_str(&params.name)) {
        return Err(CommandError::failed(format!("Couldn't load '{}'. The log has the details.", params.name)));
    }

    let map_size = context.session.tile_map().size_in_cells();
    done(json!({ "name": params.name, "map_size": [map_size.width, map_size.height] }))
}

fn cmd_save(context: &mut CommandContext, params: Value) -> CommandResult {
    let params: SaveNameParams = parse_params(params)?;
    validate_save_name(&params.name)?;
    context.require_map()?;

    if !context.session.save_game(PathRef::from_str(&params.name)) {
        return Err(CommandError::failed(format!("Couldn't save '{}'. The log has the details.", params.name)));
    }

    done(json!({ "name": params.name }))
}

fn cmd_quit_to_menu(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.session_cmds.push_quit_to_main_menu();

    let requested_frame = context.frame();
    pending(move |context| (context.frame() > requested_frame).then(|| Ok(Value::Null)))
}
