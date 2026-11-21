use common::{Color, format_small};
use engine::ui::{self, DrawDebugUi, UiNumberFormat, UiStaticVar, UiSystem};

use crate::{
    building::{
        BuildingKind,
        risks::{RISK_HIGH, RiskEntry, RiskKind, RiskSummary},
    },
    camera::Camera,
    cheats,
    faith::{
        FaithConfigs,
        Mood,
        config::{self as faith_config, FaithSystems},
        events as faith_events,
    },
    floods,
    sim::{CityFaith, CityFloods, CityTrade, Finance, risks::CityRisks, trade::TradeLimit},
    trade,
    world::{World, stats::WorldStats},
};

// How many of the riskiest buildings the Risks tab lists.
const RISKIEST_LISTED: usize = 10;

// ----------------------------------------------
// World / WorldStats Debug UI
// ----------------------------------------------

// The simulation's city-wide state the window's tabs show, and the calendar's ordinal month.
pub(crate) struct CityDebugState<'a> {
    pub finance: &'a mut Finance,
    pub risks: &'a mut CityRisks,
    pub faith: &'a mut CityFaith,
    pub floods: &'a mut CityFloods,
    pub trade: &'a mut CityTrade,
    pub now: u32,
}

impl World {
    pub(crate) fn draw_debug_ui(&mut self, city: CityDebugState, camera: &mut Camera, ui_sys: &UiSystem) {
        let CityDebugState { finance, risks, faith, floods: city_floods, trade: city_trade, now } = city;
        const TABS: [&str; 7] =
            ["Population/Workers/Housing", "Tax/Treasury", "Resources", "Risks", "Faith", "Floods", "Trade"];
        let Some(tab) = ui::tab_bar(ui_sys, "World Stats Tab Bar", &TABS) else {
            return;
        };

        if tab.index == 3 {
            self.draw_risks_debug_ui(risks, camera, ui_sys);
        } else if tab.index == 4 {
            draw_faith_debug_ui(faith, FaithSystems::of(city_floods, city_trade), ui_sys);
        } else if tab.index == 5 {
            draw_floods_debug_ui(city_floods, camera, ui_sys);
        } else if tab.index == 6 {
            draw_trade_debug_ui(city_trade, now, ui_sys);
        } else {
            self.stats_mut().draw_debug_ui(tab.index, finance, ui_sys);
        }
    }

