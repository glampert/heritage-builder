use common::{Color, format_small};
use engine::ui::{self, DrawDebugUi, UiStaticVar, UiSystem};

use crate::{
    sim::resources::GlobalTreasury,
    world::{World, stats::WorldStats},
};

// ----------------------------------------------
// World / WorldStats Debug UI
// ----------------------------------------------

impl World {
    pub(crate) fn draw_debug_ui(&mut self, treasury: &mut GlobalTreasury, ui_sys: &UiSystem) {
        self.stats_mut().draw_debug_ui(treasury, ui_sys);
    }
}

impl WorldStats {
    pub(crate) fn draw_debug_ui(&mut self, treasury: &mut GlobalTreasury, ui_sys: &UiSystem) {
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

        const TABS: [&str; 3] = ["Population/Workers/Housing", "Tax/Treasury", "Resources"];
        let Some(tab) = ui::tab_bar(ui_sys, "World Stats Tab Bar", &TABS) else {
            return;
        };

        if tab.index == 0 {
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

        if tab.index == 1 {
            ui::bullet_text(ui_sys, "Tax:");
            highlight_zero_value("Tax Generated", self.treasury.tax_generated, Color::red());
            highlight_nonzero_value("Tax Available", self.treasury.tax_available, Color::yellow());
            highlight_zero_value("Tax Collected", self.treasury.tax_collected, Color::yellow());

            ui::separator(ui_sys);

            ui::bullet_text(ui_sys, "Treasury:");
            highlight_zero_value("Total Gold Units", self.treasury.gold_units_total, Color::red());
            highlight_zero_value("Gold In Global Treasury", treasury.gold_units(), Color::red());
            highlight_zero_value("Gold In Buildings", self.treasury.gold_units_in_buildings, Color::gray());

            ui::separator(ui_sys);

            static GOLD_UNITS: UiStaticVar<u32> = UiStaticVar::new(0);
            ui::input_scalar(ui_sys, "Gold Units", GOLD_UNITS.as_mut(), 100);

            if ui::button(ui_sys, "Give Gold") {
                treasury.add_gold_units(*GOLD_UNITS);
            }

            if ui::button(ui_sys, "Subtract Gold") {
                treasury.subtract_gold_units(*GOLD_UNITS);
            }
        }

        if tab.index == 2 {
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
