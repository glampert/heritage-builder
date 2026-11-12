// The world map screen (docs/plans/realm-ui.md, Phase 2; world-map-<realm>.jpg): a full-screen
// dialog laid out as the mockups are, on the skin's world map backdrop:
// - the top bar: "World Map", the realm's epithet, the date in the realm's era and the season, the
//   treasury, and "Return to the city", which closes it;
// - the map (overlay.rs): the campaign's base art in a UiMapView, its markers, labels and routes
//   drawn live over it at their screen size, the frame around it. It fits its panel at first; the
//   wheel zooms in about the cursor, and back out to the fit; zoomed in, the cursor near the
//   map's edges pans it as the camera's edge scrolling does (GameConfigs' camera speed, margin and
//   switch). Hovering a city lights it; clicking selects it;
// - the side panel of the selected city: its emblem, name, subtitle and role, relations, the route
//   line, what it sells and buys, its court's request, and Set trade limits and Send a gift
//   (disabled until M10's trade);
// - the legend bar.
//
// The screen reads everything from the campaign's WorldMapProvider (crate::world_map): the
// campaign whose map it shows is the UI realm's (screen_campaign), so each realm's skin shows its
// own map. What's selected and the view live outside the widgets (STATE), so a rebuild (a city
// selected, the window resized, the skin or language changed) keeps them.
//
// Paths: WorldMap/bar/title, subtitle, date, season, coin, treasury, Return to the city;
// WorldMap/map (the map view: zoom, centre, fit) and its edge strips WorldMap/map/left, right, top,
// bottom (kind map_edge, for ui.hover to pan); a marker per city and next-mission site,
// WorldMap/<name> (kind map_marker); WorldMap/panel/emblem, name, subtitle, relations,
// relations_meter, route, route_detail, They sell, They buy, request/title, request/goods, note,
// Set trade limits, Send a gift; WorldMap/legend (kind canvas).

mod overlay;

use std::rc::Rc;

use common::{Color, Rect};
use engine::ui::{UiLabelLook, UiTextAnchor, sound::{self as ui_sound, UiSoundKey as SoundKey}};

use super::*;
use crate::{
    GameLoop,
    campaign::config::CampaignConfigs,
    config::GameConfigs,
    menu::skin::SkinPanel,
    realm::{self, Realm},
    sim::calendar::Season,
    world_map::{self as world_map_data, WorldMapProvider, config::CityKind},
};

use overlay::MapScene;


// ----------------------------------------------
// Layout (points on a 1920 × 1080 window, as the mockup generator draws it; scaled by the window:
// screen_scale)
// ----------------------------------------------

const REFERENCE: Vec2 = Vec2::new(1920.0, 1080.0);

const BAR_RECT: [f32; 4] = [30.0, 20.0, 1860.0, 92.0];
const MAP_ORIGIN: [f32; 2] = [52.0, 132.0];
const MAP_WIDTH: f32 = 1290.0; // Its height follows the map's extent: 866.88 for 1000 × 672 units.
const PANEL_RECT: [f32; 4] = [1400.0, 132.0, 480.0, 867.0];
const LEGEND_RECT: [f32; 4] = [40.0, 1034.0, 1316.0, 40.0];

// The bar's parts, in from its left: the title, the date's column (its right edge), the coin, the
// treasury, the Return button (its left edge and size).
const BAR_TITLE_X: f32 = 34.0;
const BAR_TITLE_GAP: f32 = 18.0; // Between the title and the subtitle.
const BAR_DATE_RIGHT: f32 = 1120.0;
const BAR_DATE_WIDTH: f32 = 380.0;
const BAR_COIN_X: f32 = 1158.0;
const BAR_TREASURY_X: f32 = 1200.0;
const BAR_TREASURY_WIDTH: f32 = 140.0;
const BAR_BUTTON: [f32; 3] = [1370.0, 480.0, 56.0]; // Left edge, width, height.

// Text sizes in points (the generator's font sizes).
const TITLE_SIZE: f32 = 40.0;
const SUBTITLE_SIZE: f32 = 24.0;
const DATE_SIZE: f32 = 22.0;
const SEASON_SIZE: f32 = 18.0;
const TREASURY_SIZE: f32 = 26.0;
const BUTTON_TEXT_SIZE: f32 = 22.0;
const NAME_SIZE: f32 = 42.0;
const PANEL_SUB_SIZE: f32 = 19.0;
const RELATIONS_SIZE: f32 = 19.0;
const ROUTE_SIZE: f32 = 20.0;
const ROUTE_DETAIL_SIZE: f32 = 18.0;
const GOODS_SIZE: f32 = 19.0;
const REQUEST_TITLE_SIZE: f32 = 15.0;
const REQUEST_SIZE: f32 = 21.0;
const LEGEND_SIZE: f32 = 18.0;

// The side panel's parts.
const PANEL_PADDING: [f32; 4] = [32.0, 20.0, 32.0, 18.0];
const EMBLEM_SIZE: f32 = 124.0;
const METER_HEIGHT: f32 = 14.0;
const REQUEST_HEIGHT: f32 = 92.0;
const REQUEST_PADDING: [f32; 4] = [18.0, 14.0, 18.0, 10.0];
const REQUEST_SEAL_SIZE: f32 = 32.0;
const REQUEST_SEAL_INSET: f32 = 34.0; // Its centre, in from the box's right edge.
const PANEL_BUTTON_HEIGHT: f32 = 54.0;
const PANEL_BUTTON_GAP: f32 = 12.0;
const GOODS_ROWS: usize = 3;
const GOODS_ROW_HEIGHT: f32 = 40.0;
const BULLET_SIZE: f32 = 10.0;
const PRICE_WIDTH: f32 = 44.0;
const GOODS_CELL_GAP: f32 = 6.0;
const GOODS_COLUMN_GAP: f32 = 24.0;
const PANEL_SLACK: f32 = 6.0; // Left under the panel's parts for what measuring misses.
const TIGHT_LINE: f32 = 1.3; // The tallest row of text the screen keeps, in ems (tight).

