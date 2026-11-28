use std::path::PathBuf;

use common::{Size, time::Seconds};
use engine::{config::EngineConfigs, runner::RunLoopConfigs};
use proc_macros::DrawDebugUi;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumCount};

use crate::{
    camera::*,
    sim::{calendar::CalendarSettings, finance::TaxLevel},
};

// ----------------------------------------------
// GameConfigs
// ----------------------------------------------

#[derive(Default, DrawDebugUi, Serialize, Deserialize)]
#[serde(default)] // Missing fields in the config file get defaults from GameConfigs::default().
pub struct GameConfigs {
    // Low-level Engine:
    #[debug_ui(nested)]
    pub engine: EngineConfigs,

    // Save Games:
    #[debug_ui(nested)]
    pub save: SaveGameConfigs,

    // Camera:
    #[debug_ui(nested)]
    pub camera: CameraConfigs,

    // Simulation/World:
    #[debug_ui(nested)]
    pub sim: SimConfigs,

    // The player's display choices (the Settings dialog):
    #[debug_ui(nested)]
    pub display: DisplayConfigs,

    // Debug:
    #[debug_ui(nested)]
    pub debug: DebugConfigs,
}

impl GameConfigs {
    // The configs as assets/configs has them, without the user configs: what the Settings dialog's
    // Restore defaults goes back to. GameConfigs::default() if they weren't loaded.
    pub fn defaults() -> GameConfigs {
        engine::config::user_configs_defaults(GAME_CONFIGS_USER_FILE)
            .and_then(|json| serde_json::from_value(json).ok())
            .unwrap_or_default()
    }
}

// ----------------------------------------------
// Sub Config Categories
// ----------------------------------------------

#[derive(Default, Display, Serialize, Deserialize)]
pub enum LoadMapSetting {
    #[default]
    None,
    EmptyMap {
        size_in_cells: Size,
        terrain_tile_category: String,
        terrain_tile_name: String,
    },
    Preset {
        preset_number: usize,
    },
    SaveGame {
        save_file: PathBuf,
    },
}

#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveGameConfigs {
    pub load_map_setting: LoadMapSetting,
    pub enable_autosave: bool,
    pub autosave_frequency_secs: Seconds,

    // The size in pixels of the thumbnail written beside each save (save_header), the city cut
    // from the middle of the screen. The Load / Save preview shows it larger, fitted to its frame,
    // so raise it if it looks blurry there.
    pub thumbnail_size: Size,
}

impl Default for SaveGameConfigs {
    fn default() -> Self {
        Self {
            load_map_setting: LoadMapSetting::default(),
            enable_autosave: true,
            autosave_frequency_secs: 60.0,
            thumbnail_size: Size::new(320, 180),
        }
    }
}

#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraConfigs {
    pub zoom: f32,
    pub offset: CameraOffset,

    // For fixed step zoom with CTRL +/= key shortcuts.
    pub fixed_step_zoom_amount: f32,

    // Use fixed step zoom with mouse scroll zoom instead of smooth interpolation.
    pub disable_smooth_mouse_scroll_zoom: bool,

    // Disables mouse scroll zoom altogether.
    pub disable_mouse_scroll_zoom: bool,

    // Disables zooming with keyboard shortcuts.
    pub disable_key_shortcut_zoom: bool,

    // Constrain camera movement to inner map diamond playable area? (debug option).
    pub constrain_to_playable_map_area: bool,

    // Constrain camera movement to map AABB? This is a superset of the playable area. (debug option).
    pub clamp_to_map_bounds: bool,

    // Display map debug bounds and camera debug overlays.
    pub enable_debug_draw: bool,

    // Camera scroll/movement speed in pixels per second.
    pub scroll_speed: f32,

    // In pixels from screen edge.
    pub scroll_margin: f32,

    // Scroll the map when the cursor is at the window's edge (scroll_margin).
    pub edge_scrolling: bool,
}

