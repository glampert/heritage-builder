use smallvec::SmallVec;
use bitflags::Flags;

use common::{
    Color,
    coords::{Cell, CellRange},
};
use engine::{
    log,
    ui::{UiFontScale, UiStaticVar, UiSystem},
};

use super::*;
use crate::debug::utils::UpdateTimerDebugUi;

// ----------------------------------------------
// Building Debug UI
// ----------------------------------------------

impl Building {
    pub(super) fn draw_debug_ui_overview(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let color_bullet_bool = |label: &str, value: bool| {
            ui.bullet_text(format!("{label}:"));
            ui.same_line();
            if value {
                ui.text("yes");
            } else {
                ui.text_colored(Color::red().to_array(), "no");
            }
        };

        let color_bullet_text = |label: &str, value: &str| {
            ui.bullet_text(format!("{label}:"));
            ui.same_line();
            ui.text_colored(Color::red().to_array(), value);
        };

        ui_sys.set_window_font_scale(UiFontScale(1.2));
        ui.text(format!("{} | ID{} @{}", self.name(), self.id(), self.base_cell()));
        ui_sys.set_window_font_scale(UiFontScale::default());

        color_bullet_bool("Linked to road", self.is_linked_to_road(context.sim_ctx));

        if self.archetype_kind() == BuildingArchetypeKind::HouseBuilding {
            let house = self.as_house();
            ui.bullet_text(format!("Tax: (generated: {}, avail: {})", house.tax_generated(), house.tax_available()));

            if !house.level().is_max() {
                let upgrade_requirements = house.upgrade_requirements(context);
                let has_required_resources = upgrade_requirements.has_required_resources();
                let has_required_services = upgrade_requirements.has_required_services();

                color_bullet_bool("Has resources to upgrade", has_required_resources);
                if !has_required_resources {
                    color_bullet_text("Missing", &upgrade_requirements.resources_missing().to_string());
                }

                color_bullet_bool("Has services to upgrade", has_required_services);
                if !has_required_services {
                    color_bullet_text("Missing", &upgrade_requirements.services_missing().to_string());
                }

                color_bullet_bool("Has room to upgrade", house.is_upgrade_available(context));
            } else {
                ui.bullet_text("Max house level reached");
            }
        } else {
            color_bullet_bool("Is operational", self.archetype().is_operational());
            color_bullet_bool("Has resources", self.archetype().has_min_required_resources());

            if self.archetype().has_stock() {
                color_bullet_bool("Stock is full", self.archetype().is_stock_full());
            }
        }

        if let Some(population) = self.archetype().population() {
            ui.bullet_text(format!("Population: {} (max: {})", population.count(), population.max()));
        }

        if let Some(workers) = self.archetype().workers() {
            if let Some(worker_pool) = workers.as_household_worker_pool() {
                ui.bullet_text(format!(
                    "Workers: {} (employed: {}, unemployed: {})",
                    worker_pool.total_workers(),
                    worker_pool.employed_count(),
                    worker_pool.unemployed_count()
                ));
            } else if let Some(employer) = workers.as_employer() {
                color_bullet_bool("Has min workers", self.archetype().has_min_required_workers());
                color_bullet_bool("Has all workers", employer.is_at_max_capacity());
                if employer.is_below_min_required() {
                    ui.bullet_text("Workers:");
                    ui.same_line();
                    ui.text_colored(Color::red().to_array(), format!("{}", employer.employee_count()));
                    ui.same_line();
                    ui.text(format!("(min: {}, max: {})", employer.min_employees(), employer.max_employees()));
                } else {
                    ui.bullet_text(format!(
                        "Workers: {} (min: {}, max: {})",
                        employer.employee_count(),
                        employer.min_employees(),
                        employer.max_employees()
                    ));
                }
            }
        }
    }

    pub(super) fn draw_debug_ui_detailed(&mut self, cmds: &mut SimCmds, context: &BuildingContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        // NOTE: Use the special ##id here so we don't collide with Tile/Properties.
        if ui.collapsing_header("Properties##_building_properties", imgui::TreeNodeFlags::empty()) {
            #[derive(DrawDebugUi)]
            struct DrawDebugUiVariables<'a> {
                name: &'a str,
                kind: BuildingKind,
                archetype: BuildingArchetypeKind,
                cells: CellRange,
                road_link: Cell,
                id: BuildingId,
            }
            let debug_vars = DrawDebugUiVariables {
                name: self.name(),
                kind: self.kind(),
                archetype: self.archetype_kind(),
                cells: self.cell_range(),
                road_link: self.road_link(context.sim_ctx).unwrap_or_default(),
                id: self.id(),
            };
            debug_vars.draw_debug_ui(ui_sys);
        }