// Zoom: how far in, and by what factor a wheel step zooms.
const MAX_ZOOM: f32 = 3.0;
const WHEEL_STEP: f32 = 1.25;

// How much the screen's sizes scale on a window of `window` points: as the generator's 1920 × 1080
// composition fits it (the smaller of the width's and the height's ratios). The composition is
// centred on a window of another shape.
pub fn screen_scale(window: Vec2) -> f32 {
    (window.x.max(1.0) / REFERENCE.x).min(window.y.max(1.0) / REFERENCE.y)
}

// ----------------------------------------------
// State
// ----------------------------------------------

// What the screen shows, kept outside its widgets: the campaign whose map it is, the city
// selected and the one under the cursor, and the map's view. Set afresh when it opens (prepare).
struct WorldMapScreenState {
    campaign: Option<usize>,
    selected: Option<String>,
    hovered: Option<String>,
    view: Option<UiMapViewState>,
    active: bool,            // Prepared for opening, until it closes: only then is the screen built in full.
    built: Option<BuildKey>, // What the dialog was last built from.
    rebuilds: u32,           // Bumped to build it again (a view set by automation).
}

// What the dialog is built from: built again when any of it changes.
#[derive(Clone, PartialEq, Debug)]
struct BuildKey {
    window: (u32, u32),
    campaign: Option<usize>,
    selected: Option<String>,
    bar: (String, String, String), // The date, the season and the treasury as shown.
    rebuilds: u32,
}

static STATE: UiStaticVar<WorldMapScreenState> =
    UiStaticVar::new(WorldMapScreenState { campaign: None, selected: None, hovered: None, view: None, active: false, built: None, rebuilds: 0 });

// The campaign whose map the screen shows in `realm`'s skin: the mission being played's (or the
// default campaign, world_map::default_campaign) when it's that realm's, else the first visible
// campaign of the realm, so each realm's skin shows the map its base art is drawn for.
pub fn screen_campaign(realm: Realm) -> Option<usize> {
    if !CampaignConfigs::is_initialized() {
        return None;
    }
    let configs = CampaignConfigs::get();
    let default = world_map_data::default_campaign();
    if configs.campaign(default).is_some_and(|campaign| campaign.realm == realm) {
        return Some(default);
    }
    configs.visible().find(|(_, campaign)| campaign.realm == realm).map(|(index, _)| index).or(Some(default))
}

// The city selected when the screen opens: the first trade partner, else the player's capital,
// else the first city.
fn default_selection(provider: &impl WorldMapProvider) -> Option<String> {
    let cities = provider.cities();
    cities
        .iter()
        .find(|city| city.has_trade)
        .or_else(|| cities.iter().find(|city| city.player_owned && city.kind == CityKind::Capital))
        .or_else(|| cities.first())
        .map(|city| city.id.clone())
}

// Sets the screen up for opening: the UI realm's campaign, its default selection, the fitted view.
pub(super) fn prepare() {
    let realm = realm::ui_realm_for(false).realm;
    let campaign = screen_campaign(realm);
    let selected = campaign.and_then(world_map_data::provider).and_then(|provider| default_selection(&provider));
    let state = STATE.as_mut();
    state.campaign = campaign;
    state.selected = selected;
    state.hovered = None;
    state.view = None;
    state.active = true;
    state.built = None;
}

// Whether the open dialog must be built again: the realm's campaign, the selection, the window or
// the top bar's values changed, or automation set the view.
pub(super) fn needs_rebuild(context: &GameUiContext) -> bool {
    follow_realm();
    STATE.as_ref().built.as_ref() != Some(&build_key(context))
}

// Follows the UI realm's campaign: when the skin's realm changed (ui.realm), its map, its default
// selection and the fitted view. Called wherever the open screen is built or checked, so a rebuild
// for a realm switch (dialog::rebuild_all) already shows the new realm's map.
fn follow_realm() {
    let state = STATE.as_mut();
    let realm = realm::ui_realm_for(false).realm;
    let campaign = screen_campaign(realm);
    if state.campaign != campaign {
        state.campaign = campaign;
        state.selected = campaign.and_then(world_map_data::provider).and_then(|provider| default_selection(&provider));
        state.hovered = None;
        state.view = None;
    }
}

fn build_key(context: &GameUiContext) -> BuildKey {
    let state = STATE.as_ref();
    let window = engine::ui::screen_size(context.ui_sys);
    BuildKey {
        window: (window.x.round() as u32, window.y.round() as u32),
        campaign: state.campaign,
        selected: state.selected.clone(),
        bar: bar_values(context),
        rebuilds: state.rebuilds,
    }
}

// The screen's state for automation (world_map.state): the campaign shown, the city selected and
// the view. None while the screen isn't open.
pub struct WorldMapScreenInfo {
    pub campaign: Option<usize>,
    pub selected: Option<String>,
    pub hovered: Option<String>,
    pub view: UiMapViewState, // The fit, centred on the map, until the view first changes.
}

pub fn screen_info() -> Option<WorldMapScreenInfo> {
    if !super::is_initialized() || !super::is_open(DialogMenuKind::WorldMap) {
        return None;
    }
    let state = STATE.as_ref();
    let view = state.view.unwrap_or_else(fitted_view);
    Some(WorldMapScreenInfo { campaign: state.campaign, selected: state.selected.clone(), hovered: state.hovered.clone(), view })
}

// The view the map opens with: the whole map, fit to its panel, centred on the skin's map extent.
fn fitted_view() -> UiMapViewState {
    let extent = skin::world_map().extent;
    UiMapViewState { zoom: 1.0, centre: Vec2::new(extent[0] * 0.5, extent[1] * 0.5) }
}

// Selects a city of the screen's map (automation, a click): the panel shows it from the next frame.
// False if the map has no city of that id.
pub fn select_city(city_id: &str) -> bool {
    let state = STATE.as_mut();
    let Some(provider) = state.campaign.and_then(world_map_data::provider) else {
        return false;
    };
    if !provider.cities().iter().any(|city| city.id == city_id) {
        return false;
    }
    state.selected = Some(city_id.to_string());
    true
}

