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
// TODO:
// ----------------------------------------------

// Fix UI layout issues:
// - Close button should be always at the bottom of the window.
// - Red/yellow colored text to highlight missing workers/resources.
//
// Building Inspector:
// - Display overview of building function:
//   - Houses: Reason for not upgrading: missing resources / services.
//   - Other buildings: If operational / not operational due to missing workers or resources.
// - Display building stock. Read only mode for now.
//
// Prop/Tile Inspector:
// - Display a simple overview / tagline about the prop/tile.

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

const FMT_STRING_SIZE: usize = 128;

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

    fn set_icon(&mut self, context: &mut UiWidgetContext, icon_sprite: TileIconSprite) {
        let icon = self.find_icon();

        let sprite = context.ui_sys.to_ui_texture(context.tex_cache, icon_sprite.tex_info.texture);
        icon.set_sprite(sprite);

        let tex_coords = icon_sprite.tex_info.coords;
        icon.set_tex_coords(tex_coords);

        // Scale proportionally to desired min/max icon size:
        const MIN_SIZE: f32 = 64.0;
        const MAX_SIZE: f32 = 140.0;

        let size = icon_sprite.size.to_vec2();

        let min_scale_x = MIN_SIZE / size.x;
        let max_scale_x = MAX_SIZE / size.x;

        let min_scale_y = MIN_SIZE / size.y;
        let max_scale_y = MAX_SIZE / size.y;

        let min_scale = min_scale_x.max(min_scale_y);
        let max_scale = max_scale_x.min(max_scale_y);

        let scale = 1.0_f32.clamp(min_scale, max_scale);
        let scaled_size = size * scale;

        icon.set_size(scaled_size);
    }

    fn set_heading(&mut self, text: &str) {
        let heading = self.find_heading();

        // heading[0]: game object name
        heading.set_line_string(0, text);
    }

    fn set_subheading_1(&mut self, text: &str) {
        let heading = self.find_heading();

        // heading[1]: subheading 1 -> unit inventory | building population/workers
        heading.set_line_string(1, text);
    }

    fn set_subheading_2(&mut self, text: &str) {
        let heading = self.find_heading();

        // heading[2]: subheading 2 -> building population/workers
        heading.set_line_string(2, text);
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
                    // placeholder: game object name
                    UiText { string: String::new(), font_scale: INSPECTOR_HEADING_FONT_SCALE, color: None },
                    // placeholder: unit inventory | building population/workers
                    UiText { string: String::new(), font_scale: INSPECTOR_SUBHEADING_FONT_SCALE, color: None },
                    // placeholder: building population/workers
                    UiText { string: String::new(), font_scale: INSPECTOR_SUBHEADING_FONT_SCALE, color: None },
                ],
                center_vertically: false,
                center_horizontally: false,
                ..Default::default()
            }
        );

        let mut icon_and_heading_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                widget_spacing: Vec2::new(20.0, 8.0),
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
        self.helper.set_icon(context, selected_tile.icon_sprite());

        if let Some(unit) = context.world.find_unit_for_tile(selected_tile) {
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
    fn new(context: &mut UiWidgetContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
        let body_text = UiMenuHeading::new(
            context,
            UiMenuHeadingParams {
                // placeholder
                lines: vec![UiText { string: String::new(), font_scale: INSPECTOR_BODY_FONT_SCALE, color: None }],
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

    fn set_unit_name(&mut self, name: &str) {
        self.helper.set_heading(name);
    }

    fn set_unit_inventory(&mut self, inventory: Option<StockItem>) {
        if let Some(item) = inventory {
            let inventory = format_fixed_string!(FMT_STRING_SIZE, "{}: {}", item.kind, item.count);
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
            lines.push(UiText { string: line.into(), font_scale: INSPECTOR_BODY_FONT_SCALE, color: None });
        }
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
        self.helper.set_icon(context, selected_tile.icon_sprite());

        if let Some(building) = context.world.find_building_for_tile(selected_tile) {
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
    fn new(context: &mut UiWidgetContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
        let helper = InspectorMenuHelper::new(
            context,
            tile_inspector_menu_weak_ref,
            "BuildingInspector",
            None
        );

        Self { helper }
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
                    FMT_STRING_SIZE,
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
                        FMT_STRING_SIZE,
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
                    FMT_STRING_SIZE,
                    "{} worker{} out of {}",
                    employer.employee_count(),
                    if plural { "s" } else { "" },
                    employer.max_employees());

                let line2 = format_fixed_string!(
                    FMT_STRING_SIZE,
                    "Minimum required {}",
                    employer.min_employees());

                self.helper.set_subheading_1(&line1);
                self.helper.set_subheading_2(&line2);
            }
        }
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
        self.helper.set_icon(context, selected_tile.icon_sprite());

        if let Some(prop) = context.world.find_prop_for_tile(selected_tile) {
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
    fn new(context: &mut UiWidgetContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
        let helper = InspectorMenuHelper::new(
            context,
            tile_inspector_menu_weak_ref,
            "PropInspector",
            None
        );

        Self { helper }
    }

    fn set_prop_name(&mut self, name: &str) {
        self.helper.set_heading(name);
    }

    fn set_prop_harvestable_resource(&mut self, resource: ResourceKind, amount: u32) {
        if !resource.is_empty() {
            let harvestable = format_fixed_string!(FMT_STRING_SIZE, "{}: {}", resource, amount);
            self.helper.set_subheading_1(&harvestable);
        } else {
            self.helper.set_subheading_1("");
        }
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
        self.helper.set_icon(context, selected_tile.icon_sprite());
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
    fn new(context: &mut UiWidgetContext, tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut) -> Self {
        let helper = InspectorMenuHelper::new(
            context,
            tile_inspector_menu_weak_ref,
            "TerrainInspector",
            None
        );

        Self { helper }
    }

    fn set_tile_name(&mut self, name: &str) {
        self.helper.set_heading(&utils::fixed_string::snake_case_to_title::<FMT_STRING_SIZE>(name));
    }
}
