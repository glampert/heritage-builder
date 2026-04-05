use engine::ui::{self, text::UiTextCategory, widgets::{UiWidget, UiMenu}};
use common::{mem::{RcMut, WeakMut, WeakRef}, fixed_string::snake_case_to_title};
use crate::{
    tile::{Tile, TileKind},
    { building::{Building, BuildingKind, BuildingArchetypeKind}, sim::resources::{ResourceKind, StockItem}, menu::TileInspector, ui_context::GameUiContext, },
};

mod renderer;
use renderer::*;

// ----------------------------------------------
// TileInspectorMenu
// ----------------------------------------------

pub struct TileInspectorMenu {
    current_inspector_kind: Option<GameObjectInspectorKind>,

    unit_inspector: UnitInspector,
    building_inspector: BuildingInspector,
    prop_inspector: PropInspector,
    terrain_inspector: TerrainInspector,
}

pub type TileInspectorMenuRcMut   = RcMut<TileInspectorMenu>;
pub type TileInspectorMenuWeakMut = WeakMut<TileInspectorMenu>;
pub type TileInspectorMenuWeakRef = WeakRef<TileInspectorMenu>;

impl TileInspector for TileInspectorMenu {
    fn open(&mut self, context: &mut GameUiContext) {
        debug_assert!(self.current_inspector_kind.is_none());

        self.current_inspector_kind = {
            let selected_tile = match context.topmost_selected_tile() {
                Some(tile) => tile,
                None => return,
            };

            if selected_tile.is(TileKind::Unit) {
                Some(GameObjectInspectorKind::Unit)
            } else if selected_tile.is(TileKind::Building) {
                Some(GameObjectInspectorKind::Building)
            } else if selected_tile.is_harvestable_prop() {
                // NOTE: Only harvestable props have an associated Prop object. Anything else uses the terrain inspector.
                Some(GameObjectInspectorKind::Prop)
            } else if selected_tile.is(TileKind::Terrain | TileKind::Rocks | TileKind::Vegetation) {
                // NOTE: Rocks and non-harvestable vegetation are considered part of terrain (no associated Prop object).
                Some(GameObjectInspectorKind::Terrain)
            } else {
                None
            }
        };

        if let Some(inspector) = self.current_inspector() {
            inspector.open(context);
        }
    }

    fn close(&mut self, context: &mut GameUiContext) {
        if let Some(inspector) = self.current_inspector() {
            inspector.close(context);
            self.current_inspector_kind = None;
        }
    }
}

impl TileInspectorMenu {
    pub fn new(context: &mut GameUiContext) -> TileInspectorMenuRcMut {
        TileInspectorMenuRcMut::new_cyclic(|tile_inspector_menu_weak_ref| {
            Self {
                current_inspector_kind: None,
                unit_inspector: UnitInspector::new(context, &tile_inspector_menu_weak_ref),
                building_inspector: BuildingInspector::new(context, &tile_inspector_menu_weak_ref),
                prop_inspector: PropInspector::new(context, &tile_inspector_menu_weak_ref),
                terrain_inspector: TerrainInspector::new(context, &tile_inspector_menu_weak_ref),
            }
        })
    }

    pub fn draw(&mut self, context: &mut GameUiContext) {
        if let Some(inspector) = self.current_inspector() {
            let is_open = inspector.draw(context);

            // If the menu was closed just now, clear current inspector.
            if !is_open {
                self.close(context);
            }
        }
    }

    fn current_inspector(&mut self) -> Option<&mut dyn GameObjectInspector> {
        self.current_inspector_kind.map(|kind| {
            let inspector: &mut dyn GameObjectInspector = {
                match kind {
                    GameObjectInspectorKind::Unit     => &mut self.unit_inspector,
                    GameObjectInspectorKind::Building => &mut self.building_inspector,
                    GameObjectInspectorKind::Prop     => &mut self.prop_inspector,
                    GameObjectInspectorKind::Terrain  => &mut self.terrain_inspector,
                }
            };
            inspector
        })
    }
}

