// What the City book shows of the city now, read from the world, the simulation and the configs
// when the book is built (docs/plans/realm-ui.md, Phase 6), and what automation's city.state
// reports. The monthly changes and trends come from the history (history.rs).

use engine::ui::text::{self, UiTextCategory};
use serde::Serialize;

use super::{CityHistory, spaced_name};
use crate::{
    building::{BuildingKind, config::BuildingConfigs, house::HouseLevel},
    config::GameConfigs,
    sim::{
        Simulation,
        resources::{RESOURCE_KIND_COUNT, ResourceKind},
    },
    trade::requests::{self, RequestRow},
    world::World,
};

// The goods' groups, as the City book's Goods page shows them.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoodsGroup {
    Foods,
    ConsumerGoods,
    RawMaterials,
}

impl GoodsGroup {
    pub const ALL: [Self; 3] = [Self::Foods, Self::ConsumerGoods, Self::RawMaterials];

    pub fn kinds(self) -> ResourceKind {
        match self {
            Self::Foods => ResourceKind::foods(),
            Self::ConsumerGoods => ResourceKind::consumer_goods(),
            Self::RawMaterials => ResourceKind::raw_materials(),
        }
    }

    // Its City string's key part (`book.goods.<key>`).
    pub fn key(self) -> &'static str {
        match self {
            Self::Foods => "foods",
            Self::ConsumerGoods => "consumer_goods",
            Self::RawMaterials => "raw_materials",
        }
    }
}

// A house level: how many houses are at it and how many people live in them, and what the next
// level needs on top of this one's.
#[derive(Clone, Debug, Serialize)]
pub struct HousingRow {
    pub level: u32, // From 1.
    pub houses: u32,
    pub residents: u32,
    pub next_needs: Option<String>, // None at the top level.
}

// A good in the storage yards and granaries: what's stored, the room there is for it (its slots'
// and the free slots' that take it) and how it changed since the month began.
#[derive(Clone, Debug, Serialize)]
pub struct GoodRow {
    #[serde(serialize_with = "serialize_kind")]
    pub kind: ResourceKind,
    pub name: String,
    pub stored: u32,
    pub room: u32,
    pub change: Option<i64>, // None before the history has a month.
}

fn serialize_kind<S: serde::Serializer>(kind: &ResourceKind, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(kind.name())
}

// The storage yards or the granaries: how many there are (finished), and what they hold of what
// they could.
#[derive(Clone, Default, Debug, Serialize)]
pub struct StorageRow {
    pub buildings: u32,
    pub stored: u32,
    pub capacity: u32,
}

// A thing the overview lists under Needs attention: its icon and its text.
#[derive(Clone, Debug, Serialize)]
pub struct Warning {
    pub icon: &'static str, // skin::hud_icon's name.
    pub text: String,
    pub page: Option<&'static str>, // The page that tells more, by id.
}

#[derive(Clone, Debug, Serialize)]
pub struct CityReport {
    pub population: u32,
    pub population_change: Option<i64>, // Since the month began.
    pub employed: u32,
    pub unemployed: u32,
    pub workforce: u32,
    pub jobs_not_filled: u32,
    pub not_working_age: u32, // Too young or old to work: the population outside the workforce.
    pub housing: Vec<HousingRow>,
    pub goods: Vec<(GoodsGroup, Vec<GoodRow>)>,
    pub storage_yards: StorageRow,
    pub granaries: StorageRow,
    pub food_stored: u32,         // In granaries, markets and houses.
    pub food_a_month: f32,        // What the houses eat in a month at their levels.
    pub food_months: Option<f32>, // None while nobody eats.
    pub granary_food: u32,
    pub balance: i64,
    pub net_this_year: i64,
    pub taxes_owed: u32,
    pub requests: Vec<RequestRow>, // Under way, soonest due first (crate::trade::requests).
    pub warnings: Vec<Warning>,
}