impl Default for CameraConfigs {
    fn default() -> Self {
        Self {
            zoom: CameraZoom::DEFAULT,
            offset: CameraOffset::Center,
            fixed_step_zoom_amount: 0.5,
            disable_smooth_mouse_scroll_zoom: false,
            disable_mouse_scroll_zoom: false,
            disable_key_shortcut_zoom: false,
            constrain_to_playable_map_area: true,
            clamp_to_map_bounds: true,
            enable_debug_draw: false,
            scroll_speed: 500.0,
            scroll_margin: 20.0,
            edge_scrolling: true,
        }
    }
}

#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct SimConfigs {
    // Simulation:
    pub random_seed: u64,
    pub update_frequency_secs: Seconds,
    pub paused_update_frequency_secs: Seconds,
    pub starting_gold_units: u32,
    pub start_paused: bool,

    // The speed a new game starts at (1 to 10 times; the Settings dialog's default game speed).
    pub default_speed: f32,

    // Length of one in-game day, in simulation seconds. Lets rates (e.g. house
    // resource consumption) be expressed in intuitive units-per-day.
    pub seconds_per_day: Seconds,

    // Calendar: simulated seconds per month, days per month, and the calendar of games outside
    // a campaign. A campaign mission's start applies its own (see campaign::config).
    pub seconds_per_month: Seconds,
    pub days_per_month: u32,
    #[debug_ui(skip)]
    pub calendar: CalendarSettings,

    // How long construction sites take (docs/plans/construction.md).
    #[debug_ui(nested)]
    pub construction: ConstructionConfigs,

    // Wages, tax levels and debt (docs/plans/finance.md).
    #[debug_ui(nested)]
    pub finance: FinanceConfigs,

    // Fire and collapse risk (docs/plans/city-risks.md).
    #[debug_ui(nested)]
    pub risks: RiskConfigs,

    // Patrons' favour, blessings and misfortunes (docs/plans/faith.md).
    #[debug_ui(nested)]
    pub faith: FaithRules,

    // The river's floods and the land's fertility (docs/plans/seasons-floods.md).
    #[debug_ui(nested)]
    pub floods: FloodRules,

    // Caravans, ships, docks and prices (docs/plans/trade.md).
    #[debug_ui(nested)]
    pub trade: TradeRules,

    // The four ratings, the failure clocks and the court's requests (docs/plans/ratings.md).
    #[debug_ui(nested)]
    pub ratings: RatingRules,

    // Labour priorities and the allocation (docs/plans/messages.md, M11 Phase 2).
    #[debug_ui(nested)]
    pub labour: LabourRules,

    // Raids, walls and the city's defenders (docs/plans/defence.md).
    #[debug_ui(nested)]
    pub defence: DefenceRules,

    // Workers/Population: how far along the roads an employer's workers may live.
    pub workers_search_radius: i32,

    // A store's runner (Get and Empty): simulated seconds between its looks for an errand, and how
    // far along the roads the other store may be.
    pub storage_errand_secs: Seconds,
    pub storage_reach: i32,

    // Game Systems:
    pub birds_spawn_frequency: Seconds,
    pub settlers_spawn_frequency_secs: Seconds,
    pub population_per_settler_unit: u32,
}

impl Default for SimConfigs {
    fn default() -> Self {
        Self {
            // Simulation:
            random_seed: 0xCAFE1CAFE2CAFE3A,
            update_frequency_secs: 0.5,
            paused_update_frequency_secs: 0.5,
            starting_gold_units: 0,
            start_paused: false,
            default_speed: 1.0,
            seconds_per_day: 600.0,
            // Calendar:
            seconds_per_month: 60.0,
            days_per_month: 30,
            calendar: CalendarSettings::default(),
            construction: ConstructionConfigs::default(),
            finance: FinanceConfigs::default(),
            risks: RiskConfigs::default(),
            faith: FaithRules::default(),
            floods: FloodRules::default(),
            trade: TradeRules::default(),
            ratings: RatingRules::default(),
            labour: LabourRules::default(),
            defence: DefenceRules::default(),
            // Workers/Population:
            workers_search_radius: 20,
            storage_errand_secs: 10.0,
            storage_reach: 40,
            // Game Systems:
            birds_spawn_frequency: 20.0,
            settlers_spawn_frequency_secs: 20.0,
            population_per_settler_unit: 1,
        }
    }
}

