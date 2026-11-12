// The maps a custom game can start on, and the starting treasuries it offers, from
// assets/configs/game/maps.json:
//
// - `maps`: each with an `id` (saves name the map by it), a `name` (the Menu string `map.<id>`, if
//   en.json has one, wins: MapDef::display_name) and a `source`: a Blank map with its default
//   width, height and terrain (the player picks them), a preset map (debug/preset_maps.rs) or a
//   save's map (by its name in saves/; listed only while that save is there).
//   TODO(realm-ui): real custom maps, made for it, in place of the sample saves' test maps and the
//   debug presets (which turn on their cheats, as session.load_preset does).
// - `starting_treasury`: the gold choices, and the one picked at first.

use serde::{Deserialize, Serialize};

use common::hash;
use engine::{
    log,
    save::storage,
    ui::text::{self, UiTextCategory},
};

use super::{BLANK_MAP_SIZES, blank_terrain};

#[derive(Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MapConfigs {
    pub maps: Vec<MapDef>,
    pub starting_treasury: TreasuryDef,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapDef {
    pub id: String,
    pub name: String,
    pub source: MapSource,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum MapSource {
    // An empty map of one terrain: its default size and terrain ("grass", "dirt" or "water").
    Blank { width: i32, height: i32, terrain: String },
    Preset { preset_number: usize },
    SaveGame { save_file: String },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TreasuryDef {
    pub choices: Vec<u32>,
    pub default: u32,
}

impl Default for TreasuryDef {
    fn default() -> Self {
        Self { choices: vec![1000, 2500, 5000, 10000, 25000], default: 5000 }
    }
}

impl MapDef {
    // Its name as the player reads it: the Menu string `map.<id>` (en.json), else its `name`.
    pub fn display_name(&self) -> &str {
        let key = format!("map.{}", self.id);
        text::find_str(UiTextCategory::Menu, hash::fnv1a_from_str(&key)).unwrap_or(&self.name)
    }

    pub fn is_blank(&self) -> bool {
        matches!(self.source, MapSource::Blank { .. })
    }

    // Whether it can be played now: a save's map only while the save is in saves/ (`saves`, its
    // list).
    fn is_available(&self, saves: &[String]) -> bool {
        match &self.source {
            MapSource::SaveGame { save_file } => saves.iter().any(|save| save == save_file),
            _ => true,
        }
    }
}

impl MapConfigs {
    // Leaves out maps with an id another has and preset maps with no such preset (the presets
    // would start the last one in its place), and keeps blank maps' defaults within the sizes the
    // dialog offers. Called once the file has loaded.
    pub fn post_load(&mut self) {
        let mut seen: Vec<String> = Vec::new();
        self.maps.retain(|map| {
            if map.id.is_empty() || seen.contains(&map.id) {
                log::error!(log::channel!("game"), "maps.json: a map with an empty or repeated id ('{}') is left out.", map.id);
                return false;
            }
            if let MapSource::Preset { preset_number } = &map.source
                && crate::debug::preset_maps::preset_size(*preset_number).is_none()
            {
                log::error!(log::channel!("game"), "maps.json: the map '{}' names no preset {preset_number}; it's left out.", map.id);
                return false;
            }
            seen.push(map.id.clone());
            true
        });
        for map in &mut self.maps {
            if let MapSource::Blank { width, height, terrain } = &mut map.source {
                *width = nearest_blank_size(*width);
                *height = nearest_blank_size(*height);
                *terrain = blank_terrain(terrain).to_string().to_lowercase();
            }
        }
        if self.starting_treasury.choices.is_empty() {
            self.starting_treasury = TreasuryDef::default();
        }
    }

    // The maps a custom game can start on now, in the file's order.
    pub fn available_maps(&self) -> Vec<MapDef> {
        let saves: Vec<String> = storage::list_save_files().iter().map(|file| file.to_string_lossy().into_owned()).collect();
        self.maps.iter().filter(|map| map.is_available(&saves)).cloned().collect()
    }

    // The index in the treasury choices of the default one.
    pub fn default_treasury_index(&self) -> usize {
        let treasury = &self.starting_treasury;
        treasury.choices.iter().position(|gold| *gold == treasury.default).unwrap_or(0)
    }
}

// The size among BLANK_MAP_SIZES nearest `size`.
pub fn nearest_blank_size(size: i32) -> i32 {
    BLANK_MAP_SIZES.into_iter().min_by_key(|choice| (choice - size).abs()).unwrap_or(BLANK_MAP_SIZES[0])
}

engine::configurations! { MAP_CONFIGS_SINGLETON, MapConfigs, "game/maps" }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_sizes_snap() {
        assert_eq!(nearest_blank_size(64), 64);
        assert_eq!(nearest_blank_size(100), 96);
        assert_eq!(nearest_blank_size(10), 64);
        assert_eq!(nearest_blank_size(1000), 256);
    }

    #[test]
    fn saves_maps_need_their_save() {
        let map = MapDef { id: "isle".into(), name: "Isle".into(), source: MapSource::SaveGame { save_file: "tiny_island".into() } };
        assert!(map.is_available(&["tiny_island".to_string()]));
        assert!(!map.is_available(&["other".to_string()]));
        let blank = MapDef { id: "blank".into(), name: "Blank".into(), source: MapSource::Blank { width: 64, height: 64, terrain: "grass".into() } };
        assert!(blank.is_available(&[]));
    }

    #[test]
    fn preset_maps_need_their_preset() {
        let preset = |id: &str, preset_number| MapDef { id: id.into(), name: id.into(), source: MapSource::Preset { preset_number } };
        let mut configs = MapConfigs { maps: vec![preset("farm", 2), preset("nowhere", 9999)], ..Default::default() };
        configs.post_load();
        let ids: Vec<&str> = configs.maps.iter().map(|map| map.id.as_str()).collect();
        assert_eq!(ids, ["farm"]);
    }
}