impl CityReport {
    pub fn read(world: &World, sim: &Simulation) -> Self {
        let stats = world.stats();
        let history = CityHistory::get();
        let this_month = history.sample(super::history::current_month(sim));
        let configs = BuildingConfigs::get();

        // The houses by level, and the food they eat.
        let mut houses = [0u32; HouseLevel::count()];
        let mut residents = [0u32; HouseLevel::count()];
        let mut food_a_day = 0.0f32;
        let mut hungry_houses = 0u32;
        let mut sick_houses = 0u32;
        let house_config = configs.house_config();
        world.for_each_building(BuildingKind::House, |building| {
            if building.is_under_construction() {
                return true;
            }
            let house = building.as_house();
            let level = house.level() as usize;
            let people = building.population_count();
            houses[level] += 1;
            residents[level] += people;
            // A house eats one of each food group its level needs (house.rs: stock_update).
            for group in configs.find_house_level_config(house.level()).resources_required.iter() {
                let foods = group.intersection(ResourceKind::foods());
                if let Some(food) = foods.iter().next() {
                    food_a_day += house_config.consumption_rate_table[food.index()] * people as f32;
                }
            }
            if people > 0 && !house.has_food() {
                hungry_houses += 1;
            }
            if building.has_outbreak() {
                sick_houses += 1;
            }
            true
        });
        let housing = (0..HouseLevel::count())
            .map(|level| HousingRow {
                level: level as u32 + 1,
                houses: houses[level],
                residents: residents[level],
                next_needs: next_level_needs(level),
            })
            .collect();

        // The storage: per good, what's stored and the room for it.
        let mut room = [0u32; RESOURCE_KIND_COUNT];
        let mut storage_yards = StorageRow::default();
        let mut granaries = StorageRow::default();
        world.for_each_building(BuildingKind::StorageYard | BuildingKind::Granary, |building| {
            if building.is_under_construction() {
                return true;
            }
            let (building_room, capacity) = building.as_storage().storage_slots.room();
            for (total, more) in room.iter_mut().zip(building_room) {
                *total += more;
            }
            let row = if building.is(BuildingKind::Granary) { &mut granaries } else { &mut storage_yards };
            row.buildings += 1;
            row.capacity += capacity;
            true
        });
        let mut stored = [0u32; RESOURCE_KIND_COUNT];
        stats.resources.storage_yards.for_each(|_, item| {
            stored[item.kind.index()] += item.count;
            storage_yards.stored += item.count;
        });
        stats.resources.granaries.for_each(|_, item| {
            stored[item.kind.index()] += item.count;
            granaries.stored += item.count;
        });
        let goods = GoodsGroup::ALL
            .iter()
            .map(|&group| {
                let rows = group
                    .kinds()
                    .iter()
                    .map(|kind| GoodRow {
                        kind,
                        name: kind.display_name().into_owned(),
                        stored: stored[kind.index()],
                        room: room[kind.index()].max(stored[kind.index()]),
                        change: this_month
                            .map(|sample| i64::from(stored[kind.index()]) - i64::from(sample.stored[kind.index()])),
                    })
                    .collect();
                (group, rows)
            })
            .collect();

        // Food: what's stored anywhere houses get it from, against what they eat in a month.
        let food_in = |stock: &crate::sim::resources::ResourceStock| {
            let mut count = 0;
            stock.for_each(|_, item| {
                if item.kind.intersects(ResourceKind::foods()) {
                    count += item.count;
                }
            });
            count
        };
        let granary_food = food_in(&stats.resources.granaries);
        let food_stored = granary_food + food_in(&stats.resources.markets) + food_in(&stats.resources.houses);
        let sim_configs = &GameConfigs::get().sim;
        let days_a_month = if sim_configs.seconds_per_day > 0.0 {
            sim_configs.seconds_per_month / sim_configs.seconds_per_day
        } else {
            0.0
        };
        let food_a_month = food_a_day * days_a_month;
        let food_months = (food_a_month > 0.0).then(|| food_stored as f32 / food_a_month);

        let population = stats.population.total;
        let workforce = stats.population.workforce();
        let finance = sim.finance();
        let mut report = Self {
            population,
            population_change: this_month.map(|sample| i64::from(population) - i64::from(sample.population)),
            employed: stats.population.employed,
            unemployed: stats.population.unemployed,
            workforce,
            jobs_not_filled: stats.workers.max_employed.saturating_sub(stats.population.employed),
            not_working_age: population.saturating_sub(workforce),
            housing,
            goods,
            storage_yards,
            granaries,
            food_stored,
            food_a_month,
            food_months,
            granary_food,
            balance: finance.balance(),
            net_this_year: finance.this_year().net(),
            taxes_owed: stats.treasury.tax_available,
            requests: requests::rows(world, sim.trade(), super::history::current_month(sim)),
            warnings: Vec::new(),
        };
        report.warnings = warnings(world, sim, &report, hungry_houses, sick_houses);
        report
    }

    pub fn employment_ratio(&self) -> f32 {
        let workforce = self.employed + self.unemployed;
        if workforce == 0 { 0.0 } else { self.employed as f32 / workforce as f32 }
    }

    pub fn goods_in(&self, group: GoodsGroup) -> &[GoodRow] {
        self.goods.iter().find(|(g, _)| *g == group).map_or(&[], |(_, rows)| rows.as_slice())
    }
}

