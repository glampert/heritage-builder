use common::{Color, format_small};
use engine::ui::{self, DrawDebugUi, UiSystem};

use crate::{
    building::BuildingKind,
    cheats,
    sim::resources::{Employer, HouseholdWorkerPool, ResourceStock, Workers},
    world::{World, object::GameObject},
};

// ----------------------------------------------
// ResourceStock Debug UI
// ----------------------------------------------

impl DrawDebugUi for ResourceStock {
    // Read-only debug display.
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        ui::indent(ui_sys, 5.0, |ui_sys| {
            self.for_each(|index, item| {
                let label = format_small!("{}##_stock_item_{}", item.kind, index);
                ui::input_text(ui_sys, &label, &mut item.count.to_string(), true);
            });
        });
    }
}

// ----------------------------------------------
// Workers Debug UI
// ----------------------------------------------

pub(crate) fn draw_workers_debug_ui(workers: &Workers, world: &World, ui_sys: &UiSystem) {
    if let Some(pool) = workers.as_household_worker_pool() {
        draw_household_worker_pool_debug_ui(pool, world, ui_sys);
    } else if let Some(employer) = workers.as_employer() {
        draw_employer_debug_ui(employer, world, ui_sys);
    }
}

fn draw_household_worker_pool_debug_ui(pool: &HouseholdWorkerPool, world: &World, ui_sys: &UiSystem) {
    ui::text(ui_sys, format_small!("Employed   : {}", pool.employed_count()));
    ui::text(ui_sys, format_small!("Unemployed : {}", pool.unemployed_count()));
    ui::text(ui_sys, format_small!("Total      : {}", pool.total_workers()));

    let mut employers = pool.employers_iter().peekable();
    if employers.peek().is_some() {
        ui::text(ui_sys, "Employers:");
        ui::indent(ui_sys, 10.0, |ui_sys| {
            for (employer_info, employed_count) in employers {
                if let Some(employer) = world.find_building(employer_info.kind, employer_info.id) {
                    ui::text(ui_sys, format_small!(
                        "- {} cell={} id={}: {}",
                        employer.name(),
                        employer.base_cell(),
                        employer.id(),
                        employed_count
                    ));
                } else {
                    ui::text_colored(ui_sys, Color::red(), "<unknown employer record>");
                }
            }
        });
    }
}

fn draw_employer_debug_ui(employer: &Employer, world: &World, ui_sys: &UiSystem) {
    ui::text(ui_sys, format_small!("Workers Employed : {}", employer.employee_count()));
    ui::text(ui_sys, format_small!("Min Required     : {}", employer.min_employees()));
    ui::text(ui_sys, format_small!("Max Employed     : {}", employer.max_employees()));
    ui::text(ui_sys, format_small!("Work Efficiency  : {:.0}%", employer.work_efficiency() * 100.0));

    if cheats::get().ignore_worker_requirements {
        ui::text_colored(ui_sys, Color::green(), "CHEAT ignore_worker_requirements ON");
    } else if employer.is_below_min_required() {
        ui::text_colored(ui_sys, Color::red(), "Below Min Required Workers");
    } else if employer.is_at_max_capacity() {
        ui::text_colored(ui_sys, Color::green(), "Has All Required Workers");
    }

    let mut households = employer.employee_households_iter().peekable();
    if households.peek().is_some() {
        ui::text(ui_sys, "Worker Households:");
        ui::indent(ui_sys, 10.0, |ui_sys| {
            for (house_id, employee_count) in households {
                if let Some(house) = world.find_building(BuildingKind::House, house_id) {
                    ui::text(ui_sys, format_small!("- {} cell={} id={}: {}", house.name(), house.base_cell(), house.id(), employee_count));
                } else {
                    ui::text_colored(ui_sys, Color::red(), "<unknown employee household>");
                }
            }
        });
    }
}