// Simulated seconds a construction site takes, by construction tier, before `time_scale`.
#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct ConstructionConfigs {
    pub decorative_secs: Seconds,
    pub basic_secs: Seconds,
    pub civic_secs: Seconds,
    pub grand_secs: Seconds,

    // The house a settler builds on a vacant lot.
    pub house_build_secs: Seconds,

    // Multiplies every duration: 0 builds everything at once, 2 takes twice as long.
    pub time_scale: f32,

    // Multiplies them again in a custom game with fast construction on (custom_game).
    pub fast_construction_scale: f32,
}

impl Default for ConstructionConfigs {
    fn default() -> Self {
        Self {
            decorative_secs: 8.0,
            basic_secs: 15.0,
            civic_secs: 40.0,
            grand_secs: 75.0,
            house_build_secs: 4.0,
            time_scale: 1.0,
            fast_construction_scale: 0.25,
        }
    }
}

// The defaults of a new game's finances, and the tax levels' rates (sim::finance).
#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct FinanceConfigs {
    // Gold per employed worker per year, paid monthly.
    pub default_wage: u32,

    #[debug_ui(skip)]
    pub default_tax_level: TaxLevel,

    // Percentage of the houses' base tax each level pays, in TaxLevel order: None, Low, Normal,
    // High, Harsh.
    #[debug_ui(skip)]
    pub tax_level_percent: [u32; TaxLevel::COUNT],

    // Months in debt before the debt is overdue (a mission failure from M9).
    pub debt_limit_months: u32,
}

impl Default for FinanceConfigs {
    fn default() -> Self {
        Self {
            default_wage: 6,
            default_tax_level: TaxLevel::Normal,
            tax_level_percent: [0, 50, 100, 150, 200],
            debt_limit_months: 12,
        }
    }
}

// Fire, collapse and disease risk (building::risks). Each building's rates come from its building config.
#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct RiskConfigs {
    // Whether a new game outside a campaign has risks. A mission's `risks` block decides for it.
    pub enabled: bool,

    // A building that a small or large well reaches builds up fire risk at this fraction of its rate.
    pub near_water_fire_scale: f32,

    // Each building's rates vary by up to this fraction either way, so neighbours built together
    // don't all burn at once.
    pub rate_jitter: f32,

    // Simulated seconds between a building's checks for a well within reach.
    pub water_check_secs: Seconds,

    // How long a building burns before it burns down, in simulated seconds, unless a fire
    // watcher puts the fire out first.
    pub burn_secs: Seconds,

    // Fire risk a burning building adds to each building next to it, per simulated second.
    pub spread_per_sec: f32,

    // Disease (houses only). A house's disease risk builds up at its level's rate times how full
    // it is, faster without water or food, slower with an apothecary in reach; a medic's visit
    // resets it. Every `disease_check_secs` a house with people rolls for an outbreak, at
    // `outbreak_chance_at_max` a month times (risk / 100) squared.
    pub disease_check_secs: Seconds,
    pub outbreak_chance_at_max: f32,

    // Added to the disease rate's needs factor (1) without a well in reach, and without food.
    pub missing_water_scale: f32,
    pub missing_food_scale: f32,

    // An apothecary in reach scales the disease rate and an outbreak's deaths by this.
    pub apothecary_scale: f32,

    // Disease risk a sick house adds to every house within `disease_spread_cells` of it, per
    // simulated second.
    pub disease_spread_per_sec: f32,
    pub disease_spread_cells: i32,

    // Deaths in a sick house: none for `death_grace_secs`, then every `death_interval_secs`
    // round(population x fraction) residents die, at least one. The fraction starts at
    // `death_fraction` and rises by `death_fraction_step` each time, up to `death_fraction_max`.
    pub death_grace_secs: Seconds,
    pub death_interval_secs: Seconds,
    pub death_fraction: f32,
    pub death_fraction_step: f32,
    pub death_fraction_max: f32,
}

