use common::format_fixed_string;
use engine::log;
use strum::EnumCount;

use super::*;

// ----------------------------------------------
// Enums / Constants
// ----------------------------------------------

#[repr(usize)]
#[derive(EnumCount)]
enum PopulationStatsIdx {
    TotalPopulation,
    Employed,
    Unemployed,
    EmploymentRate,
    UnemploymentRate,
}

#[repr(usize)]
#[derive(EnumCount)]
enum WorkerStatsIdx {
    TotalWorkforce,
    MinWorkersRequired,
    MaxWorkersEmployed,
    BuildingsBelowMinWorkers,
    BuildingsBelowMaxWorkers,
}

// ----------------------------------------------
// PopulationManagement
// ----------------------------------------------

pub struct PopulationManagement {
    menu: UiMenuRcMut,
    population_stats_heading_index: UiMenuWidgetIndex,
    worker_stats_heading_index: UiMenuWidgetIndex,
}

implement_dialog_menu! { PopulationManagement, ["Population"] }

impl PopulationManagement {
    pub fn new(context: &mut GameUiContext) -> Self {
        // Population stats placeholder text.
        const POPULATION_STATS_TEXT: [UiText; PopulationStatsIdx::COUNT] = [
            PLACEHOLDER_HEADING,
            PLACEHOLDER_BODY,
            PLACEHOLDER_BODY,
            PLACEHOLDER_BODY,
            PLACEHOLDER_BODY,
        ];

        // Workforce stats placeholder text.
        const WORKER_STATS_TEXT: [UiText; WorkerStatsIdx::COUNT] = [
            PLACEHOLDER_HEADING,
            PLACEHOLDER_BODY,
            PLACEHOLDER_BODY,
            PLACEHOLDER_BODY,
            PLACEHOLDER_BODY,
        ];

        let mut menu =
            make_default_layout_dialog_menu(context, Self::KIND, Self::TITLE, DEFAULT_DIALOG_MENU_TEXT_SPACING, []);

        let population_stats_heading =
            UiMenuHeading::new(POPULATION_STATS_TEXT).with_separator(context, LARGE_HORIZONTAL_SEPARATOR_SPRITE);
        let population_stats_heading_index = menu.add_widget(population_stats_heading);

        let worker_stats_heading =
            UiMenuHeading::new(WORKER_STATS_TEXT).with_separator(context, LARGE_HORIZONTAL_SEPARATOR_SPRITE);
        let worker_stats_heading_index = menu.add_widget(worker_stats_heading);

        menu.add_widget(UiWidgetGroup::new().with_widget(make_dialog_close_button(context, "Ok")));

        // Refresh body text when menu is opened.
        menu.set_open_close_callback(|_, context, is_open| {
            if is_open {
                let this_dialog = super::find::<PopulationManagement>();
                this_dialog.update_stats(context_as_mut::<GameUiContext>(context));
            }
        });

        Self { menu, population_stats_heading_index, worker_stats_heading_index }
    }

    fn update_stats(&mut self, context: &GameUiContext) {
        let world_stats = context.world.stats();
        debug_assert!(world_stats.population.is_valid());

        if world_stats.population.workforce() != world_stats.workers.total {
            log::error!(
                log::channel!("game"),
                "Population vs Workforce stats mismatch: Population Workforce: {}, Workers Total: {}",
                world_stats.population.workforce(),
                world_stats.workers.total
            );
        }

        const FMT_LEN: usize = 128;

        {
            let heading = self.menu.widget_as_mut::<UiMenuHeading>(self.population_stats_heading_index).unwrap();

            let population = &world_stats.population;

            let employment_percent   = population.employment_ratio()   * 100.0;
            let unemployment_percent = population.unemployment_ratio() * 100.0;

            heading.set_line_string(
                PopulationStatsIdx::TotalPopulation as usize,
                &format_fixed_string!(FMT_LEN, "Total Population: {}", population.total),
            );

            heading.set_line_string(
                PopulationStatsIdx::Employed as usize,
                &format_fixed_string!(FMT_LEN, "Employed: {}", population.employed),
            );

            heading.set_line_string(
                PopulationStatsIdx::Unemployed as usize,
                &format_fixed_string!(FMT_LEN, "Unemployed: {}", population.unemployed),
            );

            heading.set_line_string(
                PopulationStatsIdx::EmploymentRate as usize,
                &format_fixed_string!(FMT_LEN, "Employment Rate: {}%", employment_percent.round() as u32),
            );

            heading.set_line_string(
                PopulationStatsIdx::UnemploymentRate as usize,
                &format_fixed_string!(FMT_LEN, "Unemployment Rate: {}%", unemployment_percent.round() as u32),
            );
        }

        {
            let heading = self.menu.widget_as_mut::<UiMenuHeading>(self.worker_stats_heading_index).unwrap();

            let workers = &world_stats.workers;

            heading.set_line_string(
                WorkerStatsIdx::TotalWorkforce as usize,
                &format_fixed_string!(FMT_LEN, "Total Workforce: {}", workers.total),
            );

            heading.set_line_string(
                WorkerStatsIdx::MinWorkersRequired as usize,
                &format_fixed_string!(FMT_LEN, "Min Workers Required: {}", workers.min_required),
            );

            heading.set_line_string(
                WorkerStatsIdx::MaxWorkersEmployed as usize,
                &format_fixed_string!(FMT_LEN, "Max Workers Employed: {}", workers.max_employed),
            );

            heading.set_line_string(
                WorkerStatsIdx::BuildingsBelowMinWorkers as usize,
                &format_fixed_string!(FMT_LEN, "Buildings Below Min Workers: {}", workers.buildings_below_min),
            );

            heading.set_line_string(
                WorkerStatsIdx::BuildingsBelowMaxWorkers as usize,
                &format_fixed_string!(FMT_LEN, "Buildings Below Max Workers: {}", workers.buildings_below_max),
            );
        }
    }
}
