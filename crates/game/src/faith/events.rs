// Blessings and misfortunes (docs/plans/faith.md, §3, decision Q6). Each new month, after the
// favour's drift, each patron of the game's realm rolls the simulation's RNG, in the file's order so
// runs repeat: a patron at or above `blessing_favour` has `blessing_chance` of one of its blessings,
// one at or below `misfortune_favour` `misfortune_chance` of one of its misfortunes, at most one
// event every `event_cooldown_months` each. An event comes only if it can happen now: its system
// exists (`FaithEventDef::is_available`, decision Q7) and it has something to land on (a farm for a
// harvest, a house that can fall sick for a sickness, risks on for a fire). What a lasting effect
// does (a harvest, upkeep) is read from `CityFaith::effects` by the producers and the finance's
// month-end.

use common::{Color, mem::SingleThreadStatic};
use engine::{
    log,
    ui::text::{self, UiTextCategory},
};
use rand::Rng;

use super::config::{self, FaithConfigs, FaithEffectDef, FaithEventDef, PatronDef};
use crate::{
    building::{
        Building,
        BuildingKind,
        BuildingKindAndId,
        destruction::{self, Destruction},
    },
    config::FaithRules,
    sim::{
        SimContext,
        SimContextMut,
        calendar::CalendarDate,
        faith::{FaithNotice, FaithNoticeKind},
        finance::LedgerCategory,
    },
    tile::minimap::{MINIMAP_ICON_DEFAULT_LIFETIME, MinimapIcon},
};

// The minimap's ping on the buildings a harvest misfortune falls on (blight).
pub const FAITH_ALERT_TINT: Color = Color::new(0.62, 0.38, 0.75, 1.0);

// A new month's faith (Simulation::update, after the finance's month-end; `date` is the month's
// first day): the patrons' favour drifts and the effects under way run down
// (CityFaith::on_new_month), the festival planned for the month is held (faith::festivals), then
// each patron rolls for a blessing or a misfortune. Returns the events that came, as (patron,
// event) ids.
pub fn on_new_month(context: &SimContextMut, rules: &FaithRules, date: CalendarDate) -> Vec<(String, String)> {
    context.faith_mut().on_new_month(context.world(), context.tile_map(), rules);
    super::festivals::on_new_month(context, date);
    roll_events(context, rules)
}

// The month's rolls (§3). Nothing comes while favour is off for the mission or no one lives in the
// city, as favour holds then. A patron draws from the RNG only once its cooldown is over and its
// favour is past a threshold, and again to pick among its events when more than one can happen.
pub fn roll_events(context: &SimContextMut, rules: &FaithRules) -> Vec<(String, String)> {
    let mut came = Vec::new();
    let faith = context.faith();
    if !faith.is_favour_enabled() || super::population(context.world()) == 0 {
        return came;
    }
    let Some(def) = FaithConfigs::get_or_load().for_realm(faith.realm()) else {
        return came;
    };

    for patron in &def.patrons {
        let Some(state) = context.faith().patron(&patron.id) else {
            continue;
        };
        if state.months_since_event < rules.event_cooldown_months {
            continue;
        }
        let (events, chance) = if state.favour >= rules.blessing_favour {
            (&patron.blessings, rules.blessing_chance)
        } else if state.favour <= rules.misfortune_favour {
            (&patron.misfortunes, rules.misfortune_chance)
        } else {
            continue;
        };
        if chance <= 0.0 || !context.rng_mut().random_bool(f64::from(chance.min(1.0))) {
            continue;
        }

        let candidates: Vec<&FaithEventDef> =
            events.iter().filter_map(|id| def.event(id)).filter(|event| can_happen(context, event)).collect();
        let event = match candidates.len() {
            0 => continue,
            1 => candidates[0],
            n => candidates[context.rng_mut().random_range(0..n)],
        };
        if set_off(context, patron, event) {
            came.push((patron.id.clone(), event.id.clone()));
        }
    }
    came
}

