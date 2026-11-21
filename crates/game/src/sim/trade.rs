// The city's trade (docs/plans/trade.md, §2). Saved in `Simulation` and read through
// `SimContext::trade()`.
//
// A game trades only when its mission's `trade` block (or its custom map's) lists partners: cities of
// its campaign's world map with trade terms (world_map::config::CityTradeDef). The state keeps, per
// partner, the mission's entry for it (its overrides of the terms, where it comes in), whether its
// route is open, the player's limits per good, what it sold the city and bought from it this year,
// its prices' offsets and the month its next caravan or ship sets out; then the court's requests
// under way and their log. The terms themselves are read from the world map file each time
// (crate::trade::terms), so a change to them reaches older saves.

use serde::{Deserialize, Serialize};

use crate::{
    campaign::config::{MissionPartnerDef, MissionTradeDef},
    config::TradeRules,
    sim::resources::{RESOURCE_KIND_COUNT, ResourceKind, per_kind_map},
};

// ----------------------------------------------
// TradeLimit
// ----------------------------------------------

// What the player lets a partner trade in a good (docs/plans/trade.md, §3): nothing, imports until
// the city holds N, or exports of what it holds above N. The city's holdings are counted across its
// storage yards and granaries.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TradeLimit {
    #[default]
    Off,
    Import(u32),
    Export(u32),
}

impl TradeLimit {
    #[inline]
    pub fn is_off(self) -> bool {
        self == Self::Off
    }
}

// ----------------------------------------------
// TradePartner
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct TradePartner {
    // The mission's entry for it: its city, open at the start, where it comes in, its overrides.
    pub def: MissionPartnerDef,
    pub open: bool,
    #[serde(with = "per_kind_map")]
    pub limits: [TradeLimit; RESOURCE_KIND_COUNT],
    // What it sold the city and bought from it this calendar year.
    #[serde(with = "per_kind_map")]
    pub sold: [u32; RESOURCE_KIND_COUNT],
    #[serde(with = "per_kind_map")]
    pub bought: [u32; RESOURCE_KIND_COUNT],
    // Each good's price now is its base price × (1 + offset) (docs/plans/trade.md, §7).
    #[serde(with = "per_kind_map")]
    pub price_offsets: [f32; RESOURCE_KIND_COUNT],
    // The ordinal month (year × 12 + month - 1) its next caravan or ship sets out in. None while its
    // route is closed.
    pub next_arrival: Option<u32>,
}

impl TradePartner {
    fn new(def: &MissionPartnerDef) -> Self {
        Self {
            def: def.clone(),
            open: false,
            limits: [TradeLimit::Off; RESOURCE_KIND_COUNT],
            sold: [0; RESOURCE_KIND_COUNT],
            bought: [0; RESOURCE_KIND_COUNT],
            price_offsets: [0.0; RESOURCE_KIND_COUNT],
            next_arrival: None,
        }
    }

    // Its city's id on the world map.
    #[inline]
    pub fn city(&self) -> &str {
        &self.def.city
    }

    #[inline]
    pub fn limit(&self, good: ResourceKind) -> TradeLimit {
        self.limits[good.index()]
    }

    #[inline]
    pub fn price_offset(&self, good: ResourceKind) -> f32 {
        self.price_offsets[good.index()]
    }
}

// ----------------------------------------------
// Court requests
// ----------------------------------------------

// A request under way (docs/plans/trade.md, §6): a good and an amount, from the court or a partner,
// due by an ordinal month.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct TradeRequest {
    pub id: String,   // In the mission's schedule.
    pub from: String, // A city id: the capital for the court, or a partner.
    pub good: ResourceKind,
    pub amount: u32,
    pub opened: u32, // The ordinal month it opened in.
    pub due: u32,    // The ordinal month it fails at, unmet.
}

// A request met or failed, for the goals and M9's Legitimacy.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct RequestRecord {
    pub request: TradeRequest,
    pub closed: u32, // The ordinal month it was met or failed in.
    pub met: bool,
}

// ----------------------------------------------
// CityTrade
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CityTrade {
    enabled: bool,
    // The world map file whose cities the partners are (CampaignDef::world_map_name).
    world_map: String,
    partners: Vec<TradePartner>,
    requests: Vec<TradeRequest>,
    request_log: Vec<RequestRecord>,
}

