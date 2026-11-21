// Each realm's patrons, their blessings and misfortunes, and its festival year (docs/plans/faith.md,
// §1): one file per realm, assets/configs/game/faith/<realm>.json, loaded once at boot after
// campaigns.json (FaithConfigs::load) and checked. Names and texts are keys in en.json: a patron's
// `patron.<realm>.<id>` and `.domain` (Realm), an event's message `faith.event.<realm>.<id>`
// (Messages) and short name, the same key in Realm, a scale's `festival_scale.<realm>.<id>` (Realm), a festival's `festival.<realm>.<id>`
// and `.venue` (Realm) and `.text` (Story).

use std::collections::{BTreeMap, BTreeSet};

use common::Color;
use engine::{
    file_sys::paths::PathRef,
    log,
    ui::text::{self, UiTextCategory},
};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use crate::{building::BuildingKind, campaign::config::CampaignConfigs, realm::Realm, sim::resources::ResourceKind};

const FAITH_DIR: &str = "game/faith";

// ----------------------------------------------
// PatronDef
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PatronDef {
    // Unique in its realm, `[a-z0-9_]`: saves keep favour by it, and its shrine's and temple's tile
    // variations are named by it (§2a).
    pub id: String,
    // The HUD line icon its row shows (the Heritage page's Patrons tab).
    pub icon: String,
    // `#RRGGBB`: its shrines and temples on the Faith lens.
    pub colour: String,
    // Event ids (FaithEventDef) a pleased patron sends, and a neglected one.
    pub blessings: Vec<String>,
    pub misfortunes: Vec<String>,
    // The mission it appears from, if not from the start (decision Q14). None: from the start.
    pub appears: Option<PatronAppearsDef>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PatronAppearsDef {
    pub campaign: String, // A campaign id.
    pub mission: usize,   // Its mission index, from 0.
}

impl PatronDef {
    pub fn colour(&self) -> Color {
        parse_colour(&self.colour).unwrap_or_else(Color::white)
    }
}

// ----------------------------------------------
// Events
// ----------------------------------------------

// A blessing or a misfortune: what it does, and the system it needs, if the game doesn't have it
// yet (decision Q7). Its message is `faith.event.<realm>.<id>`, its `{patron}` the patron's name.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaithEventDef {
    pub id: String,
    pub effect: FaithEffectDef,
    #[serde(default)]
    pub requires: Option<FaithRequirement>,
}

// What an event does (§3). Those whose system the game lacks are listed with a `requires`.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FaithEffectDef {
    // Those buildings produce `scale` times as much for `months` (a rich harvest, blight).
    Harvest { buildings: BuildingKind, scale: f32, months: u32 },
    // Gold into the treasury.
    Windfall { gold: u32 },
    // Sets that many random buildings that can burn on fire (city risks).
    Fire { count: u32 },
    // Starts an outbreak in that many random houses with people (city risks).
    Sickness { count: u32 },
    // Collapses that many random buildings that can collapse (city risks).
    Collapse { count: u32 },
    // Building upkeep scaled for `months`.
    Upkeep { scale: f32, months: u32 },
    // The next flood's severity scaled (M7).
    Flood { severity: f32 },
    // Scholars come to the city (education, later).
    Scholars { count: u32 },
    // Envoys bring gifts: a caravan from a partner whose route is open unloads that many of its
    // goods at the stores, free (trade, M10).
    Envoys { goods: u32 },
    // Novices join the monasteries (Tampadesa's monasteries, later).
    Novices,
    // Trade comes easily for `months`: partners pay `1 + scale` times their price for the city's
    // exports and ask that much less for its imports (trade, M10).
    TradeWindfall { scale: f32, months: u32 },
}

// A system an event needs before it can happen.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum FaithRequirement {
    Floods,      // M7: met while the game's river floods.
    Reservoirs,  // Kambara's barays, Campaign II.
    Trade,       // M10.
    Education,   // Schools and scholars, later.
    Monasteries, // Tampadesa's monastic order, Campaign III.
}

// What the game has of the systems an event may need, read from its simulation.
#[derive(Copy, Clone, Default, Debug)]
pub struct FaithSystems {
    pub floods: bool,
    pub trade: bool,
}

