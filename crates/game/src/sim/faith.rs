// The city's faith (docs/plans/faith.md): its realm's patrons' favour, whether favour moves and
// festivals can be planned in this game, the festival planned, the festivals held, and the
// blessings' and misfortunes' effects under way. Saved in `Simulation` and read through
// `SimContext::faith()`. The patrons, events and festivals themselves are data (faith::config).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    campaign::config::MissionFaithDef,
    config::FaithRules,
    faith::{FaithConfigs, Mood},
    realm::Realm,
};

// A patron's state, by its id.
#[derive(Clone, Serialize, Deserialize)]
pub struct PatronState {
    pub favour: f32, // 0 to 100.
    // Months since its last blessing or misfortune, for the cooldown between them (§3).
    pub months_since_event: u32,
}

// The festival the player has planned, held at the start of its month (§4).
#[derive(Clone, Serialize, Deserialize)]
pub struct FestivalPlan {
    pub festival: String, // FestivalDef::id.
    pub scale: String,    // FestivalScaleDef::id.
}

// A festival held, for the Heritage page's "last held" and M9's Culture and Legitimacy.
#[derive(Clone, Serialize, Deserialize)]
pub struct FestivalRecord {
    pub festival: String,
    pub scale: String,
    pub ordinal_month: u32, // GameCalendar's: year x 12 + month - 1.
    pub culture: f32,
    pub legitimacy: f32,
}

// A blessing's or a misfortune's effect that lasts (a harvest, upkeep), with its months left.
#[derive(Clone, Serialize, Deserialize)]
pub struct ActiveFaithEffect {
    pub patron: String,
    pub event: String, // FaithEventDef::id.
    pub months_left: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CityFaith {
    // The game's realm, whose patrons and festivals it has: a campaign's, a custom game's, or Đại
    // Long's for any other game.
    realm: Realm,

    // Whether favour moves and blessings and misfortunes come, and whether festivals can be
    // planned: a mission's `faith` block (decision Q13).
    favour_enabled: bool,
    festivals_enabled: bool,

    patrons: BTreeMap<String, PatronState>,
    plan: Option<FestivalPlan>,
    log: Vec<FestivalRecord>,
    effects: Vec<ActiveFaithEffect>,
}

impl CityFaith {
    // A new game's: Đại Long's patrons at the starting favour, everything on.
    pub fn new(rules: &FaithRules) -> Self {
        let mut faith = Self {
            realm: Realm::DaiLong,
            favour_enabled: true,
            festivals_enabled: true,
            patrons: BTreeMap::new(),
            plan: None,
            log: Vec::new(),
            effects: Vec::new(),
        };
        faith.start(Realm::DaiLong, rules);
        faith
    }

    // A new city in `realm`: its patrons at the starting favour, no plan, nothing held, no
    // effects. The switches are left as they are (start_mission sets them).
    pub fn start(&mut self, realm: Realm, rules: &FaithRules) {
        self.realm = realm;
        self.patrons.clear();
        self.plan = None;
        self.log.clear();
        self.effects.clear();
        self.sync_patrons(rules);
    }

    // A campaign mission's switches, or the defaults (on).
    pub fn start_mission(&mut self, mission: &MissionFaithDef) {
        self.favour_enabled = mission.favour.unwrap_or(true);
        self.festivals_enabled = mission.festivals.unwrap_or(true);
    }

    // After a load, or a change to the realm's file: a patron the file no longer lists is
    // dropped, and a new one starts at the starting favour.
    pub fn sync_patrons(&mut self, rules: &FaithRules) {
        let configs = FaithConfigs::get_or_load();
        let Some(def) = configs.for_realm(self.realm) else {
            self.patrons.clear();
            return;
        };
        self.patrons.retain(|id, _| def.patron(id).is_some());
        for patron in &def.patrons {
            self.patrons
                .entry(patron.id.clone())
                .or_insert_with(|| PatronState { favour: rules.starting_favour.clamp(0.0, 100.0), months_since_event: 0 });
        }
    }

    #[inline]
    pub fn realm(&self) -> Realm {
        self.realm
    }

    // The patrons' states, by id.
    #[inline]
    pub fn patrons(&self) -> &BTreeMap<String, PatronState> {
        &self.patrons
    }

    #[inline]
    pub fn patron(&self, id: &str) -> Option<&PatronState> {
        self.patrons.get(id)
    }

    pub fn favour(&self, id: &str) -> Option<f32> {
        self.patrons.get(id).map(|patron| patron.favour)
    }

    pub fn mood(&self, id: &str) -> Option<Mood> {
        self.favour(id).map(Mood::from_favour)
    }

    // Sets a patron's favour, clamped to 0 to 100. False: the realm has no such patron.
    pub fn set_favour(&mut self, id: &str, favour: f32) -> bool {
        match self.patrons.get_mut(id) {
            Some(patron) => {
                patron.favour = favour.clamp(0.0, 100.0);
                true
            }
            None => false,
        }
    }

    #[inline]
    pub fn is_favour_enabled(&self) -> bool {
        self.favour_enabled
    }

    #[inline]
    pub fn set_favour_enabled(&mut self, enabled: bool) {
        self.favour_enabled = enabled;
    }

    #[inline]
    pub fn is_festivals_enabled(&self) -> bool {
        self.festivals_enabled
    }

    #[inline]
    pub fn set_festivals_enabled(&mut self, enabled: bool) {
        self.festivals_enabled = enabled;
    }

    #[inline]
    pub fn plan(&self) -> Option<&FestivalPlan> {
        self.plan.as_ref()
    }

    #[inline]
    pub fn log(&self) -> &[FestivalRecord] {
        &self.log
    }

    #[inline]
    pub fn effects(&self) -> &[ActiveFaithEffect] {
        &self.effects
    }
}