// Sets the map's view (automation): kept in its limits once drawn.
pub fn set_view(view: UiMapViewState) {
    let state = STATE.as_mut();
    state.view = Some(view);
    state.rebuilds = state.rebuilds.wrapping_add(1);
}

// The top bar's date, season and treasury as the player reads them.
fn bar_values(context: &GameUiContext) -> (String, String, String) {
    let realm = realm::ui_realm_for(false).realm;
    let calendar = context.sim.calendar();
    let date = realm::date_text(realm, calendar);
    let season = season_text(realm, calendar.season());
    let treasury = super::load_save::group_digits(context.sim.finance().balance());
    (date, season, treasury)
}

// "Wet season" as `realm` names it ("Season of high water" in Kambara).
fn season_text(realm: Realm, season: Season) -> String {
    let key = match season {
        Season::Wet => format!("season.{realm}.wet"),
        Season::Dry => format!("season.{realm}.dry"),
    };
    ui::text::get(UiTextCategory::Realm, &key).to_string()
}

// ----------------------------------------------
// WorldMap
// ----------------------------------------------

pub struct WorldMap {
    menu: UiMenuRcMut,
}

impl DialogMenuFactory for WorldMap {
    const KIND: DialogMenuKind = DialogMenuKind::WorldMap;
    const TITLE: &'static [&'static str] = &["World Map"];

    fn create(context: &mut GameUiContext) -> DialogMenuImpl {
        DialogMenuImpl::from(WorldMap::new(context))
    }
}

impl DialogMenu for WorldMap {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> DialogMenuKind {
        Self::KIND
    }

    fn menu(&self) -> &UiMenuRcMut {
        &self.menu
    }

    // Only in game: the home menu has no city to return to.
    fn open(&mut self, context: &mut GameUiContext) -> bool {
        if super::in_home_menu() || !GameLoop::is_initialized() || !GameLoop::get().is_in_game() || !STATE.as_ref().active {
            STATE.as_mut().active = false;
            return false;
        }
        self.menu_mut().open(context);
        true
    }

    fn close(&mut self, context: &mut GameUiContext) -> bool {
        let menu = self.menu_mut();
        if menu.is_message_box_open() {
            menu.close_message_box(context);
            return false;
        }
        menu.close(context);
        STATE.as_mut().active = false;
        true
    }

    fn reset(&mut self) {
        let menu = self.menu_mut();
        menu.reset_message_box();
        menu.set_flags(UiMenuFlags::IsOpen, false);
        STATE.as_mut().active = false;
    }
}

// The screen's scales on the current window: its sizes', and its text's over the realm theme's body
// size (which the UI scale grows; the screen's layout follows the window only, so its text takes
// the UI scale back out).
#[derive(Copy, Clone)]
struct Scales {
    size: f32,
    body: f32, // The theme's body text size at 100%.
    origin: Vec2, // The composition's top left in the window.
}

impl Scales {
    fn new(window: Vec2) -> Self {
        let size = screen_scale(window);
        let origin = Vec2::new(((window.x - REFERENCE.x * size) * 0.5).max(0.0), ((window.y - REFERENCE.y * size) * 0.5).max(0.0));
        Self { size, body: body_text_size(), origin }
    }

    #[inline]
    fn pt(&self, points: f32) -> f32 {
        points * self.size
    }

    // The font scale of body text `points` large (the generator's font size: an em, where the
    // theme's sizes are line heights, skin::font_em).
    fn font(&self, points: f32) -> UiFontScale {
        UiFontScale((points / skin::font_em(false) * self.size / (self.body * skin::ui_scale())).max(0.05))
    }

    // The font scale of text `points` large in the heading font (UiRichText::with_heading_font).
    fn heading_font(&self, points: f32) -> UiFontScale {
        UiFontScale((points / skin::font_em(true) * self.size / (self.body * skin::ui_scale())).max(0.05))
    }
}

// The realm theme's body text size at 100% (its style's `text_styles.body.size`).
fn body_text_size() -> f32 {
    style_number("/text_styles/body/size", 20.0)
}

fn style_number(pointer: &str, default: f32) -> f32 {
    skin::current().style.pointer(pointer).and_then(serde_json::Value::as_f64).map_or(default, |value| value as f32).max(0.01)
}

// A world map colour of the skin (its `world_map.colors`, a `text` name or "#RRGGBB(AA)").
fn map_color(name: &str) -> Option<Color> {
    skin::world_map().colors.get(name).and_then(|value| skin::color(value))
}

fn map_color_or(name: &str, fallback: &str) -> Color {
    map_color(name).or_else(|| skin::color(fallback)).unwrap_or(Color::black())
}

