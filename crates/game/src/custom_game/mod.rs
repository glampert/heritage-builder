// Custom Game (docs/plans/realm-ui.md, Phase 1, Custom Game): a game outside the campaigns, on a
// map from assets/configs/game/maps.json (config.rs), in a realm the player's profile has unlocked,
// with the options the Custom Game dialog sets (menu/dialog/custom_game.rs).
//
// - CustomGameSettings: what the dialog chose. start() queues the session command that replaces the
//   session with the game (GameSession::start_custom_game sets its date, treasury and risks).
// - The active custom game (ActiveCustomGame): its realm, which the in-game skin takes
//   (realm::ui_realm), its map and the options the session doesn't hold itself (difficulty, fast
//   construction, the switches of systems that don't exist yet). Runtime state, not part of
//   GameSession: a save writes it in its header (save_header: `realm` and `custom_game`), and a load
//   reads it back from there (from_header). So it needs no save-version bump.
// - Any other session (the home menu, a campaign mission, a preset or a new map from automation or
//   the developer editor) ends it.

pub mod config;
pub mod preview;

use serde::{Deserialize, Serialize};
use strum::{Display, EnumCount, EnumIter, IntoEnumIterator};

use engine::ui::text::{self, UiTextCategory};

use crate::{
    GameLoop,
    campaign::config::CampaignConfigs,
    config::{GameConfigs, LoadMapSetting},
    realm::Realm,
    save_header::SaveHeader,
    sim::calendar::{CalendarSettings, EraDef},
    tile::sets::{PresetTiles, TERRAIN_LAND_CATEGORY, TERRAIN_WATER_CATEGORY},
};

pub use config::{MapConfigs, MapDef, MapSource};

// ----------------------------------------------
// Difficulty
// ----------------------------------------------

// Five steps, as the dialog's step selector shows them.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Display, EnumCount, EnumIter, Serialize, Deserialize)]
pub enum Difficulty {
    VeryEasy,
    Easy,
    #[default]
    Normal,
    Hard,
    VeryHard,
}

impl Difficulty {
    pub fn label(self) -> &'static str {
        match self {
            Self::VeryEasy => text::get(UiTextCategory::Menu, "custom_game.difficulty.very_easy"),
            Self::Easy => text::get(UiTextCategory::Menu, "custom_game.difficulty.easy"),
            Self::Normal => text::get(UiTextCategory::Menu, "custom_game.difficulty.normal"),
            Self::Hard => text::get(UiTextCategory::Menu, "custom_game.difficulty.hard"),
            Self::VeryHard => text::get(UiTextCategory::Menu, "custom_game.difficulty.very_hard"),
        }
    }

    pub fn from_index(index: usize) -> Self {
        Self::iter().nth(index).unwrap_or_default()
    }
}

// ----------------------------------------------
// The map a game starts on
// ----------------------------------------------

// Blank maps' sides, in cells, and the terrain they're filled with.
pub const BLANK_MAP_SIZES: [i32; 7] = [64, 96, 128, 160, 192, 224, 256];
pub const BLANK_MAP_TERRAINS: [PresetTiles; 3] = [PresetTiles::Grass, PresetTiles::Dirt, PresetTiles::Water];

// The terrain a blank map is filled with, by its tile name ("grass", "dirt", "water").
pub fn blank_terrain(name: &str) -> PresetTiles {
    BLANK_MAP_TERRAINS.into_iter().find(|terrain| terrain.to_string().eq_ignore_ascii_case(name)).unwrap_or_default()
}

// What a game's map is, as the dialog chose it: a blank map's size and terrain are the player's.
#[derive(Clone, PartialEq, Debug)]
pub enum ChosenMap {
    Blank { width: i32, height: i32, terrain: PresetTiles },
    Preset { preset_number: usize },
    SaveGame { save_file: String },
}

