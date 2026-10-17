use smallvec::{SmallVec, smallvec};
use bitflags::Flags;
use rand::seq::IteratorRandom;
use num_enum::TryFromPrimitive;

use common::{
    Color,
    coords::{Cell, CellRange},
    format_small,
};
use engine::{
    log,
    ui::{self, DrawDebugUi, UiStaticVar, UiSystem},
};
use proc_macros::DrawDebugUi;

use crate::{
    building::{
        Building,
        BuildingArchetypeKind,
        BuildingBehavior,
        BuildingContext,
        BuildingId,
        BuildingKind,
        BuildingKindAndId,
        BuildingStock,
        config::BuildingConfigs,
        house::{HouseBuilding, HouseLevel, HouseLevelRequirements, HouseUpgradeDirection},
        house_upgrade::{CANDIDATE_RECTS_COUNT, candidate_target_rects},
        producer::{ProducerBuilding, ProducerInputsLocalStock, ProducerOutputLocalStock},
        service::{ServiceBuilding, StockOrTreasury},
        storage::{MAX_STORAGE_SLOTS, StorageBuilding, StorageSlots},
    },
    pathfind,
    debug::DebugUiMode,
    sim::{SimCmds, SimContext, resources::ResourceKind},
    tile::{TileFlags, TileKind},
    unit::UnitTaskHelper,
    world::object::GameObject,
};

// ----------------------------------------------
// Building Debug UI
// ----------------------------------------------

// All debug-UI drawing for `Building`.
// The `GameObject::draw_debug_ui` method on `Building` is a thin forward into this.
impl Building {
    pub(crate) fn draw_debug_ui_dispatch(
        &mut self,
        cmds: &mut SimCmds,
        context: &SimContext,
        ui_sys: &UiSystem,
        mode: DebugUiMode,
    ) {
        debug_assert!(self.is_spawned());

        match mode {
            DebugUiMode::Overview => {
                self.draw_debug_ui_overview(&self.new_context(context), ui_sys);
            }
            DebugUiMode::Detailed => {
                if let Some(_path) = ui::collapsing_header_scope(ui_sys, "Building", false) {
                    ui::indent(ui_sys, 10.0, |ui_sys| {
                        self.draw_debug_ui_detailed(cmds, &self.new_context(context), ui_sys);
                    });
                }
            }
        }
    }

    fn draw_debug_ui_overview(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        let color_bullet_bool = |label: &str, value: bool| {
            ui::horizontal(ui_sys, |ui_sys| {
                ui::bullet_text(ui_sys, format_small!("{label}:"));
                if value {
                    ui::text(ui_sys, "yes");
                } else {
                    ui::text_colored(ui_sys, Color::red(), "no");
                }
            });
        };

        let color_bullet_text = |label: &str, value: &str| {
            ui::horizontal(ui_sys, |ui_sys| {
                ui::bullet_text(ui_sys, format_small!("{label}:"));
                ui::text_colored(ui_sys, Color::red(), value);
            });
        };

        ui::with_font_scale(ui_sys, 1.2, |ui_sys| ui::text(ui_sys, format_small!("{} | ID{} @{}", self.name(), self.id(), self.base_cell())));

        color_bullet_bool("Linked to road", self.is_linked_to_road());

        if self.archetype_kind() == BuildingArchetypeKind::HouseBuilding {
            let house = self.as_house();
            ui::bullet_text(ui_sys, format_small!("Tax: (generated: {}, avail: {})", house.tax_generated(), house.tax_available()));

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
                ui::bullet_text(ui_sys, "Max house level reached");
            }
        } else {
            color_bullet_bool("Is operational", self.archetype().is_operational());
            color_bullet_bool("Has resources", self.archetype().has_min_required_resources());

            if self.archetype().has_stock() {
                color_bullet_bool("Stock is full", self.archetype().is_stock_full());
            }
        }

        if let Some(population) = self.archetype().population() {
            ui::bullet_text(ui_sys, format_small!("Population: {} (max: {})", population.count(), population.max()));
        }

