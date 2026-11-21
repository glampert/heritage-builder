// The city's faith (docs/plans/faith.md): its realm's patrons' favour, whether favour moves and
// festivals can be planned in this game, the festival planned, the festivals held, and the
// blessings' and misfortunes' effects under way. Saved in `Simulation` and read through
// `SimContext::faith()`. The patrons, events and festivals themselves are data (faith::config).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    building::BuildingKind,
    campaign::config::MissionFaithDef,
    config::FaithRules,
    faith::{self, FaithConfigs, Mood, config::FaithEffectDef},
    realm::Realm,
    sim::calendar::MONTHS_PER_YEAR,
    tile::TileMap,
    world::{World, stats::WorldStats},
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

// What a faith notice tells.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FaithNoticeKind {
    Blessing,
    Misfortune,
    FestivalHeld,
    FestivalNotHeld, // Its venue or its costs were short (faith::festivals).
}

// A blessing or a misfortune that came, or a festival held or not, for the HUD's message log
// (GameLoop::notify_faith).
#[derive(Clone, Debug)]
pub struct FaithNotice {
    pub kind: FaithNoticeKind,
    pub patron: String, // The event's patron, or the festival's (EVERY_PATRON for all of them).
    pub id: String,     // The event's or the festival's id.
    pub text: String,   // The message, with the patron's or the festival's name.
}

impl FaithNotice {
    // A blessing or a festival held: the log's Faith message; else a warning.
    #[inline]
    pub fn is_good_news(&self) -> bool {
        matches!(self.kind, FaithNoticeKind::Blessing | FaithNoticeKind::FestivalHeld)
    }
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

    // The blessings and misfortunes that came since the game loop last took them. Runtime only,
    // as the finance's notices are.
    #[serde(skip)]
    notices: Vec<FaithNotice>,
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
            notices: Vec::new(),
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
        self.notices.clear();
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

    // A new month (§2, decision Q4): the effects under way run down, each patron's months since
    // an event go up, and its favour moves `favour_drift` of the way to 100 x its coverage. While
    // favour is off for the mission, or no one lives in the city (no one to tend a patron or
    // neglect it), favour holds. The month's rolls follow (faith::events::on_new_month).
    pub fn on_new_month(&mut self, world: &World, tile_map: &TileMap, rules: &FaithRules) {
        for effect in &mut self.effects {
            effect.months_left = effect.months_left.saturating_sub(1);
        }
        self.effects.retain(|effect| effect.months_left != 0);

        for patron in self.patrons.values_mut() {
            patron.months_since_event = patron.months_since_event.saturating_add(1);
        }
        if !self.favour_enabled || faith::population(world) == 0 {
            return;
        }
        let coverage = faith::coverage(world, tile_map, self.realm, rules);
        let drift = rules.favour_drift.clamp(0.0, 1.0);
        for (id, patron) in &mut self.patrons {
            let target = 100.0 * coverage.get(id).map_or(0.0, |coverage| coverage.coverage);
            patron.favour = (patron.favour + drift * (target - patron.favour)).clamp(0.0, 100.0);
        }
    }

    // A blessing or a misfortune came (faith::events): the patron's cooldown starts again, and its
    // notice waits for the game loop.
    pub fn on_event(&mut self, notice: FaithNotice) {
        if let Some(patron) = self.patrons.get_mut(&notice.patron) {
            patron.months_since_event = 0;
        }
        self.notices.push(notice);
    }

    // A notice for the game loop that starts no cooldown (a festival's).
    pub fn push_notice(&mut self, notice: FaithNotice) {
        self.notices.push(notice);
    }

    // Raises a patron's favour by `amount`, capped at 100 (a festival's).
    pub fn add_favour(&mut self, id: &str, amount: f32) {
        if let Some(patron) = self.patrons.get_mut(id) {
            patron.favour = (patron.favour + amount).clamp(0.0, 100.0);
        }
    }

