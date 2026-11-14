// The build palette (docs/plans/realm-ui.md, Phase 5: P5c; build-palette-<realm>.jpg): the sector
// column at the window's right edge, a sector icon tile per TilePaletteMainButtonDef (clear land,
// housing, roads, the sectors, undo and redo, with rules between the groups), and a sector's
// buildings as a grid of icon tiles, four to a row, left of its tile. Hovering a building shows its
// realm name, size, cost and materials against the storage yards, and why it can't be built (no
// gold, debt, materials short, or locked by the mission: BuildingAvailability); a building that
// can't be built stays in the grid, disabled. Clicking one picks it and closes the grid.
// Paths: the column is `TilePaletteMenu`, its tiles named by their labels (`TilePaletteMenu/services`,
// tooltip "Services"); a grid is `TilePaletteChildMenu: <label>`, its tiles named by the building's
// tile name in title case ("Apothecary", "Dirt Road"), stable in every realm.

use std::{cell::RefCell, rc::Rc};

use arrayvec::ArrayVec;
use common::{
    self,
    Color,
    Rect,
    RectTexCoords,
    Vec2,
    constants::BASE_TILE_SIZE_F32,
    coords::WorldToScreenTransform,
    hash,
    mem::{RcMut, WeakMut, WeakRef},
};
use engine::{
    app::input::{InputAction, MouseButton},
    file_sys::paths::PathRef,
    render::texture::TextureHandle,
    ui::{
        self,
        UiInputEvent,
        sound::{self, UiSoundKey},
        text::UiTextCategory,
        widgets::*,
    },
};
use strum::{EnumCount, EnumIter, EnumProperty, IntoEnumIterator};

use super::layout;
use crate::{
    campaign::BuildingAvailability,
    menu::{skin::{HudGlyphGroup, HudTileKind, SkinPanel}, *},
    realm::Realm,
    tile::{
        TileKind,
        rendering::INVALID_TILE_COLOR,
        sets::{
            OBJECTS_BUILDINGS_CATEGORY,
            PresetTiles,
            TERRAIN_LAND_CATEGORY,
            TileDef,
            TileDefHandle,
            TileSector,
            TileSets,
        },
    },
    ui_context::GameUiContext,
    undo_redo,
    world::{city_stock::CityStock, object::tile_cost_text},
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

const TILE_PALETTE_MAIN_BUTTON_COUNT: usize = TilePaletteMainButtonDef::COUNT;

// The grid: buildings per row and the room between tiles, in points.
const GRID_COLUMNS: usize = 4;
const GRID_GAP: f32 = 10.0;
const GRID_TITLE_HEIGHT: f32 = 20.0; // The title's row, the first row's crests below it.
const GRID_TITLE_BASELINE: f32 = 15.0;

// The rule between the column's groups (the column's spacing round it).
const SEPARATOR_LINE: f32 = 2.0;

// A few points spare in the column, so its window never scrolls.
const COLUMN_SLACK: f32 = 8.0;

// The smallest the column's tiles get on a short window.
const MIN_COLUMN_SCALE: f32 = 0.35;

// ----------------------------------------------
// TilePaletteMainButtonDef
// ----------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum TilePaletteMainButtonDef {
    #[strum(props(Label = "clear_land", Glyph = "clear", Title = "sector.clear"))]
    ClearLand,

    #[strum(props(Label = "housing", Glyph = "housing", Title = "sector.housing"))]
    Housing,

    #[strum(props(Label = "roads", Glyph = "roads", Title = "sector.roads", SeparatorFollows = true))]
    Roads,

    #[strum(props(Label = "food_and_farming", Glyph = "food", Title = "sector.food", DisabledIfEmpty = true))]
    FoodAndFarming,

    #[strum(props(Label = "industry_and_resources", Glyph = "industry", Title = "sector.industry", DisabledIfEmpty = true))]
    IndustryAndResources,

    #[strum(props(Label = "services", Glyph = "services", Title = "sector.services", DisabledIfEmpty = true))]
    Services,

    #[strum(props(Label = "infrastructure", Glyph = "infrastructure", Title = "sector.infrastructure", DisabledIfEmpty = true))]
    Infrastructure,

    #[strum(props(Label = "culture_and_religion", Glyph = "culture", Title = "sector.culture", DisabledIfEmpty = true))]
    CultureAndReligion,

    #[strum(props(Label = "trade_and_economy", Glyph = "trade", Title = "sector.trade", DisabledIfEmpty = true))]
    TradeAndEconomy,

    #[strum(props(Label = "beautification", Glyph = "beauty", Title = "sector.beauty", DisabledIfEmpty = true, SeparatorFollows = true))]
    Beautification,

    #[strum(props(Label = "undo", Glyph = "undo", Title = "sector.undo"))]
    Undo,

    #[strum(props(Label = "redo", Glyph = "redo", Title = "sector.redo"))]
    Redo,
}