    // The Risks tab: the switches, the counts behind risks.state, the fires and outbreaks, and
    // the riskiest buildings, each with a button that moves the camera to it.
    fn draw_risks_debug_ui(&self, risks: &mut CityRisks, camera: &mut Camera, ui_sys: &UiSystem) {
        ui::bullet_text(ui_sys, "Switches:");
        let mut enabled = risks.is_enabled();
        if ui::checkbox(ui_sys, "Risks Enabled", &mut enabled) {
            risks.set_enabled(enabled);
        }
        let mut disease_enabled = risks.is_disease_enabled();
        if ui::checkbox(ui_sys, "Disease Enabled", &mut disease_enabled) {
            risks.set_disease_enabled(disease_enabled);
        }
        ui::checkbox(ui_sys, "No City Risks Cheat", &mut cheats::get_mut().no_city_risks);
        let state = |active: bool| if active { "active" } else { "stopped" };
        ui::text(
            ui_sys,
            format_small!("Risks: {}, disease: {}", state(risks.is_active()), state(risks.is_disease_active())),
        );

        ui::separator(ui_sys);

        let summary = RiskSummary::of(self, RISKIEST_LISTED);
        ui::bullet_text(ui_sys, "City:");
        ui::text(ui_sys, format_small!("Buildings With Risks : {}", summary.buildings));
        ui::grid(ui_sys, "Risk Counts", 3, |grid| {
            grid.text("");
            grid.text(format_small!("High (>= {RISK_HIGH})"));
            grid.text("Highest");
            for kind in [RiskKind::Fire, RiskKind::Collapse, RiskKind::Disease] {
                grid.text(format_small!("{kind}"));
                let high = summary.high(kind);
                grid.cell(|ui_sys| {
                    if high != 0 {
                        ui::text_colored(ui_sys, Color::yellow(), format_small!("{high}"));
                    } else {
                        ui::text(ui_sys, "0");
                    }
                });
                grid.text(format_small!("{:.1}", summary.highest(kind)));
            }
        });
        ui::spacing(ui_sys);
        let count_line = |label: &str, count: usize| {
            ui::horizontal(ui_sys, |ui_sys| {
                ui::text(ui_sys, format_small!("{label} :"));
                if count != 0 {
                    ui::text_colored(ui_sys, Color::red(), format_small!("{count}"));
                } else {
                    ui::text(ui_sys, "0");
                }
            });
        };
        count_line("Burning", summary.fires.len());
        count_line("Sick", summary.outbreaks.len());
        count_line("Deaths From Sickness", risks.disease_deaths() as usize);

        // A row per building: its name, `details`, and its cell as a button that moves the camera
        // there. Each list scopes its buttons' paths, e.g. "World Debug/Risks/Outbreaks/8,18".
        let mut go_to = None;
        let mut building_rows = |list: &str, entries: &[RiskEntry], details: &dyn Fn(&RiskEntry) -> String| {
            let _path = ui::path_scope(ui_sys, list);
            ui::grid(ui_sys, &format!("Risks {list}"), 3, |grid| {
                for entry in entries {
                    grid.text(entry.name);
                    grid.text(details(entry).as_str());
                    grid.cell(|ui_sys| {
                        if ui::small_button(ui_sys, &format!("{},{}", entry.cell.x, entry.cell.y)) {
                            go_to = Some(entry.cell);
                        }
                        ui::item_tooltip_text(ui_sys, "Move the camera here");
                    });
                }
            });
        };

        if !summary.fires.is_empty() {
            ui::separator(ui_sys);
            ui::bullet_text(ui_sys, "Fires:");
            building_rows("Fires", &summary.fires, &|entry| {
                format!("{:.1}s left", entry.burning.as_ref().map_or(0.0, |fire| fire.secs_left))
            });
        }

        if !summary.outbreaks.is_empty() {
            ui::separator(ui_sys);
            ui::bullet_text(ui_sys, "Outbreaks:");
            building_rows("Outbreaks", &summary.outbreaks, &|entry| match &entry.outbreak {
                Some(outbreak) => format!("{:.0}s, {} died, {} left", outbreak.secs, outbreak.deaths, entry.population),
                None => String::new(),
            });
        }

        ui::separator(ui_sys);
        ui::bullet_text(ui_sys, format_small!("Riskiest (top {RISKIEST_LISTED}):"));
        if summary.riskiest.is_empty() {
            ui::text(ui_sys, "No buildings with risks.");
        }
        building_rows("Riskiest", &summary.riskiest, &|entry| {
            format!("F {:.0}  C {:.0}  D {:.0}", entry.fire, entry.collapse, entry.disease)
        });

        if let Some(cell) = go_to {
            camera.teleport(cell);
        }
    }
}

// The Faith tab: the game's realm, the switches, each patron's favour as a slider, with its mood and
// a button per blessing (+) and misfortune (-) to set it off, and the effects under way.
fn draw_faith_debug_ui(faith: &mut CityFaith, systems: FaithSystems, ui_sys: &UiSystem) {
    let realm = faith.realm();
    ui::text(ui_sys, format_small!("Realm : {realm}"));

    ui::bullet_text(ui_sys, "Switches:");
    let mut favour_enabled = faith.is_favour_enabled();
    if ui::checkbox(ui_sys, "Favour Enabled", &mut favour_enabled) {
        faith.set_favour_enabled(favour_enabled);
    }
    let mut festivals_enabled = faith.is_festivals_enabled();
    if ui::checkbox(ui_sys, "Festivals Enabled", &mut festivals_enabled) {
        faith.set_festivals_enabled(festivals_enabled);
    }
    ui::separator(ui_sys);

    ui::bullet_text(ui_sys, "Patrons:");
    let Some(def) = FaithConfigs::get().for_realm(realm) else {
        ui::text(ui_sys, "No patrons: the realm's faith file didn't load.");
        return;
    };
    for patron in &def.patrons {
        let Some(mut favour) = faith.favour(&patron.id) else {
            continue;
        };
        let label = format!("{}##{}", faith_config::patron_name(realm, &patron.id), patron.id);
        if ui::slider(ui_sys, &label, 0.0, 100.0, UiNumberFormat::Decimals(1), &mut favour) {
            faith.set_favour(&patron.id, favour);
        }
        let months = faith.patron(&patron.id).map_or(0, |state| state.months_since_event);
        ui::text(ui_sys, format_small!("    {}, {months} months since an event", Mood::from_favour(favour).display_name()));

        // A button per event that can happen in this game, set off at the start of the next frame
        // (faith::events::request): the window draws while the world is borrowed.
        ui::horizontal(ui_sys, |ui_sys| {
            for (id, blessing) in
                patron.blessings.iter().map(|id| (id, true)).chain(patron.misfortunes.iter().map(|id| (id, false)))
            {
                if def.event(id).is_none_or(|event| !event.is_available(systems)) {
                    continue;
                }
                let label = format!("{}{id}##{}_{id}", if blessing { "+ " } else { "- " }, patron.id);
                if ui::small_button(ui_sys, &label) {
                    faith_events::request(&patron.id, id);
                }
            }
        });
    }

    if !faith.effects().is_empty() {
        ui::separator(ui_sys);
        ui::bullet_text(ui_sys, "Effects under way:");
        for effect in faith.effects() {
            ui::text(ui_sys, format_small!("{} ({}): {} months left", effect.event, effect.patron, effect.months_left));
        }
        ui::text(
            ui_sys,
            format_small!("Farms x{:.2}, upkeep x{:.2}", faith.production_scale(BuildingKind::Farm), faith.upkeep_scale()),
        );
    }
}

