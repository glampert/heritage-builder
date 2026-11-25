// Each realm's ratings (docs/plans/ratings.md, §1): one file per realm,
// assets/configs/game/ratings/<realm>.json, loaded once at boot and checked. Each of the four
// ratings lists its drivers: what raises or lowers it, its cap and, for one that can go negative,
// its floor. What a driver reads is code (crate::ratings, DriverKind); which drivers a realm's
// ratings take, their caps and their parameters are data, so Kambara's Divine Favour can lean on
// the patrons' favour and the Mandate on flood control.
//
// A driver's name is `rating.driver.<id>` (Hud), or the realm's own, `rating.driver.<realm>.<id>`
// (Realm), where it has one ("Dance halls").

use std::collections::BTreeMap;

use engine::{
    file_sys::paths::PathRef,
    log,
    ui::text::{self, UiTextCategory},
};
use serde::{Deserialize, Serialize};
use strum::{EnumCount, IntoEnumIterator};

use crate::{
    building::BuildingKind,
    realm::Realm,
    sim::{
        finance::TaxLevel,
        ratings::{MAX_RATING, RatingKind},
    },
};

const RATINGS_DIR: &str = "game/ratings";

// The id the Culture festivals' driver shows in place of its own while no festival was held in the
// last year (the mockups' "No festival this year −2").
pub const NO_FESTIVAL_DRIVER: &str = "no_festival";

// ----------------------------------------------
// DriverKind
// ----------------------------------------------

// What a driver reads (§1). Each is a function of the city at the month's start (crate::ratings);
// its value is scaled to the driver's `max` (and `min`, for one that can go negative).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DriverKind {
    // Prosperity: the residents' mean house level over the top level.
    HousingLevels {},
    // Prosperity: gold per resident against `per_resident`; in debt, down to `min` at a debt of
    // `per_resident` a resident.
    Treasury {
        per_resident: f32,
    },
    // Prosperity: the share of the workforce employed; full once all but `free_share` work.
    Employment {
        free_share: f32,
    },
    // Prosperity: the wage against `standard` (the finance's default wage if left out): half the
    // cap at the standard, the cap at twice it, `min` at nothing.
    Wages {
        #[serde(default)]
        standard: Option<u32>,
    },
    // Prosperity: the exports' gold this year or last, the higher, against `per_resident` a
    // resident. Only in a game that trades.
    TradeIncome {
        per_resident: f32,
    },
    // Culture: the share of residents a working shrine or temple reaches.
    FaithCoverage {},
    // Culture and Security: the share of residents a working `service` reaches (a theater, a
    // university, a police station), along the grid, as faith's coverage counts it.
    ServiceCoverage {
        service: BuildingKind,
    },
    // Culture: the culture of the festivals held, fading; `none_penalty` taken away (shown as
    // NO_FESTIVAL_DRIVER) while festivals are on, people live in the city and none was held in the
    // last year.
    FestivalCulture {
        #[serde(default)]
        none_penalty: i32,
    },
    // Legitimacy: the legitimacy of the festivals held, fading.
    FestivalLegitimacy {},
    // Security: the share of the buildings with risks whose every risk is below Medium (all of them
    // while risks are off).
    RiskSafety {},
    // Security, until game Phase 2's crime and unrest: the cap less the unemployed's and the hungry
    // houses' shares of it, less `high_taxes` at a High tax level and twice that at Harsh.
    CalmStreets {
        #[serde(default)]
        high_taxes: i32,
    },
    // Security: buildings burnt down or collapsed and deaths from sickness, fading; a house lost
    // counts `per_house` in place of `per_building`. Never above 0.
    Disasters {
        per_building: f32,
        per_house: f32,
        per_death: f32,
    },
    // Legitimacy: each request met adds its legitimacy, and each failed takes it away times
    // RatingRules::request_failed_scale, fading over RatingRules::request_fade_months. Only in a
    // game with requests.
    Requests {},
    // Legitimacy: a value per tax level, None to Harsh.
    FairTaxes {
        by_level: [i32; TaxLevel::COUNT],
    },
    // Legitimacy: each flood that breached no dyke adds `held`; each breach and each building it
    // damaged take away `per_breach` and `per_building_damaged`, fading. Only while the river floods.
    Floods {
        held: f32,
        per_breach: f32,
        per_building_damaged: f32,
    },
    // Legitimacy: the mean favour of the realm's patrons over 100.
    Favour {},
}