impl WorldMap {
    pub fn new(context: &mut GameUiContext) -> Self {
        let kind = Self::KIND.to_string();
        let window = ui::screen_size(context.ui_sys);
        let scales = Scales::new(window);
        let flags = DialogFrame::from_skin(Self::KIND).menu_flags();

        let mut menu = UiMenu::new(kind).with_flags(flags).with_size(window).with_content_insets(UiMenuInsets::same(0.0)).with_widget_spacing(Vec2::zero());

        // Built at boot and on every rebuild (a realm switch, a skin or language change) for every
        // dialog kind, but shown only once prepared for opening, in game: until then an empty
        // menu, so its large textures (the base, the frame, the backdrop) load only when needed.
        if !STATE.as_ref().active || super::in_home_menu() {
            return Self { menu: menu.into_rc() };
        }

        follow_realm();
        let art = skin::world_map().clone();
        if !art.backdrop.is_empty() {
            menu = menu.with_background(context, PathRef::from_str(&art.backdrop));
        }

        let state = STATE.as_ref();
        let provider = state.campaign.and_then(world_map_data::provider);
        let scene = provider.as_ref().map(|provider| Rc::new(MapScene::new(provider)));
        let panel = provider.as_ref().and_then(|provider| state.selected.as_deref().and_then(|id| provider.city_panel(id)));
        let selected_city = scene.as_ref().and_then(|scene| state.selected.as_deref().and_then(|id| scene.city(id).cloned()));

        // The composition's rows, from the top: the bar, the map and the panel, the legend.
        let map_height = map_height(&art);
        menu.add_widget(spacer(scales.origin.y + scales.pt(BAR_RECT[1])));
        menu.add_widget(row(scales, BAR_RECT[0], top_bar(context, scales)));
        menu.add_widget(spacer(scales.pt(MAP_ORIGIN[1] - BAR_RECT[1] - BAR_RECT[3])));

        let mut middle = UiColumns::new().with_spacing(0.0);
        middle.add_column(UiColumnWidth::Fixed(scales.origin.x + scales.pt(MAP_ORIGIN[0])), UiWidgetGroup::new());
        middle.add_column(UiColumnWidth::Fixed(scales.pt(MAP_WIDTH)), left_group().with_widget(map_view(context, scales, &art, scene.clone())));
        middle.add_column(UiColumnWidth::Fixed(scales.pt(PANEL_RECT[0] - MAP_ORIGIN[0] - MAP_WIDTH)), UiWidgetGroup::new());
        let side = side_panel(context, scales, panel.as_ref(), selected_city.as_ref());
        middle.add_column(UiColumnWidth::Fixed(scales.pt(PANEL_RECT[2])), left_group().with_widget(side));
        middle.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());
        menu.add_widget(middle);

        menu.add_widget(spacer(scales.pt(LEGEND_RECT[1] - MAP_ORIGIN[1] - map_height)));
        menu.add_widget(row(scales, LEGEND_RECT[0], legend(context, scales)));

        STATE.as_mut().built = Some(build_key(context));
        Self { menu: menu.into_rc() }
    }
}

// The map's height on the reference window: its width's share of the map's extent.
fn map_height(art: &skin::SkinWorldMapDef) -> f32 {
    MAP_WIDTH * art.extent[1] / art.extent[0].max(1.0)
}

// A blank row `height` points tall.
fn spacer(height: f32) -> UiSeparator {
    UiSeparator::new().with_size(Vec2::new(1.0, height.max(0.0)))
}

// A group whose widgets keep to its left.
fn left_group() -> UiWidgetGroup {
    UiWidgetGroup::new().with_center_horizontally(false)
}

// A row holding `widget` `x` points (on the reference window) in from the composition's left.
fn row(scales: Scales, x: f32, widget: impl Into<UiWidgetImpl>) -> UiColumns {
    let mut columns = UiColumns::new().with_spacing(0.0);
    columns.add_column(UiColumnWidth::Fixed(scales.origin.x + scales.pt(x)), UiWidgetGroup::new());
    columns.add_column(UiColumnWidth::Fraction(1.0), left_group().with_widget(widget));
    columns
}

// A line of text, `width` points wide (0: its own), labeled for automation.
fn text_line(text: &str, label: &str, font: UiFontScale, color: Color, width: f32, align: UiTextAlign) -> UiRichText {
    let mut line = UiRichText::from_spans([UiTextSpan::plain(text)]).with_label(label).with_font_scale(font).with_align(align).with_text_color(color);
    if width > 0.0 {
        line = line.with_width(width);
    }
    line
}

// `line` with its row cut to TIGHT_LINE ems of its text, `points` large, where the realm font's own
// line is taller (Tampadesa's Amita draws its rows twice as tall as its letters): the mockup's
// rows. Its letters stay centred in the row.
fn tight(context: &GameUiContext, line: UiRichText, font: UiFontScale, points: f32, scales: Scales) -> UiRichText {
    let row = context.calc_text_size(font, "Ag").y;
    let spacing = (points * scales.size * TIGHT_LINE - row).min(0.0);
    if spacing < 0.0 { line.with_line_spacing(spacing) } else { line }
}

// How wide `text` is at `font` in the theme's body (or heading) font.
fn text_width(context: &GameUiContext, text: &str, font: UiFontScale) -> f32 {
    context.calc_text_size(font, text).x.ceil() + 2.0
}

// ----------------------------------------------
// Top bar
// ----------------------------------------------

