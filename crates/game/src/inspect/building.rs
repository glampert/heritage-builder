// The inspector's buildings: a workshop, a house, a store, a service (a market among them) and a
// construction site, as inspector-<kind>-<realm>.jpg lays them out.

use common::coords::Cell;
use engine::ui::text::{self as ui_text, UiTextCategory};

use super::{
    Action,
    ActionKind,
    InspectIcon,
    InspectReport,
    InspectTarget,
    PanelKind,
    ProductionRow,
    RiskRow,
    Section,
    Status,
    StockRow,
    StorageOrder,
    StorageRow,
    TickRow,
    Tone,
    ValueRow,
    duration_text,
    t,
    tc,
    tf,
    unit,
};
use crate::{
    building::{
        Building,
        BuildingArchetypeKind,
        BuildingKind,
        config::BuildingConfigs,
        construction::ConstructionSite,
        house::{HouseLevel, HouseLevelRequirements},
        risks::{self, RiskKind},
    },
    city::spaced_name,
    config::GameConfigs,
    realm::Realm,
    sim::{SimContext, resources::ResourceKind},
    tile::{TileKind, sets::TileDef},
    unit::{UnitId, UnitTaskHelper},
};

// More goods than this in a store make its panel wide (inspector-storage-<realm>.jpg).
const WIDE_STORE_ROWS: usize = 5;

pub(super) fn read(context: &SimContext, building: &Building, realm: Realm) -> InspectReport {
    let tile_def = building_tile_def(context, building);
    let name = name_of(building, tile_def, realm);
    let size = tile_def.map(|def| def.size_in_cells());
    let size_text = size.map(|size| format!("{}×{}", size.width, size.height));
    let target =
        InspectTarget::Building { building: building.kind(), handle: building.kind_and_id(), cell: building.base_cell() };
    let icon = match building.archetype_kind() {
        BuildingArchetypeKind::HouseBuilding => InspectIcon::Sector("housing".to_string()),
        _ => InspectIcon::Building(tile_def.map_or_else(String::new, |def| def.name.to_string())),
    };

    let mut report = InspectReport {
        target,
        panel: PanelKind::Workshop,
        icon,
        name,
        subtitle: String::new(),
        status: None,
        sections: Vec::new(),
        actions: Vec::new(),
        wide: false,
    };
    let subtitle = |kind: String| match &size_text {
        Some(size) => format!("{kind} · {size}"),
        None => kind,
    };

    if let Some(site) = building.construction_site() {
        report.panel = PanelKind::Site;
        report.subtitle = subtitle(t("kind.site"));
        read_site(context, building, site, tile_def, &mut report);
        return report;
    }

    match building.archetype_kind() {
        BuildingArchetypeKind::ProducerBuilding => {
            report.panel = PanelKind::Workshop;
            report.subtitle = subtitle(t(producer_kind_key(building.kind())));
            read_producer(context, building, tile_def, realm, &mut report);
        }
        BuildingArchetypeKind::StorageBuilding => {
            report.panel = PanelKind::Storage;
            report.subtitle = subtitle(t("kind.storage"));
            read_storage(context, building, tile_def, &mut report);
        }
        BuildingArchetypeKind::ServiceBuilding => {
            let market = building.as_service().service_stock.is_stock();
            report.panel = if market { PanelKind::Market } else { PanelKind::Service };
            report.subtitle = subtitle(t(if market { "kind.market" } else { "kind.service" }));
            read_service(context, building, tile_def, realm, &mut report);
        }
        BuildingArchetypeKind::HouseBuilding => {
            report.panel = PanelKind::House;
            let house = building.as_house();
            let level = house.level() as usize + 1;
            report.name = house_level_name(realm, level);
            report.subtitle =
                subtitle(tf("kind.house", &[("level", &level.to_string()), ("max", &HouseLevel::count().to_string())]));
            read_house(context, building, realm, &mut report);
        }
    }
    report
}

// ----------------------------------------------
// Names
// ----------------------------------------------

