use strum::EnumCount;

use super::*;
use crate::{
    log,
    utils::fixed_string::format_fixed_string,
    game::menu::TEXT_BUTTON_HOVERED_SPRITE,
};

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
    pub fn new(context: &mut UiWidgetContext) -> Self {
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

        let mut menu = make_default_layout_dialog_menu(
            context,
            Self::KIND,
            Self::TITLE,
            DEFAULT_DIALOG_MENU_WIDGET_SPACING,
            Option::<Vec<UiWidgetImpl>>::None
        );

        let population_stats_heading = UiMenuHeading::new(
            context,
            UiMenuHeadingParams {
                lines: POPULATION_STATS_TEXT.into(),
                separator: Some(LARGE_HORIZONTAL_SEPARATOR_SPRITE),
                ..Default::default()
            }
        );

        let population_stats_heading_index = menu.add_widget(population_stats_heading);

        let worker_stats_heading = UiMenuHeading::new(
            context,
            UiMenuHeadingParams {
                lines: WORKER_STATS_TEXT.into(),
                separator: Some(LARGE_HORIZONTAL_SEPARATOR_SPRITE),
                ..Default::default()
            }
        );

        let worker_stats_heading_index = menu.add_widget(worker_stats_heading);

        let mut button_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                center_vertically: false,
                ..Default::default()
            }
        );

        let ok_button = UiTextButton::new(
            context,
            UiTextButtonParams {
                label: "Ok".into(),
                hover: Some(TEXT_BUTTON_HOVERED_SPRITE),
                sounds_enabled: UiButtonSoundsEnabled::all(),
                on_pressed: UiTextButtonPressed::with_fn(|_, context| {
                    super::close_current(context);
                }),
                ..Default::default()
            }
        );

        button_group.add_widget(ok_button);
        menu.add_widget(button_group);

        // Refresh body text when menu is opened.
        menu.set_open_close_callback(UiMenuOpenClose::with_fn(
            |_, context, is_open| {
                if is_open {
                    let this_dialog = super::find::<PopulationManagement>();
                    this_dialog.update_stats(context);
                }
            }
        ));

        Self {
            menu,
            population_stats_heading_index,
            worker_stats_heading_index,
        }
    }

    fn update_stats(&mut self, context: &UiWidgetContext) {
        let world_stats = context.world.stats();
        debug_assert!(world_stats.population.is_valid());

        if world_stats.population.workforce() != world_stats.workers.total {
            log::error!(log::channel!("game"),
                        "Population vs Workforce stats mismatch: Population Workforce: {}, Workers Total: {}",
                        world_stats.population.workforce(), world_stats.workers.total);
        }

        const FMT_LEN: usize = 128;

        {
            let heading =
                self.menu.widget_as_mut::<UiMenuHeading>(self.population_stats_heading_index)
                .unwrap();

            let population = &world_stats.population;

            let employment_percent   = population.employment_ratio()   * 100.0;
            let unemployment_percent = population.unemployment_ratio() * 100.0;

            heading.set_line_string(
                PopulationStatsIdx::TotalPopulation as usize,
                &format_fixed_string!(FMT_LEN, "Total Population: {}", population.total));

            heading.set_line_string(
                PopulationStatsIdx::Employed as usize,
                &format_fixed_string!(FMT_LEN, "Employed: {}", population.employed));

            heading.set_line_string(
                PopulationStatsIdx::Unemployed as usize,
                &format_fixed_string!(FMT_LEN, "Unemployed: {}", population.unemployed));

            heading.set_line_string(
                PopulationStatsIdx::EmploymentRate as usize,
                &format_fixed_string!(FMT_LEN, "Employment Rate: {}%", employment_percent.round() as u32));

            heading.set_line_string(
                PopulationStatsIdx::UnemploymentRate as usize,
                &format_fixed_string!(FMT_LEN, "Unemployment Rate: {}%", unemployment_percent.round() as u32));
        }

        {
            let heading =
                self.menu.widget_as_mut::<UiMenuHeading>(self.worker_stats_heading_index)
                .unwrap();

            let workers = &world_stats.workers;

            heading.set_line_string(
                WorkerStatsIdx::TotalWorkforce as usize,
                &format_fixed_string!(FMT_LEN, "Total Workforce: {}", workers.total));

            heading.set_line_string(
                WorkerStatsIdx::MinWorkersRequired as usize,
                &format_fixed_string!(FMT_LEN, "Min Workers Required: {}", workers.min_required));

            heading.set_line_string(
                WorkerStatsIdx::MaxWorkersEmployed as usize,
                &format_fixed_string!(FMT_LEN, "Max Workers Employed: {}", workers.max_employed));

            heading.set_line_string(
                WorkerStatsIdx::BuildingsBelowMinWorkers as usize,
                &format_fixed_string!(FMT_LEN, "Buildings Below Min Workers: {}", workers.buildings_below_min));

            heading.set_line_string(
                WorkerStatsIdx::BuildingsBelowMaxWorkers as usize,
                &format_fixed_string!(FMT_LEN, "Buildings Below Max Workers: {}", workers.buildings_below_max));
        }
    }
}