// What the next house level needs on top of `level`'s (from 0): its services and goods, a group's
// choices joined by "/" ("Small well/Large well"). None at the top level.
fn next_level_needs(level: usize) -> Option<String> {
    if level + 1 >= HouseLevel::count() {
        return None;
    }
    let configs = BuildingConfigs::get();
    let this = configs.find_house_level_config(HouseLevel::try_from(level as u8).ok()?);
    let next = configs.find_house_level_config(HouseLevel::try_from(level as u8 + 1).ok()?);
    let or = "/";

    let mut needs: Vec<String> = Vec::new();
    for group in next.services_required.iter() {
        if !this.services_required.iter().any(|have| have == group) {
            needs.push(group.iter().map(|kind| spaced_name(&kind.to_string())).collect::<Vec<_>>().join(or));
        }
    }
    for group in next.resources_required.iter() {
        if !this.resources_required.iter().any(|have| have == group) {
            needs.push(group.iter().map(|kind| kind.display_name().to_lowercase()).collect::<Vec<_>>().join(or));
        }
    }
    if needs.is_empty() {
        return Some(text::get(UiTextCategory::City, "book.population.nothing_more").to_string());
    }
    let mut line = needs.join(", ");
    if let Some(first) = line.get(..1) {
        line = first.to_uppercase() + &line[1..];
    }
    Some(line)
}

// What the overview lists under Needs attention, most pressing first: fires, sickness, debt, the
// requests under way, hungry houses, people out of work, buildings short of workers, full storage.
fn warnings(world: &World, sim: &Simulation, report: &CityReport, hungry_houses: u32, sick_houses: u32) -> Vec<Warning> {
    let mut warnings = Vec::new();
    let count_text = |key: &str, count: u32| text::format_count(UiTextCategory::City, key, i64::from(count), &[]);

    let mut burning = 0;
    world.for_each_building_of_any_kind(|building| {
        if building.is_burning() {
            burning += 1;
        }
        true
    });
    if burning > 0 {
        warnings.push(Warning { icon: "warn", text: count_text("book.warning.fires", burning), page: None });
    }
    if sick_houses > 0 {
        warnings.push(Warning {
            icon: "warn",
            text: count_text("book.warning.sickness", sick_houses),
            page: Some("population"),
        });
    }
    let finance = sim.finance();
    if finance.is_in_debt() {
        let limit = GameConfigs::get().sim.finance.debt_limit_months;
        let months = finance.months_in_debt();
        let text = if limit > months {
            count_text("book.warning.debt", limit - months)
        } else {
            text::get(UiTextCategory::City, "book.warning.debt_overdue").to_string()
        };
        warnings.push(Warning { icon: "coin", text, page: Some("finances") });
    }
    // "The court's pottery request: 18 of 30, 4 months left."
    for request in &report.requests {
        let key = if request.court { "book.warning.request_court" } else { "book.warning.request" };
        let text = text::format_count(UiTextCategory::City, key, i64::from(request.months_left), &[
            ("city", &request.from),
            ("good", &request.good.display_name().to_lowercase()),
            ("held", &request.held.min(request.amount).to_string()),
            ("amount", &request.amount.to_string()),
        ]);
        warnings.push(Warning { icon: "request", text, page: Some("goods") });
    }
    if hungry_houses > 0 {
        warnings.push(Warning { icon: "food", text: count_text("book.warning.hungry", hungry_houses), page: Some("goods") });
    }
    if report.unemployed > 0 {
        warnings.push(Warning {
            icon: "population",
            text: count_text("book.warning.unemployed", report.unemployed),
            page: Some("population"),
        });
    }
    let short = world.stats().workers.buildings_below_min;
    if short > 0 {
        warnings.push(Warning {
            icon: "population",
            text: count_text("book.warning.short_of_workers", short),
            page: Some("population"),
        });
    }
    // Fields the farmsteads own but have no workers to tend (crate::farms), and those under the
    // flood (crate::floods).
    let (mut fallow, mut under_water) = (0, 0);
    world.for_each_building(BuildingKind::Farm, |building| {
        if crate::farms::crop_of(building).is_some() {
            let producer = building.as_producer();
            if !building.is_under_construction() {
                fallow += (producer.fields().len() - producer.tended_field_count()) as u32;
            }
            under_water += producer.fields().iter().filter(|plot| sim.floods().is_flooded(plot.cell)).count() as u32;
        }
        true
    });
    if fallow > 0 {
        warnings.push(Warning {
            icon: "food",
            text: count_text("book.warning.fields_fallow", fallow),
            page: Some("population"),
        });
    }
    if under_water > 0 {
        warnings.push(Warning {
            icon: "water",
            text: count_text("book.warning.fields_under_water", under_water),
            page: Some("goods"),
        });
    }
    // Dyke sections worn enough to breach in a flood (crate::floods).
    let worn = sim.floods().worn_dykes(crate::config::GameConfigs::get().sim.floods.worn_wear) as u32;
    if worn > 0 {
        warnings.push(Warning { icon: "water", text: count_text("book.warning.dykes_worn", worn), page: None });
    }
    for (row, key) in
        [(&report.storage_yards, "book.warning.yards_full"), (&report.granaries, "book.warning.granaries_full")]
    {
        if row.capacity > 0 && row.stored * 10 >= row.capacity * 9 {
            warnings.push(Warning {
                icon: "request",
                text: text::get(UiTextCategory::City, key).to_string(),
                page: Some("goods"),
            });
        }
    }
    warnings
}