        if let Some(workers) = self.archetype().workers() {
            if let Some(worker_pool) = workers.as_household_worker_pool() {
                ui::bullet_text(ui_sys, format_small!(
                    "Workers: {} (employed: {}, unemployed: {})",
                    worker_pool.total_workers(),
                    worker_pool.employed_count(),
                    worker_pool.unemployed_count()
                ));
            } else if let Some(employer) = workers.as_employer() {
                color_bullet_bool("Has min workers", self.archetype().has_min_required_workers());
                color_bullet_bool("Has all workers", employer.is_at_max_capacity());
                if employer.is_below_min_required() {
                    ui::horizontal(ui_sys, |ui_sys| {
                        ui::bullet_text(ui_sys, "Workers:");
                        ui::text_colored(ui_sys, Color::red(), format_small!("{}", employer.employee_count()));
                        ui::text(ui_sys, format_small!("(min: {}, max: {})", employer.min_employees(), employer.max_employees()));
                    });
                } else {
                    ui::bullet_text(ui_sys, format_small!(
                        "Workers: {} (min: {}, max: {})",
                        employer.employee_count(),
                        employer.min_employees(),
                        employer.max_employees()
                    ));
                }
            }
        }
    }

    fn draw_debug_ui_detailed(&mut self, cmds: &mut SimCmds, context: &BuildingContext, ui_sys: &UiSystem) {
        // NOTE: Use the special ##id here so we don't collide with Tile/Properties.
        if let Some(_path) =
            ui::collapsing_header_scope(ui_sys, "Properties##_building_properties", false)
        {
            #[derive(DrawDebugUi)]
            struct DrawDebugUiVariables<'a> {
                name: &'a str,
                kind: BuildingKind,
                archetype: BuildingArchetypeKind,
                cells: CellRange,
                road_link: Cell,
                id: BuildingId,
            }
            let mut debug_vars = DrawDebugUiVariables {
                name: self.name(),
                kind: self.kind(),
                archetype: self.archetype_kind(),
                cells: self.cell_range(),
                road_link: self.road_link().unwrap_or_default(),
                id: self.id(),
            };
            debug_vars.draw_debug_ui(ui_sys);
        }

        self.configs().draw_debug_ui(ui_sys);

        if let Some(mut population) = self.archetype().population() {
            if let Some(_path) = ui::collapsing_header_scope(ui_sys, "Population", false) {
                population.draw_debug_ui(ui_sys);

                if ui::button(ui_sys, "Increase Population (+1)") {
                    self.archetype_mut().add_population(cmds, context, 1);
                }

                if ui::button(ui_sys, "Evict Resident (-1)") {
                    self.archetype_mut().remove_population(cmds, context, 1);
                }

                if ui::button(ui_sys, "Evict All Residents") {
                    self.remove_all_population(cmds, context.sim_ctx);
                }
            }
        }

        if let Some(workers) = self.archetype().workers() {
            if let Some(_path) = ui::collapsing_header_scope(ui_sys, "Workers", false) {
                let is_household_worker_pool = workers.is_household_worker_pool();
                let is_employer = workers.is_employer();

                super::sim::draw_workers_debug_ui(workers, context.sim_ctx.world(), ui_sys);
                ui::separator(ui_sys);

                let source = {
                    static BUILDING_KIND_IDX: UiStaticVar<usize> = UiStaticVar::new(0);
                    static BUILDING_GEN: UiStaticVar<u32> = UiStaticVar::new(0);
                    static BUILDING_ID: UiStaticVar<usize> = UiStaticVar::new(0);

                    if is_household_worker_pool {
                        ui::text(ui_sys, "Select Employer:");
                    } else {
                        ui::text(ui_sys, "Select Worker Household:");
                    }

                    let kind = {
                        if is_employer {
                            // Employers only source workers from houses.
                            BUILDING_KIND_IDX.set(0);
                            ui::combo(ui_sys, "Kind", BUILDING_KIND_IDX.as_mut(), &["House"]);
                            BuildingKind::House
                        } else {
                            let mut building_kind_names: SmallVec<[&'static str; BuildingKind::count()]> = SmallVec::new();
                            for kind in BuildingKind::FLAGS {
                                if *kind.value() != BuildingKind::House {
                                    building_kind_names.push(kind.name());
                                }
                            }
                            ui::combo(ui_sys, "Kind", BUILDING_KIND_IDX.as_mut(), &building_kind_names);
                            *BuildingKind::FLAGS[*BUILDING_KIND_IDX + 1].value() // We've skipped House @ [0]
                        }
                    };

                    ui::input_scalar(ui_sys, "Gen", BUILDING_GEN.as_mut(), 1);
                    ui::input_scalar(ui_sys, "Idx", BUILDING_ID.as_mut(), 1);

                    BuildingKindAndId { kind, id: BuildingId::new(*BUILDING_GEN, *BUILDING_ID) }
                };

                if ui::button(ui_sys, "Add Worker (+1)") && !self.workers_is_maxed() {
                    if let Some(building) = context.sim_ctx.find_building_mut(source.kind, source.id) {
                        let removed_count = building.remove_workers(1, self.kind_and_id());
                        let added_count = self.add_workers(removed_count, source);
                        debug_assert!(removed_count == added_count);
                    } else {
                        log::error!("Add Worker: Invalid source building id!");
                    }
                }

                if ui::button(ui_sys, "Remove Worker (-1)") && self.workers_count() != 0 {
                    if let Some(building) = context.sim_ctx.find_building_mut(source.kind, source.id) {
                        let added_count = building.add_workers(1, self.kind_and_id());
                        let removed_count = self.remove_workers(added_count, source);
                        debug_assert!(added_count == removed_count);
                    } else {
                        log::error!("Remove Worker: Invalid source building id!");
                    }
                }

                if ui::button(ui_sys, "Remove All Workers") {
                    self.remove_all_workers(context.sim_ctx);
                }

                if is_employer {
                    // Only employers need to search for workers.
                    self.workers_update_timer_mut().draw_debug_ui_with_header("Update", ui_sys);
                }
            }
        }

        if let Some(_path) = ui::collapsing_header_scope(ui_sys, "Access", false) {
            if self.is_linked_to_road() {
                ui::text_colored(ui_sys, Color::green(), "Has road access.");
            } else {
                ui::text_colored(ui_sys, Color::red(), "No road access!");
            }

            ui::text(ui_sys, format_small!("Road Link Tile : {}", self.road_link().unwrap_or_default()));

            let mut show_road_link = self.is_showing_road_link_debug(context.sim_ctx);
            if ui::checkbox(ui_sys, "Show Road Link", &mut show_road_link) {
                self.set_show_road_link_debug(context.sim_ctx, show_road_link);
            }

            if ui::button(ui_sys, "Highlight Access Tiles") {
                pathfind::highlight_building_access_tiles(context.sim_ctx.tile_map_mut(), self.cell_range());
            }
        }

        self.archetype_mut().debug_options().draw_debug_ui(ui_sys);
        self.archetype_mut().draw_debug_ui(cmds, context, ui_sys);
    }
}

