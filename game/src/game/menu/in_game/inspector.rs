use crate::{
    format_fixed_string,
    ui::{UiFontScale, widgets::*},
    tile::{Tile, TileKind, sets::TileIconSprite},
    utils::{self, Vec2, mem::{RcMut, WeakMut, WeakRef}},
    game::{
        sim::resources::{ResourceKind, StockItem, Population, Workers},
        menu::{TileInspector, GameMenusContext, TEXT_BUTTON_HOVERED_SPRITE},
    },
};

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
            } else if selected_tile.is(TileKind::Prop) && selected_tile.path_kind().is_harvestable_tree() {
                Some(GameObjectInspectorKind::Prop)
            } else if selected_tile.is(TileKind::Terrain | TileKind::Rocks | TileKind::Vegetation) {
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
// Constants
// ----------------------------------------------

const INSPECTOR_MENU_BACKGROUND_SPRITE: &str = "misc/square_page_bg.png";

const INSPECTOR_MENU_FLAGS: UiMenuFlags =
    UiMenuFlags::from_bits_retain(
        UiMenuFlags::PauseSimIfOpen.bits()
        | UiMenuFlags::AlignCenter.bits()
        | UiMenuFlags::Modal.bits()
        | UiMenuFlags::CloseModalOnEscape.bits()
    );

const INSPECTOR_HEADING_FONT_SCALE: UiFontScale = UiFontScale(1.5);
const INSPECTOR_SUBHEADING_FONT_SCALE: UiFontScale = UiFontScale(1.0);
const INSPECTOR_BODY_FONT_SCALE: UiFontScale = UiFontScale(1.1);

fn calc_inspector_menu_size(context: &UiWidgetContext) -> Vec2 {
    Vec2::new(
        context.viewport_size.width  as f32 * 0.5 - 120.0,
        context.viewport_size.height as f32 * 0.5
    )
}

// ----------------------------------------------
// InspectorMenuHelper
// ----------------------------------------------

struct InspectorMenuHelper {
    menu: UiMenuRcMut,

    // Indices within `icon_and_heading_group`.
    icon_index: usize,
    heading_index: usize,

    // Indices withing `self.menu`.
    icon_and_heading_group_index: usize,
    body_index: Option<usize>,
}

impl InspectorMenuHelper {
    fn find_icon_and_heading_group(&mut self) -> &mut UiWidgetGroup {
        self.menu.widget_as_mut::<UiWidgetGroup>(self.icon_and_heading_group_index).unwrap()
    }

    fn find_icon(&mut self) -> &mut UiSpriteIcon {
        let icon_index = self.icon_index;
        let icon_and_heading_group = self.find_icon_and_heading_group();
        icon_and_heading_group.widget_as_mut::<UiSpriteIcon>(icon_index).unwrap()
    }

    fn find_heading(&mut self) -> &mut UiMenuHeading {
        let heading_index = self.heading_index;
        let icon_and_heading_group = self.find_icon_and_heading_group();
        icon_and_heading_group.widget_as_mut::<UiMenuHeading>(heading_index).unwrap()
    }

    fn find_body<Widget: UiWidget>(&mut self) -> &mut Widget {
        let body_index = self.body_index.expect("No menu body widget found!");
        self.menu.widget_as_mut::<Widget>(body_index).unwrap()
    }

    fn set_icon(&mut self, context: &mut UiWidgetContext, icon_sprite: TileIconSprite, scale: f32) {
        let icon = self.find_icon();

        let sprite = context.ui_sys.to_ui_texture(context.tex_cache, icon_sprite.tex_info.texture);
        icon.set_sprite(sprite);

        let tex_coords = icon_sprite.tex_info.coords;
        icon.set_tex_coords(tex_coords);

        let size = icon_sprite.size.to_vec2();
        icon.set_size(size * scale);
    }

    fn set_heading(&mut self, text: &str) {
        let heading = self.find_heading();

        // heading[0]: game object name
        heading.set_line_text(0, text);
    }

    fn set_subheading_1(&mut self, text: &str) {
        let heading = self.find_heading();

        // heading[1]: subheading 1 -> unit inventory | building population/workers
        heading.set_line_text(1, text);
    }

    fn set_subheading_2(&mut self, text: &str) {
        let heading = self.find_heading();

        // heading[2]: subheading 2 -> building population/workers
        heading.set_line_text(2, text);
    }

    fn new(context: &mut UiWidgetContext,
           tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut,
           menu_name: &str,
           body: Option<UiWidgetImpl>) -> Self
    {
        let icon = UiSpriteIcon::new(
            context,
            UiSpriteIconParams {
                size: Vec2::one(), // placeholder
                outline: true,
                clip_to_parent_menu: true,
                ..Default::default()
            }
        );

        let heading = UiMenuHeading::new(
            context,
            UiMenuHeadingParams {
                lines: vec![
                    (String::new(), INSPECTOR_HEADING_FONT_SCALE),    // placeholder: game object name
                    (String::new(), INSPECTOR_SUBHEADING_FONT_SCALE), // placeholder: unit inventory | building population/workers
                    (String::new(), INSPECTOR_SUBHEADING_FONT_SCALE), // placeholder: building population/workers
                ],
                center_vertically: false,
                center_horizontally: false,
                ..Default::default()
            }
        );

        let mut icon_and_heading_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                widget_spacing: 20.0,
                center_vertically: false,
                center_horizontally: true,
                stack_vertically: false,
                ..Default::default()
            }
        );

        let icon_index = icon_and_heading_group.add_widget(icon);
        let heading_index = icon_and_heading_group.add_widget(heading);

        let close_button_inspector_menu_weak_ref = tile_inspector_menu_weak_ref.clone();
        let close_button = UiTextButton::new(
            context,
            UiTextButtonParams {
                label: "Close".into(),
                size: UiTextButtonSize::Normal,
                hover: Some(TEXT_BUTTON_HOVERED_SPRITE),
                enabled: true,
                on_pressed: UiTextButtonPressed::with_closure(move |_, context| {
                    let mut inspector_menu = close_button_inspector_menu_weak_ref.upgrade().unwrap();
                    inspector_menu.close_inspector(context);
                }),
                ..Default::default()
            }
        );

        let mut button_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                center_vertically: false,
                center_horizontally: true,
                stack_vertically: false,
                ..Default::default()
            }
        );

        button_group.add_widget(close_button);

        let separator = UiSeparator::new(
            context,
            UiSeparatorParams {
                thickness: Some(10.0),
                ..Default::default()
            }
        );

        let mut menu = UiMenu::new(
            context,
            UiMenuParams {
                label: Some(menu_name.into()),
                flags: INSPECTOR_MENU_FLAGS,
                size: Some(calc_inspector_menu_size(context)),
                background: Some(INSPECTOR_MENU_BACKGROUND_SPRITE),
                ..Default::default()
            }
        );

        menu.add_widget(separator.clone());
        let icon_and_heading_group_index = menu.add_widget(icon_and_heading_group);

        let body_index = if let Some(body) = body {
            menu.add_widget(separator.clone());
            Some(menu.add_widget(body))
        } else {
            None
        };

        menu.add_widget(separator.clone());
        menu.add_widget(button_group);

        Self {
            menu,
            icon_index,
            heading_index,
            icon_and_heading_group_index,
            body_index,
        }
    }
}

