// The inspector's walkers (inspector-unit-<realm>.jpg): what one is doing, carrying, where from and
// where to, and how far it still has to go; and the errand lines the buildings' panels show for
// their runners and patrols.

use engine::ui::text::{self, UiTextCategory};

use super::{Action, ActionKind, ErrandIcon, InspectIcon, InspectReport, InspectTarget, PanelKind, Section, Status, Tone, ValueRow, building, distance_text, duration_text, t, tc, tf};
use crate::{
    city::spaced_name,
    realm::Realm,
    sim::SimContext,
    world::object::GameObject,
    unit::{
        Unit,
        navigation::UnitNavGoal,
        task::{Emergency, UnitTaskArchetype},
    },
};

pub(super) fn read(context: &SimContext, unit: &Unit, realm: Realm) -> Option<InspectReport> {
    let doing = Doing::read(context, unit, realm);
    let name = spaced_name(unit.name());
    let subtitle = match doing.as_ref().and_then(|doing| doing.origin.clone()) {
        Some(origin) => tf("kind.walker_from", &[("building", &origin)]),
        None => t("kind.walker"),
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
            value: tc("production.units", i64::from(item.count), &[("good", &building::good_name(item.kind).to_lowercase())]),
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

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

// What a walker is doing, read from its task.
struct Doing {
    word: String,                // The status line: "Delivering".
    line: String,                // The errand line a building's panel shows.
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
                let (count, kind) = inventory.map_or((task.resource_count, task.resource_kind_to_deliver), |item| (item.count, item.kind));
                let goods = tc("production.units", i64::from(count), &[("good", &building::good_name(kind).to_lowercase())]);
                let line = match &destination {
                    Some(to) => tf("errand.delivering_to", &[("who", &who), ("goods", &goods), ("building", to)]),
                    None => tf("errand.delivering", &[("who", &who), ("goods", &goods)]),
                };
                ("status.delivering", line, ErrandIcon::Cart, Some(task.origin_building_tile.base_cell))
            }
            UnitTaskArchetype::UnitTaskFetchFromStorage(task) => {
                let goods: Vec<String> = task.resources_to_fetch.iter().map(|item| building::good_name(item.kind).to_lowercase()).collect();
                let goods = goods.join(&format!(" {} ", t("or")));
                let line = match (&destination, inventory) {
                    (_, Some(item)) => tf("errand.bringing_back", &[
                        ("who", &who),
                        ("goods", &tc("production.units", i64::from(item.count), &[("good", &building::good_name(item.kind).to_lowercase())])),
                    ]),
                    (Some(from), None) => tf("errand.fetching_from", &[("who", &who), ("goods", &goods), ("building", from)]),
                    (None, None) => tf("errand.fetching", &[("who", &who), ("goods", &goods)]),
                };
                ("status.fetching", line, ErrandIcon::Cart, Some(task.origin_building_tile.base_cell))
            }
            UnitTaskArchetype::UnitTaskRandomizedPatrol(task) => {
                ("status.on_rounds", tf("errand.on_rounds", &[("who", &who)]), ErrandIcon::Walker, Some(task.origin_building_tile.base_cell))
            }
            UnitTaskArchetype::UnitTaskRespond(task) => {
                let target = name_at(task.target_tile.base_cell).unwrap_or_default();
                let key = match task.emergency {
                    Emergency::Fire => "errand.answering_fire",
                    Emergency::Outbreak => "errand.answering_sickness",
                };
                ("status.answering", tf(key, &[("who", &who), ("building", &target)]), ErrandIcon::Walker, Some(task.origin_building_tile.base_cell))
            }
            UnitTaskArchetype::UnitTaskHarvestWood(task) => {
                ("status.gathering", tf("errand.gathering", &[("who", &who)]), ErrandIcon::Walker, Some(task.origin_building_tile.base_cell))
            }
            UnitTaskArchetype::UnitTaskSettler(_) => ("status.settling", tf("errand.settling", &[("who", &who)]), ErrandIcon::Walker, None),
            _ => ("status.walking", tf("errand.walking", &[("who", &who)]), ErrandIcon::Walker, None),
        };
        Some(Self {
            word: t(word_key),
            line,
            icon,
            origin: origin_cell.and_then(name_at),
            destination,
            home: origin_cell,
        })
    }
}

// The errand line a building's panel shows for one of its walkers, if it's out on one.
pub(super) fn errand(context: &SimContext, unit: &Unit, realm: Realm) -> Option<Section> {
    let doing = Doing::read(context, unit, realm)?;
    Some(Section::Errand { icon: doing.icon, text: doing.line })
}

// Where the walker's building is (the "Show its home" button's cell).
pub fn home_cell(context: &SimContext, unit: &Unit) -> Option<common::coords::Cell> {
    let realm = crate::realm::ui_realm_for(false).realm;
    Doing::read(context, unit, realm).and_then(|doing| doing.home)
}