        self.configs().draw_debug_ui(ui_sys);

        if let Some(population) = self.archetype().population() {
            if ui.collapsing_header("Population", imgui::TreeNodeFlags::empty()) {
                population.draw_debug_ui(ui_sys);

                if ui.button("Increase Population (+1)") {
                    self.archetype_mut().add_population(context, 1);
                }

                if ui.button("Evict Resident (-1)") {
                    self.archetype_mut().remove_population(cmds, context, 1);
                }

                if ui.button("Evict All Residents") {
                    self.remove_all_population(cmds, context.sim_ctx);
                }
            }
        }

        if let Some(workers) = self.archetype().workers() {
            if ui.collapsing_header("Workers", imgui::TreeNodeFlags::empty()) {
                let is_household_worker_pool = workers.is_household_worker_pool();
                let is_employer = workers.is_employer();

                workers.draw_debug_ui(context.sim_ctx.world(), ui_sys);
                ui.separator();

                let source = {
                    static BUILDING_KIND_IDX: UiStaticVar<usize> = UiStaticVar::new(0);
                    static BUILDING_GEN: UiStaticVar<u32> = UiStaticVar::new(0);
                    static BUILDING_ID: UiStaticVar<usize> = UiStaticVar::new(0);

                    if is_household_worker_pool {
                        ui.text("Select Employer:");
                    } else {
                        ui.text("Select Worker Household:");
                    }

                    let kind = {
                        if is_employer {
                            // Employers only source workers from houses.
                            BUILDING_KIND_IDX.set(0);
                            ui.combo_simple_string("Kind", BUILDING_KIND_IDX.as_mut(), &["House"]);
                            BuildingKind::House
                        } else {
                            let mut building_kind_names: SmallVec<[&'static str; BuildingKind::count()]> = SmallVec::new();
                            for kind in BuildingKind::FLAGS {
                                if *kind.value() != BuildingKind::House {
                                    building_kind_names.push(kind.name());
                                }
                            }
                            ui.combo_simple_string("Kind", BUILDING_KIND_IDX.as_mut(), &building_kind_names);
                            *BuildingKind::FLAGS[*BUILDING_KIND_IDX + 1].value() // We've skipped House @ [0]
                        }
                    };

                    ui.input_scalar("Gen", BUILDING_GEN.as_mut()).step(1).build();
                    ui.input_scalar("Idx", BUILDING_ID.as_mut()).step(1).build();

                    BuildingKindAndId { kind, id: BuildingId::new(*BUILDING_GEN, *BUILDING_ID) }
                };

                if ui.button("Add Worker (+1)") && !self.workers_is_maxed() {
                    if let Some(building) = context.sim_ctx.world_mut().find_building_mut(source.kind, source.id) {
                        let removed_count = building.remove_workers(1, self.kind_and_id());
                        let added_count = self.add_workers(removed_count, source);
                        debug_assert!(removed_count == added_count);
                    } else {
                        log::error!("Add Worker: Invalid source building id!");
                    }
                }

                if ui.button("Remove Worker (-1)") && self.workers_count() != 0 {
                    if let Some(building) = context.sim_ctx.world_mut().find_building_mut(source.kind, source.id) {
                        let added_count = building.add_workers(1, self.kind_and_id());
                        let removed_count = self.remove_workers(added_count, source);
                        debug_assert!(added_count == removed_count);
                    } else {
                        log::error!("Remove Worker: Invalid source building id!");
                    }
                }

                if ui.button("Remove All Workers") {
                    self.remove_all_workers(context.sim_ctx);
                }

                if is_employer {
                    // Only employers need to search for workers.
                    self.workers_update_timer.draw_debug_ui("Update", 0, ui_sys);
                }
            }
        }

        if ui.collapsing_header("Access", imgui::TreeNodeFlags::empty()) {
            if self.is_linked_to_road(context.sim_ctx) {
                ui.text_colored(Color::green().to_array(), "Has road access.");
            } else {
                ui.text_colored(Color::red().to_array(), "No road access!");
            }

            ui.text(format!("Road Link Tile : {}", self.road_link(context.sim_ctx).unwrap_or_default()));

            let mut show_road_link = self.is_showing_road_link_debug(context.sim_ctx);
            if ui.checkbox("Show Road Link", &mut show_road_link) {
                self.set_show_road_link_debug(context.sim_ctx, show_road_link);
            }

            if ui.button("Highlight Access Tiles") {
                pathfind::highlight_building_access_tiles(context.sim_ctx.tile_map_mut(), self.cell_range());
            }
        }

        self.archetype_mut().debug_options().draw_debug_ui(ui_sys);
        self.archetype_mut().draw_debug_ui(cmds, context, ui_sys);
    }
}