impl Default for RiskConfigs {
    fn default() -> Self {
        Self {
            enabled: true,
            near_water_fire_scale: 0.5,
            rate_jitter: 0.25,
            water_check_secs: 10.0,
            burn_secs: 30.0,
            spread_per_sec: 5.0,
            disease_check_secs: 10.0,
            outbreak_chance_at_max: 0.5,
            missing_water_scale: 1.0,
            missing_food_scale: 1.0,
            apothecary_scale: 0.5,
            disease_spread_per_sec: 1.0,
            disease_spread_cells: 2,
            death_grace_secs: 30.0,
            death_interval_secs: 20.0,
            death_fraction: 0.1,
            death_fraction_step: 0.05,
            death_fraction_max: 0.3,
        }
    }
}

// How patrons' favour moves and what it brings (docs/plans/faith.md, §2 and §3). The patrons, events
// and festivals themselves are data (faith::config). To be balanced in M13.
#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct FaithRules {
    // Every patron's favour in a new game or mission, 0 to 100.
    pub starting_favour: f32,

    // Each month a patron's favour moves this fraction of the way to its target, 100 x coverage.
    pub favour_drift: f32,

    // A patron's coverage is the residents its shrines and temples reach, weighted, over the city's
    // population times this, capped at 1: tended in half the city is fully covered.
    pub coverage_needed: f32,

    // A staffed temple counts this many times a shrine.
    pub temple_weight: f32,

    // A patron at or above `blessing_favour` has `blessing_chance` a month of a blessing; one at or
    // below `misfortune_favour` `misfortune_chance` of a misfortune; at most one event every
    // `event_cooldown_months` each.
    pub blessing_favour: f32,
    pub blessing_chance: f32,
    pub misfortune_favour: f32,
    pub misfortune_chance: f32,
    pub event_cooldown_months: u32,
}

impl Default for FaithRules {
    fn default() -> Self {
        Self {
            starting_favour: 50.0,
            favour_drift: 0.25,
            coverage_needed: 0.5,
            temple_weight: 3.0,
            blessing_favour: 70.0,
            blessing_chance: 0.15,
            misfortune_favour: 30.0,
            misfortune_chance: 0.15,
            event_cooldown_months: 6,
        }
    }
}

// How the river floods and how the land's fertility moves (docs/plans/seasons-floods.md). A mission's
// `floods` block sets its own flood (MissionFloodsDef); these are the defaults, a custom game's flood
// and the knobs. To be balanced in M13.
#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct FloodRules {
    // The flood's month (a wet month), the least and most cells it reaches from its water, drawn
    // each year, and how many months the water stays.
    pub month: u32,
    pub reach_min: u32,
    pub reach_max: u32,
    pub months: u32,

    // A water body floods only if it has at least this many cells (ponds don't), unless a mission
    // names its sources.
    pub min_source_cells: u32,

    // The highest ground the water rises over, in levels (docs/plans/terrain-height.md, §9, Q11):
    // higher ground stays dry. A mission's `flood_level` overrides it.
    pub flood_level: u8,

    // The land's fertility, 0 to 100, at the start: within the flood's usual reach and beyond it.
    // Where it isn't kept (floods off), every cell reads `neutral_fertility`.
    pub fertility_in_reach: u32,
    pub fertility_beyond: u32,
    pub neutral_fertility: u32,

    // Collapse risk a flood adds to each building it reaches, and disease risk to each house, at
    // severity 1; and the silt it leaves, fertility added to each cell.
    pub collapse_damage: f32,
    pub disease_damage: f32,
    pub silt: u32,

    // Fertility a harvest takes from its field.
    pub depletion_per_harvest: u32,

    // A dyke cell's wear a month, 0 to 100, scaled per cell by a factor within `wear_jitter` of
    // 1; and the wear from which a dyke counts as worn (the warnings, the Flood lens). A dyke the
    // flood reaches breaches with chance (wear / 100)² × the flood's severity.
    pub wear_per_month: f32,
    pub wear_jitter: f32,
    pub worn_wear: f32,
}