// ----------------------------------------------
// GameObjectInspectorKind / GameObjectInspector
// ----------------------------------------------

#[derive(Copy, Clone)]
enum GameObjectInspectorKind {
    Unit,
    Building,
    Prop,
    Terrain,
}

trait GameObjectInspector {
    fn update_selection(&mut self, context: &GameUiContext, selected_tile: &Tile);
    fn menu(&mut self) -> &mut UiMenu;

    fn open(&mut self, context: &mut GameUiContext) {
        let selected_tile = match context.topmost_selected_tile() {
            Some(tile) => tile,
            None => return,
        };

        self.update_selection(context, selected_tile);
        self.menu().open(context);
    }

    fn close(&mut self, context: &mut GameUiContext) {
        self.menu().close(context);
    }

    // Returns true if menu still open.
    fn draw(&mut self, context: &mut GameUiContext) -> bool {
        let menu = self.menu();
        menu.draw(context);
        menu.is_open()
    }
}

// ----------------------------------------------
// Tile description lookup helpers
// ----------------------------------------------

fn find_tile_description(tile: &Tile) -> &'static str {
    ui::text::find_str(UiTextCategory::TileDescription, tile.tile_def().hash)
        .unwrap_or("")
}

fn find_unit_dialog(unit_tile: &Tile) -> &'static str {
    ui::text::find_str(UiTextCategory::UnitDialog, unit_tile.tile_def().hash)
        .unwrap_or("")
}

// ----------------------------------------------
// UnitInspector
// ----------------------------------------------

struct UnitInspector {
    renderer: InspectorMenuRenderer,
}

impl GameObjectInspector for UnitInspector {
    fn update_selection(&mut self, context: &GameUiContext, selected_tile: &Tile) {
        if let Some(unit) = context.world.find_unit_for_tile(selected_tile) {
            self.renderer.set_icon(context, selected_tile.icon_sprite(), selected_tile.kind());
            self.renderer.set_title(unit.name());
            self.renderer.set_body_text(find_unit_dialog(selected_tile));
            self.set_inventory(unit.peek_inventory());
        }
    }

    fn menu(&mut self) -> &mut UiMenu {
        self.renderer.menu()
    }
}

impl UnitInspector {
    fn new(context: &mut GameUiContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
        Self {
            renderer: InspectorMenuRenderer::new(
                context,
                tile_inspector_menu_weak_ref,
                stringify!(UnitInspector)
            )
        }
    }

    fn set_inventory(&mut self, inventory: Option<StockItem>) {
        if let Some(item) = inventory {
            self.renderer.set_heading_pairs(&[(item.kind, item.count)]);
        } else {
            self.renderer.clear_headings();
        }
    }
}

// ----------------------------------------------
// BuildingInspector
// ----------------------------------------------

struct BuildingInspector {
    renderer: InspectorMenuRenderer,
}

impl GameObjectInspector for BuildingInspector {
    fn update_selection(&mut self, context: &GameUiContext, selected_tile: &Tile) {
        if let Some(building) = context.world.find_building_for_tile(selected_tile) {
            self.renderer.set_icon(context, selected_tile.icon_sprite(), selected_tile.kind());
            self.renderer.set_title(building.name());
            self.set_population_and_workers(building);
            self.set_stats_info(context, building);
        }
    }

    fn menu(&mut self) -> &mut UiMenu {
        self.renderer.menu()
    }
}

impl BuildingInspector {
    fn new(context: &mut GameUiContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
        Self {
            renderer: InspectorMenuRenderer::new(
                context,
                tile_inspector_menu_weak_ref,
                stringify!(BuildingInspector)
            )
        }
    }

