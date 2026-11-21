// The inspector's walkers (inspector-unit-<realm>.jpg): what one is doing, carrying, where from and
// where to, and how far it still has to go; and the errand lines the buildings' panels show for
// their runners and patrols.

use engine::ui::text::{self, UiTextCategory};

use super::{
    Action,
    ActionKind,
    ErrandIcon,
    InspectIcon,
    InspectReport,
    InspectTarget,
    PanelKind,
    Section,
    Status,
    Tone,
    ValueRow,
    building,
    distance_text,
    duration_text,
    t,
    tc,
    tf,
};
use crate::{
    city::spaced_name,
    realm::Realm,
    sim::{SimContext, resources::StockItem},
    unit::{
        Unit,
        navigation::UnitNavGoal,
        task::{
            Emergency,
            UnitTaskArchetype,
            UnitTaskFetchState,
            UnitTaskRespondState,
            UnitTaskShip,
            UnitTaskShipState,
            UnitTaskTrade,
            UnitTaskTradeState,
        },
    },
    world::object::GameObject,
};

pub(super) fn read(context: &SimContext, unit: &Unit, realm: Realm) -> Option<InspectReport> {
    let doing = Doing::read(context, unit, realm);
    let name = spaced_name(unit.name());
    let caravan = unit.current_task_as::<UnitTaskTrade>(context.task_manager());
    let ship = unit.current_task_as::<UnitTaskShip>(context.task_manager());
    let subtitle = match (caravan, ship, doing.as_ref().and_then(|doing| doing.origin.clone())) {
        (Some(caravan), _, _) => {
            tf("kind.caravan_from", &[("partner", &crate::trade::partner_name(context.trade(), &caravan.partner))])
        }
        (None, Some(ship), _) => {
            tf("kind.ship_from", &[("partner", &crate::trade::partner_name(context.trade(), &ship.partner))])
        }
        (None, None, Some(origin)) => tf("kind.walker_from", &[("building", &origin)]),
        (None, None, None) => t("kind.walker"),
    };

    let remaining = unit.navigation().remaining_cells();
    let status = doing.as_ref().map(|doing| {
        let summary = if remaining > 0 {
            match unit_speed(unit) {
                Some(speed) => tf("summary.arrives_in", &[("time", &duration_text(remaining as f32 / speed))]),
                None => distance_text(remaining),
            }
        } else {
            String::new()
        };
        Status { word: doing.word.clone(), tone: Tone::Good, summary }
    });

    let mut rows = Vec::new();
    if let Some(item) = unit.peek_inventory()
        && item.count > 0
    {
        rows.push(ValueRow {
            label: t("errand.carrying"),
            value: tc("production.units", i64::from(item.count), &[(
                "good",
                &building::good_name(item.kind).to_lowercase(),
            )]),
            warn: false,
        });
    }
    // A caravan's or a ship's cargo: the partner's goods it still carries, and what it bought and sold.
    let cargo = caravan
        .map(|caravan| (&caravan.cargo, &caravan.bought, &caravan.sold))
        .or_else(|| ship.map(|ship| (&ship.cargo, &ship.bought, &ship.sold)));
    if let Some((cargo, bought, sold)) = cargo {
        let goods = |counts: &[u32]| {
            let goods: Vec<String> = crate::sim::resources::ResourceKind::all()
                .iter()
                .filter(|good| counts[good.index()] != 0)
                .map(|good| {
                    tc("production.units", i64::from(counts[good.index()]), &[(
                        "good",
                        &building::good_name(good).to_lowercase(),
                    )])
                })
                .collect();
            goods.join(", ")
        };
        let carrying = goods(cargo);
        if !carrying.is_empty() {
            rows.push(ValueRow { label: t("errand.carrying"), value: carrying, warn: false });
        }
        let bought = goods(bought);
        if !bought.is_empty() {
            rows.push(ValueRow { label: t("errand.bought"), value: bought, warn: false });
        }
        let sold = goods(sold);
        if !sold.is_empty() {
            rows.push(ValueRow { label: t("errand.sold"), value: sold, warn: false });
        }
    }
    // A moored ship's stay: when it sails at the latest.
    if let Some(ship) = ship
        && ship.state == UnitTaskShipState::Moored
    {
        let left = (ship_stay_secs() - ship.moored_secs).max(0.0);
        rows.push(ValueRow {
            label: t("errand.stay"),
            value: tf("errand.sails_within", &[("time", &duration_text(left))]),
            warn: false,
        });
    }
    if let Some(doing) = &doing {
        if let Some(origin) = &doing.origin {
            rows.push(ValueRow { label: t("errand.from"), value: capitalized(origin), warn: false });
        }
        if let Some(destination) = &doing.destination {
            rows.push(ValueRow { label: t("errand.to"), value: capitalized(destination), warn: false });
        }
    }
    if remaining > 0 {
        rows.push(ValueRow { label: t("errand.to_go"), value: distance_text(remaining), warn: false });
    }

    let mut sections = Vec::new();
    if !rows.is_empty() {
        sections.push(Section::Values { title: t("errand.title"), note: None, rows });
    }
    let dialog = text::find_str(UiTextCategory::UnitDialog, unit.find_tile(context).tile_def().hash).unwrap_or("");
    if !dialog.is_empty() {
        sections.push(Section::Text { text: dialog.to_string() });
    }

    let home = doing.as_ref().and_then(|doing| doing.home);
    let mut show_home = Action::new(ActionKind::ShowHome, "action.show_home");
    if home.is_none() {
        show_home = show_home.disabled("action.no_home");
    }

    Some(InspectReport {
        target: InspectTarget::Unit { id: unit.id(), cell: unit.cell() },
        panel: PanelKind::Unit,
        icon: InspectIcon::Sprite,
        name,
        subtitle,
        status,
        sections,
        actions: vec![show_home],
        wide: false,
    })
}