fn building_tile_def<'a>(context: &'a SimContext, building: &Building) -> Option<&'a TileDef> {
    context.tile_map().find_tile(building.base_cell(), TileKind::Building).map(|tile| tile.tile_def())
}

// The name a realm gives a building ("Prasat"), else the tile's in sentence case ("Roof tile
// kiln"; the build palette's tooltip names it in title case).
pub fn building_name(realm: Realm, tile_def: &TileDef) -> String {
    let key = format!("building.{realm}.{}", tile_def.name);
    match ui_text::find_str(UiTextCategory::Realm, common::hash::fnv1a_from_str(&key)) {
        Some(name) => name.to_string(),
        None => sentence_case(&tile_def.name),
    }
}

// "roof_tile_kiln" as "Roof tile kiln", as the mockups name buildings.
pub(super) fn sentence_case(name: &str) -> String {
    let spaced = name.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

// A building's name inside a sentence: a realm's own name as it is ("the Prasat"), a common one
// in lower case ("the storage yard").
pub(super) fn building_name_in_text(realm: Realm, tile_def: &TileDef) -> String {
    let key = format!("building.{realm}.{}", tile_def.name);
    match ui_text::find_str(UiTextCategory::Realm, common::hash::fnv1a_from_str(&key)) {
        Some(name) => name.to_string(),
        None => tile_def.name.replace('_', " "),
    }
}

// The name of the building at `cell` inside a sentence, if one is there.
pub(super) fn building_name_at(context: &SimContext, cell: Cell, realm: Realm) -> Option<String> {
    let building = context.world().find_building_for_cell(cell, context.tile_map())?;
    let tile_def = building_tile_def(context, building);
    Some(match (building.archetype_kind(), tile_def) {
        (BuildingArchetypeKind::HouseBuilding, _) => house_name(building, realm).to_lowercase(),
        (_, Some(def)) => building_name_in_text(realm, def),
        (_, None) => spaced_name(building.name()).to_lowercase(),
    })
}

// A building's name in the panel: a house's by its level in the realm ("Thatched hut"), else the
// realm's name for it.
fn name_of(building: &Building, tile_def: Option<&TileDef>, realm: Realm) -> String {
    if building.archetype_kind() == BuildingArchetypeKind::HouseBuilding {
        return house_name(building, realm);
    }
    tile_def.map_or_else(|| spaced_name(building.name()), |def| building_name(realm, def))
}

fn house_name(building: &Building, realm: Realm) -> String {
    house_level_name(realm, building.as_house().level() as usize + 1)
}

// A house level's name in the realm (`house_level.<realm>.<level from 1>`).
fn house_level_name(realm: Realm, level: usize) -> String {
    ui_text::get(UiTextCategory::Realm, &format!("house_level.{realm}.{level}")).to_string()
}

fn producer_kind_key(kind: BuildingKind) -> &'static str {
    if kind.intersects(BuildingKind::Farm) {
        "kind.farm"
    } else if kind.intersects(BuildingKind::FishingWharf) {
        "kind.fishery"
    } else {
        "kind.workshop"
    }
}

// A good as the rows read it: "Roof Tiles" as "Roof tiles".
pub(super) fn good_name(kind: ResourceKind) -> String {
    spaced_name(kind.name())
}

// ----------------------------------------------
// Shared sections
// ----------------------------------------------

// A burning or sick building's status, which comes before anything else.
fn emergency_status(building: &Building) -> Option<Status> {
    let risks = building.risks();
    if let Some(fire) = risks.burning() {
        return Some(Status {
            word: t("status.on_fire"),
            tone: Tone::Bad,
            summary: tf("summary.burns_down", &[("time", &duration_text(fire.secs_left))]),
        });
    }
    if let Some(outbreak) = risks.outbreak() {
        let summary = match outbreak.deaths {
            0 => t("summary.sick"),
            deaths => tc("summary.sick_deaths", i64::from(deaths), &[]),
        };
        return Some(Status { word: t("status.sick"), tone: Tone::Bad, summary });
    }
    None
}