// ----------------------------------------------
// GameObjectInspector / GameObjectInspectorKind
// ----------------------------------------------

trait GameObjectInspector {
    fn open(&mut self, context: &mut UiWidgetContext, selected_tile: &Tile);
    fn close(&mut self, context: &mut UiWidgetContext);
    fn draw(&mut self, context: &mut UiWidgetContext) -> bool; // Returns true if menu still open.
}

#[derive(Copy, Clone)]
enum GameObjectInspectorKind {
    Unit,
    Building,
    Prop,
    Terrain,
}

// ----------------------------------------------
// UnitInspector
// ----------------------------------------------

struct UnitInspector {
    helper: InspectorMenuHelper,
}

impl GameObjectInspector for UnitInspector {
    fn open(&mut self, context: &mut UiWidgetContext, selected_tile: &Tile) {
        if let Some(unit) = context.world.find_unit_for_tile(selected_tile) {
            self.set_unit_icon(context, unit.icon_sprite());
            self.set_unit_name(unit.name());
            self.set_unit_inventory(unit.peek_inventory());
            self.set_unit_dialog_text(unit.dialog_text());

            self.helper.menu.open(context);
        }
    }

    fn close(&mut self, context: &mut UiWidgetContext) {
        self.helper.menu.close(context);
    }