// ----------------------------------------------
// BuildingStock Debug UI
// ----------------------------------------------

impl DrawDebugUi for BuildingStock {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        self.resources.for_each_mut(|index, item| {
            let item_label = format_small!("{}##_stock_item_{}", item.kind, index);
            let item_capacity = self.capacities[index] as u32;

            ui::horizontal(ui_sys, |ui_sys| {
                if ui::input_scalar(ui_sys, &item_label, &mut item.count, 1) {
                    item.count = item.count.min(item_capacity);
                }

                let capacity_left = item_capacity - item.count;
                let is_full = item.count >= item_capacity;

                if is_full {
                    ui::text_colored(ui_sys, Color::red(), "(full)");
                } else {
                    ui::text(ui_sys, format_small!("({} left)", capacity_left));
                }
            });
        });
    }
}

// ----------------------------------------------
// Producer local-stock Debug UI
// ----------------------------------------------

impl DrawDebugUi for ProducerOutputLocalStock {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        ui::text(ui_sys, "Local Stock:");

        if ui::input_scalar(ui_sys, &format_small!("{}", self.item.kind), &mut self.item.count, 1) {
            self.item.count = self.item.count.min(self.capacity());
        }

        ui::horizontal(ui_sys, |ui_sys| {
            ui::text(ui_sys, "Is full:");
            if self.is_full() {
                ui::text_colored(ui_sys, Color::red(), "yes");
            } else {
                ui::text(ui_sys, "no");
            }
        });
    }
}