impl TilePaletteMainButtonDef {
    fn separator_follows(self) -> bool {
        self.get_bool("SeparatorFollows").is_some_and(|val| val)
    }

    fn disabled_if_empty(self) -> bool {
        self.get_bool("DisabledIfEmpty").is_some_and(|val| val)
    }

    fn glyph(self) -> &'static str {
        self.get_str("Glyph").unwrap_or_default()
    }

    // Its tooltip: the sector's title ("Services"); the housing tile names its vacant lot and cost.
    fn tooltip(self) -> String {
        if self == Self::Housing
            && let Some(tile_def) = PresetTiles::VacantLot.find_tile_def()
        {
            let cost = tile_cost_text(tile_def);
            return format!("{}\n{}", ui::text::get(UiTextCategory::Hud, "sector.housing"), ui::text::format(UiTextCategory::Hud, "sector.housing.tooltip", &[("cost", &cost)]));
        }
        ui::text::get(UiTextCategory::Hud, self.get_str("Title").unwrap_or_default()).to_string()
    }

    fn build_child_button_defs(self) -> Vec<TilePaletteChildButtonDef> {
        let mut children = Vec::new();
        let button_name = self.name();

        TileSets::get().for_each_category(|_, category| {
            if category.hash == OBJECTS_BUILDINGS_CATEGORY.hash || category.hash == TERRAIN_LAND_CATEGORY.hash {
                category.for_each_tile_def(|tile_def| {
                    if tile_def.sector != TileSector::Housing && // NOTE: No child menu for housing; selects vacant lot instead.
                       tile_def.sector.name() == button_name
                    {
                        children.push(TilePaletteChildButtonDef::new(tile_def));
                    }
                    true
                });
            }
            true
        });

        children
    }

    fn to_tile_selection(self, tile_def_handle: Option<TileDefHandle>) -> TilePaletteSelection {
        match self {
            TilePaletteMainButtonDef::ClearLand => TilePaletteSelection::Clear,
            TilePaletteMainButtonDef::Housing => {
                if let Some(tile_def) = PresetTiles::VacantLot.find_tile_def() {
                    TilePaletteSelection::Tile(TileDefHandle::from_tile_def(tile_def))
                } else {
                    TilePaletteSelection::None
                }
            }
            _ => TilePaletteSelection::Tile(tile_def_handle.unwrap()),
        }
    }
}

impl ButtonDef for TilePaletteMainButtonDef {}

// ----------------------------------------------
// TilePaletteChildButtonDef
// ----------------------------------------------

struct TilePaletteChildButtonDef {
    label: String, // The tile's name in title case: the tile's automation path.
    tile_def_handle: TileDefHandle,
}

impl TilePaletteChildButtonDef {
    fn new(tile_def: &TileDef) -> Self {
        let label = common::fixed_string::snake_case_to_title::<128>(&tile_def.name).to_string();
        Self { label, tile_def_handle: TileDefHandle::from_tile_def(tile_def) }
    }
}

// The tile def a palette tile was built for (the tile sets don't change while the game runs).
fn tile_def_of(handle: TileDefHandle) -> &'static TileDef {
    TileSets::get().handle_to_tile_def(handle).expect("A palette tile names a tile def")
}