fn top_bar(context: &mut GameUiContext, scales: Scales) -> UiFrame {
    let realm = realm::ui_realm_for(false).realm;
    let (date, season, treasury) = bar_values(context);
    let bar_text = map_color_or("bar_text", "light");
    let bar_sub = map_color_or("bar_sub", "light");

    // Title and subtitle, side by side on one baseline (the mockup's): the subtitle, smaller and
    // in the body font, moves down by the difference of the two lines' baselines.
    let title_text = ui::text::get(UiTextCategory::Menu, "world_map.title");
    let title_font = scales.heading_font(TITLE_SIZE);
    let title = text_line(title_text, "title", title_font, bar_text, text_width(context, title_text, title_font) * 1.06, UiTextAlign::Left).with_heading_font();
    let subtitle_key = format!("realm.{realm}.epithet");
    let subtitle_text = ui::text::get(UiTextCategory::Realm, &subtitle_key);
    let subtitle_font = scales.font(SUBTITLE_SIZE);
    let subtitle = text_line(subtitle_text, "subtitle", subtitle_font, bar_sub, text_width(context, subtitle_text, subtitle_font), UiTextAlign::Left);
    let drop = (title.baseline(context) - subtitle.baseline(context)).max(0.0);
    let subtitle =
        UiWidgetGroup::new().with_center_horizontally(false).with_widget_spacing(Vec2::zero()).with_widget(spacer(drop)).with_widget(subtitle);
    let titles = UiWidgetGroup::new()
        .with_stack_vertically(false)
        .with_center_horizontally(false)
        .with_center_vertically(false)
        .with_widget_spacing(Vec2::new(scales.pt(BAR_TITLE_GAP), 0.0))
        .with_widget(title)
        .with_widget(subtitle);

    let date_width = scales.pt(BAR_DATE_WIDTH);
    let dates = UiWidgetGroup::new()
        .with_center_vertically(true)
        .with_widget(tight(context, text_line(&date, "date", scales.font(DATE_SIZE), bar_text, date_width, UiTextAlign::Right), scales.font(DATE_SIZE), DATE_SIZE, scales))
        .with_widget(tight(context, text_line(&season, "season", scales.font(SEASON_SIZE), bar_sub, date_width, UiTextAlign::Right), scales.font(SEASON_SIZE), SEASON_SIZE, scales));

    let coin = skin::world_map().marks.get("coin").cloned().map(|path| {
        let (texture, size) = skin::sprite_and_size(context, &path);
        UiImage::from_texture(texture, size, size * scales.size).with_label("coin")
    });
    let treasury_line = text_line(&treasury, "treasury", scales.heading_font(TREASURY_SIZE), bar_text, scales.pt(BAR_TREASURY_WIDTH), UiTextAlign::Left).with_heading_font();

    let button = skin::button(context, ButtonRole::Primary, UiString::key(UiTextCategory::Menu, "world_map.return"), |_, context| {
        super::close(DialogMenuKind::WorldMap, context_as_mut::<GameUiContext>(context));
    })
    .with_width(UiTextButtonWidth::Fixed(scales.pt(BAR_BUTTON[1])))
    .with_min_height(scales.pt(BAR_BUTTON[2]));
    let button = scale_button_text(button, scales);

    let centred = |widget: UiWidgetImpl| left_group().with_center_vertically(true).with_widget(widget);
    let mut columns = UiColumns::new().with_spacing(0.0);
    columns.add_column(UiColumnWidth::Fixed(scales.pt(BAR_TITLE_X)), UiWidgetGroup::new());
    columns.add_column(UiColumnWidth::Fixed(scales.pt(BAR_DATE_RIGHT - BAR_DATE_WIDTH - BAR_TITLE_X)), titles);
    columns.add_column(UiColumnWidth::Fixed(date_width), dates);
    columns.add_column(UiColumnWidth::Fixed(scales.pt(BAR_COIN_X - BAR_DATE_RIGHT)), UiWidgetGroup::new());
    columns.add_column(UiColumnWidth::Fixed(scales.pt(BAR_TREASURY_X - BAR_COIN_X)), coin.map_or_else(UiWidgetGroup::new, |coin| centred(coin.into())));
    columns.add_column(UiColumnWidth::Fixed(scales.pt(BAR_TREASURY_WIDTH)), centred(treasury_line.into()));
    columns.add_column(UiColumnWidth::Fixed(scales.pt(BAR_BUTTON[0] - BAR_TREASURY_X - BAR_TREASURY_WIDTH)), UiWidgetGroup::new());
    columns.add_column(UiColumnWidth::Fixed(scales.pt(BAR_BUTTON[1])), centred(button.into()));
    columns.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());

    let mut frame = UiFrame::new(UiWidgetGroup::new().with_center_vertically(true).with_widget(columns), Vec2::new(scales.pt(BAR_RECT[2]), scales.pt(BAR_RECT[3])))
        .with_label("bar");
    if let SkinPanel::NineSlice { slice, .. } = skin::panel(context, "bar") {
        frame = frame.with_panel(slice);
    }
    frame
}

// A button's text at the generator's button size (in the title font), over the button text style's
// own size at the UI scale.
fn scale_button_text(button: UiTextButton, scales: Scales) -> UiTextButton {
    let points = BUTTON_TEXT_SIZE / skin::font_em(true) * scales.size;
    let font = points / (style_number("/text_styles/button/size", 20.0) * skin::ui_scale());
    button.with_font_scale(UiFontScale(font.max(0.05)))
}

// ----------------------------------------------
// Map
// ----------------------------------------------

fn map_view(context: &mut GameUiContext, scales: Scales, art: &skin::SkinWorldMapDef, scene: Option<Rc<MapScene>>) -> UiMapView {
    let size = Vec2::new(scales.pt(MAP_WIDTH), scales.pt(map_height(art)));
    let (texture, texture_size) = if art.base.is_empty() {
        (None, Vec2::new(1.0, 1.0))
    } else {
        let texture = context.load_texture(PathRef::from_str(&art.base));
        let texture_size = context.render_sys().texture_cache().texture_size(texture).map_or(Vec2::new(1.0, 1.0), |size| Vec2::new(size.width as f32, size.height as f32));
        (Some(texture), texture_size)
    };
    let extent = Vec2::new(art.extent[0], art.extent[1]);

    let camera = &GameConfigs::get().camera;
    let edge_pan = camera.edge_scrolling.then_some(UiEdgePan { margin: camera.scroll_margin, speed: camera.scroll_speed });

    let mut view = UiMapView::new(texture, texture_size, extent, art.pixels_per_unit, size)
        .with_label("map")
        .with_zoom_limits(MAX_ZOOM, WHEEL_STEP)
        .with_edge_pan(edge_pan)
        .on_view_changed(|view| STATE.as_mut().view = Some(view));
    if let Some(state) = STATE.as_ref().view {
        view = view.with_state(state);
    }
    if !art.frame.is_empty() {
        let (frame, frame_size) = {
            let texture = context.load_texture(PathRef::from_str(&art.frame));
            let size = context.render_sys().texture_cache().texture_size(texture).map_or(Vec2::new(1.0, 1.0), |size| Vec2::new(size.width as f32, size.height as f32));
            (texture, size)
        };
        let [x, y, width, height] = art.frame_map_rect;
        view = view.with_frame(frame, frame_size, Rect::from_pos_and_size(Vec2::new(x, y), Vec2::new(width, height)));
    }

    if let Some(scene) = scene {
        let look = overlay::MapLook::load(context, scales.size);
        view = view.with_overlay(move |context, painter, transform, pointer| {
            let state = STATE.as_mut();
            let hit = scene.draw(context.ui_sys(), painter, transform, pointer, &look, state.selected.as_deref(), state.hovered.as_deref());
            state.hovered = hit.clone();
            if pointer.clicked
                && let Some(id) = hit
                && state.selected.as_deref() != Some(id.as_str())
                && scene.city(&id).is_some()
            {
                state.selected = Some(id);
                ui_sound::play(*context.sound_sys(), SoundKey::ItemSelected);
            }
        });
    }
    view
}

