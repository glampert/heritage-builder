use common::Color;
use engine::ui::{UiStaticVar, UiSystem};

use crate::{
    sim::resources::GlobalTreasury,
    world::{World, stats::WorldStats},
};

// ----------------------------------------------
// World / WorldStats Debug UI
// ----------------------------------------------

impl World {
    pub(crate) fn draw_debug_ui(&self, treasury: &mut GlobalTreasury, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if let Some(_tab_bar) = ui.tab_bar("World Stats Tab Bar") {
            self.stats().draw_debug_ui(treasury, ui_sys);
        }
    }
}

impl WorldStats {
    pub(crate) fn draw_debug_ui(&self, treasury: &mut GlobalTreasury, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let highlight_zero_value = |label: &str, value: u32, color: Color| {
            if value == 0 {
                ui.text(format!("{label} :"));
                ui.same_line();
                ui.text_colored(color.to_array(), format!("{value}"));
            } else {
                ui.text(format!("{label} :"));
                ui.same_line();
                ui.text(format!("{value}"));
            }
        };

        let highlight_nonzero_value = |label: &str, value: u32, color: Color| {
            if value != 0 {
                ui.text(format!("{label} :"));
                ui.same_line();
                ui.text_colored(color.to_array(), format!("{value}"));
            } else {
                ui.text(format!("{label} :"));
                ui.same_line();
                ui.text(format!("{value}"));
            }
        };

        if let Some(_tab) = ui.tab_item("Population/Workers/Housing") {
            ui.bullet_text("Population:");
            ui.spacing();
            {
                let employment_percent = self.population.employment_ratio() * 100.0;
                let unemployment_percent = self.population.unemployment_ratio() * 100.0;

                ui.text(format!("Total : {}", self.population.total));
                ui.spacing();
                ui.text(format!("Employed : {}", self.population.employed));
                ui.text(format!("Employment : {employment_percent:.2}%"));
                ui.spacing();
                ui.text(format!("Unemployed : {}", self.population.unemployed));
                ui.text(format!("Unemployment : {unemployment_percent:.2}%"));
            }
            ui.separator();

            ui.bullet_text("Workers:");
            ui.spacing();
            {
                ui.text(format!("Total : {}", self.workers.total));
                ui.spacing();
                ui.text(format!("Min Required : {}", self.workers.min_required));
                ui.text(format!("Max Employed : {}", self.workers.max_employed));
                ui.spacing();
                highlight_nonzero_value("Buildings Below Min", self.workers.buildings_below_min, Color::red());
                highlight_nonzero_value("Buildings Below Max", self.workers.buildings_below_max, Color::yellow());
            }
            ui.separator();

            if self.houses.total != 0 {
                ui.bullet_text("Housing:");
                ui.spacing();
                ui.text(format!("Number Of Houses    : {}", self.houses.total));
                ui.text(format!("Lowest House Level  : {}", self.houses.lowest_level as u32));
                ui.text(format!("Highest House Level : {}", self.houses.highest_level as u32));
            }
        }

        if let Some(_tab) = ui.tab_item("Tax/Treasury") {
            ui.bullet_text("Tax:");
            highlight_zero_value("Tax Generated", self.treasury.tax_generated, Color::red());
            highlight_nonzero_value("Tax Available", self.treasury.tax_available, Color::yellow());
            highlight_zero_value("Tax Collected", self.treasury.tax_collected, Color::yellow());

            ui.separator();

            ui.bullet_text("Treasury:");
            highlight_zero_value("Total Gold Units", self.treasury.gold_units_total, Color::red());
            highlight_zero_value("Gold In Global Treasury", treasury.gold_units(), Color::red());
            highlight_zero_value("Gold In Buildings", self.treasury.gold_units_in_buildings, Color::gray());

            ui.separator();

            static GOLD_UNITS: UiStaticVar<u32> = UiStaticVar::new(0);
            ui.input_scalar("Gold Units", GOLD_UNITS.as_mut()).step(100).build();

            if ui.button("Give Gold") {
                treasury.add_gold_units(*GOLD_UNITS);
            }

            if ui.button("Subtract Gold") {
                treasury.subtract_gold_units(*GOLD_UNITS);
            }
        }

        if let Some(_tab) = ui.tab_item("Resources") {
            let resources = &self.resources;
            resources.all.draw_debug_ui("All Resources", ui_sys);

            ui.separator();

            ui.text("In Storage:");
            resources.storage_yards.draw_debug_ui("Storage Yards", ui_sys);
            resources.granaries.draw_debug_ui("Granaries", ui_sys);

            ui.separator();

            ui.text("Buildings:");
            resources.houses.draw_debug_ui("Houses", ui_sys);
            resources.producers.draw_debug_ui("Producers", ui_sys);
            resources.services.draw_debug_ui("Services", ui_sys);

            ui.separator();

            ui.text("Other:");
            resources.units.draw_debug_ui("Units", ui_sys);
            resources.markets.draw_debug_ui("Markets", ui_sys);
        }
    }
}