// The Trade tab: the switch, the partners (their routes, open or not, a button to open or close one
// for free, `now` being the calendar's ordinal month, the next arrival, their limits and the prices
// trade has moved) and the requests.
fn draw_trade_debug_ui(city_trade: &mut CityTrade, now: u32, ui_sys: &UiSystem) {
    let mut enabled = city_trade.is_enabled();
    if ui::checkbox(ui_sys, "Trade Enabled", &mut enabled) {
        city_trade.set_enabled(enabled);
    }
    ui::text(ui_sys, format_small!("World map: {}", city_trade.world_map()));
    if city_trade.partners().is_empty() {
        ui::text(ui_sys, "No trade partners in this game.");
    }
    let mut toggle: Option<(String, bool)> = None;
    for partner in city_trade.partners() {
        ui::separator(ui_sys);
        let terms = trade::terms(city_trade, partner.city());
        let (kind, every, cost) = terms
            .as_ref()
            .map_or(("?".to_string(), 0, 0), |terms| (terms.route_kind.to_string(), terms.every_months, terms.open_cost));
        ui::text(
            ui_sys,
            format_small!(
                "{}: {} route, every {} months, opens for {} gold: {}",
                partner.city(),
                kind,
                every,
                cost,
                if partner.open { "open" } else { "closed" }
            ),
        );
        if let Some(month) = partner.next_arrival {
            ui::text(ui_sys, format_small!("    Next sets out in year {} month {}", month / 12, month % 12 + 1));
        }
        let label = if partner.open { "Close Route" } else { "Open Route (free)" };
        if ui::small_button(ui_sys, &format!("{label}##trade_{}", partner.city())) {
            toggle = Some((partner.city().to_string(), !partner.open));
        }
        for good in crate::sim::resources::ResourceKind::all().iter() {
            match partner.limit(good) {
                TradeLimit::Off => {}
                TradeLimit::Import(amount) => ui::text(ui_sys, format_small!("    {}: import up to {amount}", good.name())),
                TradeLimit::Export(amount) => ui::text(ui_sys, format_small!("    {}: export above {amount}", good.name())),
            }
            let offset = partner.price_offset(good);
            if offset != 0.0 {
                ui::text(ui_sys, format_small!("    {}: price {:+.1}%", good.name(), offset * 100.0));
            }
        }
    }
    if let Some((city, open)) = toggle {
        if open {
            city_trade.open_route(&city, now, &crate::config::GameConfigs::get().sim.trade);
        } else {
            city_trade.close_route(&city);
        }
    }
    ui::separator(ui_sys);
    ui::text(
        ui_sys,
        format_small!(
            "Requests under way: {}, met: {} of {}",
            city_trade.requests().len(),
            city_trade.requests_met(),
            city_trade.request_log().len()
        ),
    );
}