impl Default for FloodRules {
    fn default() -> Self {
        Self {
            month: 7,
            reach_min: 3,
            reach_max: 6,
            months: 2,
            min_source_cells: 30,
            flood_level: 0,
            fertility_in_reach: 70,
            fertility_beyond: 40,
            neutral_fertility: 50,
            collapse_damage: 40.0,
            disease_damage: 30.0,
            silt: 30,
            depletion_per_harvest: 10,
            wear_per_month: 8.0,
            wear_jitter: 0.25,
            worn_wear: 50.0,
        }
    }
}

// How raids come (docs/plans/defence.md). The raids a mission sends are data (its `defence` block in
// campaigns.json). To be balanced in M13.
#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct DefenceRules {
    // The months of warning a raid gives when its mission doesn't say.
    pub warning_months: u32,

    // A custom game with its Raids switch on (decision Q9): a raid every this many years at least
    // and at most (drawn from the simulation's RNG), the first no sooner than the least.
    pub random_raid_min_years: u32,
    pub random_raid_max_years: u32,
    // Its size: one raider for this many residents, within these bounds.
    pub residents_per_raider: u32,
    pub random_raid_min_size: u32,
    pub random_raid_max_size: u32,
}

impl Default for DefenceRules {
    fn default() -> Self {
        Self {
            warning_months: 2,
            random_raid_min_years: 2,
            random_raid_max_years: 4,
            residents_per_raider: 25,
            random_raid_min_size: 4,
            random_raid_max_size: 40,
        }
    }
}

// The labour allocation's knobs (crate::labour).
#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct LabourRules {
    // Simulated seconds between the allocation's passes.
    pub allocate_secs: Seconds,
}

impl Default for LabourRules {
    fn default() -> Self {
        Self { allocate_secs: 10.0 }
    }
}

// How trade goes (docs/plans/trade.md). The partners and their terms are data (the world map files and
// the missions' `trade` blocks). To be balanced in M13.
#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct TradeRules {
    // Months from a route's opening (or a mission's start, for one open then) to its first caravan
    // or ship.
    pub first_arrival_months: u32,

    // How many goods a caravan carries, all told, and how many stores it trades at before it goes.
    pub caravan_capacity: u32,
    pub caravan_stops: u32,

    // How many goods a ship carries, and the longest it stays moored, in months.
    pub ship_capacity: u32,
    pub ship_stay_months: u32,

    // A dock's dockers at a time, and how many goods one carries a trip.
    pub dockers: u32,
    pub docker_load: u32,

    // Prices: a good's price moves by `price_step` (a share of its base price) for every
    // `price_units` traded, at most `price_cap` either way, and drifts back by `price_drift` of the
    // way each month. Relations move prices by `relation_step` a level from the middle (3).
    pub price_step: f32,
    pub price_units: u32,
    pub price_cap: f32,
    pub price_drift: f32,
    pub relation_step: f32,
}

impl Default for TradeRules {
    fn default() -> Self {
        Self {
            first_arrival_months: 1,
            caravan_capacity: 32,
            caravan_stops: 6,
            ship_capacity: 64,
            ship_stay_months: 1,
            dockers: 2,
            docker_load: 8,
            price_step: 0.02,
            price_units: 10,
            price_cap: 0.4,
            price_drift: 0.25,
            relation_step: 0.03,
        }
    }
}