    // A lasting effect starts, for `months`; the same patron's same event again runs for whichever
    // is longer, its months left or `months`.
    pub fn add_effect(&mut self, patron: &str, event: &str, months: u32) {
        if months == 0 {
            return;
        }
        match self.effects.iter_mut().find(|effect| effect.patron == patron && effect.event == event) {
            Some(effect) => effect.months_left = effect.months_left.max(months),
            None => self.effects.push(ActiveFaithEffect {
                patron: patron.to_string(),
                event: event.to_string(),
                months_left: months,
            }),
        }
    }

    // The effects under way, each with its event's effect from the realm's file (one the file no
    // longer lists is left out).
    fn active_effects(&self) -> impl Iterator<Item = &'static FaithEffectDef> + '_ {
        let def = if self.effects.is_empty() { None } else { FaithConfigs::get_or_load().for_realm(self.realm) };
        self.effects.iter().filter_map(move |effect| def?.event(&effect.event).map(|event| &event.effect))
    }

    // How fast a building of `kind` produces now: the harvests' and blights' scales under way that
    // name it, multiplied (a rich harvest's 1.5; with a blight as well, 0.75).
    pub fn production_scale(&self, kind: BuildingKind) -> f32 {
        self.active_effects()
            .filter_map(|effect| match effect {
                FaithEffectDef::Harvest { buildings, scale, .. } if buildings.intersects(kind) => Some(*scale),
                _ => None,
            })
            .product()
    }

    // How much more partners pay for exports, and less they ask for imports, now: the easy-trade
    // effects under way, 1 + their scales multiplied (1 with none).
    pub fn trade_scale(&self) -> f32 {
        self.active_effects()
            .filter_map(|effect| match effect {
                FaithEffectDef::TradeWindfall { scale, .. } => Some(1.0 + *scale),
                _ => None,
            })
            .product()
    }

    // What the month's upkeep is multiplied by now: the upkeep effects under way, multiplied.
    pub fn upkeep_scale(&self) -> f32 {
        self.active_effects()
            .filter_map(|effect| match effect {
                FaithEffectDef::Upkeep { scale, .. } => Some(*scale),
                _ => None,
            })
            .product()
    }

    // The blessings and misfortunes that came and the game loop hasn't taken yet.
    #[inline]
    pub fn notices(&self) -> &[FaithNotice] {
        &self.notices
    }

    // The blessings and misfortunes that came since the last call.
    pub fn take_notices(&mut self) -> Vec<FaithNotice> {
        std::mem::take(&mut self.notices)
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

    // Plans a festival (faith::festivals::plan checks it): one at a time, so it replaces any other.
    pub fn set_plan(&mut self, plan: Option<FestivalPlan>) {
        self.plan = plan;
    }

    #[inline]
    pub fn log(&self) -> &[FestivalRecord] {
        &self.log
    }

    // A festival held (faith::festivals::hold).
    pub fn record_festival(&mut self, record: FestivalRecord) {
        self.log.push(record);
    }

    // The latest time `festival` was held, if ever.
    pub fn last_held(&self, festival: &str) -> Option<&FestivalRecord> {
        self.log.iter().rev().find(|record| record.festival == festival)
    }

    // Whether `festival` was held in `year` (a calendar year).
    pub fn was_held_in(&self, festival: &str, year: u32) -> bool {
        self.log.iter().any(|record| record.festival == festival && record.ordinal_month / MONTHS_PER_YEAR == year)
    }

    // The festivals held in this city (the mission's: the log starts with it), for the
    // FestivalsHeld goal.
    #[inline]
    pub fn festivals_held(&self) -> u32 {
        self.log.len() as u32
    }

    // The world stats' faith figures, recounted with them (World::update, refresh_stats).
    #[inline]
    pub fn tally(&self, stats: &mut WorldStats) {
        stats.faith.festivals_held = self.festivals_held();
    }

    #[inline]
    pub fn effects(&self) -> &[ActiveFaithEffect] {
        &self.effects
    }
}