impl DrawDebugUi for ProducerInputsLocalStock {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        if self.slots.is_empty() {
            ui::text(ui_sys, "<none>");
        } else {
            let capacity = self.capacity();

            for (index, item) in self.slots.iter_mut().enumerate() {
                let label = format_small!("{}##_stock_item_{}", item.kind, index);

                ui::horizontal(ui_sys, |ui_sys| {
                    if ui::input_scalar(ui_sys, &label, &mut item.count, 1) {
                        item.count = item.count.min(capacity);
                    }

                    let capacity_left = capacity - item.count;
                    let is_full = item.count >= capacity;

                    if is_full {
                        ui::text_colored(ui_sys, Color::red(), "(full)");
                    } else {
                        ui::text(ui_sys, format_small!("({} left)", capacity_left));
                    }
                });
            }
        }
    }
}

// ----------------------------------------------
// StorageSlots Debug UI
// ----------------------------------------------

impl DrawDebugUi for StorageSlots {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        if self.slots.is_empty() {
            return;
        }

        ui::horizontal(ui_sys, |ui_sys| {
            if ui::button(ui_sys, "Fill up all slots") {
                let add_amount = self.slot_capacity();
                let slot_capacity = self.slot_capacity();

                for slot in &mut self.slots {
                    if let Some(allocated_kind) = slot.allocated_resource_kind() {
                        // Fill up slot with existing resource.
                        slot.increment_resource_count(allocated_kind, add_amount, slot_capacity);
                    } else {
                        let accepted_kinds = slot.accepted_kinds();

                        // Pick a random resource kind from the accepted kinds.
                        let mut rng = rand::rng();
                        let random_kind = accepted_kinds.iter().choose(&mut rng).unwrap_or(ResourceKind::Rice);

                        slot.increment_resource_count(random_kind, add_amount, slot_capacity);
                    }
                }
            }

            if ui::button(ui_sys, "Clear all slots") {
                for slot in &mut self.slots {
                    slot.clear();
                }
            }
        });

        ui::separator(ui_sys);

        let mut display_slots: SmallVec<[SmallVec<[ResourceKind; 8]>; MAX_STORAGE_SLOTS]> =
            smallvec![SmallVec::new(); MAX_STORAGE_SLOTS];

        for (slot_index, slot) in self.slots.iter().enumerate() {
            if let Some(allocated_kind) = slot.allocated_resource_kind() {
                // Display only the allocated resource kind.
                display_slots[slot_index].push(allocated_kind);
            } else {
                // No resource allocated for the slot, display all possible resource kinds
                // accepted.
                slot.for_each_accepted_resource(|kind| {
                    display_slots[slot_index].push(kind);
                });
            }
        }

        ui::indent(ui_sys, 10.0, |ui_sys| {
            for (slot_index, slot) in display_slots.iter().enumerate() {
                let slot_label = {
                    if self.is_slot_free(slot_index) {
                        format_small!("Slot {} (Free)", slot_index)
                    } else {
                        format_small!("Slot {} ({})", slot_index, display_slots[slot_index].last().unwrap())
                    }
                };

                let header_label = format_small!("{}##_stock_slot_{}", slot_label, slot_index);

                if let Some(_path) = ui::collapsing_header_scope(ui_sys, &header_label, true) {
                    for (res_index, res_kind) in slot.iter().enumerate() {
                        let res_label = format_small!("{}##_stock_item_{}_slot_{}", res_kind, res_index, slot_index);

                        let prev_count = self.slot_resource_count(slot_index, *res_kind);
                        let mut new_count = prev_count;

                        ui::horizontal(ui_sys, |ui_sys| {
                            if ui::input_scalar(ui_sys, &res_label, &mut new_count, 1) {
                                match new_count.cmp(&prev_count) {
                                    std::cmp::Ordering::Greater => {
                                        new_count =
                                            self.increment_slot_resource_count(slot_index, *res_kind, new_count - prev_count);
                                    }
                                    std::cmp::Ordering::Less => {
                                        new_count =
                                            self.decrement_slot_resource_count(slot_index, *res_kind, prev_count - new_count);
                                    }
                                    std::cmp::Ordering::Equal => {} // nothing
                                }
                            }

                            let capacity_left = self.slot_capacity() - new_count;
                            let is_full = new_count >= self.slot_capacity();

                            if is_full {
                                ui::text_colored(ui_sys, Color::red(), "(full)");
                            } else {
                                ui::text(ui_sys, format_small!("({} left)", capacity_left));
                            }
                        });
                    }
                }
            }
        });
    }
}

// ----------------------------------------------
// StorageBuilding Debug UI
// ----------------------------------------------

