use common::{Color, format_small};
use engine::ui::UiSystem;

use crate::{
    building::BuildingKind,
    cheats,
    sim::resources::{Employer, HouseholdWorkerPool, Workers},
    world::{World, object::GameObject},
};

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
    let ui = ui_sys.ui();
    ui.text(format_small!("Employed   : {}", pool.employed_count()));
    ui.text(format_small!("Unemployed : {}", pool.unemployed_count()));
    ui.text(format_small!("Total      : {}", pool.total_workers()));

    let mut employers = pool.employers_iter().peekable();
    if employers.peek().is_some() {
        ui.text("Employers:");
        ui.indent_by(10.0);

        for (employer_info, employed_count) in employers {
            if let Some(employer) = world.find_building(employer_info.kind, employer_info.id) {
                ui.text(format_small!(
                    "- {} cell={} id={}: {}",
                    employer.name(),
                    employer.base_cell(),
                    employer.id(),
                    employed_count
                ));
            } else {
                ui.text_colored(Color::red().to_array(), "<unknown employer record>");
            }
        }

        ui.unindent_by(10.0);
    }
}

fn draw_employer_debug_ui(employer: &Employer, world: &World, ui_sys: &UiSystem) {
    let ui = ui_sys.ui();
    ui.text(format_small!("Workers Employed : {}", employer.employee_count()));
    ui.text(format_small!("Min Required     : {}", employer.min_employees()));
    ui.text(format_small!("Max Employed     : {}", employer.max_employees()));
    ui.text(format_small!("Work Efficiency  : {:.0}%", employer.work_efficiency() * 100.0));

    if cheats::get().ignore_worker_requirements {
        ui.text_colored(Color::green().to_array(), "CHEAT ignore_worker_requirements ON");
    } else if employer.is_below_min_required() {
        ui.text_colored(Color::red().to_array(), "Below Min Required Workers");
    } else if employer.is_at_max_capacity() {
        ui.text_colored(Color::green().to_array(), "Has All Required Workers");
    }

    let mut households = employer.employee_households_iter().peekable();
    if households.peek().is_some() {
        ui.text("Worker Households:");
        ui.indent_by(10.0);

        for (house_id, employee_count) in households {
            if let Some(house) = world.find_building(BuildingKind::House, house_id) {
                ui.text(format_small!("- {} cell={} id={}: {}", house.name(), house.base_cell(), house.id(), employee_count));
            } else {
                ui.text_colored(Color::red().to_array(), "<unknown employee household>");
            }
        }

        ui.unindent_by(10.0);
    }
}