impl FaithSystems {
    pub fn of(floods: &crate::sim::CityFloods, trade: &crate::sim::CityTrade) -> Self {
        Self {
            floods: floods.is_enabled(),
            trade: trade.is_enabled() && trade.partners().iter().any(|partner| partner.open),
        }
    }
}

impl FaithRequirement {
    // Whether the game has the system now: floods while they're on, trade while a route is open;
    // the others land with their milestones.
    pub fn is_met(self, systems: FaithSystems) -> bool {
        match self {
            Self::Floods => systems.floods,
            Self::Trade => systems.trade,
            Self::Reservoirs | Self::Education | Self::Monasteries => false,
        }
    }
}

impl FaithEffectDef {
    // The system the effect itself needs, whatever its event says.
    pub fn needs(&self) -> Option<FaithRequirement> {
        match self {
            Self::Flood { .. } => Some(FaithRequirement::Floods),
            Self::Scholars { .. } => Some(FaithRequirement::Education),
            Self::Envoys { .. } | Self::TradeWindfall { .. } => Some(FaithRequirement::Trade),
            Self::Novices => Some(FaithRequirement::Monasteries),
            _ => None,
        }
    }
}

impl FaithEventDef {
    // Whether it can happen in this game: its system, if it needs one, exists.
    pub fn is_available(&self, systems: FaithSystems) -> bool {
        self.requires.or(self.effect.needs()).is_none_or(|needs| needs.is_met(systems))
    }
}

// ----------------------------------------------
// Festivals
// ----------------------------------------------

// A festival scale (Modest, Grand, Imperial or Royal): the festival's costs and effects, which are
// its middle scale's, times `cost` and `effect`.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FestivalScaleDef {
    pub id: String,
    pub cost: f32,
    pub effect: f32,
}

// A festival of the realm's year, held in its month, for its patron or `"all"`. Costs and effects
// are at the middle scale; `culture` and `legitimacy` are recorded for M9 (decision Q12).
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FestivalDef {
    pub id: String,
    pub month: u32, // 1 to 12.
    pub patron: String,
    pub gold: u32,
    pub goods: BTreeMap<String, u32>, // By good name.
    pub favour: f32,
    pub culture: f32,
    pub legitimacy: f32,
}

pub const EVERY_PATRON: &str = "all";

impl FestivalDef {
    // Its goods, by kind; a name that isn't a good is left out (validate reports it).
    pub fn goods(&self) -> Vec<(ResourceKind, u32)> {
        self.goods.iter().filter_map(|(name, count)| ResourceKind::from_name(name).map(|kind| (kind, *count))).collect()
    }

    pub fn is_for_every_patron(&self) -> bool {
        self.patron == EVERY_PATRON
    }
}

// ----------------------------------------------
// RealmFaithDef
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RealmFaithDef {
    pub patrons: Vec<PatronDef>,
    pub events: Vec<FaithEventDef>,
    pub scales: Vec<FestivalScaleDef>,
    pub festivals: Vec<FestivalDef>,
}

impl RealmFaithDef {
    pub fn patron(&self, id: &str) -> Option<&PatronDef> {
        self.patrons.iter().find(|patron| patron.id == id)
    }

    pub fn event(&self, id: &str) -> Option<&FaithEventDef> {
        self.events.iter().find(|event| event.id == id)
    }

    pub fn festival(&self, id: &str) -> Option<&FestivalDef> {
        self.festivals.iter().find(|festival| festival.id == id)
    }

    pub fn scale(&self, id: &str) -> Option<&FestivalScaleDef> {
        self.scales.iter().find(|scale| scale.id == id)
    }