impl StorageBuilding {
    pub(crate) fn draw_debug_ui_dispatch(&mut self, _cmds: &mut SimCmds, _context: &BuildingContext, ui_sys: &UiSystem) {
        self.storage_slots.draw_debug_ui_with_header("Stock Slots", ui_sys);
    }
}

// ----------------------------------------------
// ProducerBuilding Debug UI
// ----------------------------------------------

impl ProducerBuilding {
    pub(crate) fn draw_debug_ui_dispatch(&mut self, cmds: &mut SimCmds, context: &BuildingContext, ui_sys: &UiSystem) {
        self.draw_debug_ui_input_stock(ui_sys);
        self.draw_debug_ui_production_output(context, ui_sys);
        self.draw_debug_ui_ambient_patrol(cmds, context, ui_sys);
    }

    fn draw_debug_ui_input_stock(&mut self, ui_sys: &UiSystem) {
        if self.production_input_stock.requires_any_resource() {
            if let Some(_path) =
                ui::collapsing_header_scope(ui_sys, "Raw Materials In Stock", false)
            {
                self.production_input_stock.draw_debug_ui(ui_sys);

                ui::horizontal(ui_sys, |ui_sys| {
                    if ui::button(ui_sys, "Fill Stock##_fill_input_stock") {
                        self.production_input_stock.fill();
                    }
                    if ui::button(ui_sys, "Clear Stock##_clear_input_stock") {
                        self.production_input_stock.clear();
                    }
                });
            }
        }
    }

    fn draw_debug_ui_production_output(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        let Some(_path) = ui::collapsing_header_scope(ui_sys, "Production Output", false) else {
            return; // collapsed.
        };

        if self.is_production_halted() {
            ui::horizontal(ui_sys, |ui_sys| {
                ui::text_colored(ui_sys, Color::red(), "Production Halted:");
                if self.production_output_stock.is_full() {
                    ui::text_colored(ui_sys, Color::red(), "Local Stock Full!");
                } else if self.production_input_stock.requires_any_resource()
                    && !self.production_input_stock.has_required_resources()
                {
                    ui::text_colored(ui_sys, Color::red(), "Missing Resources!");
                } else {
                    ui::text_colored(ui_sys, Color::red(), "Production Frozen.");
                }
            });
        }

        if self.runner.failed_to_spawn() {
            ui::text_colored(ui_sys, Color::red(), "Failed to spawn last Runner!");
        }

        if self.harvester.failed_to_spawn() {
            ui::text_colored(ui_sys, Color::red(), "Failed to spawn last Harvester!");
        }

        if self.is_waiting_on_runner() {
            if self.is_runner_delivering_resources(context.sim_ctx) {
                ui::text_colored(ui_sys, Color::yellow(), "Runner sent on Delivery Task.");
            } else if self.is_runner_fetching_resources(context.sim_ctx) {
                ui::text_colored(ui_sys, Color::yellow(), "Runner sent on Fetch Task.");
            } else {
                ui::text_colored(ui_sys, Color::yellow(), "Runner sent out. Waiting...");
            }

            if ui::button(ui_sys, "Forget Runner") {
                self.runner.reset();
            }
        }

        if self.is_waiting_on_harvester() {
            if self.is_harvester_fetching_resources(context.sim_ctx) {
                ui::text_colored(ui_sys, Color::yellow(), "Harvester sent out. Waiting...");
            }

            if ui::button(ui_sys, "Forget Harvester") {
                self.harvester.reset();
            }
        }

        self.production_update_timer.draw_debug_ui_with_header("Update", ui_sys);
        self.production_output_stock.draw_debug_ui(ui_sys);

        ui::horizontal(ui_sys, |ui_sys| {
            if ui::button(ui_sys, "Fill Stock##_fill_output_stock") {
                self.production_output_stock.fill();
            }
            if ui::button(ui_sys, "Clear Stock##_clear_output_stock") {
                self.production_output_stock.clear();
            }
        });
    }

    fn draw_debug_ui_ambient_patrol(&mut self, cmds: &mut SimCmds, context: &BuildingContext, ui_sys: &UiSystem) {
        let Some(_path) = ui::collapsing_header_scope(ui_sys, "Ambient Patrol", false) else {
            return; // collapsed.
        };

        self.ambient_patrol.spawn_timer.draw_debug_ui_with_header("Spawn Patrol", ui_sys);

        if ui::button(ui_sys, "Force Spawn Ambient Patrol") {
            self.spawn_ambient_patrol(cmds, context, true);
        }

        if self.ambient_patrol.patrol.is_spawned_or_pending_spawn() {
            ui::text_colored(ui_sys, Color::yellow(), "Ambient Patrol Spawned...");
        }
    }
}