// ----------------------------------------------
// Side panel
// ----------------------------------------------

fn side_panel(context: &mut GameUiContext, scales: Scales, panel: Option<&world_map_data::provider::CityPanel>, city: Option<&world_map_data::provider::MapCity>) -> UiFrame {
    let size = Vec2::new(scales.pt(PANEL_RECT[2]), scales.pt(PANEL_RECT[3]));
    let padding = PANEL_PADDING.map(|side| scales.pt(side));
    let inner = Vec2::new(size.x - padding[0] - padding[2], size.y - padding[1] - padding[3]);

    // The trade buttons at the bottom, disabled until M10.
    // TODO(realm-ui): M10's trade opens the trade limits dialog and the gift dialog from these.
    let buttons: Vec<UiTextButton> = ["world_map.set_trade_limits", "world_map.send_gift"]
        .into_iter()
        .map(|key| {
            let button = skin::button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, key), |_, _| {})
                .with_width(UiTextButtonWidth::Fixed(inner.x))
                .with_min_height(scales.pt(PANEL_BUTTON_HEIGHT))
                .with_enabled(false)
                .with_tooltip(skin::tooltip(context, UiString::key(UiTextCategory::Menu, "world_map.coming_with_trade")));
            scale_button_text(button, scales)
        })
        .collect();
    let gap = scales.pt(PANEL_BUTTON_GAP);
    let buttons_height: f32 = buttons.iter().map(|button| button.measure(context).y).sum::<f32>() + gap;

    // The parts at the top, as the mockup spaces them; closer together where the realm's font's
    // tall lines (Tampadesa's) would push the buttons off the panel.
    let room_for_parts = inner.y - buttons_height - scales.pt(PANEL_SLACK);
    let mut parts = Vec::new();
    let mut parts_height = 0.0;
    for tightness in [1.0, 0.6, 0.3, 0.0] {
        parts = panel_parts(context, scales, panel, city, inner.x, tightness);
        parts_height = parts.iter().map(|part| part.measure(context).y).sum();
        if parts_height <= room_for_parts {
            break;
        }
    }
    let room = (room_for_parts - parts_height).floor();

    let mut group = UiWidgetGroup::new().with_widget_spacing(Vec2::zero());
    for part in parts {
        group.add_widget(part);
    }
    if room > 0.0 {
        group.add_widget(spacer(room));
    }
    let mut buttons = buttons.into_iter();
    if let Some(first) = buttons.next() {
        group.add_widget(first);
    }
    for button in buttons {
        group.add_widget(spacer(gap));
        group.add_widget(button);
    }

    let mut frame = UiFrame::new(group, size).with_label("panel").with_padding(padding);
    if let SkinPanel::NineSlice { slice, .. } = skin::panel(context, "side") {
        frame = frame.with_panel(slice);
    }
    frame
}

