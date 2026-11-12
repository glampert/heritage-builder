use common::{Color, format_small};
use engine::ui::{self, DrawDebugUi, UiStaticVar, UiSystem};

use crate::{
    building::risks::{RiskEntry, RiskKind, RiskSummary, RISK_HIGH},
    camera::Camera,
    cheats,
    sim::{Finance, risks::CityRisks},
    world::{World, stats::WorldStats},
};

// How many of the riskiest buildings the Risks tab lists.
const RISKIEST_LISTED: usize = 10;

// ----------------------------------------------
// World / WorldStats Debug UI
// ----------------------------------------------

impl World {
    pub(crate) fn draw_debug_ui(&mut self, finance: &mut Finance, risks: &mut CityRisks, camera: &mut Camera, ui_sys: &UiSystem) {
        const TABS: [&str; 4] = ["Population/Workers/Housing", "Tax/Treasury", "Resources", "Risks"];
        let Some(tab) = ui::tab_bar(ui_sys, "World Stats Tab Bar", &TABS) else {
            return;
        };

        if tab.index == 3 {
            self.draw_risks_debug_ui(risks, camera, ui_sys);
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
        ui::text(ui_sys, format_small!("Risks: {}, disease: {}", state(risks.is_active()), state(risks.is_disease_active())));

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
        let highlight_nonzero_value = |label: &str, value: u32, color: Color| highlight_value(label, value, value != 0, color);

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