impl ChosenMap {
    // How the session loads it.
    pub fn load_map_setting(&self) -> LoadMapSetting {
        match self {
            Self::Blank { width, height, terrain } => LoadMapSetting::EmptyMap {
                size_in_cells: common::Size::new(*width, *height),
                terrain_tile_category: if *terrain == PresetTiles::Water { TERRAIN_WATER_CATEGORY } else { TERRAIN_LAND_CATEGORY }
                    .string
                    .to_string(),
                terrain_tile_name: terrain.to_string().to_lowercase(),
            },
            Self::Preset { preset_number } => LoadMapSetting::Preset { preset_number: *preset_number },
            Self::SaveGame { save_file } => LoadMapSetting::SaveGame { save_file: save_file.into() },
        }
    }
}

// ----------------------------------------------
// CustomGameSettings
// ----------------------------------------------

// Everything the Custom Game dialog chose.
#[derive(Clone, PartialEq, Debug)]
pub struct CustomGameSettings {
    pub realm: Realm,
    pub map_id: String, // In maps.json.
    pub map: ChosenMap,
    pub calendar: CalendarSettings, // The starting date, in the realm's eras.
    pub starting_gold: u32,
    pub difficulty: Difficulty,
    pub fires_and_collapses: bool,
    pub fast_construction: bool,
    // Stored for the systems that will read them: seasons and floods (M7), raids (game Phase 2).
    pub seasons_and_floods: bool,
    pub raids: bool,
}

// Starts the game: the session is replaced with it at the start of the next frame
// (GameSession::start_custom_game).
pub fn start(settings: CustomGameSettings) {
    GameLoop::get_mut().start_custom_game(settings);
}

// ----------------------------------------------
// Starting dates
// ----------------------------------------------

// The eras of `realm`: those of its first campaign in campaigns.json that has any.
pub fn realm_eras(realm: Realm) -> Vec<EraDef> {
    if !CampaignConfigs::is_initialized() {
        return Vec::new();
    }
    CampaignConfigs::get()
        .visible()
        .map(|(_, campaign)| campaign)
        .filter(|campaign| campaign.realm == realm)
        .find(|campaign| !campaign.eras.is_empty())
        .map(|campaign| campaign.eras.clone())
        .unwrap_or_default()
}

// The starting dates a custom game in `realm` can take: the first year of each of its eras, or the
// first World Year of the calendar's defaults for a realm without eras.
pub fn starting_dates(realm: Realm) -> Vec<CalendarSettings> {
    let defaults = &GameConfigs::get().sim.calendar;
    let eras = realm_eras(realm);
    if eras.is_empty() {
        return vec![CalendarSettings { start_month: 1, ..defaults.clone() }];
    }
    eras.iter()
        .map(|era| CalendarSettings { start_year: era.start_year.max(1), start_month: 1, eras: eras.clone(), ..defaults.clone() })
        .filter(|settings| settings.validate().is_ok())
        .collect()
}

// ----------------------------------------------
// The active custom game
// ----------------------------------------------

// What a save's header keeps of a custom game, besides its realm (the header's own `realm`). Every
// field has a default, so a header that leaves one out still reads.
#[derive(Clone, Default, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomGameRecord {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<String>, // The map's id in maps.json.
    // TODO(realm-ui): applied by M4 (the economy's balance) and M9 (ratings); stored until then.
    pub difficulty: Difficulty,
    pub fast_construction: bool,
    // TODO(realm-ui): read by M7 (seasons and floods) and game Phase 2 (raids); stored until then.
    pub seasons_and_floods: bool,
    pub raids: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub struct ActiveCustomGame {
    pub realm: Realm,
    pub record: CustomGameRecord,
}

impl ActiveCustomGame {
    pub fn from_settings(settings: &CustomGameSettings) -> Self {
        Self {
            realm: settings.realm,
            record: CustomGameRecord {
                map: Some(settings.map_id.clone()),
                difficulty: settings.difficulty,
                fast_construction: settings.fast_construction,
                seasons_and_floods: settings.seasons_and_floods,
                raids: settings.raids,
            },
        }
    }