// The side panel's parts above its buttons, `width` points wide, their gaps, the emblem and the
// goods' rows at `tightness` (1: as the mockup spaces them; 0: closest).
fn panel_parts(
    context: &mut GameUiContext,
    scales: Scales,
    panel: Option<&world_map_data::provider::CityPanel>,
    city: Option<&world_map_data::provider::MapCity>,
    width: f32,
    tightness: f32,
) -> Vec<UiWidgetImpl> {
    let mut parts: Vec<UiWidgetImpl> = Vec::new();
    let (Some(panel), Some(city)) = (panel, city) else {
        return parts;
    };
    let text = map_color_or("panel_text", "ink");
    let sub = map_color_or("panel_sub", "sub");
    let gap = |points: f32| spacer(scales.pt(points) * tightness);

    if let Some(emblem) = city_emblem(context, city) {
        let height = scales.pt(EMBLEM_SIZE * (0.6 + 0.4 * tightness));
        parts.push(emblem.with_size(Vec2::new(width, height)).with_fit(UiImageFit::Contain).into());
    }
    let name = text_line(&panel.name, "name", scales.heading_font(NAME_SIZE), text, width, UiTextAlign::Center).with_heading_font();
    parts.push(tight(context, name, scales.heading_font(NAME_SIZE), NAME_SIZE, scales).into());
    let subtitle = match &panel.role {
        Some(role) => ui::text::format(UiTextCategory::Menu, "world_map.subtitle_role", &[("subtitle", &panel.subtitle), ("role", role)]),
        None => panel.subtitle.clone(),
    };
    let subtitle = text_line(&subtitle, "subtitle", scales.font(PANEL_SUB_SIZE), sub, width, UiTextAlign::Center);
    parts.push(tight(context, subtitle, scales.font(PANEL_SUB_SIZE), PANEL_SUB_SIZE, scales).into());
    parts.push(gap(6.0).into());
    parts.push(skin::separator(context, "rule").into());

    if let Some(relation) = &panel.relation {
        parts.push(gap(8.0).into());
        let look = UiRowLook { label_color: Some(sub), value_color: Some(text), header_color: None, rule_color: None, padding: scales.pt(2.0) * tightness };
        let relations = skin::key_values()
            .with_label("relations")
            .with_look(look)
            .with_width(width)
            .with_font_scale(scales.font(RELATIONS_SIZE))
            .with_pair(UiString::key(UiTextCategory::Menu, "world_map.relations"), relation.word.clone());
        parts.push(relations.into());
        let mut meter_look = skin::meter_look();
        if let Some(fill) = map_color("relations_fill") {
            meter_look.fill = fill;
        }
        if let Some(track) = map_color("relations_track") {
            meter_look.track = track;
        }
        if let Some(outline) = map_color("relations_outline") {
            meter_look.outline = Some(outline);
        }
        let meter = UiMeter::new(UiMeterKind::Steps(u32::from(relation.max)), Vec2::new(width, scales.pt(METER_HEIGHT)))
            .with_label("relations_meter")
            .with_look(meter_look)
            .with_value(f32::from(relation.level), f32::from(relation.max.max(1)));
        parts.push(meter.into());
    }

    if let Some(route) = &panel.route_line {
        parts.push(gap(16.0).into());
        let route = text_line(route, "route", scales.font(ROUTE_SIZE), text, width, UiTextAlign::Left);
        parts.push(tight(context, route, scales.font(ROUTE_SIZE), ROUTE_SIZE, scales).into());
        if let Some(detail) = &panel.route_detail {
            let detail = text_line(detail, "route_detail", scales.font(ROUTE_DETAIL_SIZE), sub, width, UiTextAlign::Left);
            parts.push(tight(context, detail, scales.font(ROUTE_DETAIL_SIZE), ROUTE_DETAIL_SIZE, scales).into());
        }
    }

    if !panel.sells.is_empty() || !panel.buys.is_empty() {
        parts.push(gap(6.0).into());
        parts.push(skin::separator(context, "rule").into());
        parts.push(gap(8.0).into());
        // The goods' names in the body size, smaller if the longest wouldn't fit its column.
        let half = (width - scales.pt(GOODS_COLUMN_GAP)) * 0.5;
        let name_room = half - scales.pt(BULLET_SIZE) * 2.4 - scales.pt(PRICE_WIDTH) - scales.pt(GOODS_CELL_GAP);
        let widest = panel.sells.iter().chain(&panel.buys).map(|good| context.calc_text_size(scales.font(GOODS_SIZE), &good.good).x).fold(0.0, f32::max);
        let font = UiFontScale(scales.font(GOODS_SIZE).0 * if widest > name_room && widest > 0.0 { name_room / widest } else { 1.0 });
        // Rows as far apart as the mockup's, else as close as their text allows.
        let line = context.calc_text_size(font, "Ag").y;
        let padding = ((scales.pt(GOODS_ROW_HEIGHT) - line) * 0.5 * tightness).max(scales.pt(1.0));
        let mut goods = UiColumns::new().with_spacing(scales.pt(GOODS_COLUMN_GAP)).with_width(width);
        goods.add_column(UiColumnWidth::Fixed(half), left_group().with_widget(goods_table(scales, "world_map.they_sell", &panel.sells, half, padding, font)));
        goods.add_column(UiColumnWidth::Fixed(half), left_group().with_widget(goods_table(scales, "world_map.they_buy", &panel.buys, half, padding, font)));
        parts.push(goods.into());
    }

    if let Some(request) = &panel.request {
        parts.push(gap(14.0).into());
        parts.push(request_box(context, scales, width, request).into());
    }

    if panel.relation.is_none() && panel.route_line.is_none() {
        parts.push(gap(16.0).into());
        let note_key = if city.player_owned { "world_map.note.your_city" } else { "world_map.note.no_trade" };
        parts.push(text_line(ui::text::get(UiTextCategory::Menu, note_key), "note", scales.font(ROUTE_DETAIL_SIZE), sub, width, UiTextAlign::Center).into());
    }
    parts
}

// The side panel's emblem of a city: its owner's from the skin's world map emblems, else its
// owner's realm emblem, else the skin's default.
fn city_emblem(context: &mut GameUiContext, city: &world_map_data::provider::MapCity) -> Option<UiImage> {
    let emblems = &skin::world_map().emblems;
    let own = emblems.get(&city.owner).cloned().flatten();
    let (texture, size) = match own {
        Some(path) => skin::sprite_and_size(context, &path),
        None => match Realm::parse(&city.owner) {
            Ok(realm) => skin::emblem(context, realm, skin::EmblemState::Idle)?,
            Err(_) => skin::sprite_and_size(context, emblems.get("default").cloned().flatten()?.as_str()),
        },
    };
    Some(UiImage::from_texture(texture, size, size).with_label("emblem"))
}

// "They sell" or "They buy": a header in the section colour, a row per good with its price.
fn goods_table(scales: Scales, title_key: &'static str, goods: &[world_map_data::provider::TradeOffer], width: f32, padding: f32, font: UiFontScale) -> UiTable {
    let mut look = skin::row_look();
    look.header_color = Some(map_color_or("section", "accent"));
    look.label_color = Some(map_color_or("panel_text", "ink"));
    look.rule_color = map_color("panel_rule").or(look.rule_color);
    look.padding = padding;
    let bullet = match skin::world_map().bullet_shape.as_str() {
        "diamond" => UiBullet::Diamond,
        "circle" => UiBullet::Circle,
        _ => UiBullet::Square,
    };
    let title = ui::text::get(UiTextCategory::Menu, title_key).to_uppercase();
    let mut table = skin::table()
        .with_label(UiString::key(UiTextCategory::Menu, title_key))
        .with_look(look)
        .with_width(width)
        .with_font_scale(font)
        .with_column_spacing(scales.pt(GOODS_CELL_GAP))
        .with_bullets(bullet, map_color_or("bullet", "accent"), scales.pt(BULLET_SIZE))
        .with_column(UiTableColumn::new(title, UiColumnWidth::Fraction(1.0), UiColumnAlign::Left))
        .with_column(UiTableColumn::new("", UiColumnWidth::Fixed(scales.pt(PRICE_WIDTH)), UiColumnAlign::Right));
    for good in goods.iter().take(GOODS_ROWS) {
        table = table.with_row(UiTableRow::new([good.good.clone(), good.price.to_string()]));
    }
    table
}