// The name a realm gives a building ("Prasat"), else the tile's in title case.
fn building_name(realm: Realm, tile_def: &TileDef) -> String {
    let key = format!("building.{realm}.{}", tile_def.name);
    match ui::text::find_str(UiTextCategory::Realm, hash::fnv1a_from_str(&key)) {
        Some(name) => name.to_string(),
        None => common::fixed_string::snake_case_to_title::<128>(&tile_def.name).to_string(),
    }
}

// Why the player can't build `tile_def` now, if they can't: locked by the mission, the treasury in
// debt or short of its gold, or a material short in the storage yards.
fn unbuildable_reason(context: &GameUiContext, tile_def: &TileDef) -> Option<String> {
    if !BuildingAvailability::current().allows(&tile_def.name) {
        return Some(ui::text::get(UiTextCategory::Hud, "palette.locked").to_string());
    }
    if cheats::get().ignore_tile_cost {
        return None;
    }
    let finance = context.sim.finance();
    if tile_def.cost > 0 && finance.is_in_debt() {
        return Some(ui::text::get(UiTextCategory::Hud, "palette.in_debt").to_string());
    }
    if tile_def.cost > 0 && !finance.can_afford(tile_def.cost) {
        return Some(ui::text::get(UiTextCategory::Hud, "palette.cannot_afford").to_string());
    }
    tile_def.materials.iter().find(|item| CityStock::available(context.world, item.kind) < item.count).map(|item| {
        ui::text::format(UiTextCategory::Hud, "palette.short", &[("name", &item.kind.display_name())])
    })
}

// A building's tooltip: its realm name, size and gold cost, its materials against what the storage
// yards hold (short ones marked), and why it can't be built.
fn building_tooltip(context: &GameUiContext, tile_def: &TileDef, reason: Option<&str>) -> String {
    let size = tile_def.size_in_cells();
    let mut lines = vec![building_name(skin::current_realm(), tile_def)];
    let size_text = ui::text::format(UiTextCategory::Hud, "palette.size", &[("width", &size.width.to_string()), ("height", &size.height.to_string())]);
    let cost_text = ui::text::format(UiTextCategory::Hud, "palette.cost", &[("gold", &tile_def.cost.to_string())]);
    lines.push(format!("{size_text} · {cost_text}"));
    for item in tile_def.materials.iter() {
        let stored = CityStock::available(context.world, item.kind);
        let mut line = ui::text::format(
            UiTextCategory::Hud,
            "palette.material",
            &[("name", &item.kind.display_name()), ("needed", &item.count.to_string()), ("stored", &stored.to_string())],
        );
        if stored < item.count {
            line = format!("{line} ({})", ui::text::get(UiTextCategory::Hud, "palette.short_mark"));
        }
        lines.push(line);
    }
    if let Some(reason) = reason {
        lines.push(reason.to_string());
    }
    lines.join("\n")
}

// What a tile of the palette asks for when pressed; TilePaletteMenu::draw carries it out after its
// menus have drawn.
#[derive(Copy, Clone)]
enum PaletteAction {
    Sector(TilePaletteMainButtonDef),
    Pick(TilePaletteMainButtonDef, TileDefHandle),
}

type PaletteActions = Rc<RefCell<Vec<PaletteAction>>>;

// ----------------------------------------------
// SectorGrid
// ----------------------------------------------

// A sector's buildings, four to a row, on the HUD's large panel: built when the sector opens.
struct SectorGrid {
    menu: UiMenuRcMut,
    grid: UiMenuWidgetIndex,
    tiles: Vec<(UiWidgetGroupWidgetIndex, UiWidgetGroupWidgetIndex, TileDefHandle)>, // Row, tile in the row.
    shown: Vec<Option<String>>, // Each tile's unbuildable reason, as last set.
    stock_key: (i64, Vec<u32>),   // The treasury and stock its tooltips were made with.
}