// The Floods tab: the switch, a flood now and the water going (each run at the start of the next
// frame, floods::request), the game's flood, the next flood's severity, the flood under way, the log
// and the land's fertility.
fn draw_floods_debug_ui(city_floods: &mut CityFloods, camera: &mut Camera, ui_sys: &UiSystem) {
    let mut enabled = city_floods.is_enabled();
    if ui::checkbox(ui_sys, "Floods Enabled", &mut enabled) {
        floods::request(floods::Request::SetEnabled(enabled));
    }
    let profile = city_floods.profile();
    ui::text(
        ui_sys,
        format_small!(
            "Month {}, reach {} to {} cells, {} months under water",
            profile.month,
            profile.reach[0],
            profile.reach[1],
            profile.months
        ),
    );
    ui::text(ui_sys, format_small!("Sources: {}", profile.sources.len()));
    for &cell in &profile.sources {
        if ui::small_button(ui_sys, &format!("{cell}##flood_source_{}_{}", cell.x, cell.y)) {
            camera.teleport(cell);
        }
    }

    let mut severity = city_floods.next_severity();
    if ui::slider(ui_sys, "Next Severity", 0.0, 2.0, UiNumberFormat::Decimals(2), &mut severity) {
        city_floods.set_next_severity(severity);
    }

    ui::separator(ui_sys);
    match city_floods.flood() {
        Some(flood) => {
            ui::text(
                ui_sys,
                format_small!(
                    "Under water: {} cells, {} months left, severity {:.2}",
                    flood.cells.len(),
                    flood.months_left,
                    flood.severity
                ),
            );
            if ui::button(ui_sys, "Recede Now") {
                floods::request(floods::Request::Recede);
            }
        }
        None => {
            ui::text(ui_sys, "No flood under way.");
            if city_floods.is_enabled() && ui::button(ui_sys, "Flood Now") {
                floods::request(floods::Request::FloodNow);
            }
        }
    }
    ui::text(
        ui_sys,
        format_small!("Floods: {}, without a breach: {}", city_floods.log().len(), city_floods.floods_survived()),
    );
    for record in city_floods.log().iter().rev() {
        ui::text(
            ui_sys,
            format_small!(
                "    Year {} month {}: reach {}, {} cells, {} buildings damaged, {} fields, {} breaches",
                record.ordinal_month / 12,
                record.ordinal_month % 12 + 1,
                record.reach,
                record.cells,
                record.buildings_damaged,
                record.fields_flooded,
                record.breaches
            ),
        );
    }
    // The dykes, the worn ones first, each with its cell as a button that moves the camera there.
    let worn_wear = crate::config::GameConfigs::get().sim.floods.worn_wear;
    ui::text(ui_sys, format_small!("Dykes: {}, worn: {}", city_floods.dyke_wear().len(), city_floods.worn_dykes(worn_wear)));
    let mut dykes: Vec<_> = city_floods.dyke_wear().to_vec();
    dykes.sort_by(|a, b| b.wear.total_cmp(&a.wear));
    for dyke in dykes.iter().take(8) {
        let cell = dyke.cell;
        let label = format!("{cell}: wear {:.0}##dyke_{}_{}", dyke.wear, cell.x, cell.y);
        if ui::small_button(ui_sys, &label) {
            camera.teleport(cell);
        }
    }
    match city_floods.fertility().mean() {
        Some(mean) => ui::text(ui_sys, format_small!("Fertility: mean {mean:.1}")),
        None => ui::text(ui_sys, format_small!("Fertility: not kept ({} everywhere)", city_floods.neutral_fertility())),
    }
}