impl DriverKind {
    // Its name in data: "service_coverage".
    pub fn name(&self) -> &'static str {
        match self {
            Self::HousingLevels {} => "housing_levels",
            Self::Treasury { .. } => "treasury",
            Self::Employment { .. } => "employment",
            Self::Wages { .. } => "wages",
            Self::TradeIncome { .. } => "trade_income",
            Self::FaithCoverage {} => "faith_coverage",
            Self::ServiceCoverage { .. } => "service_coverage",
            Self::FestivalCulture { .. } => "festival_culture",
            Self::FestivalLegitimacy {} => "festival_legitimacy",
            Self::RiskSafety {} => "risk_safety",
            Self::CalmStreets { .. } => "calm_streets",
            Self::Disasters { .. } => "disasters",
            Self::Requests {} => "requests",
            Self::FairTaxes { .. } => "fair_taxes",
            Self::Floods { .. } => "floods",
            Self::Favour {} => "favour",
        }
    }

    // What makes two drivers of a rating the same: their kind, and a coverage driver's service.
    fn identity(&self) -> (&'static str, Option<BuildingKind>) {
        match self {
            Self::ServiceCoverage { service } => (self.name(), Some(*service)),
            _ => (self.name(), None),
        }
    }

    // Whether its value can go below 0.
    pub fn can_be_negative(&self) -> bool {
        matches!(
            self,
            Self::Treasury { .. }
                | Self::Wages { .. }
                | Self::FestivalCulture { .. }
                | Self::CalmStreets { .. }
                | Self::Disasters { .. }
                | Self::Requests {}
                | Self::FairTaxes { .. }
                | Self::Floods { .. }
        )
    }

    // Its parameters' problems.
    fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut positive = |name: &str, value: f32| {
            if !(value.is_finite() && value > 0.0) {
                problems.push(format!("its {name} must be above 0"));
            }
        };
        match self {
            Self::Treasury { per_resident } | Self::TradeIncome { per_resident } => positive("per_resident", *per_resident),
            Self::Employment { free_share } => {
                if !(0.0..1.0).contains(free_share) {
                    problems.push("its free_share must be from 0 up to 1".into());
                }
            }
            Self::Wages { standard: Some(0) } => problems.push("its standard wage must be above 0".into()),
            Self::ServiceCoverage { service } => {
                if !service.is_single_building() || !service.intersects(BuildingKind::services()) {
                    problems.push(format!("its service must be one service building kind, not '{service}'"));
                }
            }
            Self::FestivalCulture { none_penalty } if *none_penalty < 0 => {
                problems.push("its none_penalty must be 0 or more (it's taken away)".into())
            }
            Self::CalmStreets { high_taxes } if *high_taxes < 0 => {
                problems.push("its high_taxes must be 0 or more (it's taken away)".into())
            }
            Self::Disasters { per_building, per_house, per_death } => {
                for (name, value) in [("per_building", per_building), ("per_house", per_house), ("per_death", per_death)] {
                    if !(value.is_finite() && *value >= 0.0) {
                        problems.push(format!("its {name} must be 0 or more (it's taken away)"));
                    }
                }
            }
            Self::Floods { held, per_breach, per_building_damaged } => {
                for (name, value) in
                    [("held", held), ("per_breach", per_breach), ("per_building_damaged", per_building_damaged)]
                {
                    if !(value.is_finite() && *value >= 0.0) {
                        problems.push(format!("its {name} must be 0 or more"));
                    }
                }
            }
            _ => {}
        }
        problems
    }
}

// ----------------------------------------------
// DriverDef
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverDef {
    // Unique in its rating, `[a-z0-9_]`: its name's key, `rating.driver.<id>`.
    pub id: String,
    pub driver: DriverKind,
    // Its cap, 0 or more, and its floor, 0 or less (only a driver that can go negative has one).
    pub max: i32,
    #[serde(default)]
    pub min: i32,
}

// ----------------------------------------------
// RealmRatingsDef
// ----------------------------------------------

// A realm's file: each rating's drivers, in the order its breakdown lists them.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RealmRatingsDef {
    pub prosperity: Vec<DriverDef>,
    pub culture: Vec<DriverDef>,
    pub security: Vec<DriverDef>,
    pub legitimacy: Vec<DriverDef>,
}

impl RealmRatingsDef {
    pub fn drivers(&self, kind: RatingKind) -> &[DriverDef] {
        match kind {
            RatingKind::Prosperity => &self.prosperity,
            RatingKind::Culture => &self.culture,
            RatingKind::Security => &self.security,
            RatingKind::Legitimacy => &self.legitimacy,
        }
    }