// The court's request: a box with its title and the goods asked for, within so long.
fn request_box(context: &mut GameUiContext, scales: Scales, width: f32, request: &world_map_data::provider::CourtRequest) -> UiFrame {
    let padding = REQUEST_PADDING.map(|side| scales.pt(side));
    let inner_width = width - padding[0] - padding[2];

    // Đại Long's box ends in a seal marked "!" (the skin's `request_seal`): the city seal's sprite,
    // REQUEST_SEAL_SIZE points large, its centre REQUEST_SEAL_INSET in from the box's right edge.
    let art = skin::world_map();
    let seal = art
        .request_seal
        .then(|| art.marks.get("city").cloned())
        .flatten()
        .map(|path| (skin::sprite_and_size(context, &path), art.places.get("city").map_or(24.0, |place| place.seal).max(1.0), art.seal_letter.clone()));
    let seal_room = if seal.is_some() { scales.pt(REQUEST_SEAL_INSET + REQUEST_SEAL_SIZE * 0.5) - padding[2] } else { 0.0 };
    let text_width = inner_width - seal_room;

    let title = ui::text::get(UiTextCategory::Menu, "world_map.request.title").to_uppercase();
    let goods = ui::text::format(UiTextCategory::Menu, "world_map.request.line", &[("goods", &request.goods), ("within", &request.within)]);
    let text = UiWidgetGroup::new()
        .with_center_horizontally(false)
        .with_widget_spacing(Vec2::new(0.0, scales.pt(6.0)))
        .with_widget(text_line(&title, "title", scales.heading_font(REQUEST_TITLE_SIZE), map_color_or("request_title", "accent"), text_width, UiTextAlign::Left).with_heading_font().with_letter_spacing(scales.pt(1.5)))
        .with_widget(text_line(&goods, "goods", scales.font(REQUEST_SIZE), map_color_or("request_text", "ink"), text_width, UiTextAlign::Left));

    // The mockup's height, or more for a realm font's taller lines.
    let height = scales.pt(REQUEST_HEIGHT).max((text.measure(context).y + padding[1] + padding[3]).ceil());
    let group = match seal {
        None => text,
        Some(((texture, sprite_size), seal_points, letter_def)) => {
            // The sprite's size at 1080p is for a seal `seal_points` large: drawn REQUEST_SEAL_SIZE.
            let draw_size = sprite_size * (REQUEST_SEAL_SIZE / seal_points * scales.size);
            let canvas = UiCanvas::new(Vec2::new(seal_room, (height - padding[1] - padding[3]).max(1.0)), move |_, painter, rect| {
                // The canvas reaches from the seal's left edge to the box's padding.
                let centre = Vec2::new(rect.min.x + scales.pt(REQUEST_SEAL_SIZE * 0.5), rect.center().y);
                painter.image(texture, Rect::from_pos_and_size(centre - draw_size * 0.5, draw_size), Color::white());
                if let Some(def) = &letter_def {
                    let seal = scales.pt(REQUEST_SEAL_SIZE);
                    let look = overlay::map_label_look(def, REQUEST_SEAL_SIZE * def.size, scales.size);
                    painter.label(centre + Vec2::new(0.0, seal * 0.2), UiTextAnchor::Middle, "!", &look);
                }
            })
            .with_label("seal", None);
            UiWidgetGroup::new()
                .with_stack_vertically(false)
                .with_center_horizontally(false)
                .with_center_vertically(false)
                .with_widget_spacing(Vec2::zero())
                .with_widget(text)
                .with_widget(canvas)
        }
    };
    let mut frame = UiFrame::new(group, Vec2::new(width, height)).with_label("request").with_padding(padding);
    if let Some(fill) = map_color("request_fill") {
        frame = frame.with_fill(fill, 0.0);
    }
    let dash = skin::world_map().request_dashed.then_some([scales.pt(6.0), scales.pt(3.0)]);
    if let Some(border) = map_color("request_border") {
        frame = frame.with_outline(UiFrameOutline { color: border, width: scales.pt(1.5).max(1.0), inset: 0.0, dash });
    }
    if let Some(inner) = map_color("request_inner") {
        frame = frame.with_outline(UiFrameOutline { color: inner, width: scales.pt(1.5).max(1.0), inset: scales.pt(4.0), dash: None });
    }
    frame
}

// ----------------------------------------------
// Legend
// ----------------------------------------------

// The legend's entries, in order: what each glyph is, and its text.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum LegendEntry {
    Player,
    Foreign,
    Choice,
    River,
    Sea,
    Land,
    Closed,
}

impl LegendEntry {
    const ALL: [Self; 7] = [Self::Player, Self::Foreign, Self::Choice, Self::River, Self::Sea, Self::Land, Self::Closed];

    fn text(self) -> &'static str {
        let key = match self {
            Self::Player => "world_map.legend.player",
            Self::Foreign => "world_map.legend.foreign",
            Self::Choice => "world_map.legend.choice",
            Self::River => "world_map.legend.river",
            Self::Sea => "world_map.legend.sea",
            Self::Land => "world_map.legend.land",
            Self::Closed => "world_map.legend.closed",
        };
        ui::text::get(UiTextCategory::Menu, key)
    }
}

fn legend(context: &mut GameUiContext, scales: Scales) -> UiCanvas {
    let size = Vec2::new(scales.pt(LEGEND_RECT[2]), scales.pt(LEGEND_RECT[3]));
    let look = overlay::MapLook::load(context, scales.size);
    let fill = map_color("legend_fill").unwrap_or(Color::new(0.0, 0.0, 0.0, 0.45));
    let text_look = UiLabelLook { size: scales.pt(LEGEND_SIZE) / skin::font_em(false), color: map_color_or("legend_text", "light"), ..UiLabelLook::default() };
    let texts: Vec<&'static str> = LegendEntry::ALL.iter().map(|entry| entry.text()).collect();
    let value = serde_json::json!(texts);
    let k = scales.size;
    UiCanvas::new(size, move |_, painter, rect| {
        painter.rect_filled(rect, fill, 6.0 * k);
        let baseline = rect.min.y + 26.0 * k;
        let glyph_y = baseline - 6.0 * k;
        let mut x = rect.min.x + 30.0 * k;
        for (entry, text) in LegendEntry::ALL.iter().zip(&texts) {
            look.draw_legend_glyph(painter, *entry, Vec2::new(x, glyph_y));
            let drawn = painter.label(Vec2::new(x + 28.0 * k, baseline), UiTextAnchor::Start, text, &text_look);
            x = drawn.max.x + 46.0 * k;
        }
    })
    .with_label("legend", Some(value))
}