    fn set_population_and_workers(&mut self, building: &Building) {
        let mut headings = InspectorMenuHeadings::new();

        // Population + Workers: Household.
        if let Some(population) = building.population() {
            add_heading!(&mut headings, "Residents", "{}/{}", population.count(), population.max());

            if let Some(workers) = building.workers() {
                let household = workers.as_household_worker_pool().expect("Expected Household!");
                let total_workers = household.total_workers();

                add_heading!(&mut headings, "Workers Available", "{}", total_workers);
                if total_workers != 0 {
                    add_heading!(&mut headings, "Workers Employed", "{}", household.employed_count());
                }
            }

            let house = building.as_house();
            let tax_generated = house.tax_generated();

            if tax_generated != 0 {
                add_heading!(&mut headings, "Tax", "Generated {} / Available {}", tax_generated, house.tax_available());
            } else {
                add_heading!(&mut headings, "Tax", "No Income Generated");
            }
        // Just Workers: Employer Building.
        } else if let Some(workers) = building.workers() {
            let employer = workers.as_employer().expect("Expected Employer Building!");
            if employer.min_employees() != 0 {
                add_heading!(&mut headings, "Workers", "{}/{}", employer.employee_count(), employer.max_employees());
                add_heading!(&mut headings, "Min Workers Required", "{}", employer.min_employees());
            }
        }

        self.renderer.set_headings(&headings);
    }

    fn set_stats_info(&mut self, context: &GameUiContext, building: &Building) {
        let body = {
            if building.is(BuildingKind::House) {
                Self::gather_house_stats(context, building)
            } else {
                Self::gather_building_stats(context, building)
            }
        };
        self.renderer.set_body(&body);
    }

    fn gather_house_stats(context: &GameUiContext, building: &Building) -> InspectorMenuBody {
        let mut body = InspectorMenuBody::new();

        let house = building.as_house();

        if !house.level().is_max() {
            let sim_context = context.new_sim_context();
            let building_context = building.new_context(&sim_context);

            if !building.is_linked_to_road(&sim_context) {
                add_body_line!(&mut body, "House lacks road access!");
            } else if !house.is_upgrade_available(&building_context) {
                add_body_line!(&mut body, "House has no room to expand!");
            } else {
                let upgrade_requirements = house.upgrade_requirements(&building_context);
                let has_required_resources = upgrade_requirements.has_required_resources();
                let has_required_services  = upgrade_requirements.has_required_services();

                if !has_required_resources {
                    add_body_line!(&mut body, "Resources required before it can upgrade:");
                    add_body_line!(&mut body, "{}", upgrade_requirements.resources_missing());
                }

                if !has_required_services {
                    add_body_line!(&mut body, "Services required before it can upgrade:");
                    add_body_line!(&mut body, "{}", upgrade_requirements.services_missing());
                }
            }
        } else {                
            add_body_line!(&mut body, "This house is upgraded to its highest level!");
        }

        let skip_empty = true;
        let stock = building.stock();

        if !stock.is_empty() {
            let stock_items = Self::gather_stock_items(&stock, skip_empty);
            if !stock_items.is_empty() {
                add_body_line!(&mut body, "Resources stocked:");
                body.append(&stock_items);
            }
        }

        body
    }