    // Its problems, each naming the rating and driver.
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for kind in RatingKind::iter() {
            let drivers = self.drivers(kind);
            if drivers.is_empty() {
                problems.push(format!("{kind} has no drivers."));
                continue;
            }
            let mut ids = Vec::new();
            let mut identities = Vec::new();
            for driver in drivers {
                let name = format!("{kind}'s driver '{}'", driver.id);
                if driver.id.is_empty()
                    || !driver.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                {
                    problems.push(format!("{name}: its id must be [a-z0-9_]."));
                }
                if ids.contains(&driver.id) {
                    problems.push(format!("{name} is listed twice."));
                }
                ids.push(driver.id.clone());
                let identity = driver.driver.identity();
                if identities.contains(&identity) {
                    problems.push(format!("{name}: another driver of {kind} reads the same ({}).", driver.driver.name()));
                }
                identities.push(identity);
                if driver.max < 0 {
                    problems.push(format!("{name}: its max must be 0 or more."));
                }
                if driver.min > 0 {
                    problems.push(format!("{name}: its min must be 0 or less."));
                }
                if driver.min < 0 && !driver.driver.can_be_negative() {
                    problems
                        .push(format!("{name}: a {} driver can't go below 0, so it takes no min.", driver.driver.name()));
                }
                for problem in driver.driver.problems() {
                    problems.push(format!("{name}: {problem}."));
                }
            }
            let caps: i32 = drivers.iter().map(|driver| driver.max).sum();
            if caps > MAX_RATING as i32 {
                problems.push(format!("{kind}'s drivers' caps add up to {caps}, over {MAX_RATING}."));
            }
            let floors: i32 = drivers.iter().map(|driver| driver.min).sum();
            if floors < -(MAX_RATING as i32) {
                problems.push(format!("{kind}'s drivers' floors add up to {floors}, under -{MAX_RATING}."));
            }
        }
        problems
    }

    // The ids its breakdowns can show: each driver's, and NO_FESTIVAL_DRIVER where a festivals
    // driver takes a penalty.
    pub fn shown_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        for kind in RatingKind::iter() {
            for driver in self.drivers(kind) {
                ids.push(driver.id.clone());
                if matches!(driver.driver, DriverKind::FestivalCulture { none_penalty } if none_penalty > 0) {
                    ids.push(NO_FESTIVAL_DRIVER.to_string());
                }
            }
        }
        ids.sort();
        ids.dedup();
        ids
    }
}

// ----------------------------------------------
// Names
// ----------------------------------------------

pub fn driver_key(id: &str) -> String {
    format!("rating.driver.{id}")
}

pub fn realm_driver_key(realm: Realm, id: &str) -> String {
    format!("rating.driver.{realm}.{id}")
}

// A driver's name in the current language: the realm's own where it has one, else the general
// one ("Housing levels").
pub fn driver_name(realm: Realm, id: &str) -> String {
    let (text, found) = text::peek(UiTextCategory::Realm, &realm_driver_key(realm, id), None);
    if found.is_some() {
        return text;
    }
    text::get(UiTextCategory::Hud, &driver_key(id)).to_string()
}

// A rating's name in the realm: "Prosperity", or its fourth's, "The Mandate".
pub fn rating_name(realm: Realm, kind: RatingKind) -> String {
    match kind {
        RatingKind::Prosperity => text::get(UiTextCategory::Hud, "rating.prosperity").to_string(),
        RatingKind::Culture => text::get(UiTextCategory::Hud, "rating.culture").to_string(),
        RatingKind::Security => text::get(UiTextCategory::Hud, "rating.security").to_string(),
        RatingKind::Legitimacy => text::get(UiTextCategory::Realm, &format!("rating.{realm}.fourth")).to_string(),
    }
}

// ----------------------------------------------
// RatingConfigs
// ----------------------------------------------

// Every realm's ratings file. A realm whose file doesn't load has no drivers, so its ratings stay 0.
#[derive(Default)]
pub struct RatingConfigs {
    pub realms: BTreeMap<Realm, RealmRatingsDef>,
}

impl RatingConfigs {
    // The file `assets/configs/game/ratings/<realm>.json` is.
    pub fn file_path(realm: Realm) -> String {
        format!("{RATINGS_DIR}/{realm}")
    }

    // Loads every realm's file and checks them; errors are logged on the ratings channel.
    pub fn load_files() -> Self {
        let mut realms = BTreeMap::new();
        for realm in Realm::iter() {
            let def: RealmRatingsDef = engine::config::load_asset_file(PathRef::from_str(&Self::file_path(realm)));
            realms.insert(realm, def);
        }
        let configs = Self { realms };
        for error in configs.validate().into_iter().chain(configs.missing_text_keys()) {
            log::error!(log::channel!("ratings"), "Ratings {error}");
        }
        configs
    }

    // Each file's problems, prefixed with its realm.
    pub fn validate(&self) -> Vec<String> {
        self.realms
            .iter()
            .flat_map(|(realm, def)| def.validate().into_iter().map(move |error| format!("{realm}: {error}")))
            .collect()
    }

    // The general names (`rating.driver.<id>`, Hud) the strings lack, as errors. A realm's own
    // names are optional.
    pub fn missing_text_keys(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.realms.values().flat_map(RealmRatingsDef::shown_ids).collect();
        ids.sort();
        ids.dedup();
        ids.into_iter()
            .map(|id| driver_key(&id))
            .filter(|key| text::peek(UiTextCategory::Hud, key, None).1.is_none())
            .map(|key| format!("text: no Hud '{key}' in the strings."))
            .collect()
    }

    pub fn for_realm(&self, realm: Realm) -> Option<&RealmRatingsDef> {
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

common::singleton_late_init! { RATING_CONFIGS_SINGLETON, RatingConfigs }