// ----------------------------------------------
// ServiceBuilding Debug UI
// ----------------------------------------------

impl ServiceBuilding {
    pub(crate) fn draw_debug_ui_dispatch(&mut self, _cmds: &mut SimCmds, context: &BuildingContext, ui_sys: &UiSystem) {
        self.draw_debug_ui_resources_stock(context, ui_sys);
        self.draw_debug_ui_patrol(ui_sys);
        self.draw_debug_ui_treasury(ui_sys);
    }

    fn draw_debug_ui_resources_stock(&mut self, context: &BuildingContext, ui_sys: &UiSystem) {
        if !self.stock_or_treasury.is_stock_and_requires_resources() {
            return;
        }
        let Some(_path) = ui::collapsing_header_scope(ui_sys, "Stock", false) else {
            return; // collapsed.
        };

        if self.runner.failed_to_spawn() {
            ui::text_colored(ui_sys, Color::red(), "Failed to spawn last Runner!");
        }

        if self.is_waiting_on_runner() {
            if self.is_runner_fetching_resources(context.sim_ctx) {
                ui::text_colored(ui_sys, Color::yellow(), "Runner sent on Fetch Task.");
            } else {
                ui::text_colored(ui_sys, Color::yellow(), "Runner sent out. Waiting...");
            }

            if ui::button(ui_sys, "Forget Runner") {
                self.runner.reset();
            }
        }

        if let StockOrTreasury::Stock { update_timer, stock } = &mut self.stock_or_treasury {
            update_timer.draw_debug_ui_with_header("Update", ui_sys);

            ui::horizontal(ui_sys, |ui_sys| {
                if ui::button(ui_sys, "Fill Stock") {
                    // Set all to capacity.
                    stock.fill();
                }
                if ui::button(ui_sys, "Clear Stock") {
                    stock.clear();
                }
            });

            stock.draw_debug_ui_with_header("Resources", ui_sys);
        }
    }

    fn draw_debug_ui_patrol(&mut self, ui_sys: &UiSystem) {
        if !self.has_patrol_unit() {
            return;
        }
        let Some(_path) = ui::collapsing_header_scope(ui_sys, "Patrol", false) else {
            return; // collapsed.
        };

        if self.patrol.failed_to_spawn() {
            ui::text_colored(ui_sys, Color::red(), "Failed to spawn last Patrol!");
        }

        if self.is_waiting_on_patrol() {
            ui::text_colored(ui_sys, Color::yellow(), "Patrol sent out. Waiting...");
            if ui::button(ui_sys, "Forget Patrol") {
                self.patrol.reset();
            }
        }

        self.patrol_timer.draw_debug_ui_with_header("Patrol", ui_sys);

        ui::text(ui_sys, format_small!("Spawn State: {:?}", self.patrol.spawn_state()));

        self.patrol.draw_debug_ui_with_header("Patrol Params", ui_sys);
    }

    fn draw_debug_ui_treasury(&mut self, ui_sys: &UiSystem) {
        if let StockOrTreasury::Treasury { gold_units } = &mut self.stock_or_treasury {
            if let Some(_path) = ui::collapsing_header_scope(ui_sys, "Treasury", false) {
                ui::input_scalar(ui_sys, "Gold Units", gold_units, 1);
            }
        }
    }
}

// ----------------------------------------------
// HouseBuilding Debug UI
// ----------------------------------------------

impl HouseBuilding {
    pub(crate) fn draw_debug_ui_dispatch(&mut self, cmds: &mut SimCmds, context: &BuildingContext, ui_sys: &UiSystem) {
        draw_house_merge_debug_ui(context, ui_sys);
        self.draw_debug_ui_timers(cmds, context, ui_sys);
        self.draw_debug_ui_stock(context, ui_sys);
        self.draw_debug_ui_upgrade_state(cmds, context, ui_sys);
    }

