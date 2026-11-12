// Realms (docs/plans/realm-ui.md, Realm and progress): Đại Long, Kambara and Tampadesa. Each
// campaign belongs to one, and the UI wears the look of one at a time: ui_realm() decides which.
//
// The choice:
// - an override set by `ui.realm` (automation, the developer editor) wins over everything;
// - in game: the active campaign's realm, else the Custom Game's, else Đại Long;
// - in the menus (the home menu mode): the latest realm the player's profile has reached, else
//   Đại Long.
// choose_ui_realm() is that decision as a pure function of its inputs; ui_realm() gathers them.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

use engine::ui::text::{self, UiTextCategory};

use crate::{
    GameLoop,
    campaign::{self, CampaignManager, config::CampaignConfigs},
    profile,
};

// ----------------------------------------------
// Realm
// ----------------------------------------------

// Saves, configs (campaigns.json), the profile and automation spell a realm in lowercase:
// "dailong", "kambara", "tampadesa". strum's names are for Display and FromStr, serde's for JSON;
// both attributes are needed.
#[derive(
    Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug,
    Display, EnumIter, EnumString, IntoStaticStr, Serialize, Deserialize,
)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Realm {
    #[default]
    DaiLong,
    Kambara,
    Tampadesa,
}

impl Realm {
    // The names automation and the files use, in order.
    pub fn names() -> Vec<String> {
        Self::iter().map(|realm| realm.to_string()).collect()
    }

    // A realm from its name, with an error that lists the names.
    pub fn parse(name: &str) -> Result<Self, String> {
        name.parse::<Self>().map_err(|_| format!("Unknown realm '{name}'. Realms: {}.", Self::names().join(", ")))
    }

    // The name the player reads, in the current language.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::DaiLong => text::get(UiTextCategory::Realm, "realm.dailong"),
            Self::Kambara => text::get(UiTextCategory::Realm, "realm.kambara"),
            Self::Tampadesa => text::get(UiTextCategory::Realm, "realm.tampadesa"),
        }
    }

    // The realm's minimap style, under assets/configs/.
    pub fn minimap_style_file(self) -> String {
        format!("ui/minimap/{self}.json")
    }
}

// The calendar's year as `realm` writes it, in the current language: "Long Thái era, year 3"
// (Đại Long), "Reign of Suryadeva, year 41" (Kambara), "Year 451 of the Pugara Era" (Tampadesa),
// or the World Year, "Year 3", without an era.
pub fn date_text(realm: Realm, calendar: &crate::sim::calendar::GameCalendar) -> String {
    let Some((era, year)) = calendar.era() else {
        let year = calendar.date(1, 1.0).year.to_string();
        return text::format(UiTextCategory::Realm, "date.world_year", &[("year", &year)]);
    };
    let year = year.to_string();
    let args = [("era", era), ("year", year.as_str())];
    match realm {
        Realm::DaiLong => text::format(UiTextCategory::Realm, "date.era.dailong", &args),
        Realm::Kambara => text::format(UiTextCategory::Realm, "date.era.kambara", &args),
        Realm::Tampadesa => text::format(UiTextCategory::Realm, "date.era.tampadesa", &args),
    }
}

// ----------------------------------------------
// UI realm: the decision
// ----------------------------------------------

// Why the UI shows the realm it does.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Display)]
#[strum(serialize_all = "snake_case")]
pub enum UiRealmSource {
    Override,   // ui.realm (automation, the developer editor).
    Campaign,   // In game: the active campaign's realm.
    CustomGame, // In game: the Custom Game's realm.
    Profile,    // In the menus: the latest realm the profile has reached.
    Default,    // Nothing else applies: Đại Long.
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct UiRealm {
    pub realm: Realm,
    pub source: UiRealmSource,
}

// What choose_ui_realm decides from.
#[derive(Copy, Clone, Default, Debug)]
pub struct UiRealmInputs {
    pub override_realm: Option<Realm>,
    pub in_home_menu: bool,
    pub campaign_realm: Option<Realm>,    // The active campaign's realm, if one is being played.
    pub custom_game_realm: Option<Realm>, // The Custom Game's realm, if one is being played.
    pub profile_realm: Option<Realm>,     // The latest realm the profile has reached.
}

// Which realm the UI shows, and why. Pure, for tests.
pub fn choose_ui_realm(inputs: &UiRealmInputs) -> UiRealm {
    let chosen = |realm, source| UiRealm { realm, source };

    if let Some(realm) = inputs.override_realm {
        return chosen(realm, UiRealmSource::Override);
    }

    if inputs.in_home_menu {
        return match inputs.profile_realm {
            Some(realm) => chosen(realm, UiRealmSource::Profile),
            None => chosen(Realm::DaiLong, UiRealmSource::Default),
        };
    }

    match (inputs.campaign_realm, inputs.custom_game_realm) {
        (Some(realm), _) => chosen(realm, UiRealmSource::Campaign),
        (None, Some(realm)) => chosen(realm, UiRealmSource::CustomGame),
        (None, None) => chosen(Realm::DaiLong, UiRealmSource::Default),
    }
}

// ----------------------------------------------
// UI realm: the inputs
// ----------------------------------------------

struct RealmState {
    override_realm: Option<Realm>,
}

impl RealmState {
    const fn new() -> Self {
        Self { override_realm: None }
    }
}

common::singleton! { REALM_STATE, RealmState }

// The realm ui.realm set, which wins over the game's. None: follow the game.
pub fn set_override(realm: Option<Realm>) {
    RealmState::get_mut().override_realm = realm;
}

pub fn override_realm() -> Option<Realm> {
    RealmState::get().override_realm
}

// The realm of the campaign being played, if any. None before the campaign configs load (tests).
pub fn active_campaign_realm() -> Option<Realm> {
    if !CampaignManager::is_initialized() || !CampaignConfigs::is_initialized() {
        return None;
    }
    let active = campaign::active_mission()?;
    CampaignConfigs::get().campaign(active.campaign_id).map(|campaign| campaign.realm)
}

// The latest realm the player's profile has reached, in campaigns.json order.
pub fn profile_realm() -> Option<Realm> {
    if !CampaignConfigs::is_initialized() {
        return None;
    }
    profile::get().latest_reached_realm(CampaignConfigs::get())
}

// The UI realm, given whether the home menu is up. Automation commands call this with their
// session's menus mode.
pub fn ui_realm_for(in_home_menu: bool) -> UiRealm {
    choose_ui_realm(&UiRealmInputs {
        override_realm: override_realm(),
        in_home_menu,
        campaign_realm: active_campaign_realm(),
        // A save keeps it in its header, and a load reads it back (custom_game).
        custom_game_realm: crate::custom_game::active_realm(),
        profile_realm: profile_realm(),
    })
}

// The realm the UI shows now, for code that draws a running session's UI each frame (the
// minimap). It asks GameLoop's current session whether the home menu is up, which is wrong while
// a session's menus are built: during GameLoop::start (GameLoop isn't initialized yet, so the
// in-game rule applies) and during a session switch (GameLoop still holds the old session). Code
// that builds menus must call ui_realm_for(mode == GameMenusMode::Home) with the menus mode it
// builds instead, as the retail menus do for their skin (menu::skin::follow). Outside a running
// game (tests), the in-game rule applies.
pub fn ui_realm() -> Realm {
    ui_realm_choice().realm
}

// ui_realm(), with why. The same limits apply.
pub fn ui_realm_choice() -> UiRealm {
    let in_home_menu = GameLoop::is_initialized() && GameLoop::get().is_in_home_menu();
    ui_realm_for(in_home_menu)
}