// How the ratings count what they remember, when a city fails, and what a court request is worth
// (docs/plans/ratings.md). Which drivers feed which rating, and their caps, are data
// (ratings::config). To be balanced in M13.
#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct RatingRules {
    // An event (a festival held, a building burnt down, a flood) counts in full in its month,
    // then less each month until it's gone after this many months (decision Q2).
    pub fade_months: u32,

    // A request met or failed is remembered this long instead.
    pub request_fade_months: u32,

    // A court request's Legitimacy when its mission doesn't say (decision Q4).
    pub request_legitimacy: i32,

    // A request failed takes away its Legitimacy times this.
    pub request_failed_scale: f32,

    // A failed court request is asked again this many months later, for this times the amount.
    pub ask_again_months: u32,
    pub ask_again_scale: f32,

    // The court recalls the governor once Legitimacy has been below `recall_below` at this many
    // month-ends in a row (GDD 7).
    pub recall_below: u32,
    pub recall_months: u32,
}

impl Default for RatingRules {
    fn default() -> Self {
        Self {
            fade_months: 12,
            request_fade_months: 24,
            request_legitimacy: 6,
            request_failed_scale: 1.5,
            ask_again_months: 3,
            ask_again_scale: 1.25,
            recall_below: 10,
            recall_months: 12,
        }
    }
}

// The player's display choices, set in the Settings dialog.
#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfigs {
    // The retail UI's size, in percent (75 to 200): its text and its dialogs (menu::skin::set_ui_scale).
    pub ui_scale_percent: u32,

    // Overlays drawn in colours that colour-blind players can tell apart.
    // TODO(realm-ui): stored only: the Risk lens (M6) and the other overlays (M7) should read it
    // and pick their colour-blind-safe palette.
    pub color_blind_overlays: bool,

    // Animate the water tiles.
    // TODO(realm-ui): stored only: the water tiles have one frame each today. The tile renderer
    // should skip their animation when it's off, once water animates.
    pub animated_water: bool,

    // Important events open a popup (docs/plans/messages.md, §2); off, they go to the message log
    // only, where a click on their line opens it.
    pub event_popups: bool,
}

impl DisplayConfigs {
    pub const UI_SCALE_PERCENT_RANGE: std::ops::RangeInclusive<u32> = 75..=200;

    // The UI scale as a factor (1 at 100%), within the range the setting allows.
    pub fn ui_scale(&self) -> f32 {
        let range = Self::UI_SCALE_PERCENT_RANGE;
        self.ui_scale_percent.clamp(*range.start(), *range.end()) as f32 / 100.0
    }
}

impl Default for DisplayConfigs {
    fn default() -> Self {
        Self { ui_scale_percent: 100, color_blind_overlays: false, animated_water: true, event_popups: true }
    }
}

#[derive(DrawDebugUi, Serialize, Deserialize)]
#[serde(default)]
pub struct DebugConfigs {
    pub skip_home_menu: bool,
    pub start_in_dev_editor_mode: bool,
    pub skip_loading_tile_sets: bool,
    pub show_popups: bool,
    pub enable_dev_tile_inspector: bool,
    pub disable_ambient_music: bool,
    pub disable_ambient_sounds: bool,
}

impl Default for DebugConfigs {
    fn default() -> Self {
        Self {
            skip_home_menu: false,
            start_in_dev_editor_mode: false,
            skip_loading_tile_sets: false,
            show_popups: false,
            enable_dev_tile_inspector: true,
            disable_ambient_music: false,
            disable_ambient_sounds: false,
        }
    }
}

impl RunLoopConfigs for GameConfigs {
    fn engine(&self) -> &EngineConfigs {
        &self.engine
    }

    fn load() -> &'static Self {
        GameConfigs::load()
    }

    fn get() -> &'static Self {
        GameConfigs::get()
    }
}

// ----------------------------------------------
// GameConfigs Global Singleton
// ----------------------------------------------

// Defaults in assets/configs/game/configs.json; the settings dialog's changes go to the user
// configs, saves/user/configs.json (see engine::config).
// The user configs file's name: saves/user/configs.json.
const GAME_CONFIGS_USER_FILE: &str = "configs";

engine::configurations! { GAME_CONFIGS_SINGLETON, GameConfigs, "game/configs", user_configs: "configs" }