    fn draw(&mut self, context: &mut UiWidgetContext) -> bool {
        self.helper.menu.draw(context);
        self.helper.menu.is_open()
    }
}

impl UnitInspector {
    fn set_unit_icon(&mut self, context: &mut UiWidgetContext, icon_sprite: TileIconSprite) {
        const SCALE: f32 = 2.0;
        self.helper.set_icon(context, icon_sprite, SCALE);
    }

    fn set_unit_name(&mut self, name: &str) {
        self.helper.set_heading(name);
    }

    fn set_unit_inventory(&mut self, inventory: Option<StockItem>) {
        if let Some(item) = inventory {
            let inventory = format_fixed_string!(128, "{}: {}", item.kind, item.count);
            self.helper.set_subheading_1(&inventory);
        } else {
            self.helper.set_subheading_1("");
        }
    }

    fn set_unit_dialog_text(&mut self, text: &str) {
        let body_text = self.helper.find_body::<UiMenuHeading>();
        let lines = body_text.lines_mut();

        lines.clear();

        for line in text.split('\n') {
            lines.push((line.into(), INSPECTOR_BODY_FONT_SCALE));
        }
    }

    fn new(context: &mut UiWidgetContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
        let body_text = UiMenuHeading::new(
            context,
            UiMenuHeadingParams {
                lines: vec![(String::new(), INSPECTOR_BODY_FONT_SCALE)], // placeholder
                center_vertically: false,
                center_horizontally: true,
                ..Default::default()
            }
        );

        let helper = InspectorMenuHelper::new(
            context,
            tile_inspector_menu_weak_ref,
            "UnitInspector",
            Some(UiWidgetImpl::from(body_text))
        );

        Self { helper }
    }
}

// ----------------------------------------------
// BuildingInspector
// ----------------------------------------------

struct BuildingInspector {
    helper: InspectorMenuHelper,
}

impl GameObjectInspector for BuildingInspector {
    fn open(&mut self, context: &mut UiWidgetContext, selected_tile: &Tile) {
        if let Some(building) = context.world.find_building_for_tile(selected_tile) {
            self.set_building_icon(context, building.icon_sprite());
            self.set_building_name(building.name());
            self.set_building_population_workers(building.population(), building.workers());

            self.helper.menu.open(context);
        }
    }

    fn close(&mut self, context: &mut UiWidgetContext) {
        self.helper.menu.close(context);
    }

    fn draw(&mut self, context: &mut UiWidgetContext) -> bool {
        self.helper.menu.draw(context);
        self.helper.menu.is_open()
    }
}

impl BuildingInspector {
    fn set_building_icon(&mut self, context: &mut UiWidgetContext, icon_sprite: TileIconSprite) {
        const SCALE: f32 = 1.0;
        self.helper.set_icon(context, icon_sprite, SCALE);
    }

    fn set_building_name(&mut self, name: &str) {
        self.helper.set_heading(name);
    }

    fn set_building_population_workers(&mut self, population: Option<Population>, workers: Option<&Workers>) {
        // Clear previous first:
        self.helper.set_subheading_1("");
        self.helper.set_subheading_2("");

        // Population + Workers: Household.
        if let Some(population) = population {
            {
                let plural = population.count() != 1;
                let line1 = format_fixed_string!(
                    128,
                    "{} resident{}, house capacity {}",
                    population.count(),
                    if plural { "s" } else { "" },
                    population.max());

                self.helper.set_subheading_1(&line1);
            }

            if let Some(workers) = workers {
                let household = workers.as_household_worker_pool().expect("Expected Household!");
                if household.total_workers() != 0 {
                    let plural = household.total_workers() != 1;
                    let line2 = format_fixed_string!(
                        128,
                        "{} worker{}, {} employed",
                        household.total_workers(),
                        if plural { "s" } else { "" },
                        household.employed_count());

                    self.helper.set_subheading_2(&line2);
                }
            }
        // Just Workers: Employer building.
        } else if let Some(workers) = workers {
            let employer = workers.as_employer().expect("Expected Employer!");
            if employer.min_employees() != 0 {
                let plural = employer.employee_count() != 1;
                let line1 = format_fixed_string!(
                    128,
                    "{} worker{} out of {}",
                    employer.employee_count(),
                    if plural { "s" } else { "" },
                    employer.max_employees());

                let line2 = format_fixed_string!(
                    128,
                    "Minimum required {}",
                    employer.min_employees());

                self.helper.set_subheading_1(&line1);
                self.helper.set_subheading_2(&line2);
            }
        }
    }