// Whether the event can happen in this city now: its system exists, and what it acts on is there.
pub fn can_happen(context: &SimContext, event: &FaithEventDef) -> bool {
    if !event.is_available() {
        return false;
    }
    match &event.effect {
        FaithEffectDef::Harvest { buildings, .. } => any_building(context, *buildings, |b| !b.is_under_construction()),
        FaithEffectDef::Fire { .. } => {
            context.risks().is_active() && any_building(context, BuildingKind::all(), Building::can_catch_fire)
        }
        FaithEffectDef::Collapse { .. } => {
            context.risks().is_active() && any_building(context, BuildingKind::all(), Building::can_collapse)
        }
        FaithEffectDef::Sickness { .. } => {
            context.risks().is_disease_active() && any_building(context, BuildingKind::House, Building::can_fall_sick)
        }
        FaithEffectDef::Windfall { .. } | FaithEffectDef::Upkeep { .. } => true,
        // Those wait for their systems (is_available above).
        FaithEffectDef::Flood { .. }
        | FaithEffectDef::Scholars { .. }
        | FaithEffectDef::Envoys
        | FaithEffectDef::Novices
        | FaithEffectDef::TradeWindfall => false,
    }
}

// Sets off one of the patron's events now, whatever its favour, its cooldown and the dice say
// (`faith.event`, the World Debug window). Errors: no such patron or event in the game's realm, an
// event that isn't the patron's, or one that can't happen now.
pub fn set_off_now(context: &SimContextMut, patron_id: &str, event_id: &str) -> Result<(), String> {
    let realm = context.faith().realm();
    let def = FaithConfigs::get_or_load().for_realm(realm).ok_or_else(|| format!("{realm} has no patrons."))?;
    let patron = def.patron(patron_id).ok_or_else(|| {
        let ids: Vec<&str> = def.patrons.iter().map(|patron| patron.id.as_str()).collect();
        format!("No patron '{patron_id}' in {realm}. Patrons: {}.", ids.join(", "))
    })?;
    if !patron.blessings.iter().chain(&patron.misfortunes).any(|id| id == event_id) {
        let ids: Vec<&str> = patron.blessings.iter().chain(&patron.misfortunes).map(String::as_str).collect();
        return Err(format!("'{event_id}' isn't one of {patron_id}'s events: {}.", ids.join(", ")));
    }
    let event = def.event(event_id).ok_or_else(|| format!("No event '{event_id}' in {realm}."))?;
    if !event.is_available() {
        let needs = event.requires.or(event.effect.needs()).map_or_else(String::new, |needs| needs.to_string());
        return Err(format!("'{event_id}' waits for {needs}, which the game doesn't have yet."));
    }
    if !can_happen(context, event) {
        return Err(format!("'{event_id}' has nothing to act on now (or risks are off)."));
    }
    if !set_off(context, patron, event) {
        return Err(format!("'{event_id}' didn't take."));
    }
    Ok(())
}

