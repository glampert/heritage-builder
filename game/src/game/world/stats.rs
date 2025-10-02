use crate::{
    game::{
        building::HouseLevel,
        sim::resources::{GlobalTreasury, ResourceKind, ResourceStock},
    },
    imgui_ui::UiSystem,
    utils::Color,
};

// ----------------------------------------------
// WorldStats
// ----------------------------------------------

#[derive(Default)]
pub struct PopulationStats {
    pub total: u32,
    pub employed: u32,
    pub unemployed: u32,
}

#[derive(Default)]
pub struct WorkerStats {
    pub total: u32,
    pub min_required: u32,
    pub max_employed: u32,
    pub buildings_below_min: u32,
    pub buildings_below_max: u32,
}

#[derive(Default)]
pub struct TreasuryStats {
    pub gold_units_total: u32,
    pub gold_units_in_buildings: u32,
    pub tax_generated: u32,
    pub tax_available: u32,
    pub tax_collected: u32,
}

struct HousingStats {
    total: u32,
    lowest_level: HouseLevel,
    highest_level: HouseLevel,
}

struct GlobalResourceCounts {
    // Combined sum of resources (all units + all buildings).
    all: ResourceStock,

    // Resources held by spawned units.
    units: ResourceStock,

    // Resources held by each kind of building.
    storage_yards: ResourceStock,
    granaries: ResourceStock,
    houses: ResourceStock,
    markets: ResourceStock,
    producers: ResourceStock,
    services: ResourceStock,
}

pub struct WorldStats {
    // Global counts:
    pub population: PopulationStats,
    pub workers: WorkerStats,
    pub treasury: TreasuryStats,

    // Housing stats:
    houses: HousingStats,

    // Global resource tally:
    resources: GlobalResourceCounts,
}

impl Default for WorldStats {
    fn default() -> Self {
        Self {
            population: PopulationStats::default(),
            workers: WorkerStats::default(),
            treasury: TreasuryStats::default(),
            houses: HousingStats {
                total: 0,
                lowest_level: HouseLevel::max(),
                highest_level: HouseLevel::min(),
            },
            resources: GlobalResourceCounts {
                all: ResourceStock::accept_all(),
                units: ResourceStock::accept_all(),
                storage_yards: ResourceStock::accept_all(),
                granaries: ResourceStock::with_accepted_kinds(ResourceKind::foods()),
                houses: ResourceStock::with_accepted_kinds(ResourceKind::foods() | ResourceKind::consumer_goods()),
                markets: ResourceStock::with_accepted_kinds(ResourceKind::foods() | ResourceKind::consumer_goods()),
                producers: ResourceStock::accept_all(),
                services: ResourceStock::accept_all(),
            },
        }
    }
}

impl WorldStats {
    pub fn reset(&mut self) {
        // Reset all counts to zero.
        *self = Self::default();
    }

    pub fn add_unit_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.units.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_storage_yard_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.storage_yards.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_granary_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.granaries.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_house_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.houses.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_market_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.markets.add(kind, count);
            self.resources.services.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_producer_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.producers.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn add_service_resources(&mut self, kind: ResourceKind, count: u32) {
        if count != 0 {
            self.resources.services.add(kind, count);
            self.resources.all.add(kind, count);
        }
    }

    pub fn update_housing_stats(&mut self, level: HouseLevel) {
        if level < self.houses.lowest_level {
            self.houses.lowest_level = level;
        }
        if level > self.houses.highest_level {
            self.houses.highest_level = level;
        }
        self.houses.total += 1;
    }

    pub fn draw_debug_ui(&self, treasury: &mut GlobalTreasury, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

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
                let (employment_percentage, unemployment_percentage) = {
                    if self.population.total != 0 {
                        (((self.population.employed as f32) / (self.population.total as f32))
                         * 100.0,
                         ((self.population.unemployed as f32) / (self.population.total as f32))
                         * 100.0)
                    } else {
                        (0.0, 0.0)
                    }
                };

                ui.text(format!("Total : {}", self.population.total));
                ui.spacing();
                ui.text(format!("Employed : {}", self.population.employed));
                ui.text(format!("Employment : {employment_percentage:.2}%"));
                ui.spacing();
                ui.text(format!("Unemployed : {}", self.population.unemployed));
                ui.text(format!("Unemployment : {unemployment_percentage:.2}%"));
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
                highlight_nonzero_value("Buildings Below Min",
                                        self.workers.buildings_below_min,
                                        Color::red());
                highlight_nonzero_value("Buildings Below Max",
                                        self.workers.buildings_below_max,
                                        Color::yellow());
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
            highlight_zero_value("Gold In Buildings",
                                 self.treasury.gold_units_in_buildings,
                                 Color::gray());

            ui.separator();

            #[allow(static_mut_refs)]
            let gold_units = unsafe {
                static mut GOLD_UNITS: u32 = 0;
                ui.input_scalar("Gold Units", &mut GOLD_UNITS).step(100).build();
                GOLD_UNITS
            };

            if ui.button("Give Gold") {
                treasury.add_gold_units(gold_units);
            }

            if ui.button("Subtract Gold") {
                treasury.subtract_gold_units(gold_units);
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