// Why a workplace isn't working, if it isn't, in order: no road (when it needs one), too few
// workers.
fn workplace_status(building: &Building, needs_road: bool) -> Option<Status> {
    if let Some(status) = emergency_status(building) {
        return Some(status);
    }
    if needs_road && !building.is_linked_to_road() {
        return Some(Status { word: t("status.no_road"), tone: Tone::Bad, summary: t("summary.no_road") });
    }
    if !building.has_min_required_workers() {
        return Some(Status { word: t("status.short_of_workers"), tone: Tone::Bad, summary: t("summary.short_of_workers") });
    }
    None
}

fn workers_section(building: &Building, compact: bool) -> Option<Section> {
    let employer = building.workers()?.as_employer()?;
    let need = employer.max_employees();
    if need == 0 {
        return None;
    }
    let have = employer.employee_count();
    let short = need.saturating_sub(have);
    let note = if have < employer.min_employees() {
        tf("workers.needs_min", &[("min", &employer.min_employees().to_string())])
    } else if short > 0 {
        tc("workers.short", i64::from(short), &[])
    } else {
        t("workers.full")
    };
    Some(Section::Workers { have, need, note, short: short > 0, compact })
}

fn costs_rows(context: &SimContext, building: &Building, tile_def: Option<&TileDef>) -> Vec<ValueRow> {
    let mut rows = Vec::new();
    let upkeep = tile_def.map_or(0, |def| def.upkeep);
    let upkeep = if upkeep == 0 { t("costs.none") } else { tc("costs.gold_a_year", i64::from(upkeep), &[]) };
    rows.push(ValueRow { label: t("costs.upkeep"), value: upkeep, warn: false });
    if let Some(employer) = building.workers().and_then(|workers| workers.as_employer())
        && employer.max_employees() > 0
    {
        let workers = employer.employee_count();
        let gold = workers * context.finance().wage();
        let value = tf("costs.wages_value", &[
            ("workers", &tc("costs.workers", i64::from(workers), &[])),
            ("gold", &tc("costs.gold_a_year", i64::from(gold), &[])),
        ]);
        rows.push(ValueRow { label: t("costs.wages"), value, warn: false });
    }
    rows
}

fn costs_section(context: &SimContext, building: &Building, tile_def: Option<&TileDef>) -> Section {
    Section::Values { title: t("costs.title"), note: None, rows: costs_rows(context, building, tile_def) }
}

// The risks the building builds up (its config's rates), each with its level now.
fn risk_rows(building: &Building) -> Vec<RiskRow> {
    let rates = building.configs().risk_rates();
    let shown = [
        (RiskKind::Fire, "risks.fire", rates.fire),
        (RiskKind::Collapse, "risks.collapse", rates.collapse),
        (RiskKind::Disease, "risks.disease", rates.disease),
    ];
    shown
        .into_iter()
        .filter(|(_, _, rate)| *rate > 0.0)
        .map(|(kind, key, _)| {
            let value = building.risks().get(kind);
            let high = value >= risks::RISK_HIGH;
            RiskRow { label: t(key), value, level: super::risk_level(value), high }
        })
        .collect()
}

fn risks_section(building: &Building) -> Option<Section> {
    let rows = risk_rows(building);
    if rows.is_empty() { None } else { Some(Section::Risks { rows }) }
}

// Pause work (for a building with workers; M11's labour hooks it up) and Demolish.
fn workplace_actions(building: &Building) -> Vec<Action> {
    let mut actions = Vec::new();
    if building.workers().and_then(|workers| workers.as_employer()).is_some_and(|employer| employer.max_employees() > 0) {
        actions.push(Action::new(ActionKind::PauseWork, "action.pause_work").disabled("action.pause_work_soon"));
    }
    actions.push(Action::new(ActionKind::Demolish, "action.demolish"));
    actions
}