    // The file's errors, without its realm's name.
    pub fn validate(&self, campaigns: Option<&CampaignConfigs>) -> Vec<String> {
        let mut errors = Vec::new();
        let valid_id =
            |id: &str| !id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');

        if self.patrons.is_empty() {
            errors.push("no patrons.".to_string());
        }
        check_unique(&mut errors, "patron", self.patrons.iter().map(|patron| patron.id.as_str()), valid_id);
        check_unique(&mut errors, "event", self.events.iter().map(|event| event.id.as_str()), valid_id);
        check_unique(&mut errors, "scale", self.scales.iter().map(|scale| scale.id.as_str()), valid_id);
        check_unique(&mut errors, "festival", self.festivals.iter().map(|festival| festival.id.as_str()), valid_id);
        if self.patrons.iter().any(|patron| patron.id == EVERY_PATRON) {
            errors.push(format!("a patron can't be called '{EVERY_PATRON}': festivals use it for every patron."));
        }

        for patron in &self.patrons {
            if parse_colour(&patron.colour).is_none() {
                errors.push(format!("patron '{}': its colour '{}' isn't #RRGGBB.", patron.id, patron.colour));
            }
            for id in patron.blessings.iter().chain(&patron.misfortunes) {
                if self.event(id).is_none() {
                    errors.push(format!("patron '{}': no event '{id}'.", patron.id));
                }
            }
            if let (Some(appears), Some(campaigns)) = (&patron.appears, campaigns) {
                match campaigns.find_by_id(&appears.campaign).and_then(|index| campaigns.campaign(index)) {
                    Some(campaign) if appears.mission < campaign.missions.len() || campaign.missions.is_empty() => {}
                    Some(_) => errors.push(format!(
                        "patron '{}': campaign '{}' has no mission {}.",
                        patron.id, appears.campaign, appears.mission
                    )),
                    None => errors.push(format!("patron '{}': no campaign '{}'.", patron.id, appears.campaign)),
                }
            }
        }

        for event in &self.events {
            let numbers_ok = match &event.effect {
                FaithEffectDef::Harvest { scale, months, .. } | FaithEffectDef::Upkeep { scale, months } => {
                    *scale >= 0.0 && *months > 0
                }
                FaithEffectDef::Flood { severity } => *severity >= 0.0,
                _ => true,
            };
            if !numbers_ok {
                errors.push(format!("event '{}': its scale must be 0 or more and its months more than 0.", event.id));
            }
        }

        if self.scales.is_empty() && !self.festivals.is_empty() {
            errors.push("festivals but no scales.".to_string());
        }
        for scale in &self.scales {
            if scale.cost < 0.0 || scale.effect < 0.0 {
                errors.push(format!("scale '{}': its cost and effect must be 0 or more.", scale.id));
            }
        }
        let mut months = BTreeSet::new();
        for festival in &self.festivals {
            if !(1..=12).contains(&festival.month) {
                errors.push(format!("festival '{}': month {} isn't 1 to 12.", festival.id, festival.month));
            } else if !months.insert(festival.month) {
                errors.push(format!("festival '{}': another festival is in month {}.", festival.id, festival.month));
            }
            if !festival.is_for_every_patron() && self.patron(&festival.patron).is_none() {
                errors.push(format!("festival '{}': no patron '{}'.", festival.id, festival.patron));
            }
            for name in festival.goods.keys() {
                if ResourceKind::from_name(name).is_none_or(|kind| !kind.is_single_resource()) {
                    errors.push(format!("festival '{}': '{name}' isn't a good.", festival.id));
                }
            }
        }
        errors
    }

    // Every text key the file names, with its category.
    pub fn text_keys(&self, realm: Realm) -> Vec<(UiTextCategory, String)> {
        let mut keys = Vec::new();
        for patron in &self.patrons {
            keys.push((UiTextCategory::Realm, patron_name_key(realm, &patron.id)));
            keys.push((UiTextCategory::Realm, patron_domain_key(realm, &patron.id)));
        }
        for event in &self.events {
            keys.push((UiTextCategory::Messages, event_message_key(realm, &event.id)));
            keys.push((UiTextCategory::Realm, event_name_key(realm, &event.id)));
        }
        for scale in &self.scales {
            keys.push((UiTextCategory::Realm, scale_name_key(realm, &scale.id)));
        }
        for festival in &self.festivals {
            keys.push((UiTextCategory::Realm, festival_name_key(realm, &festival.id)));
            keys.push((UiTextCategory::Realm, format!("{}.venue", festival_name_key(realm, &festival.id))));
            keys.push((UiTextCategory::Story, format!("{}.text", festival_name_key(realm, &festival.id))));
        }
        keys
    }
}

fn check_unique<'a>(errors: &mut Vec<String>, what: &str, ids: impl Iterator<Item = &'a str>, valid: impl Fn(&str) -> bool) {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !valid(id) {
            errors.push(format!("{what} id '{id}' must be [a-z0-9_] and not empty."));
        }
        if !seen.insert(id) {
            errors.push(format!("{what} '{id}' is given twice."));
        }
    }
}