    fn gather_building_stats(context: &GameUiContext, building: &Building) -> InspectorMenuBody {
        let mut body = InspectorMenuBody::new();

        let is_operational = building.is_operational();
        if !is_operational {
            let sim_context = context.new_sim_context();
            let is_linked_to_road = building.is_linked_to_road(&sim_context);

            let has_min_required_workers = building.has_min_required_workers();
            let has_min_required_resources = building.has_min_required_resources();
            let is_production_halted = building.is_production_halted();

            if !is_linked_to_road {
                add_body_line!(&mut body, "Building not running because it lacks road access!");
            } else if !has_min_required_workers {
                add_body_line!(&mut body, "Building not running because it doesn't have enough workers!");
            } else if !has_min_required_resources {
                add_body_line!(&mut body, "Building not running because it doesn't have the required resources!");
            }

            if is_production_halted && has_min_required_workers && has_min_required_resources {
                // If we have workers and resources but halted production, our local output stock mut be full.
                add_body_line!(&mut body, "Production halted! Waiting for production stock to be shipped out.");
            }
        } else {
            let has_all_workers = building.workers_is_maxed();
            if has_all_workers {
                add_body_line!(&mut body, "Building is operational and running at full capacity!");
            } else {
                add_body_line!(&mut body, "Building is operational but doesn't have all required workers.");
            }
        }

        let skip_empty =
            building.archetype_kind() == BuildingArchetypeKind::ServiceBuilding ||
            building.archetype_kind() == BuildingArchetypeKind::StorageBuilding;

        let stock = building.stock();

        if !stock.is_empty() || !skip_empty {
            let stock_items = Self::gather_stock_items(&stock, skip_empty);
            if !stock_items.is_empty() || !skip_empty {
                add_body_line!(&mut body, "Stock:");
                body.append(&stock_items);
            }
        }

        body
    }

    fn gather_stock_items(stock: &[StockItem], skip_empty: bool) -> InspectorMenuBody {
        let mut body = InspectorMenuBody::new();

        if !stock.is_empty() || !skip_empty {
            let mut items_in_line = 0;

            for item in stock {
                if item.count != 0 || !skip_empty {
                    // Up to 3 items per line.
                    if items_in_line == 3 {
                        body.add_line(""); // newline.
                        items_in_line = 0;
                    } else if items_in_line != 0 {
                        body.add_str(" | ");
                    }

                    add_body_str!(&mut body, "{}: {}", item.kind, item.count);
                    items_in_line += 1;
                }
            }
        }

        body
    }
}

// ----------------------------------------------
// PropInspector
// ----------------------------------------------

struct PropInspector {
    renderer: InspectorMenuRenderer,
}

impl GameObjectInspector for PropInspector {
    fn update_selection(&mut self, context: &GameUiContext, selected_tile: &Tile) {
        if let Some(prop) = context.world.find_prop_for_tile(selected_tile) {
            self.renderer.set_icon(context, selected_tile.icon_sprite(), selected_tile.kind());
            self.renderer.set_title(prop.name());
            self.renderer.set_body_text(find_tile_description(selected_tile));
            self.set_harvestable_resource(prop.harvestable_resource(), prop.harvestable_amount());
        }
    }

    fn menu(&mut self) -> &mut UiMenu {
        self.renderer.menu()
    }
}

impl PropInspector {
    fn new(context: &mut GameUiContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
        Self {
            renderer: InspectorMenuRenderer::new(
                context,
                tile_inspector_menu_weak_ref,
                stringify!(PropInspector)
            )
        }
    }

    fn set_harvestable_resource(&mut self, resource: ResourceKind, amount: u32) {
        if !resource.is_empty() {
            self.renderer.set_heading_pairs(&[(resource, amount)]);
        } else {
            self.renderer.clear_headings();
        }
    }
}

// ----------------------------------------------
// TerrainInspector
// ----------------------------------------------

struct TerrainInspector {
    renderer: InspectorMenuRenderer,
}

impl GameObjectInspector for TerrainInspector {
    fn update_selection(&mut self, context: &GameUiContext, selected_tile: &Tile) {
        self.renderer.set_icon(context, selected_tile.icon_sprite(), selected_tile.kind());
        self.renderer.set_title(&snake_case_to_title::<128>(selected_tile.name()));
        self.renderer.set_body_text(find_tile_description(selected_tile));
    }

    fn menu(&mut self) -> &mut UiMenu {
        self.renderer.menu()
    }
}

impl TerrainInspector {
    fn new(context: &mut GameUiContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
        Self {
            renderer: InspectorMenuRenderer::new(
                context,
                tile_inspector_menu_weak_ref,
                stringify!(TerrainInspector),
            )
        }
    }
}