// What the event does, then the patron's cooldown starts and the notice is queued. False if it
// found nothing to act on after all.
fn set_off(context: &SimContextMut, patron: &PatronDef, event: &FaithEventDef) -> bool {
    let blessing = patron.blessings.contains(&event.id);
    let landed = match &event.effect {
        FaithEffectDef::Harvest { buildings, months, .. } => {
            context.faith_mut().add_effect(&patron.id, &event.id, *months);
            if !blessing {
                ping_buildings(context, *buildings);
            }
            true
        }
        FaithEffectDef::Upkeep { months, .. } => {
            context.faith_mut().add_effect(&patron.id, &event.id, *months);
            true
        }
        FaithEffectDef::Windfall { gold } => {
            context.finance_mut().earn(LedgerCategory::Offerings, *gold);
            true
        }
        FaithEffectDef::Fire { count } => {
            let targets = pick_buildings(context, BuildingKind::all(), *count, Building::can_catch_fire);
            targets.iter().filter(|target| find(context, **target).is_some_and(|b| b.ignite_now(context))).count() != 0
        }
        FaithEffectDef::Collapse { count } => {
            let targets = pick_buildings(context, BuildingKind::all(), *count, Building::can_collapse);
            targets.iter().filter(|target| destruction::destroy_building(context, **target, Destruction::Collapsed)).count()
                != 0
        }
        FaithEffectDef::Sickness { count } => {
            let targets = pick_buildings(context, BuildingKind::House, *count, Building::can_fall_sick);
            targets.iter().filter(|target| find(context, **target).is_some_and(|b| b.start_outbreak_now(context))).count()
                != 0
        }
        FaithEffectDef::Flood { .. }
        | FaithEffectDef::Scholars { .. }
        | FaithEffectDef::Envoys
        | FaithEffectDef::Novices
        | FaithEffectDef::TradeWindfall => false,
    };
    if !landed {
        return false;
    }

    let realm = context.faith().realm();
    let patron_name = config::patron_name(realm, &patron.id);
    let message =
        text::format(UiTextCategory::Messages, &config::event_message_key(realm, &event.id), &[("patron", &patron_name)]);
    log::info!(
        log::channel!("faith"),
        "{} from {}: {}",
        if blessing { "Blessing" } else { "Misfortune" },
        patron.id,
        event.id
    );
    context.faith_mut().on_event(FaithNotice {
        kind: if blessing { FaithNoticeKind::Blessing } else { FaithNoticeKind::Misfortune },
        patron: patron.id.clone(),
        id: event.id.clone(),
        text: message,
    });
    true
}

fn any_building(context: &SimContext, kinds: BuildingKind, eligible: impl Fn(&Building) -> bool) -> bool {
    let mut found = false;
    context.world().for_each_building_of_any_kind(|building| {
        found = building.is(kinds) && eligible(building);
        !found
    });
    found
}

// Up to `count` buildings of `kinds` that are `eligible`, picked at random with the simulation's
// RNG from the world's buildings in its fixed order.
fn pick_buildings(
    context: &SimContextMut,
    kinds: BuildingKind,
    count: u32,
    eligible: impl Fn(&Building) -> bool,
) -> Vec<BuildingKindAndId> {
    let mut candidates = Vec::new();
    context.world().for_each_building_of_any_kind(|building| {
        if building.is(kinds) && eligible(building) {
            candidates.push(building.kind_and_id());
        }
        true
    });
    let mut picked = Vec::new();
    while picked.len() < count as usize && !candidates.is_empty() {
        let index = context.rng_mut().random_range(0..candidates.len());
        picked.push(candidates.swap_remove(index));
    }
    picked
}

fn find<'a>(context: &'a SimContextMut, target: BuildingKindAndId) -> Option<&'a mut Building> {
    context.find_building_mut(target.kind, target.id)
}

// A minimap alert on each finished building of `kinds`: where a misfortune lands.
fn ping_buildings(context: &SimContextMut, kinds: BuildingKind) {
    let mut cells = Vec::new();
    context.world().for_each_building_of_any_kind(|building| {
        if building.is(kinds) && !building.is_under_construction() {
            cells.push(building.base_cell());
        }
        true
    });
    for cell in cells {
        context.minimap_mut().push_icon(MinimapIcon::Alert, cell, FAITH_ALERT_TINT, MINIMAP_ICON_DEFAULT_LIFETIME);
    }
}

// ----------------------------------------------
// Requests
// ----------------------------------------------

// Events the World Debug window's buttons asked for while it draws the world: Simulation::update
// sets them off at the start of the next frame, when nothing holds the world.
static REQUESTS: SingleThreadStatic<Vec<(String, String)>> = SingleThreadStatic::new(Vec::new());

pub fn request(patron: &str, event: &str) {
    REQUESTS.as_mut().push((patron.to_string(), event.to_string()));
}

#[inline]
pub fn has_requests() -> bool {
    !REQUESTS.as_ref().is_empty()
}

pub fn run_requests(context: &SimContextMut) {
    for (patron, event) in std::mem::take(REQUESTS.as_mut()) {
        if let Err(error) = set_off_now(context, &patron, &event) {
            log::warning!(log::channel!("faith"), "{error}");
        }
    }
}