impl SectorGrid {
    fn new(
        context: &mut GameUiContext,
        def: TilePaletteMainButtonDef,
        children: &[TilePaletteChildButtonDef],
        actions: &PaletteActions,
        column_x: f32,
        tile_y: f32,
    ) -> Self {
        let panel = skin::panel(context, "hud_large");
        let insets = match &panel {
            SkinPanel::NineSlice { content, .. } => [content.left, content.top, content.right, content.bottom],
            SkinPanel::Sprite { .. } => [16.0; 4],
        };
        // The buildings' tiles at the sector column's size: the large tile's art scaled down.
        let scale = skin::hud_tile_size(HudTileKind::Sector).x / skin::hud_tile_size(HudTileKind::Large).x;
        let tile = skin::hud_tile_size(HudTileKind::Large) * scale;
        let crest = skin::hud_tile_bleed(HudTileKind::Large)[1] * scale;
        let columns = children.len().clamp(1, GRID_COLUMNS);
        let rows = children.len().div_ceil(GRID_COLUMNS).max(1);
        let row_gap = GRID_GAP + crest;
        let inner = Vec2::new(
            columns as f32 * tile.x + (columns - 1) as f32 * GRID_GAP,
            GRID_TITLE_HEIGHT + crest + rows as f32 * tile.y + (rows - 1) as f32 * row_gap,
        );
        let size = Vec2::new(inner.x + insets[0] + insets[2], inner.y + insets[1] + insets[3] + 8.0);
        let window = ui::screen_size(context.ui_sys);
        // Level with its sector's tile, under the top bar, on the window if it fits.
        let lowest = (window.y - size.y - layout::MARGIN).max(0.0);
        let y = (tile_y - insets[1] - GRID_TITLE_HEIGHT).min(lowest).max((layout::MARGIN + 90.0).min(lowest));
        let position = Vec2::new((column_x - size.x - 8.0).max(0.0), y);

        let title = ui::text::get(UiTextCategory::Hud, def.get_str("Title").unwrap_or_default()).to_string();
        let accent = skin::hud_color("accent", "accent");
        let title_look = engine::ui::UiLabelLook { size: 17.0, heading_font: true, letter_spacing: 1.5, color: accent, ..Default::default() };
        let heading = UiCanvas::new(Vec2::new(inner.x, GRID_TITLE_HEIGHT + crest), move |_, painter, rect| {
            painter.label(Vec2::new(rect.min.x, rect.min.y + GRID_TITLE_BASELINE), engine::ui::UiTextAnchor::Start, &title.to_uppercase(), &title_look);
        });

        let mut menu = panel.apply_to_menu(
            UiMenu::new(format!("TilePaletteChildMenu: {}", def.name()))
                .with_flags(UiMenuFlags::IsOpen)
                .with_position(position)
                .with_size(size)
                .with_widget_spacing(Vec2::zero()),
        );
        // The rows in one group: the room between them holds the next row's crests.
        let mut grid = UiWidgetGroup::new().with_center_horizontally(false).with_widget_spacing(Vec2::new(0.0, row_gap));
        grid.add_widget(heading);
        let mut rows = Vec::new();
        for chunk in children.chunks(GRID_COLUMNS) {
            let mut row = UiWidgetGroup::new().with_stack_vertically(false).with_center_horizontally(false).with_widget_spacing(Vec2::new(GRID_GAP, 0.0));
            let mut in_row = Vec::new();
            for child in chunk {
                let handle = child.tile_def_handle;
                let actions = actions.clone();
                let tile = skin::hud_tile_scaled(context, HudTileKind::Large, scale, &child.label, HudGlyphGroup::Building, &tile_def_of(handle).name)
                    .on_pressed(move |_, _| actions.borrow_mut().push(PaletteAction::Pick(def, handle)));
                in_row.push((row.add_widget(tile), handle));
            }
            rows.push((grid.add_widget(row), in_row));
        }
        let grid_index = menu.add_widget(grid);
        let tiles = rows.into_iter().flat_map(|(row, in_row)| in_row.into_iter().map(move |(index, handle)| (row, index, handle))).collect::<Vec<_>>();
        let shown = vec![None; tiles.len()];
        let mut grid = Self { menu: menu.into_rc(), grid: grid_index, tiles, shown, stock_key: (i64::MIN, Vec::new()) };
        grid.refresh(context, true);
        grid
    }