    // The custom game a save's header names: a game outside a campaign with a custom game record.
    // None for a campaign mission, a save without a header, and a game outside both (a preset, a
    // new map or the developer editor's), whose header has its realm but no record.
    pub fn from_header(header: &SaveHeader) -> Option<Self> {
        if header.campaign_id.is_some() {
            return None;
        }
        let record = header.custom_game.clone()?;
        Some(Self { realm: header.realm.unwrap_or_default(), record })
    }
}

struct CustomGameState {
    active: Option<ActiveCustomGame>,
}

impl CustomGameState {
    const fn new() -> Self {
        Self { active: None }
    }
}

common::singleton! { CUSTOM_GAME_STATE, CustomGameState }

// The custom game being played, if any.
pub fn active() -> Option<&'static ActiveCustomGame> {
    CustomGameState::get().active.as_ref()
}

pub fn set_active(game: Option<ActiveCustomGame>) {
    CustomGameState::get_mut().active = game;
}

// Ends the custom game: another session replaces it.
pub fn end() {
    set_active(None);
}

// The realm of the custom game being played, if any.
pub fn active_realm() -> Option<Realm> {
    active().map(|game| game.realm)
}

// What every construction time is multiplied by on top of SimConfigs::construction.time_scale: the
// fast construction option's scale in a custom game that has it on, else 1.
pub fn construction_time_scale() -> f32 {
    match active() {
        Some(game) if game.record.fast_construction => GameConfigs::get().sim.construction.fast_construction_scale.max(0.0),
        _ => 1.0,
    }
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_games_from_headers() {
        // A campaign mission is none; nor is a save without a header.
        let campaign = SaveHeader { campaign_id: Some("dragon_legacy".into()), realm: Some(Realm::DaiLong), ..SaveHeader::default() };
        assert_eq!(ActiveCustomGame::from_header(&campaign), None);
        assert_eq!(ActiveCustomGame::from_header(&SaveHeader::default()), None);

        // Nor is a game outside a campaign without a custom game record (a preset, a new map).
        let plain = SaveHeader { realm: Some(Realm::DaiLong), ..SaveHeader::default() };
        assert_eq!(ActiveCustomGame::from_header(&plain), None);

        let record = CustomGameRecord { map: Some("blank".into()), difficulty: Difficulty::Hard, fast_construction: true, ..CustomGameRecord::default() };
        let custom = SaveHeader { realm: Some(Realm::Kambara), custom_game: Some(record.clone()), ..SaveHeader::default() };
        assert_eq!(ActiveCustomGame::from_header(&custom), Some(ActiveCustomGame { realm: Realm::Kambara, record }));
    }

    #[test]
    fn difficulties_and_terrains() {
        assert_eq!(Difficulty::COUNT, 5);
        assert_eq!(Difficulty::from_index(2), Difficulty::Normal);
        assert_eq!(Difficulty::from_index(99), Difficulty::Normal);
        assert_eq!(blank_terrain("water"), PresetTiles::Water);
        assert_eq!(blank_terrain("Dirt"), PresetTiles::Dirt);
        assert_eq!(blank_terrain("lava"), PresetTiles::Grass);
    }

    #[test]
    fn blank_maps_load_as_empty_maps() {
        let map = ChosenMap::Blank { width: 64, height: 96, terrain: PresetTiles::Water };
        let LoadMapSetting::EmptyMap { size_in_cells, terrain_tile_category, terrain_tile_name } = map.load_map_setting() else {
            panic!("a blank map is an empty map");
        };
        assert_eq!(size_in_cells, common::Size::new(64, 96));
        assert_eq!(terrain_tile_category, "water");
        assert_eq!(terrain_tile_name, "water");
        let map = ChosenMap::Blank { width: 64, height: 64, terrain: PresetTiles::Dirt };
        assert!(matches!(map.load_map_setting(), LoadMapSetting::EmptyMap { terrain_tile_category, .. } if terrain_tile_category == "land"));
    }
}