    fn draw_debug_ui_upgrade_state(&mut self, cmds: &mut SimCmds, context: &BuildingContext, ui_sys: &UiSystem) {
        let Some(_path) = ui::collapsing_header_scope(ui_sys, "Upgrade", false) else {
            return; // collapsed.
        };

        let draw_level_requirements = |label: &str, level_requirements: &HouseLevelRequirements, header_id: u32| {
            ui::separator(ui_sys);
            ui::text(ui_sys, label);

            ui::text(ui_sys, format_small!(
                "  Resources avail : {} (req: {})",
                level_requirements.resources_available_count(),
                level_requirements.level_config.resources_required.len()
            ));
            ui::text(ui_sys, format_small!(
                "  Services avail  : {} (req: {})",
                level_requirements.services_available_count(),
                level_requirements.level_config.services_required.len()
            ));

            let header = format_small!("Resources##_building_resources_{}", header_id);
            if let Some(_path) = ui::collapsing_header_scope(ui_sys, &header, false) {
                if !level_requirements.level_config.resources_required.is_empty() {
                    ui::text(ui_sys, "Available:");
                    if level_requirements.resources_available.is_empty() {
                        ui::text(ui_sys, "  <none>");
                    }
                    for resource in level_requirements.resources_available.iter() {
                        ui::text(ui_sys, format_small!("  {}", resource));
                    }
                }

                ui::text(ui_sys, "Required:");
                if level_requirements.level_config.resources_required.is_empty() {
                    ui::text(ui_sys, "  <none>");
                }
                for resource in level_requirements.level_config.resources_required.iter() {
                    ui::text(ui_sys, format_small!("  {}", resource));
                }
            }

            let header = format_small!("Services##_building_services_{}", header_id);
            if let Some(_path) = ui::collapsing_header_scope(ui_sys, &header, false) {
                if !level_requirements.level_config.services_required.is_empty() {
                    ui::text(ui_sys, "Available:");
                    if level_requirements.services_available.is_empty() {
                        ui::text(ui_sys, "  <none>");
                    }
                    for service in level_requirements.services_available.iter() {
                        ui::text(ui_sys, format_small!("  {}", service));
                    }
                }

                ui::text(ui_sys, "Required:");
                if level_requirements.level_config.services_required.is_empty() {
                    ui::text(ui_sys, "  <none>");
                }
                for service in level_requirements.level_config.services_required.iter() {
                    ui::text(ui_sys, format_small!("  {}", service));
                }
            }
        };

        let color_text = |text: &str, value: bool| {
            ui::horizontal(ui_sys, |ui_sys| {
                ui::text(ui_sys, text);
                if value {
                    ui::text(ui_sys, "yes");
                } else {
                    ui::text_colored(ui_sys, Color::red(), "no");
                }
            });
        };

        let mut level_num: u8 = self.upgrade_state().level.into();
        if ui::input_scalar(ui_sys, "Level", &mut level_num, 1) {
            if let Ok(level) = HouseLevel::try_from_primitive(level_num) {
                match level.cmp(&self.upgrade_state().level) {
                    std::cmp::Ordering::Greater => {
                        self.perform_upgrade(cmds, context, HouseUpgradeDirection::Upgrade);
                    }
                    std::cmp::Ordering::Less => {
                        self.perform_upgrade(cmds, context, HouseUpgradeDirection::Downgrade);
                    }
                    std::cmp::Ordering::Equal => {} // nothing
                }
            }
        }

        let upgrade_state = self.upgrade_state();

        let curr_level_requirements =
            HouseLevelRequirements::new(context, upgrade_state.curr_level_config.unwrap(), &self.stock);

        let next_level_requirements =
            HouseLevelRequirements::new(context, upgrade_state.next_level_config.unwrap(), &self.stock);

        color_text(" - Has room        :", upgrade_state.has_room_to_upgrade);
        color_text(" - Has services    :", next_level_requirements.has_required_services());
        color_text(" - Has resources   :", next_level_requirements.has_required_resources());
        color_text(" - Has road access :", context.is_linked_to_road());

        draw_level_requirements(&format_small!("Curr level reqs ({}):", upgrade_state.level), &curr_level_requirements, 0);

        if !upgrade_state.level.is_max() {
            draw_level_requirements(
                &format_small!("Next level reqs ({}):", upgrade_state.level.next()),
                &next_level_requirements,
                1,
            );
        }
    }