    // What its tooltips and enabled states depend on: the treasury and the city stock.
    fn stock_key(context: &GameUiContext) -> (i64, Vec<u32>) {
        let materials = crate::sim::resources::ResourceKind::raw_materials();
        (context.sim.finance().balance(), materials.iter().map(|kind| CityStock::available(context.world, kind)).collect())
    }

    // Its tiles' tooltips and enabled states, again when the treasury or the stock changed.
    fn refresh(&mut self, context: &mut GameUiContext, force: bool) {
        let key = Self::stock_key(context);
        if !force && key == self.stock_key {
            return;
        }
        self.stock_key = key;
        for (k, (row, index, handle)) in self.tiles.iter().enumerate() {
            let tile_def = tile_def_of(*handle);
            let reason = unbuildable_reason(context, tile_def);
            let text = building_tooltip(context, tile_def, reason.as_deref());
            let tooltip = skin::tooltip_beside(context, text);
            if let Some(grid) = self.menu.widget_as_mut::<UiWidgetGroup>(self.grid)
                && let Some(row) = grid.widget_as_mut::<UiWidgetGroup>(*row)
                && let Some(tile) = row.widget_as_mut::<UiIconTile>(*index)
            {
                tile.set_enabled(reason.is_none());
                tile.set_tooltip(Some(tooltip));
            }
            self.shown[k] = reason;
        }
    }

    fn draw(&mut self, context: &mut GameUiContext) {
        self.refresh(context, false);
        self.menu.draw(context);
    }
}

// ----------------------------------------------
// TilePaletteMenu
// ----------------------------------------------

struct MainButton {
    def: TilePaletteMainButtonDef,
    children: Vec<TilePaletteChildButtonDef>,
    index: UiMenuWidgetIndex,
    y: f32, // The tile's top, in window points.
}

pub struct TilePaletteMenu {
    left_mouse_button_pressed: bool,
    current_selection: TilePaletteSelection,
    selection_renderer: TileSelectionRenderer,
    main_buttons: ArrayVec<MainButton, TILE_PALETTE_MAIN_BUTTON_COUNT>,
    menu: UiMenuRcMut,
    column_x: f32,
    window: Vec2, // The window's size it was built for.
    open: Option<(TilePaletteMainButtonDef, SectorGrid)>,
    picked_from: Option<TilePaletteMainButtonDef>, // The sector whose tool is picked.
    actions: PaletteActions,
}

// The sector column as built for one window size.
struct Column {
    menu: UiMenuRcMut,
    main_buttons: ArrayVec<MainButton, TILE_PALETTE_MAIN_BUTTON_COUNT>,
    column_x: f32,
    window: Vec2,
}

pub type TilePaletteMenuRcMut   = RcMut<TilePaletteMenu>;
pub type TilePaletteMenuWeakMut = WeakMut<TilePaletteMenu>;
pub type TilePaletteMenuWeakRef = WeakRef<TilePaletteMenu>;

impl TilePalette for TilePaletteMenu {
    fn on_mouse_button(&mut self, button: MouseButton, action: InputAction) -> UiInputEvent {
        if button == MouseButton::Left {
            if action == InputAction::Press {
                self.left_mouse_button_pressed = true;
            } else if action == InputAction::Release {
                self.left_mouse_button_pressed = false;
            }
            UiInputEvent::Handled
        } else {
            UiInputEvent::NotHandled
        }
    }

    fn on_tile_placed(&mut self, context: &mut GameUiContext) {
        if self.current_selection.is_some() {
            sound::play(*context.sound_sys(), UiSoundKey::TilePlaced);
        }
    }

    fn on_tile_cleared(&mut self, context: &mut GameUiContext) {
        if self.current_selection.is_some() {
            sound::play(*context.sound_sys(), UiSoundKey::TileCleared);
        }
    }