impl WorldStats {
    pub(crate) fn draw_debug_ui(&mut self, tab: usize, finance: &mut Finance, ui_sys: &UiSystem) {
        let highlight_value = |label: &str, value: u32, highlight: bool, color: Color| {
            ui::horizontal(ui_sys, |ui_sys| {
                ui::text(ui_sys, format_small!("{label} :"));
                if highlight {
                    ui::text_colored(ui_sys, color, format_small!("{value}"));
                } else {
                    ui::text(ui_sys, format_small!("{value}"));
                }
            });
        };

        let highlight_zero_value = |label: &str, value: u32, color: Color| highlight_value(label, value, value == 0, color);
        let highlight_nonzero_value =
            |label: &str, value: u32, color: Color| highlight_value(label, value, value != 0, color);

        if tab == 0 {
            ui::bullet_text(ui_sys, "Population:");
            ui::spacing(ui_sys);
            {
                let employment_percent = self.population.employment_ratio() * 100.0;
                let unemployment_percent = self.population.unemployment_ratio() * 100.0;

                ui::text(ui_sys, format_small!("Total : {}", self.population.total));
                ui::spacing(ui_sys);
                ui::text(ui_sys, format_small!("Employed : {}", self.population.employed));
                ui::text(ui_sys, format_small!("Employment : {employment_percent:.2}%"));
                ui::spacing(ui_sys);
                ui::text(ui_sys, format_small!("Unemployed : {}", self.population.unemployed));
                ui::text(ui_sys, format_small!("Unemployment : {unemployment_percent:.2}%"));
            }
            ui::separator(ui_sys);

            ui::bullet_text(ui_sys, "Workers:");
            ui::spacing(ui_sys);
            {
                ui::text(ui_sys, format_small!("Total : {}", self.workers.total));
                ui::spacing(ui_sys);
                ui::text(ui_sys, format_small!("Min Required : {}", self.workers.min_required));
                ui::text(ui_sys, format_small!("Max Employed : {}", self.workers.max_employed));
                ui::spacing(ui_sys);
                highlight_nonzero_value("Buildings Below Min", self.workers.buildings_below_min, Color::red());
                highlight_nonzero_value("Buildings Below Max", self.workers.buildings_below_max, Color::yellow());
            }
            ui::separator(ui_sys);

            if self.houses.total != 0 {
                ui::bullet_text(ui_sys, "Housing:");
                ui::spacing(ui_sys);
                ui::text(ui_sys, format_small!("Number Of Houses    : {}", self.houses.total));
                ui::text(ui_sys, format_small!("Lowest House Level  : {}", self.houses.lowest_level as u32));
                ui::text(ui_sys, format_small!("Highest House Level : {}", self.houses.highest_level as u32));
            }
        }

        if tab == 1 {
            ui::bullet_text(ui_sys, "Tax:");
            highlight_zero_value("Tax Generated", self.treasury.tax_generated, Color::red());
            highlight_nonzero_value("Tax Available", self.treasury.tax_available, Color::yellow());
            highlight_zero_value("Tax Collected", self.treasury.tax_collected, Color::yellow());

            ui::separator(ui_sys);

            ui::bullet_text(ui_sys, "Treasury:");
            let balance = finance.balance();
            ui::horizontal(ui_sys, |ui_sys| {
                ui::text(ui_sys, "Balance :");
                if balance <= 0 {
                    ui::text_colored(ui_sys, Color::red(), format_small!("{balance}"));
                } else {
                    ui::text(ui_sys, format_small!("{balance}"));
                }
            });
            highlight_nonzero_value("Months In Debt", finance.months_in_debt(), Color::red());

            ui::separator(ui_sys);

            static GOLD_UNITS: UiStaticVar<u32> = UiStaticVar::new(0);
            ui::input_scalar(ui_sys, "Gold Units", GOLD_UNITS.as_mut(), 100);

            // Cheats: they change the balance without recording it in the accounts.
            if ui::button(ui_sys, "Give Gold") {
                finance.add_unrecorded(i64::from(*GOLD_UNITS));
            }

            if ui::button(ui_sys, "Subtract Gold") {
                finance.add_unrecorded(-i64::from(*GOLD_UNITS));
            }
        }

        if tab == 2 {
            let resources = &mut self.resources;
            resources.all.draw_debug_ui_with_header("All Resources", ui_sys);

            ui::separator(ui_sys);

            ui::text(ui_sys, "In Storage:");
            resources.storage_yards.draw_debug_ui_with_header("Storage Yards", ui_sys);
            resources.granaries.draw_debug_ui_with_header("Granaries", ui_sys);

            ui::separator(ui_sys);

            ui::text(ui_sys, "Buildings:");
            resources.houses.draw_debug_ui_with_header("Houses", ui_sys);
            resources.producers.draw_debug_ui_with_header("Producers", ui_sys);
            resources.services.draw_debug_ui_with_header("Services", ui_sys);

            ui::separator(ui_sys);

            ui::text(ui_sys, "Other:");
            resources.units.draw_debug_ui_with_header("Units", ui_sys);
            resources.markets.draw_debug_ui_with_header("Markets", ui_sys);
        }
    }
}
