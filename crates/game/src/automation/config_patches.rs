// Runtime changes to GameConfigs made by config.set. They live in memory only. The settings
// dialog saves the whole GameConfigs struct to configs.json, so it saves through
// save_game_configs(), which writes the original values in place of patched ones.

use std::sync::Mutex;

use serde_json::Value;

use super::query::json_eq;
use crate::config::GameConfigs;

struct ConfigPatch {
    pointer: String,
    original: Value, // Value before the first patch.
    patched: Value,  // Value set by the latest patch.
}

static PATCHES: Mutex<Vec<ConfigPatch>> = Mutex::new(Vec::new());

// The configs as JSON, or the value at a JSON pointer in them.
pub fn get(pointer: &str) -> Option<Value> {
    let configs = configs_to_json();
    configs.pointer(pointer).cloned()
}

// Sets the config at `pointer` (e.g. "/sim/starting_gold_units") in memory. Returns the previous
// value and the new one, as GameConfigs stores it.
pub fn set(pointer: &str, value: Value) -> Result<(Value, Value), String> {
    let mut configs = configs_to_json();

    let target = configs.pointer_mut(pointer).ok_or_else(|| format!("No config at '{pointer}'. Try config.get."))?;
    let previous = std::mem::replace(target, value);

    let patched_configs: GameConfigs =
        serde_json::from_value(configs).map_err(|err| format!("Can't set '{pointer}': {err}"))?;
    *GameConfigs::get_mut() = patched_configs;

    // Read back, e.g. 1 for a float setting comes back as 1.0.
    let current = get(pointer).unwrap_or(Value::Null);

    let mut patches = PATCHES.lock().unwrap();
    match patches.iter_mut().find(|patch| patch.pointer == pointer) {
        Some(patch) => patch.patched = current.clone(),
        None => patches.push(ConfigPatch { pointer: pointer.to_string(), original: previous.clone(), patched: current.clone() }),
    }

    Ok((previous, current))
}

// GameConfigs::save(), without the changes made by config.set: patched settings keep their
// original values in configs.json. A setting changed again since its patch (in the settings
// dialog, for instance) is saved as it is now, and stops counting as patched.
pub fn save_game_configs() -> bool {
    let mut patches = PATCHES.lock().unwrap();
    if patches.is_empty() {
        return GameConfigs::save();
    }

    let mut file_configs = configs_to_json();

    // Newest first, so overlapping patches unwind in order.
    for index in (0..patches.len()).rev() {
        let patch = &patches[index];
        match file_configs.pointer_mut(&patch.pointer) {
            Some(value) if json_eq(value, &patch.patched) => *value = patch.original.clone(),
            _ => {
                patches.remove(index);
            }
        }
    }

    let file_configs: GameConfigs = match serde_json::from_value(file_configs) {
        Ok(file_configs) => file_configs,
        Err(_) => return GameConfigs::save(), // Can't happen: the values came from GameConfigs.
    };

    // Swap the file version in for the save, then put the patched one back.
    let current_configs = std::mem::replace(GameConfigs::get_mut(), file_configs);
    let saved = GameConfigs::save();
    *GameConfigs::get_mut() = current_configs;

    saved
}

fn configs_to_json() -> Value {
    // GameConfigs is plain data; serializing it can't fail.
    serde_json::to_value(GameConfigs::get()).expect("GameConfigs should serialize to JSON")
}