    fn draw_debug_ui_timers(&mut self, cmds: &mut SimCmds, context: &BuildingContext, ui_sys: &UiSystem) {
        let Some(_path) = ui::collapsing_header_scope(ui_sys, "Timers", false) else {
            return; // collapsed.
        };

        self.population_update_timer.draw_debug_ui_with_header("Population Update", ui_sys);
        self.upgrade_update_timer.draw_debug_ui_with_header("Upgrade Update", ui_sys);
        self.stock_update_timer.draw_debug_ui_with_header("Stock Update", ui_sys);
        self.generate_tax_timer.draw_debug_ui_with_header("Gen Tax", ui_sys);
        self.ambient_patrol.spawn_timer.draw_debug_ui_with_header("Spawn Patrol", ui_sys);

        if ui::button(ui_sys, "Force Spawn Ambient Patrol") {
            self.spawn_ambient_patrol(cmds, context, true);
        }

        if self.ambient_patrol.patrol.is_spawned_or_pending_spawn() {
            ui::text_colored(ui_sys, Color::yellow(), "Ambient Patrol Spawned...");
        }
    }

    fn draw_debug_ui_stock(&mut self, _context: &BuildingContext, ui_sys: &UiSystem) {
        self.stock.draw_debug_ui_with_header("Stock", ui_sys);
        let Some(_path) = ui::collapsing_header_scope(ui_sys, "Consumption", false) else {
            return; // collapsed.
        };

        let config = BuildingConfigs::get().house_config();

        // Deprivation timer: how long this house has gone without a basic need.
        let grace = config.deprivation_grace_secs;
        let timer = self.deprivation_timer_secs();
        let deprivation_text = format_small!("Deprivation : {:.0}s / {:.0}s", timer, grace);
        if timer > 0.0 {
            ui::text_colored(ui_sys, Color::red(), deprivation_text);
        } else {
            ui::text(ui_sys, deprivation_text);
        }

        // Per-resource fractional consumption carried between stock updates.
        ui::text(ui_sys, "Accumulators:");
        let mut any_shown = false;
        for kind in ResourceKind::all().iter() {
            let accumulated = self.consumption_accumulator()[kind.index()];
            if accumulated != 0.0 {
                let rate = config.consumption_rate_table[kind.index()];
                ui::text(ui_sys, format_small!("  {}: {:.2} ({:.2}/day/resident)", kind, accumulated, rate));
                any_shown = true;
            }
        }
        if !any_shown {
            ui::text(ui_sys, "  <none>");
        }
    }
}

// ----------------------------------------------
// House merge Debug UI
// ----------------------------------------------

fn draw_house_merge_debug_ui(context: &BuildingContext, ui_sys: &UiSystem) {
    let Some(_path) = ui::collapsing_header_scope(ui_sys, "Merge Debug", false) else {
        return; // collapsed.
    };

    static HIGHLIGHT_START_CELL: UiStaticVar<bool> = UiStaticVar::new(false);
    static CANDIDATE_RECT_IDX: UiStaticVar<usize> = UiStaticVar::new(0);

    ui::checkbox(ui_sys, "Mark Start Cell", HIGHLIGHT_START_CELL.as_mut());

    if ui::input_scalar(ui_sys, "Candidate Rect", CANDIDATE_RECT_IDX.as_mut(), 1) {
        CANDIDATE_RECT_IDX.set((*CANDIDATE_RECT_IDX).min(CANDIDATE_RECTS_COUNT - 1));
    }

    if ui::button(ui_sys, "Visualize Merge Candidate Cells") {
        let candidate_rects = candidate_target_rects(context.cell_range());
        let target_rect = candidate_rects[*CANDIDATE_RECT_IDX];

        let tile_map = context.sim_ctx.tile_map_mut();

        for cell in target_rect.iter_cells() {
            tile_map.set_tile_flags(cell, TileKind::Terrain, TileFlags::Highlighted, true);
            tile_map.set_tile_flags(cell, TileKind::Building, TileFlags::Invalidated, true);
        }

        if *HIGHLIGHT_START_CELL {
            let start_cell = Cell::new(target_rect.min_x(), target_rect.min_y());
            tile_map.set_tile_flags(start_cell, TileKind::Terrain, TileFlags::DrawDebugBounds, true);
            tile_map.set_tile_flags(start_cell, TileKind::Building, TileFlags::DrawDebugBounds, true);
        }
    }
}
