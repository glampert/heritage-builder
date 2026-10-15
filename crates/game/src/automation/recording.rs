// The game's side of --record and --replay (see engine::automation::recording): its details in
// the recording's header, and the checksum of its final state.

use engine::{Engine, log};
use serde_json::{Value, json};

use super::{commands::state_checksum, config_patches, query::json_eq, require_map};
use crate::session::GameSession;

// Header key for the game's details: version, autosave and configs.
const GAME_INFO_KEY: &str = "game";

// Called first thing in GameLoop::start: a replay runs with the configs it was recorded with,
// which may differ from configs.json, e.g. after the settings dialog saved them. Applied like
// config.set, so configs.json keeps its own values if the settings dialog saves.
pub fn use_recorded_configs(engine: &Engine) {
    let Some(game_info) = engine.replay_info(GAME_INFO_KEY) else {
        return;
    };

    if let Some(version) = game_info.get("version").and_then(Value::as_str)
        && version != crate::version()
    {
        log::warning!(
            log::channel!("game"),
            "The recording comes from version {version} of the game, and this is {}.",
            crate::version()
        );
    }

    let Some(configs) = game_info.get("configs") else {
        return;
    };

    if config_patches::get("").is_some_and(|current| json_eq(&current, configs)) {
        return;
    }

    match config_patches::set("", configs.clone()) {
        Ok(_) => log::info!(log::channel!("game"), "Using the recording's game configs, which differ from configs.json."),
        Err(err) => {
            log::warning!(log::channel!("game"), "Can't use the recording's game configs, so the replay may differ: {err}")
        }
    }
}

// Whether a replay autosaves: when the recorded run did. Saving resolves pending spawns early,
// which changes the state. None when not replaying.
pub fn recorded_autosave(engine: &Engine) -> Option<bool> {
    engine.replay_info(GAME_INFO_KEY)?.get("autosave")?.as_bool()
}

// Called from GameLoop::start once the run's settings are known.
pub fn record_game_info(engine: &mut Engine, autosave: bool) {
    if !engine.is_recording() {
        return;
    }

    let configs = config_patches::get("").unwrap_or(Value::Null);
    engine
        .set_recording_info(GAME_INFO_KEY, json!({ "version": crate::version(), "autosave": autosave, "configs": configs }));
}

// Called from GameLoop::finish_run. None at the home menu, where there's no map.
pub fn final_state_checksum(session: &mut GameSession) -> Option<String> {
    require_map(session).ok()?;

    match state_checksum(session) {
        Ok((checksum, _)) => Some(checksum),
        Err(err) => {
            log::error!(log::channel!("game"), "Can't hash the final state: {err}");
            None
        }
    }
}
