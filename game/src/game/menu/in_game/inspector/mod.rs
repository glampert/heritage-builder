use arrayvec::{ArrayVec, ArrayString};

use crate::{
    format_fixed_string,
    append_fixed_string,
    tile::{Tile, TileKind},
    ui::widgets::{UiWidgetContext, UiWidget, UiMenu},
    utils::{mem::{RcMut, WeakMut, WeakRef}, fixed_string::snake_case_to_title},
    game::{
        building::{Building, BuildingKind, BuildingArchetypeKind},
        sim::resources::{ResourceKind, StockItem},
        menu::{TileInspector, GameMenusContext},
    },
};

mod renderer;
use renderer::InspectorMenuRenderer;

// TODO: Fix UI layout issues:
// - Close button should be always at the bottom of the window.
// - Adjust menu window size to fit the content, if larger than initial size.
// - Red/yellow colored text to highlight missing workers/resources.

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
    fn open(&mut self, context: &GameMenusContext) {
        if let Some(selected_tile) = context.topmost_selected_tile() {
            self.open_inspector(&mut context.as_ui_widget_context(), selected_tile);
        }
    }

    fn close(&mut self, context: &GameMenusContext) {
        self.close_inspector(&mut context.as_ui_widget_context());
    }
}

impl TileInspectorMenu {
    pub fn new(context: &mut UiWidgetContext) -> TileInspectorMenuRcMut {
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

    pub fn draw(&mut self, context: &mut UiWidgetContext) {
        if let Some(inspector) = self.current_inspector() {
            let is_open = inspector.draw(context);

            // If the menu was closed just now, clear current inspector.
            if !is_open {
                self.close_inspector(context);
            }
        }
    }

    fn open_inspector(&mut self, context: &mut UiWidgetContext, selected_tile: &Tile) {
        debug_assert!(self.current_inspector_kind.is_none());

        self.current_inspector_kind = {
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
            inspector.open(context, selected_tile);
        }
    }

    fn close_inspector(&mut self, context: &mut UiWidgetContext) {
        if let Some(inspector) = self.current_inspector() {
            inspector.close(context);
            self.current_inspector_kind = None;
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
    fn update_selection(&mut self, context: &mut UiWidgetContext, selected_tile: &Tile);
    fn menu(&mut self) -> &mut UiMenu;

    fn open(&mut self, context: &mut UiWidgetContext, selected_tile: &Tile) {
        self.update_selection(context, selected_tile);
        self.menu().open(context);
    }

    fn close(&mut self, context: &mut UiWidgetContext) {
        self.menu().close(context);
    }

    // Returns true if menu still open.
    fn draw(&mut self, context: &mut UiWidgetContext) -> bool {
        let menu = self.menu();
        menu.draw(context);
        menu.is_open()
    }
}

// ----------------------------------------------
// UnitInspector
// ----------------------------------------------

struct UnitInspector {
    renderer: InspectorMenuRenderer,
}

impl GameObjectInspector for UnitInspector {
    fn update_selection(&mut self, context: &mut UiWidgetContext, selected_tile: &Tile) {
        if let Some(unit) = context.world.find_unit_for_tile(selected_tile) {
            self.renderer.set_icon(context, selected_tile.icon_sprite(), selected_tile.kind());
            self.renderer.set_title(unit.name());
            self.renderer.set_body_text(unit.dialog_text());
            self.set_inventory(unit.peek_inventory());
        }
    }

    fn menu(&mut self) -> &mut UiMenu {
        self.renderer.menu()
    }
}

impl UnitInspector {
    fn new(context: &mut UiWidgetContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
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
            self.renderer.set_headings(&[(item.kind, item.count)]);
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
    fn update_selection(&mut self, context: &mut UiWidgetContext, selected_tile: &Tile) {
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
    fn new(context: &mut UiWidgetContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
        Self {
            renderer: InspectorMenuRenderer::new(
                context,
                tile_inspector_menu_weak_ref,
                stringify!(BuildingInspector)
            )
        }
    }

    fn set_population_and_workers(&mut self, building: &Building) {
        const CAP: usize = 128;
        let mut key_vals = ArrayVec::<(&str, ArrayString<CAP>), 4>::new();

        // Population + Workers: Household.
        if let Some(population) = building.population() {
            key_vals.push(("Residents", format_fixed_string!(CAP, "{}/{}", population.count(), population.max())));

            if let Some(workers) = building.workers() {
                let household = workers.as_household_worker_pool().expect("Expected Household!");
                let total_workers = household.total_workers();

                key_vals.push(("Workers", format_fixed_string!(CAP, "{}", total_workers)));
                if total_workers != 0 {
                    key_vals.push(("Employed", format_fixed_string!(CAP, "{}", household.employed_count())));
                }
            }
        // Just Workers: Employer Building.
        } else if let Some(workers) = building.workers() {
            let employer = workers.as_employer().expect("Expected Employer Building!");
            if employer.min_employees() != 0 {
                key_vals.push(("Workers", format_fixed_string!(CAP, "{}/{}", employer.employee_count(), employer.max_employees())));
                key_vals.push(("Minimum Required", format_fixed_string!(CAP, "{}", employer.min_employees())));
            }
        }

        self.renderer.set_headings(&key_vals);
    }

    fn set_stats_info(&mut self, context: &mut UiWidgetContext, building: &Building) {
        let text = {
            if building.is(BuildingKind::House) {
                Self::gather_house_stats(context, building)
            } else {
                Self::gather_building_stats(context, building)
            }
        };
        self.renderer.set_body_text(&text);
    }

    fn gather_house_stats(context: &mut UiWidgetContext, building: &Building) -> ArrayString<1024> {
        let mut text = ArrayString::new();

        let house = building.as_house();
        let tax_generated = house.tax_generated();

        if tax_generated != 0 {
            append_fixed_string!(&mut text, "Tax income generated: {} / available: {}\n",
                tax_generated, house.tax_available());
        } else {
            append_fixed_string!(&mut text, "No tax income generated.\n");
        }

        if !house.level().is_max() {
            let query = context.new_sim_query();
            let building_context = building.new_context(&query);

            if !building.is_linked_to_road(&query) {
                append_fixed_string!(&mut text, "House lacks road access!\n");
            } else if !house.is_upgrade_available(&building_context) {
                append_fixed_string!(&mut text, "House has no room to expand!\n");
            } else {
                let upgrade_requirements = house.upgrade_requirements(&building_context);
                let has_required_resources = upgrade_requirements.has_required_resources();
                let has_required_services  = upgrade_requirements.has_required_services();

                if !has_required_resources {
                    append_fixed_string!(&mut text, "Resources required before it can upgrade:\n");
                    append_fixed_string!(&mut text, "{}\n", upgrade_requirements.resources_missing());
                }

                if !has_required_services {
                    append_fixed_string!(&mut text, "Services required before it can upgrade:\n");
                    append_fixed_string!(&mut text, "{}\n", upgrade_requirements.services_missing());
                }
            }
        } else {                
            append_fixed_string!(&mut text, "This house is upgraded to its highest level!\n");
        }

        let skip_empty = true;
        let stock = building.stock();

        if !stock.is_empty() {
            let list = Self::gather_stock_items(&stock, skip_empty);
            if !list.is_empty() {
                append_fixed_string!(&mut text, "Resources stocked:\n");
                text.push_str(&list);
            }
        }

        text
    }

    fn gather_building_stats(context: &mut UiWidgetContext, building: &Building) -> ArrayString<1024> {
        let mut text = ArrayString::new();

        let is_operational = building.is_operational();
        if !is_operational {
            let query = context.new_sim_query();
            let is_linked_to_road = building.is_linked_to_road(&query);

            let has_min_required_workers = building.has_min_required_workers();
            let has_min_required_resources = building.has_min_required_resources();
            let is_production_halted = building.is_production_halted();

            if !is_linked_to_road {
                append_fixed_string!(&mut text, "Building not running because it lacks road access!\n");
            } else if !has_min_required_workers {
                append_fixed_string!(&mut text, "Building not running because it doesn't have enough workers!\n");
            } else if !has_min_required_resources {
                append_fixed_string!(&mut text, "Building not running because it doesn't have the required resources!\n");
            }

            if is_production_halted && has_min_required_workers && has_min_required_resources {
                // If we have workers and resources but halted production, our local output stock mut be full.
                append_fixed_string!(&mut text, "Production halted! Waiting for production stock to be shipped out.\n");
            }
        } else {
            let has_all_workers = building.workers_is_maxed();
            if has_all_workers {
                append_fixed_string!(&mut text, "Building is operational and running at full capacity!\n");
            } else {
                append_fixed_string!(&mut text, "Building is operational but doesn't have all required workers.\n");
            }
        }

        let skip_empty =
            building.archetype_kind() == BuildingArchetypeKind::ServiceBuilding ||
            building.archetype_kind() == BuildingArchetypeKind::StorageBuilding;

        let stock = building.stock();

        if !stock.is_empty() || !skip_empty {
            let list = Self::gather_stock_items(&stock, skip_empty);
            if !list.is_empty() || !skip_empty {
                append_fixed_string!(&mut text, "Stock:\n");
                text.push_str(&list);
            }
        }

        text
    }

    fn gather_stock_items(stock: &[StockItem], skip_empty: bool) -> ArrayString<1024> {
        let mut text = ArrayString::new();

        if !stock.is_empty() || !skip_empty {
            let mut items_in_line = 0;

            for item in stock {
                if item.count != 0 || !skip_empty {
                    // Up to 3 items per line.
                    if items_in_line == 3 {
                        append_fixed_string!(&mut text, "\n");
                        items_in_line = 0;
                    } else if items_in_line != 0 {
                        append_fixed_string!(&mut text, " | ");
                    }

                    append_fixed_string!(&mut text, "{}: {}", item.kind, item.count);
                    items_in_line += 1;
                }
            }
        }

        text
    }
}

// ----------------------------------------------
// PropInspector
// ----------------------------------------------

struct PropInspector {
    renderer: InspectorMenuRenderer,
}

impl GameObjectInspector for PropInspector {
    fn update_selection(&mut self, context: &mut UiWidgetContext, selected_tile: &Tile) {
        if let Some(prop) = context.world.find_prop_for_tile(selected_tile) {
            self.renderer.set_icon(context, selected_tile.icon_sprite(), selected_tile.kind());
            self.renderer.set_title(prop.name());
            self.renderer.set_body_text(selected_tile.description());
            self.set_harvestable_resource(prop.harvestable_resource(), prop.harvestable_amount());
        }
    }

    fn menu(&mut self) -> &mut UiMenu {
        self.renderer.menu()
    }
}

impl PropInspector {
    fn new(context: &mut UiWidgetContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
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
            self.renderer.set_headings(&[(resource, amount)]);
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
    fn update_selection(&mut self, context: &mut UiWidgetContext, selected_tile: &Tile) {
        self.renderer.set_icon(context, selected_tile.icon_sprite(), selected_tile.kind());
        self.renderer.set_title(&snake_case_to_title::<128>(selected_tile.name()));
        self.renderer.set_body_text(selected_tile.description());
    }

    fn menu(&mut self) -> &mut UiMenu {
        self.renderer.menu()
    }
}

impl TerrainInspector {
    fn new(context: &mut UiWidgetContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
        Self {
            renderer: InspectorMenuRenderer::new(
                context,
                tile_inspector_menu_weak_ref,
                stringify!(TerrainInspector),
            )
        }
    }
}