    fn on_road_segment_placed(&mut self, context: &mut GameUiContext) {
        if self.current_selection.is_some() {
            sound::play(*context.sound_sys(), UiSoundKey::TilePlaced);
        }
    }

    fn on_tile_placement_failed(&mut self, context: &mut GameUiContext) {
        if self.current_selection.is_some() {
            sound::play(*context.sound_sys(), UiSoundKey::TilePlacementFailed);
        }
    }

    fn on_tile_placement_canceled(&mut self, context: &mut GameUiContext) {
        if self.current_selection.is_some() {
            sound::play(*context.sound_sys(), UiSoundKey::TilePlacementCanceled);
        }

        // NOTE: Play sound effect AND clear current selection.
        self.clear_current_selection(context);
    }

    fn clear_current_selection(&mut self, context: &mut GameUiContext) {
        self.reset_selection_internal(context);
    }

    fn current_selection(&self) -> TilePaletteSelection {
        self.current_selection
    }

    fn wants_to_place_or_clear_tile(&self) -> bool {
        self.left_mouse_button_pressed && self.has_selection()
    }
}

impl TilePaletteMenu {
    pub fn new(context: &mut GameUiContext) -> TilePaletteMenuRcMut {
        let actions: PaletteActions = Rc::new(RefCell::new(Vec::new()));
        let column = Self::build_column(context, &actions);
        TilePaletteMenuRcMut::new(Self {
            left_mouse_button_pressed: false,
            current_selection: TilePaletteSelection::None,
            selection_renderer: TileSelectionRenderer::new(context),
            main_buttons: column.main_buttons,
            menu: column.menu,
            column_x: column.column_x,
            window: column.window,
            open: None,
            picked_from: None,
            actions,
        })
    }

    // The sector column for the current window: its tiles at their size where it fits the
    // window's height, else smaller (the HUD's sizes are fixed otherwise: they don't grow with the
    // UI scale), centred in the column; a crest's room over the first; rules between the groups.
    fn build_column(context: &mut GameUiContext, actions: &PaletteActions) -> Column {
        let panel = skin::panel(context, layout::COLUMN_PANEL);
        let insets = match &panel {
            SkinPanel::NineSlice { content, .. } => [content.left, content.top, content.right, content.bottom],
            SkinPanel::Sprite { .. } => [8.0; 4],
        };
        let window = ui::screen_size(context.ui_sys);
        let full = skin::hud_tile_size(HudTileKind::Sector);
        let full_crest = skin::hud_tile_bleed(HudTileKind::Sector)[1];
        let full_spacing = (full_crest * 0.75).max(12.0); // The crests reach into it; their glow a little further.
        let separators = TilePaletteMainButtonDef::iter().filter(|def| def.separator_follows()).count() as f32;
        let count = TilePaletteMainButtonDef::COUNT as f32;
        // Its height: the insets and rules, which keep their size, and the rest, which scales.
        let fixed = insets[1] + insets[3] + separators * SEPARATOR_LINE + COLUMN_SLACK;
        let scaled = full_crest + count * full.y + (count + separators) * full_spacing;
        let room = window.y - 2.0 * layout::MARGIN - fixed;
        let scale = (room / scaled).clamp(MIN_COLUMN_SCALE, 1.0);
        let tile = full * scale;
        let crest = full_crest * scale;
        let spacing = full_spacing * scale;
        let pad = layout::COLUMN_PAD * scale;
        let width = insets[0] + insets[2] + tile.x + 2.0 * pad;
        let column_x = window.x - width - layout::MARGIN;

        let menu = panel.apply_to_menu(
            UiMenu::new("TilePaletteMenu")
                .with_flags(UiMenuFlags::IsOpen)
                .with_position(Vec2::new(column_x, layout::MARGIN))
                .with_widget_spacing(Vec2::new(0.0, spacing)),
        );
        // The tiles centred: the pad on either side of them.
        let mut menu = menu.with_content_insets(UiMenuInsets::points(insets[0] + pad, insets[1], insets[2] + pad, insets[3]));
        let rule = skin::hud_color("rule", "sub");

        // The first tile's crest stands in this room.
        menu.add_widget(UiCanvas::new(Vec2::new(1.0, (crest - spacing).max(1.0)), |_, _, _| {}));
        let mut y = layout::MARGIN + insets[1] + (crest - spacing).max(1.0) + spacing;
        let mut height = insets[1] + (crest - spacing).max(1.0) + insets[3];

        let mut main_buttons = ArrayVec::new();
        for def in TilePaletteMainButtonDef::iter() {
            let children = def.build_child_button_defs();
            let enabled = !(matches!(def, TilePaletteMainButtonDef::Undo | TilePaletteMainButtonDef::Redo)
                || (def.disabled_if_empty() && children.is_empty()));
            let actions_ref = actions.clone();
            let tooltip = if def.disabled_if_empty() && children.is_empty() {
                format!("{}\n{}", def.tooltip(), ui::text::get(UiTextCategory::Hud, "sector.empty"))
            } else {
                def.tooltip()
            };
            let sector = skin::hud_tile_scaled(context, HudTileKind::Sector, scale, def.label_str(), HudGlyphGroup::Sector, def.glyph())
                .with_tooltip(skin::tooltip_beside(context, tooltip))
                .with_enabled(enabled)
                .on_pressed(move |_, _| actions_ref.borrow_mut().push(PaletteAction::Sector(def)));
            let index = menu.add_widget(sector);
            main_buttons.push(MainButton { def, children, index, y });
            y += tile.y + spacing;
            height += spacing + tile.y;

            if def.separator_follows() {
                let line = UiCanvas::new(Vec2::new(tile.x, SEPARATOR_LINE), move |_, painter, rect| {
                    let y = rect.center().y;
                    painter.line(Vec2::new(rect.min.x - 4.0, y), Vec2::new(rect.max.x + 4.0, y), 2.0, rule);
                });
                menu.add_widget(line);
                y += SEPARATOR_LINE + spacing;
                height += spacing + SEPARATOR_LINE;
            }
        }
        Column { menu: menu.with_size(Vec2::new(width, height + COLUMN_SLACK)).into_rc(), main_buttons, column_x, window }
    }