fn push_errand(sections: &mut Vec<Section>, context: &SimContext, unit_id: UnitId, realm: Realm) {
    if let Some(unit) = context.find_unit(unit_id)
        && let Some(errand) = unit::errand(context, unit, realm)
    {
        sections.push(errand);
    }
}

// ----------------------------------------------
// Workshop
// ----------------------------------------------

fn read_producer(
    context: &SimContext,
    building: &Building,
    tile_def: Option<&TileDef>,
    realm: Realm,
    report: &mut InspectReport,
) {
    let producer = building.as_producer();
    let config = tile_def.map(|def| BuildingConfigs::get().find_producer_config(building.kind(), def.hash, &def.name));
    let output = producer.production_output_stock.item;
    let harvests = config.is_some_and(|config| !config.harvested_resource.is_empty());
    let summary_key = if harvests { "summary.gathers" } else { "summary.makes" };
    let output_name = good_name(output.kind).to_lowercase();

    report.status = Some(workplace_status(building, true).unwrap_or_else(|| {
        if let Some(missing) = producer.production_input_stock.slots.iter().find(|item| item.count == 0)
            && producer.production_input_stock.requires_any_resource()
            && !producer.production_input_stock.has_required_resources()
        {
            return Status {
                word: tf("status.no_good", &[("good", &good_name(missing.kind).to_lowercase())]),
                tone: Tone::Bad,
                summary: t("summary.waiting_for_goods"),
            };
        }
        if producer.production_output_stock.is_full() {
            return Status { word: t("status.storage_full"), tone: Tone::Pending, summary: t("summary.waiting_for_cart") };
        }
        Status { word: t("status.working"), tone: Tone::Good, summary: tf(summary_key, &[("good", &output_name)]) }
    }));

    report.sections.extend(workers_section(building, false));

    if let Some(config) = config {
        let days = super::secs_to_days(config.production_output_frequency_secs).round().max(1.0) as i64;
        let note = tf("production.batch", &[
            ("amount", &config.production_output_amount.to_string()),
            ("days", &tc("production.days", days, &[])),
        ]);
        let mut rows: Vec<ProductionRow> = producer
            .production_input_stock
            .slots
            .iter()
            .map(|item| ProductionRow {
                label: t("production.needs"),
                kind: item.kind,
                good: good_name(item.kind),
                have: item.count,
                capacity: producer.production_input_stock.capacity(),
            })
            .collect();
        rows.push(ProductionRow {
            label: t("production.makes"),
            kind: output.kind,
            good: good_name(output.kind),
            have: output.count,
            capacity: producer.production_output_stock.capacity(),
        });
        let timer = &producer.production_update_timer;
        let progress = if timer.frequency_secs() > 0.0 {
            (timer.time_since_last_secs() / timer.frequency_secs()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        report.sections.push(Section::Production { note, rows, progress });
    }

    push_errand(&mut report.sections, context, producer.runner.unit_id(), realm);
    push_errand(&mut report.sections, context, producer.harvester.unit_id(), realm);
    report.sections.push(costs_section(context, building, tile_def));
    report.sections.extend(risks_section(building));
    report.actions = workplace_actions(building);
}

// ----------------------------------------------
// Store
// ----------------------------------------------

fn read_storage(context: &SimContext, building: &Building, tile_def: Option<&TileDef>, report: &mut InspectReport) {
    let storage = building.as_storage();
    let slots = &storage.storage_slots;
    let config = BuildingConfigs::get().find_storage_config(building.kind());
    let used = (0..slots.slots.len()).filter(|&i| !slots.is_slot_free(i)).count();
    let total = slots.slots.len();
    let free = total - used;

    report.status = Some(workplace_status(building, true).unwrap_or_else(|| Status {
        word: t("status.working"),
        tone: Tone::Good,
        summary: tf("summary.slots_used", &[("used", &used.to_string()), ("total", &total.to_string())]),
    }));

    let (room, _) = slots.room();
    let rows: Vec<StorageRow> = config
        .resources_accepted
        .iter()
        .flat_map(|group| group.iter())
        .map(|kind| StorageRow {
            kind,
            good: good_name(kind),
            have: slots.available_resources(kind),
            room: room[kind.index()],
            order: storage_order(building, kind),
        })
        .collect();
    let wide = rows.len() > WIDE_STORE_ROWS;
    report.wide = wide;

    report.sections.extend(workers_section(building, true));
    let title = if building.is(BuildingKind::Granary) { t("storage.food") } else { t("storage.goods") };
    let footer = match free {
        0 => t("storage.no_free_slots"),
        free if building.is(BuildingKind::Granary) => tc("storage.free_slots", free as i64, &[]),
        free => tc("storage.free_slots_any", free as i64, &[]),
    };
    report.sections.push(Section::Storage {
        title,
        note: tf("storage.slots_of", &[("count", &slots.slot_capacity().to_string())]),
        rows,
        footer,
    });
    if wide {
        // Half the width: the wages as gold alone.
        let mut costs = costs_rows(context, building, tile_def);
        if let (Some(wages), Some(employer)) =
            (costs.get_mut(1), building.workers().and_then(|workers| workers.as_employer()))
        {
            wages.value = tc("costs.gold_a_year", i64::from(employer.employee_count() * context.finance().wage()), &[]);
        }
        report.sections.push(Section::CostsRisks { costs, risks: risk_rows(building) });
    } else {
        report.sections.push(costs_section(context, building, tile_def));
        report.sections.extend(risks_section(building));
    }
    report.actions = workplace_actions(building);
}

// A store's order for a good.
pub(crate) fn storage_order(building: &Building, kind: ResourceKind) -> StorageOrder {
    building.as_storage().storage_slots.order(kind)
}

// ----------------------------------------------
// Service and market
// ----------------------------------------------

fn read_service(
    context: &SimContext,
    building: &Building,
    tile_def: Option<&TileDef>,
    realm: Realm,
    report: &mut InspectReport,
) {
    let service = building.as_service();
    let config = BuildingConfigs::get().find_service_config(building.kind());
    let market = service.service_stock.is_stock();
    let houses = count_in_reach(context, building, BuildingKind::House, config.effect_radius);

    report.status = Some(workplace_status(building, config.requires_road_access).unwrap_or_else(|| {
        if market && let Some(item) = building.stock().into_iter().find(|item| item.count == 0) {
            return Status {
                word: tf("status.short_of", &[("good", &good_name(item.kind).to_lowercase())]),
                tone: Tone::Bad,
                summary: tc("summary.supplies_houses", i64::from(houses), &[]),
            };
        }
        let summary = if market || config.visits.building_kinds() == BuildingKind::House {
            tc("summary.serves_houses", i64::from(houses), &[])
        } else {
            let buildings = count_in_reach(context, building, config.visits.building_kinds(), config.effect_radius);
            tc("summary.watches_buildings", i64::from(buildings), &[])
        };
        Status { word: t("status.working"), tone: Tone::Good, summary }
    }));

    report.sections.extend(workers_section(building, false));

    if market {
        let capacity = config.stock_capacity;
        let rows = building
            .stock()
            .into_iter()
            .map(|item| StockRow { kind: item.kind, good: good_name(item.kind), have: item.count, capacity })
            .collect();
        report.sections.push(Section::Stock { title: t("market.stalls"), note: None, rows, footer: None, warn_empty: true });
    } else if config.effect_radius > 0 {
        let note = tf("patrol.within", &[("distance", &super::distance_text(config.effect_radius as usize).to_lowercase())]);
        let mut rows = Vec::new();
        let visits = config.visits.building_kinds();
        rows.push(ValueRow {
            label: t("patrol.in_reach"),
            value: count_in_reach(context, building, visits, config.effect_radius).to_string(),
            warn: false,
        });
        if let Some(risk) = watched_risk(building.kind())
            && let Some((name, value)) = riskiest_in_reach(context, building, visits, config.effect_radius, risk, realm)
        {
            let level = super::risk_level(value);
            let key = match risk {
                RiskKind::Fire => "patrol.highest_fire",
                RiskKind::Collapse => "patrol.highest_collapse",
                RiskKind::Disease => "patrol.highest_disease",
            };
            rows.push(ValueRow { label: t(key), value: format!("{name} · {level}"), warn: value >= risks::RISK_HIGH });
        }
        let title = if config.has_patrol_unit { t("patrol.title") } else { t("patrol.reach") };
        report.sections.push(Section::Values { title, note: Some(note), rows });
    }

    push_errand(&mut report.sections, context, service.runner.unit_id(), realm);
    push_errand(&mut report.sections, context, service.patrol.unit_id(), realm);
    report.sections.push(costs_section(context, building, tile_def));
    report.sections.extend(risks_section(building));
    report.actions = workplace_actions(building);
}

// The risk a service's walkers watch: the fire watch's fires, the builders' collapses, the
// hospital's sickness.
fn watched_risk(kind: BuildingKind) -> Option<RiskKind> {
    if kind.intersects(BuildingKind::FireWatch) {
        Some(RiskKind::Fire)
    } else if kind.intersects(BuildingKind::BuildersGuild) {
        Some(RiskKind::Collapse)
    } else if kind.intersects(BuildingKind::Hospital) {
        Some(RiskKind::Disease)
    } else {
        None
    }
}

// Within `radius` cells of the building (its footprint's nearest cell, as the crow flies along
// the grid): roughly the reach its walkers have, without a path search.
fn in_reach(building: &Building, other: &Building, radius: i32) -> bool {
    let a = building.cell_range();
    let b = other.cell_range();
    let dx = (b.start.x - a.end.x).max(a.start.x - b.end.x).max(0);
    let dy = (b.start.y - a.end.y).max(a.start.y - b.end.y).max(0);
    dx.max(dy) <= radius
}

// Every finished building of `kinds` (any archetypes) in reach of `building`, but itself.
fn for_each_in_reach(
    context: &SimContext,
    building: &Building,
    kinds: BuildingKind,
    radius: i32,
    mut visit: impl FnMut(&Building),
) {
    let this = building.kind_and_id();
    context.world().for_each_building_of_any_kind(|other| {
        if other.is(kinds)
            && other.kind_and_id() != this
            && !other.is_under_construction()
            && in_reach(building, other, radius)
        {
            visit(other);
        }
        true
    });
}

fn count_in_reach(context: &SimContext, building: &Building, kinds: BuildingKind, radius: i32) -> u32 {
    let mut count = 0;
    for_each_in_reach(context, building, kinds, radius, |_| count += 1);
    count
}

fn riskiest_in_reach(
    context: &SimContext,
    building: &Building,
    kinds: BuildingKind,
    radius: i32,
    risk: RiskKind,
    realm: Realm,
) -> Option<(String, f32)> {
    let mut best: Option<(Cell, &'static str, f32)> = None;
    for_each_in_reach(context, building, kinds, radius, |other| {
        let value = other.risks().get(risk);
        if value > 0.0 && best.is_none_or(|(_, _, top)| value > top) {
            best = Some((other.base_cell(), other.name(), value));
        }
    });
    let (cell, fallback, value) = best?;
    let name =
        building_name_at(context, cell, realm).map_or_else(|| spaced_name(fallback), |name| super::unit::capitalized(&name));
    Some((name, value))
}

// ----------------------------------------------
// House
// ----------------------------------------------

fn read_house(context: &SimContext, building: &Building, realm: Realm, report: &mut InspectReport) {
    let house = building.as_house();
    let configs = BuildingConfigs::get();
    let house_config = configs.house_config();
    let level = house.level();
    let level_config = configs.find_house_level_config(level);
    let upgrade = house.upgrade_state();
    let building_context = building.new_context(context);

    // Its taxes a year: what one tax cycle brings, as many times as cycles fit in a year.
    let cycles =
        (GameConfigs::get().sim.seconds_per_month * 12.0 / house_config.generate_tax_frequency_secs.max(1.0)).round();
    let taxes = (house.tax_generated(context.finance().tax_percent()) as f32 * cycles) as i64;
    let tax_summary = if taxes > 0 { tc("summary.taxes", taxes, &[]) } else { t("summary.no_taxes") };

    report.status = Some(emergency_status(building).unwrap_or_else(|| {
        if let Some(secs) = upgrade.renovation_secs_left {
            return Status {
                word: t("status.renovating"),
                tone: Tone::Pending,
                summary: tf("summary.upgrades_in", &[("time", &duration_text(secs))]),
            };
        }
        if upgrade.downgrade_pending_secs > 0.0 {
            let secs = (house_config.downgrade_grace_secs - upgrade.downgrade_pending_secs).max(0.0);
            return Status {
                word: t("status.declining"),
                tone: Tone::Bad,
                summary: tf("summary.downgrades_in", &[("time", &duration_text(secs))]),
            };
        }
        if upgrade.upgrade_ready_secs > 0.0 {
            return Status { word: t("status.improving"), tone: Tone::Good, summary: tax_summary.clone() };
        }
        let word = if building.population_is_maxed() { t("status.full") } else { t("status.growing") };
        Status { word, tone: Tone::Good, summary: tax_summary.clone() }
    }));

    if let Some(population) = building.population() {
        let note = match building.workers().and_then(|workers| workers.as_household_worker_pool()) {
            Some(pool) => tf("residents.note", &[
                ("working", &tc("residents.working", i64::from(pool.employed_count()), &[])),
                ("looking", &tc("residents.looking", i64::from(pool.unemployed_count()), &[])),
            ]),
            None => String::new(),
        };
        report.sections.push(Section::Residents { have: population.count(), room: population.max(), note });
    }

    // This level's services, met or not; and what the next level needs on top.
    let requirements = HouseLevelRequirements::new(&building_context, level_config, house.stock());
    let missing = requirements.services_missing();
    let rows: Vec<TickRow> = level_config
        .services_required
        .iter()
        .map(|group| TickRow { name: service_group_name(*group), met: !group.intersects(missing) })
        .collect();
    let next_needs = (!level.is_max() && building.is_linked_to_road() && house.is_upgrade_available(&building_context))
        .then(|| next_level_needs(&house.upgrade_requirements(&building_context), level))
        .flatten()
        .map(|needs| {
            tf("house.next_needs", &[
                ("house", &house_level_name(realm, level as usize + 2).to_lowercase()),
                ("needs", &needs),
            ])
        });
    if !rows.is_empty() {
        let missing_count = rows.iter().filter(|row| !row.met).count();
        let note = if missing_count == 0 {
            tf("house.all_met", &[("level", &(level as usize + 1).to_string())])
        } else {
            tc("house.missing", missing_count as i64, &[])
        };
        report.sections.push(Section::Ticks { title: t("house.services"), note: Some(note), rows, footer: next_needs });
    } else if let Some(needs) = next_needs {
        report.sections.push(Section::Text { text: needs });
    }

    // Its goods: those its level needs, and any other it holds.
    let required = level_config.resources_required.combined();
    let stock = house.stock();
    let mut rows: Vec<StockRow> = Vec::new();
    for item in building.stock() {
        let capacity = stock.capacities[item.kind.index()] as u32;
        if capacity > 0 && (item.count > 0 || item.kind.intersects(required)) {
            rows.push(StockRow { kind: item.kind, good: good_name(item.kind), have: item.count, capacity });
        }
    }
    if !rows.is_empty() {
        report.sections.push(Section::Stock { title: t("house.goods"), note: None, rows, footer: None, warn_empty: true });
    }

    report.sections.extend(risks_section(building));
    report.actions = vec![Action::new(ActionKind::Demolish, "action.demolish")];
}

// "Small well or large well".
fn service_group_name(group: BuildingKind) -> String {
    let names: Vec<String> = group.iter().map(|kind| spaced_name(&kind.to_string())).collect();

    names.join(&format!(" {} ", t("or")))
}

// What the next level still needs on top of this one's, in a line: its missing services, then its
// missing goods (this level's own missing needs are ticked above). None once it has them all.
fn next_level_needs(requirements: &HouseLevelRequirements, level: HouseLevel) -> Option<String> {
    let mut needs: Vec<String> = Vec::new();
    let current = BuildingConfigs::get().find_house_level_config(level);
    for group in requirements.level_config.services_required.iter() {
        if group.intersects(requirements.services_missing()) && !current.services_required.iter().any(|have| have == group) {
            needs.push(service_group_name(*group).to_lowercase());
        }
    }
    for group in requirements.level_config.resources_required.iter() {
        if group.intersects(requirements.resources_missing()) && !current.resources_required.iter().any(|have| have == group)
        {
            let names: Vec<String> = group.iter().map(|kind| good_name(kind).to_lowercase()).collect();
            needs.push(names.join(&format!(" {} ", t("or"))));
        }
    }
    if needs.is_empty() { None } else { Some(needs.join(", ")) }
}

// ----------------------------------------------
// Construction site
// ----------------------------------------------

fn read_site(
    _context: &SimContext,
    building: &Building,
    site: &ConstructionSite,
    tile_def: Option<&TileDef>,
    report: &mut InspectReport,
) {
    report.status = Some(Status {
        word: t("status.being_built"),
        tone: Tone::Pending,
        summary: tf("summary.ready_in", &[("time", &duration_text(site.remaining_secs()))]),
    });
    report.sections.push(Section::Progress {
        title: t("site.construction"),
        note: tf("site.stage", &[("stage", &(site.stage() + 1).to_string()), ("stages", "4")]),
        value: site.progress(),
    });
    let takes =
        if building.archetype_kind() == BuildingArchetypeKind::HouseBuilding { "site.takes_house" } else { "site.takes" };
    report.sections.push(Section::Text { text: tf(takes, &[("time", &duration_text(site.total_secs()))]) });

    let mut paid = vec![ValueRow { label: t("site.gold"), value: site.paid_gold().to_string(), warn: false }];
    let mut kinds = ResourceKind::empty();
    for item in site.withdrawal().taken.iter() {
        kinds.insert(item.kind);
    }
    for kind in kinds.iter() {
        paid.push(ValueRow { label: good_name(kind), value: site.withdrawal().count(kind).to_string(), warn: false });
    }
    report.sections.push(Section::Values { title: t("site.paid"), note: Some(t("site.refunded")), rows: paid });

    let mut built = Vec::new();
    let workers = max_workers(building, tile_def);
    if workers > 0 {
        built.push(ValueRow { label: t("site.workers"), value: workers.to_string(), warn: false });
    }
    let upkeep = tile_def.map_or(0, |def| def.upkeep);
    let upkeep = if upkeep == 0 { t("costs.none") } else { tc("costs.gold_a_year", i64::from(upkeep), &[]) };
    built.push(ValueRow { label: t("costs.upkeep"), value: upkeep, warn: false });
    report.sections.push(Section::Values { title: t("site.once_built"), note: None, rows: built });

    report.actions = vec![Action::new(ActionKind::CancelSite, "action.cancel_site")];
}

// How many work there once it's finished: its config's most.
fn max_workers(building: &Building, tile_def: Option<&TileDef>) -> u32 {
    let configs = BuildingConfigs::get();
    match building.archetype_kind() {
        BuildingArchetypeKind::ProducerBuilding => {
            tile_def.map_or(0, |def| configs.find_producer_config(building.kind(), def.hash, &def.name).max_workers)
        }
        BuildingArchetypeKind::ServiceBuilding => configs.find_service_config(building.kind()).max_workers,
        BuildingArchetypeKind::StorageBuilding => configs.find_storage_config(building.kind()).max_workers,
        BuildingArchetypeKind::HouseBuilding => 0,
    }
}