// How fast a walker goes, in cells a game second.
fn unit_speed(unit: &Unit) -> Option<f32> {
    let speed = unit.config().movement_speed;
    (speed > 0.0).then_some(speed)
}

pub(super) fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

// What a walker is doing, read from its task.
struct Doing {
    word: String, // The status line: "Delivering".
    line: String, // The errand line a building's panel shows.
    icon: ErrandIcon,
    origin: Option<String>,      // Its building, in a sentence ("the pottery kiln").
    destination: Option<String>, // Where it's going, the same.
    home: Option<common::coords::Cell>,
}

impl Doing {
    fn read(context: &SimContext, unit: &Unit, realm: Realm) -> Option<Self> {
        let task_id = unit.current_task()?;
        let (task, _) = context.task_manager().try_get_task_archetype_and_started(task_id)?;
        let walker = spaced_name(unit.name()).to_lowercase();
        let article = if walker.starts_with(['a', 'e', 'i', 'o', 'u']) { "errand.who_an" } else { "errand.who" };
        let who = tf(article, &[("walker", &walker)]);
        let name_at = |cell| building::building_name_at(context, cell, realm);
        let destination = match unit.goal() {
            Some(goal @ UnitNavGoal::Building { .. }) => name_at(goal.building_destination().1),
            _ => None,
        };
        let inventory = unit.peek_inventory().filter(|item| item.count > 0);

        let (word_key, line, icon, origin_cell) = match task {
            UnitTaskArchetype::UnitTaskDeliverToStorage(task) => {
                let (count, kind) =
                    inventory.map_or((task.resource_count, task.resource_kind_to_deliver), |item| (item.count, item.kind));
                let goods = tc("production.units", i64::from(count), &[("good", &building::good_name(kind).to_lowercase())]);
                let line = match &destination {
                    Some(to) => tf("errand.delivering_to", &[("who", &who), ("goods", &goods), ("building", to)]),
                    None => tf("errand.delivering", &[("who", &who), ("goods", &goods)]),
                };
                ("status.delivering", line, ErrandIcon::Cart, Some(task.origin_building_tile.base_cell))
            }
            UnitTaskArchetype::UnitTaskFetchFromStorage(task) => {
                let goods: Vec<String> =
                    task.resources_to_fetch.iter().map(|item| building::good_name(item.kind).to_lowercase()).collect();
                let goods = goods.join(&format!(" {} ", t("or")));
                let carried = |item: StockItem| {
                    tc("production.units", i64::from(item.count), &[(
                        "good",
                        &building::good_name(item.kind).to_lowercase(),
                    )])
                };
                let line = match (task.state, inventory) {
                    (UnitTaskFetchState::RoutingSurplus | UnitTaskFetchState::UnloadingSurplus, Some(item)) => {
                        tf("errand.storing_surplus", &[("who", &who), ("goods", &carried(item))])
                    }
                    (_, Some(item)) => tf("errand.bringing_back", &[("who", &who), ("goods", &carried(item))]),
                    (
                        UnitTaskFetchState::ReturningToOrigin
                        | UnitTaskFetchState::DeliveringToOrigin
                        | UnitTaskFetchState::Done,
                        None,
                    ) => tf("errand.returning_empty", &[("who", &who)]),
                    (_, None) => match &destination {
                        Some(from) => tf("errand.fetching_from", &[("who", &who), ("goods", &goods), ("building", from)]),
                        None => tf("errand.fetching", &[("who", &who), ("goods", &goods)]),
                    },
                };
                ("status.fetching", line, ErrandIcon::Cart, Some(task.origin_building_tile.base_cell))
            }
            UnitTaskArchetype::UnitTaskRandomizedPatrol(task) => (
                "status.on_rounds",
                tf("errand.on_rounds", &[("who", &who)]),
                ErrandIcon::Walker,
                Some(task.origin_building_tile.base_cell),
            ),
            UnitTaskArchetype::UnitTaskRespond(task) => {
                let origin = Some(task.origin_building_tile.base_cell);
                if task.state != UnitTaskRespondState::MovingToTarget {
                    ("status.going_home", tf("errand.going_home", &[("who", &who)]), ErrandIcon::Walker, origin)
                } else {
                    let line = match (task.emergency, name_at(task.target_tile.base_cell)) {
                        (Emergency::Fire, Some(target)) => {
                            tf("errand.answering_fire", &[("who", &who), ("building", &target)])
                        }
                        (Emergency::Outbreak, Some(target)) => {
                            tf("errand.answering_sickness", &[("who", &who), ("building", &target)])
                        }
                        (Emergency::Fire, None) => tf("errand.answering_fire_gone", &[("who", &who)]),
                        (Emergency::Outbreak, None) => tf("errand.answering_sickness_gone", &[("who", &who)]),
                    };
                    ("status.answering", line, ErrandIcon::Walker, origin)
                }
            }
            UnitTaskArchetype::UnitTaskHarvestWood(task) => (
                "status.gathering",
                tf("errand.gathering", &[("who", &who)]),
                ErrandIcon::Walker,
                Some(task.origin_building_tile.base_cell),
            ),
            UnitTaskArchetype::UnitTaskSettler(_) => {
                ("status.settling", tf("errand.settling", &[("who", &who)]), ErrandIcon::Walker, None)
            }
            UnitTaskArchetype::UnitTaskTrade(task) => {
                let partner = crate::trade::partner_name(context.trade(), &task.partner);
                if task.state == UnitTaskTradeState::Leaving {
                    ("status.leaving", tf("errand.caravan_leaving", &[("partner", &partner)]), ErrandIcon::Cart, None)
                } else {
                    let line = match &destination {
                        Some(store) => tf("errand.caravan_to", &[("partner", &partner), ("building", store)]),
                        None => tf("errand.caravan", &[("partner", &partner)]),
                    };
                    ("status.trading", line, ErrandIcon::Cart, None)
                }
            }
            UnitTaskArchetype::UnitTaskShip(task) => {
                let partner = crate::trade::partner_name(context.trade(), &task.partner);
                let (word, key) = match task.state {
                    UnitTaskShipState::FindingBerth | UnitTaskShipState::Sailing => {
                        ("status.sailing", "errand.ship_sailing")
                    }
                    UnitTaskShipState::WaitingOff => ("status.waiting", "errand.ship_waiting"),
                    UnitTaskShipState::Mooring | UnitTaskShipState::Moored => ("status.moored", "errand.ship_moored"),
                    UnitTaskShipState::Leaving => ("status.leaving", "errand.ship_leaving"),
                };
                (word, tf(key, &[("partner", &partner)]), ErrandIcon::Cart, None)
            }
            _ => ("status.walking", tf("errand.walking", &[("who", &who)]), ErrandIcon::Walker, None),
        };
        // Its home only while it stands.
        let origin = origin_cell.and_then(name_at);
        let home = origin_cell.filter(|_| origin.is_some());
        Some(Self { word: t(word_key), line, icon, origin, destination, home })
    }
}

// The errand line a building's panel shows for one of its walkers, if it's out on one.
pub(super) fn errand(context: &SimContext, unit: &Unit, realm: Realm) -> Option<Section> {
    let doing = Doing::read(context, unit, realm)?;
    Some(Section::Errand { icon: doing.icon, text: doing.line })
}

// Where the walker's building is (the "Show its home" button's cell).
pub fn home_cell(context: &SimContext, unit: &Unit, realm: Realm) -> Option<common::coords::Cell> {
    Doing::read(context, unit, realm).and_then(|doing| doing.home)
}

// The longest a ship stays moored, in seconds (UnitTaskShip's stay).
fn ship_stay_secs() -> f32 {
    let sim = &crate::config::GameConfigs::get().sim;
    sim.trade.ship_stay_months.max(1) as f32 * sim.seconds_per_month
}