    // Built again for a new window size, keeping the tool picked and the sector open.
    fn follow_window(&mut self, context: &mut GameUiContext) {
        if ui::screen_size(context.ui_sys) == self.window {
            return;
        }
        let column = Self::build_column(context, &self.actions);
        self.menu = column.menu;
        self.main_buttons = column.main_buttons;
        self.column_x = column.column_x;
        self.window = column.window;
        if let Some((def, _)) = self.open.take()
            && let Some(button) = self.main_buttons.iter().find(|button| button.def == def)
        {
            let grid = SectorGrid::new(context, def, &button.children, &self.actions, self.column_x, button.y);
            self.open = Some((def, grid));
        }
    }

    pub fn draw(&mut self, context: &mut GameUiContext) {
        self.follow_window(context);

        // The column's states: the open sector's tile, or the one whose tool is picked, pressed;
        // undo and redo enabled while there's something to undo or redo.
        let open = self.open.as_ref().map(|(def, _)| *def);
        let picked = if self.current_selection.is_some() { self.picked_from } else { None };
        for button in &self.main_buttons {
            if let Some(tile) = self.menu.widget_as_mut::<UiIconTile>(button.index) {
                tile.set_selected(Some(button.def) == open || Some(button.def) == picked);
                match button.def {
                    TilePaletteMainButtonDef::Undo => tile.set_enabled(undo_redo::can_undo()),
                    TilePaletteMainButtonDef::Redo => tile.set_enabled(undo_redo::can_redo()),
                    _ => {}
                }
            }
        }

        self.menu.draw(context);
        if let Some((_, grid)) = &mut self.open {
            grid.draw(context);
        }

        let actions: Vec<PaletteAction> = self.actions.borrow_mut().drain(..).collect();
        for action in actions {
            self.run(context, action);
        }

        // Draw selected tile cursor overlay:
        self.selection_renderer.draw(context, self.current_selection);
    }