    fn new(context: &mut UiWidgetContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
        let helper = InspectorMenuHelper::new(
            context,
            tile_inspector_menu_weak_ref,
            "BuildingInspector",
            None
        );

        Self { helper }
    }
}

// ----------------------------------------------
// PropInspector
// ----------------------------------------------

struct PropInspector {
    helper: InspectorMenuHelper,
}

impl GameObjectInspector for PropInspector {
    fn open(&mut self, context: &mut UiWidgetContext, selected_tile: &Tile) {
        if let Some(prop) = context.world.find_prop_for_tile(selected_tile) {
            self.set_prop_icon(context, prop.icon_sprite());
            self.set_prop_name(prop.name());
            self.set_prop_harvestable_resource(prop.harvestable_resource(), prop.harvestable_amount());

            self.helper.menu.open(context);
        }
    }

    fn close(&mut self, context: &mut UiWidgetContext) {
        self.helper.menu.close(context);
    }

    fn draw(&mut self, context: &mut UiWidgetContext) -> bool {
        self.helper.menu.draw(context);
        self.helper.menu.is_open()
    }
}

impl PropInspector {
    fn set_prop_icon(&mut self, context: &mut UiWidgetContext, icon_sprite: TileIconSprite) {
        const SCALE: f32 = 1.0;
        self.helper.set_icon(context, icon_sprite, SCALE);
    }

    fn set_prop_name(&mut self, name: &str) {
        self.helper.set_heading(name);
    }

    fn set_prop_harvestable_resource(&mut self, resource: ResourceKind, amount: u32) {
        if !resource.is_empty() {
            let harvestable = format_fixed_string!(128, "{}: {}", resource, amount);
            self.helper.set_subheading_1(&harvestable);
        } else {
            self.helper.set_subheading_1("");
        }
    }

    fn new(context: &mut UiWidgetContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
        let helper = InspectorMenuHelper::new(
            context,
            tile_inspector_menu_weak_ref,
            "PropInspector",
            None
        );

        Self { helper }
    }
}

// ----------------------------------------------
// TerrainInspector
// ----------------------------------------------

struct TerrainInspector {
    helper: InspectorMenuHelper,
}

impl GameObjectInspector for TerrainInspector {
    fn open(&mut self, context: &mut UiWidgetContext, selected_tile: &Tile) {
        self.set_tile_icon(context, selected_tile.icon_sprite());
        self.set_tile_name(selected_tile.name());

        self.helper.menu.open(context);
    }

    fn close(&mut self, context: &mut UiWidgetContext) {
        self.helper.menu.close(context);
    }

    fn draw(&mut self, context: &mut UiWidgetContext) -> bool {
        self.helper.menu.draw(context);
        self.helper.menu.is_open()
    }
}

impl TerrainInspector {
    fn set_tile_icon(&mut self, context: &mut UiWidgetContext, icon_sprite: TileIconSprite) {
        const SCALE: f32 = 1.0;
        self.helper.set_icon(context, icon_sprite, SCALE);
    }

    fn set_tile_name(&mut self, name: &str) {
        self.helper.set_heading(&utils::fixed_string::snake_case_to_title::<128>(name));
    }

    fn new(context: &mut UiWidgetContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
        let helper = InspectorMenuHelper::new(
            context,
            tile_inspector_menu_weak_ref,
            "TerrainInspector",
            None
        );

        Self { helper }
    }
}