fn parse_colour(text: &str) -> Option<Color> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    let channel = |shift: u32| ((value >> shift) & 0xff) as f32 / 255.0;
    Some(Color::new(channel(16), channel(8), channel(0), 1.0))
}

// ----------------------------------------------
// Text keys
// ----------------------------------------------

pub fn patron_name_key(realm: Realm, id: &str) -> String {
    format!("patron.{realm}.{id}")
}

pub fn patron_domain_key(realm: Realm, id: &str) -> String {
    format!("patron.{realm}.{id}.domain")
}

pub fn event_message_key(realm: Realm, id: &str) -> String {
    format!("faith.event.{realm}.{id}")
}

// An event's short name, for the Heritage page: "A rich harvest".
pub fn event_name_key(realm: Realm, id: &str) -> String {
    format!("faith.event.{realm}.{id}")
}

pub fn scale_name_key(realm: Realm, id: &str) -> String {
    format!("festival_scale.{realm}.{id}")
}

pub fn festival_name_key(realm: Realm, id: &str) -> String {
    format!("festival.{realm}.{id}")
}

// A patron's name in the current language.
pub fn patron_name(realm: Realm, id: &str) -> String {
    text::get(UiTextCategory::Realm, &patron_name_key(realm, id)).to_string()
}

// A festival's name in the current language: "Floodwater offering".
pub fn festival_name(realm: Realm, id: &str) -> String {
    text::get(UiTextCategory::Realm, &festival_name_key(realm, id)).to_string()
}

// A festival scale's name in the current language: "Grand".
pub fn scale_name(realm: Realm, id: &str) -> String {
    text::get(UiTextCategory::Realm, &scale_name_key(realm, id)).to_string()
}

// ----------------------------------------------
// FaithConfigs
// ----------------------------------------------

// Every realm's faith file. A realm whose file doesn't load has no patrons.
#[derive(Default)]
pub struct FaithConfigs {
    pub realms: BTreeMap<Realm, RealmFaithDef>,
}

impl FaithConfigs {
    // The file `assets/configs/game/faith/<realm>.json` is.
    pub fn file_path(realm: Realm) -> String {
        format!("{FAITH_DIR}/{realm}")
    }

    // Loads every realm's file and checks them; errors are logged on the faith channel. Checks a
    // patron's `appears` against the campaigns when they're loaded.
    pub fn load_files() -> Self {
        let mut realms = BTreeMap::new();
        for realm in Realm::iter() {
            let def: RealmFaithDef = engine::config::load_asset_file(PathRef::from_str(&Self::file_path(realm)));
            realms.insert(realm, def);
        }
        let configs = Self { realms };
        let campaigns = CampaignConfigs::is_initialized().then(CampaignConfigs::get);
        for error in configs.validate(campaigns).into_iter().chain(configs.missing_text_keys()) {
            log::error!(log::channel!("faith"), "Faith {error}");
        }
        configs
    }

    // Each file's errors, prefixed with its realm.
    pub fn validate(&self, campaigns: Option<&CampaignConfigs>) -> Vec<String> {
        self.realms
            .iter()
            .flat_map(|(realm, def)| def.validate(campaigns).into_iter().map(move |error| format!("{realm}: {error}")))
            .collect()
    }

    // The text keys en.json (or the current language) lacks, as errors.
    pub fn missing_text_keys(&self) -> Vec<String> {
        self.realms
            .iter()
            .flat_map(|(realm, def)| def.text_keys(*realm))
            .filter(|(category, key)| text::peek(*category, key, None).1.is_none())
            .map(|(category, key)| format!("text: no {category} '{key}' in the strings."))
            .collect()
    }

    pub fn for_realm(&self, realm: Realm) -> Option<&RealmFaithDef> {
        self.realms.get(&realm)
    }

    pub fn load() -> &'static Self {
        Self::initialize(Self::load_files());
        Self::get()
    }

    // The configs, loaded first if they weren't (tests that don't load them at setup).
    pub fn get_or_load() -> &'static Self {
        if !Self::is_initialized() {
            Self::load();
        }
        Self::get()
    }
}

common::singleton_late_init! { FAITH_CONFIGS_SINGLETON, FaithConfigs }