    // ----------------------
    // Internal:
    // ----------------------

    fn run(&mut self, context: &mut GameUiContext, action: PaletteAction) {
        match action {
            PaletteAction::Sector(def) => {
                let was_open = self.open.as_ref().is_some_and(|(open, _)| *open == def);
                self.reset_selection_internal(context);
                match def {
                    TilePaletteMainButtonDef::Undo => undo_redo::undo(&context.new_sim_context()),
                    TilePaletteMainButtonDef::Redo => undo_redo::redo(&context.new_sim_context()),
                    _ => {
                        let Some(button) = self.main_buttons.iter().find(|button| button.def == def) else { return };
                        if button.children.is_empty() {
                            // Clear land and housing pick their tool at once.
                            self.set_selection_internal(def.to_tile_selection(None));
                            self.picked_from = Some(def);
                        } else if !was_open {
                            let grid = SectorGrid::new(context, def, &button.children, &self.actions, self.column_x, button.y);
                            self.open = Some((def, grid));
                        }
                    }
                }
            }
            PaletteAction::Pick(def, handle) => {
                self.open = None;
                self.set_selection_internal(def.to_tile_selection(Some(handle)));
                self.picked_from = Some(def);
            }
        }
    }

    fn reset_selection_internal(&mut self, _context: &mut GameUiContext) {
        self.left_mouse_button_pressed = false;
        self.current_selection = TilePaletteSelection::None;
        self.picked_from = None;
        self.open = None;
    }

    fn set_selection_internal(&mut self, selection: TilePaletteSelection) {
        self.current_selection = selection;
    }
}

// ----------------------------------------------
// TileSelectionRenderer
// ----------------------------------------------

struct TileSelectionRenderer {
    clear_icon: TextureHandle,
}

impl TileSelectionRenderer {
    fn new(context: &mut GameUiContext) -> Self {
        Self { clear_icon: context.load_texture(PathRef::from_str(skin::icon("clear_tool"))) }
    }

    fn draw(&self, context: &mut GameUiContext, current_selection: TilePaletteSelection) {
        if current_selection.is_none() {
            return;
        }

        // Draw clear icon under the cursor:
        if current_selection.is_clear() {
            const CLEAR_ICON_SIZE: Vec2 = BASE_TILE_SIZE_F32;

            let rect = Rect::from_pos_and_size(context.cursor_screen_pos - (CLEAR_ICON_SIZE * 0.5), CLEAR_ICON_SIZE);

            context.render_sys().draw_textured_colored_rect(rect, &RectTexCoords::DEFAULT, self.clear_icon, Color::white());
        } else {
            let selected_tile = current_selection.as_tile_def().unwrap();
            let rect = Rect::from_pos_and_size(context.cursor_screen_pos, selected_tile.draw_size.to_vec2());

            let offset = if selected_tile.is(TileKind::Building | TileKind::Rocks | TileKind::Vegetation) {
                Vec2::new(-(selected_tile.draw_size.width as f32 / 2.0), -(selected_tile.draw_size.height as f32))
            } else {
                Vec2::new(-(selected_tile.draw_size.width as f32 / 2.0), -(selected_tile.draw_size.height as f32 / 2.0))
            };

            let has_valid_placement = context.tile_selection.has_valid_placement();
            let transform = context.camera.transform();

            let cursor_transform = WorldToScreenTransform::new(transform.scaling, offset);
            let highlight_color = if has_valid_placement { Color::white() } else { INVALID_TILE_COLOR };

            if let Some(sprite_frame) = selected_tile.anim_frame_by_index(0, 0, 0) {
                let tile_color = Color::new(
                    selected_tile.color.r,
                    selected_tile.color.g,
                    selected_tile.color.b,
                    0.7, // Semi-transparent
                );

                context.render_sys().draw_textured_colored_rect(
                    cursor_transform.scale_and_offset_rect(rect),
                    &sprite_frame.tex_info.coords,
                    sprite_frame.tex_info.texture,
                    tile_color * highlight_color,
                );
            }
        }
    }
}
