// The inspector's buildings: a workshop, a house, a store, a service (a market among them) and a
// construction site, as inspector-<kind>-<realm>.jpg lays them out.

use common::coords::Cell;
use engine::ui::text::{self as ui_text, UiTextCategory};

use super::{
    Action,
    ActionKind,
    ErrandIcon,
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
            read_storage(context, building, tile_def, realm, &mut report);
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

// Why a workplace isn't working, if it isn't, in order: paused, no road (when it needs one), too
// few workers.
fn workplace_status(building: &Building, needs_road: bool) -> Option<Status> {
    if let Some(status) = emergency_status(building) {
        return Some(status);
    }
    if building.is_paused() {
        return Some(Status { word: t("status.paused"), tone: Tone::Pending, summary: t("summary.paused") });
    }
    if needs_road && !building.is_linked_to_road() {
        return Some(Status { word: t("status.no_road"), tone: Tone::Bad, summary: t("summary.no_road") });
    }
    if !building.has_min_required_workers() {
        return Some(Status { word: t("status.short_of_workers"), tone: Tone::Bad, summary: t("summary.short_of_workers") });
    }
    None
}

// The workers a building has against its jobs, and a note: paused, short of its minimum or of its
// jobs (with its sector's labour priority: "2 short · Industry is priority 2"), or fully staffed.
fn workers_section(context: &SimContext, building: &Building, compact: bool) -> Option<Section> {
    let employer = building.workers()?.as_employer()?;
    let need = employer.max_employees();
    if need == 0 {
        return None;
    }
    let have = employer.employee_count();
    let short = need.saturating_sub(have);
    let with_priority = |note: String| {
        let sector = crate::labour::sector_of(context, building);
        tf("workers.with_priority", &[
            ("note", &note),
            ("sector", ui_text::get(UiTextCategory::Hud, &sector.name_key())),
            ("priority", &context.labour().priority(sector).to_string()),
        ])
    };
    let note = if building.is_paused() {
        t("workers.paused")
    } else if have < employer.min_employees() {
        with_priority(tf("workers.needs_min", &[("min", &employer.min_employees().to_string())]))
    } else if short > 0 {
        with_priority(tc("workers.short", i64::from(short), &[]))
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

// Pause work, or Resume work while paused (for a building with workers: crate::labour), and
// Demolish.
fn workplace_actions(building: &Building) -> Vec<Action> {
    let mut actions = Vec::new();
    if building.has_jobs() {
        let key = if building.is_paused() { "action.resume_work" } else { "action.pause_work" };
        actions.push(Action::new(ActionKind::PauseWork, key));
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

    let farmstead = crate::farms::farmstead_report(context, building);
    report.status = Some(workplace_status(building, true).unwrap_or_else(|| {
        if let Some(farmstead) = &farmstead {
            if farmstead.fields == 0 {
                return Status { word: t("status.no_fields"), tone: Tone::Bad, summary: t("summary.lay_fields") };
            }
            return Status {
                word: t("status.working"),
                tone: Tone::Good,
                summary: tc("summary.tends_fields", farmstead.tended as i64, &[]),
            };
        }
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

    report.sections.extend(workers_section(context, building, false));

    if let (Some(farmstead), Some(crop)) = (&farmstead, crate::farms::crop_of(building)) {
        report.sections.push(fields_section(farmstead, crop, output));
    } else if let Some(config) = config {
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

fn read_storage(
    context: &SimContext,
    building: &Building,
    tile_def: Option<&TileDef>,
    realm: Realm,
    report: &mut InspectReport,
) {
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

    report.sections.extend(workers_section(context, building, true));
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
    // Its runner's errand (Get and Empty), and the goods to empty that no store in reach takes.
    push_errand(&mut report.sections, context, storage.runner.unit_id(), realm);
    if !storage.cannot_empty.is_empty() && !storage.is_runner_out() {
        let goods: Vec<String> = storage.cannot_empty.iter().map(|kind| good_name(kind).to_lowercase()).collect();
        let goods = goods.join(&format!(" {} ", t("or")));
        report
            .sections
            .push(Section::Errand { icon: ErrandIcon::Cart, text: tf("storage.cannot_empty", &[("goods", &goods)]) });
    }
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

    // A sluice gate's part in the canals and fields (crate::farms::irrigation).
    let gate = crate::farms::sluice_gate_of(building).and_then(|gate| {
        let irrigation = crate::farms::Irrigation::read(context.world(), context.tile_map());
        let supply = irrigation.gate_at(gate.cells.start)?.clone();
        let watered = irrigation.fields_watered_by(context.tile_map(), supply.base_cell());
        Some((supply, watered))
    });

    report.status = Some(workplace_status(building, config.requires_road_access).unwrap_or_else(|| {
        if let Some((supply, _)) = &gate
            && !supply.intake
        {
            return Status { word: t("status.no_water"), tone: Tone::Bad, summary: t("summary.no_intake") };
        }
        if market && let Some(item) = building.stock().into_iter().find(|item| item.count == 0) {
            return Status {
                word: tf("status.short_of", &[("good", &good_name(item.kind).to_lowercase())]),
                tone: Tone::Bad,
                summary: tc("summary.supplies_houses", i64::from(houses), &[]),
            };
        }
        let summary = if building.is(BuildingKind::FestivalSquare) {
            t("summary.hosts_festivals")
        } else if let Some(berth) = crate::trade::ships::berth_of(building) {
            match crate::trade::ships::moored_ship(context, berth) {
                Some((_, ship)) => {
                    tf("summary.dock_ship", &[("partner", &crate::trade::partner_name(context.trade(), &ship.partner))])
                }
                None => t("summary.dock_waiting"),
            }
        } else if let Some((_, watered)) = &gate {
            tc("summary.waters_fields", *watered as i64, &[])
        } else if market || config.visits.building_kinds() == BuildingKind::House {
            tc("summary.serves_houses", i64::from(houses), &[])
        } else {
            let buildings = count_in_reach(context, building, config.visits.building_kinds(), config.effect_radius);
            tc("summary.watches_buildings", i64::from(buildings), &[])
        };
        Status { word: t("status.working"), tone: Tone::Good, summary }
    }));

    report.sections.extend(workers_section(context, building, false));

    if let Some((supply, watered)) = &gate {
        report.sections.push(sluice_section(supply, *watered));
    } else if market {
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
        // A shrine's or a temple's patron, and how it feels (docs/plans/faith.md, §2a).
        if let Some(patron) = crate::faith::dedicated_patron(context, building) {
            let name = crate::faith::config::patron_name(context.realm(), &patron.id);
            let favour = context.faith().favour(&patron.id).unwrap_or_default();
            let mood = crate::faith::Mood::from_favour(favour);
            rows.insert(0, ValueRow {
                label: t("faith.dedicated"),
                value: format!("{name} · {}", mood.display_name()),
                warn: matches!(mood, crate::faith::Mood::Restless | crate::faith::Mood::Neglected),
            });
        }
        let title = if config.has_patrol_unit { t("patrol.title") } else { t("patrol.reach") };
        report.sections.push(Section::Values { title, note: Some(note), rows });
    }

    if building.is(BuildingKind::FestivalSquare) {
        report.sections.push(festivals_section(context));
    }
    if let Some(berth) = crate::trade::ships::berth_of(building) {
        report.sections.push(dock_section(context, berth));
        for docker in &berth.dockers {
            push_errand(&mut report.sections, context, docker.runner.unit_id(), realm);
        }
    }

    push_errand(&mut report.sections, context, service.runner.unit_id(), realm);
    push_errand(&mut report.sections, context, service.patrol.unit_id(), realm);
    report.sections.push(costs_section(context, building, tile_def));
    report.sections.extend(risks_section(building));
    report.actions = workplace_actions(building);
    // A dock's routes' limits (the trade limits dialog), while the game trades by water.
    if building.is(BuildingKind::Dock) {
        let mut limits = Action::new(ActionKind::TradeLimits, "action.trade_limits");
        if dock_partner(context, building).is_none() {
            limits = limits.disabled("action.no_water_routes");
        }
        report.actions.insert(0, limits);
    }
}

// "16 metal, 8 rice", or None for no goods.
fn goods_list(counts: &[u32]) -> Option<String> {
    let goods: Vec<String> = ResourceKind::all()
        .iter()
        .filter(|good| counts[good.index()] != 0)
        .map(|good| tc("production.units", i64::from(counts[good.index()]), &[("good", &good_name(good).to_lowercase())]))
        .collect();
    (!goods.is_empty()).then(|| goods.join(", "))
}

// The river and sea partners the game trades with (open routes first), for a dock's panel.
fn water_partners(context: &SimContext) -> Vec<(String, bool, Option<u32>)> {
    let trade = context.trade();
    let mut partners: Vec<(String, bool, Option<u32>)> = trade
        .partners()
        .iter()
        .filter(|partner| crate::trade::terms(trade, partner.city()).is_some_and(|terms| terms.by_water()))
        .map(|partner| (partner.city().to_string(), partner.open, partner.next_arrival))
        .collect();
    partners.sort_by_key(|(_, open, _)| !open);
    partners
}

// The partner a dock's Set trade limits opens on: its ship's, else the first river or sea partner.
pub fn dock_partner(context: &SimContext, building: &Building) -> Option<String> {
    if !context.trade().is_enabled() {
        return None;
    }
    let berth = crate::trade::ships::berth_of(building)?;
    crate::trade::ships::moored_ship(context, berth)
        .map(|(_, ship)| ship.partner.clone())
        .or_else(|| water_partners(context).into_iter().next().map(|(city, _, _)| city))
}

// A dock's berth (docs/plans/trade.md, §5): the ship moored, what's on board and what it's taken on,
// when it sails, the next ship due, and the river and sea routes the dock serves.
fn dock_section(context: &SimContext, berth: &crate::trade::ships::DockBerth) -> Section {
    let trade = context.trade();
    let mut rows = Vec::new();
    match crate::trade::ships::moored_ship(context, berth) {
        Some((_, ship)) => {
            let partner = crate::trade::partner_name(trade, &ship.partner);
            rows.push(ValueRow {
                label: t("dock.ship"),
                value: tf("dock.ship_from", &[("partner", &partner)]),
                warn: false,
            });
            rows.push(ValueRow {
                label: t("dock.on_board"),
                value: goods_list(&ship.cargo).unwrap_or_else(|| t("dock.nothing")),
                warn: false,
            });
            if let Some(bought) = goods_list(&ship.bought) {
                rows.push(ValueRow { label: t("dock.taken_on"), value: bought, warn: false });
            }
        }
        None => rows.push(ValueRow { label: t("dock.ship"), value: t("dock.no_ship"), warn: false }),
    }
    let now = crate::trade::now(context);
    let next = water_partners(context)
        .into_iter()
        .filter(|(_, open, _)| *open)
        .filter_map(|(city, _, month)| Some((month?, city)))
        .min();
    let next = match next {
        Some((month, city)) => {
            let partner = crate::trade::partner_name(trade, &city);
            let months = month.saturating_sub(now);
            if months == 0 {
                tf("dock.next_this_month", &[("partner", &partner)])
            } else {
                tc("dock.next_in", i64::from(months), &[("partner", &partner)])
            }
        }
        None => t("dock.next_none"),
    };
    rows.push(ValueRow { label: t("dock.next_ship"), value: next, warn: false });
    let routes: Vec<String> = water_partners(context)
        .into_iter()
        .map(|(city, open, _)| {
            let partner = crate::trade::partner_name(trade, &city);
            tf(if open { "dock.route_open" } else { "dock.route_closed" }, &[("partner", &partner)])
        })
        .collect();
    let routes = if routes.is_empty() { t("dock.no_routes") } else { routes.join(", ") };
    rows.push(ValueRow { label: t("dock.routes"), value: routes, warn: false });
    Section::Values { title: t("dock.title"), note: Some(t("dock.note")), rows }
}

// A sluice gate's water (docs/plans/seasons-floods.md, §9): whether it has water across its dyke,
// the fields its outlet waters through the canals and fields joined to it (sown in the dry season
// too) and what it does in a flood.
fn sluice_section(supply: &crate::farms::GateSupply, watered: usize) -> Section {
    let intake = if supply.intake { t("sluice.intake.river") } else { t("sluice.intake.none") };
    let state = if !supply.gate.working {
        t("sluice.watered.no_workers")
    } else if !supply.intake {
        t("sluice.watered.no_water")
    } else {
        t("sluice.watered.both")
    };
    Section::Values {
        title: t("sluice.title"),
        note: Some(t("sluice.note")),
        rows: vec![
            ValueRow { label: t("sluice.intake"), value: intake, warn: !supply.intake },
            ValueRow { label: t("sluice.fields"), value: watered.to_string(), warn: watered == 0 },
            ValueRow { label: t("sluice.watered"), value: state, warn: !supply.supplies() },
            ValueRow { label: t("sluice.flood"), value: t("sluice.flood.silt"), warn: false },
        ],
    }
}

// A farmstead's fields (docs/plans/seasons-floods.md, §7): how many its workers tend, those left
// fallow for want of them, the crop's months, the next harvest and what it holds.
fn fields_section(
    farmstead: &crate::farms::FarmsteadReport,
    crop: &crate::building::producer::CropConfig,
    held: crate::sim::resources::StockItem,
) -> Section {
    let good = good_name(held.kind).to_lowercase();
    let month_of = |ordinal: u32| (ordinal % 12 + 1).to_string();
    let fallow = farmstead.fields - farmstead.tended;
    let mut rows = vec![
        ValueRow {
            label: t("fields.fields"),
            value: tf("fields.tended", &[
                ("fields", &farmstead.fields.to_string()),
                ("tended", &farmstead.tended.to_string()),
            ]),
            warn: farmstead.fields == 0,
        },
        ValueRow { label: t("fields.fallow"), value: fallow.to_string(), warn: fallow > 0 },
    ];
    match farmstead.next_reaped {
        Some(reaped) => rows.push(ValueRow {
            label: t("fields.next_harvest"),
            value: tf("fields.harvest", &[
                ("amount", &farmstead.expected.round().to_string()),
                ("good", &good),
                ("month", &month_of(reaped)),
            ]),
            warn: false,
        }),
        None => rows.push(ValueRow {
            label: t("fields.next_harvest"),
            value: t(&format!("fields.none_growing.{}", crop.season.to_string().to_lowercase())),
            warn: false,
        }),
    }
    rows.push(ValueRow { label: t("fields.holds"), value: format!("{} {good}", held.count), warn: false });
    Section::Values { title: t("fields.title"), note: None, rows }
}

// A festival square's festivals (docs/plans/faith.md §5): the one planned, with its scale and
// month, and the last one held.
fn festivals_section(context: &SimContext) -> Section {
    use crate::faith::config::{festival_name, scale_name};
    let realm = context.realm();
    let faith = context.faith();
    let next = match faith.plan() {
        Some(plan) => {
            let month = crate::faith::FaithConfigs::get()
                .for_realm(realm)
                .and_then(|def| def.festival(&plan.festival))
                .map_or(0, |festival| festival.month);
            tf("festivals.planned", &[
                ("festival", &festival_name(realm, &plan.festival)),
                ("scale", &scale_name(realm, &plan.scale).to_lowercase()),
                ("month", &month.to_string()),
            ])
        }
        None if !faith.is_festivals_enabled() => t("festivals.off"),
        None => t("festivals.none_planned"),
    };
    let last = match faith.log().last() {
        Some(record) => tf("festivals.held", &[
            ("festival", &festival_name(realm, &record.festival)),
            ("month", &(record.ordinal_month % 12 + 1).to_string()),
        ]),
        None => t("festivals.none_held"),
    };
    Section::Values {
        title: t("festivals.title"),
        note: None,
        rows: vec![ValueRow { label: t("festivals.next"), value: next, warn: false }, ValueRow {
            label: t("festivals.last"),
            value: last,
            warn: false,
        }],
    }
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
            && building.is_within_reach_of(other, radius)
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