impl CityTrade {
    // No trade: games outside a mission or a custom game, and missions without a trade block.
    pub fn new() -> Self {
        Self::default()
    }

    // A mission's (or a custom game's) trade: the partners `trade` lists that `has_terms` says the
    // world map `world_map` has terms for (the others are left out, with their city ids returned),
    // those it opens at the start sending their first caravan or ship `first_arrival_months` after
    // `now` (an ordinal month). The limits start off.
    pub fn start(
        &mut self,
        world_map: &str,
        trade: &MissionTradeDef,
        has_terms: impl Fn(&MissionPartnerDef) -> bool,
        now: u32,
        rules: &TradeRules,
    ) -> Vec<String> {
        let mut left_out = Vec::new();
        *self = Self::new();
        self.world_map = world_map.to_string();
        for def in &trade.partners {
            if !has_terms(def) || self.partner(&def.city).is_some() {
                left_out.push(def.city.clone());
                continue;
            }
            let mut partner = TradePartner::new(def);
            if def.open {
                partner.open = true;
                partner.next_arrival = Some(now + rules.first_arrival_months);
            }
            self.partners.push(partner);
        }
        self.enabled = trade.is_enabled() && !self.partners.is_empty();
        left_out
    }

    // Whether the game trades: its block turned trade on and named a partner with terms.
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    #[inline]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled && !self.partners.is_empty();
    }

    #[inline]
    pub fn world_map(&self) -> &str {
        &self.world_map
    }

    #[inline]
    pub fn partners(&self) -> &[TradePartner] {
        &self.partners
    }

    pub fn partner(&self, city: &str) -> Option<&TradePartner> {
        self.partners.iter().find(|partner| partner.city() == city)
    }

    pub fn partner_mut(&mut self, city: &str) -> Option<&mut TradePartner> {
        self.partners.iter_mut().find(|partner| partner.city() == city)
    }

    // Opens a partner's route (the gold is the caller's to take): its first caravan or ship sets out
    // `first_arrival_months` after `now`. False for no such partner or one already open.
    pub fn open_route(&mut self, city: &str, now: u32, rules: &TradeRules) -> bool {
        match self.partner_mut(city) {
            Some(partner) if !partner.open => {
                partner.open = true;
                partner.next_arrival = Some(now + rules.first_arrival_months);
                true
            }
            _ => false,
        }
    }

    // Closes a partner's route: nothing more sets out (automation, the debug window).
    pub fn close_route(&mut self, city: &str) -> bool {
        match self.partner_mut(city) {
            Some(partner) if partner.open => {
                partner.open = false;
                partner.next_arrival = None;
                true
            }
            _ => false,
        }
    }

    // Sets a partner's limit for a good. False for no such partner.
    pub fn set_limit(&mut self, city: &str, good: ResourceKind, limit: TradeLimit) -> bool {
        debug_assert!(good.is_single_resource());
        match self.partner_mut(city) {
            Some(partner) => {
                partner.limits[good.index()] = limit;
                true
            }
            None => false,
        }
    }

    // A new calendar year: every partner's yearly amounts start again.
    pub fn new_year(&mut self) {
        for partner in &mut self.partners {
            partner.sold = [0; RESOURCE_KIND_COUNT];
            partner.bought = [0; RESOURCE_KIND_COUNT];
        }
    }

    #[inline]
    pub fn requests(&self) -> &[TradeRequest] {
        &self.requests
    }

    #[inline]
    pub fn request_log(&self) -> &[RequestRecord] {
        &self.request_log
    }

    // How many requests were met.
    pub fn requests_met(&self) -> u32 {
        self.request_log.iter().filter(|record| record.met).count() as u32
    }

    // After a load: drops the partners the world map no longer has terms for (`has_terms`),
    // returning their city ids. The game's trade turns off if none is left.
    pub fn sync_partners(&mut self, has_terms: impl Fn(&MissionPartnerDef) -> bool) -> Vec<String> {
        let mut dropped = Vec::new();
        self.partners.retain(|partner| {
            let keep = has_terms(&partner.def);
            if !keep {
                dropped.push(partner.city().to_string());
            }
            keep
        });
        if self.partners.is_empty() {
            self.enabled = false;
        }
        dropped
    }
}
