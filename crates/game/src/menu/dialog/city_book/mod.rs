// The City book (docs/plans/realm-ui.md, Phase 6; city-<page>-<realm>.jpg): one dialog with a
// page per area of the city, laid out as the mockup generator's pages.py draws it on its
// 1620 × 960 panel:
// - left, the city's name, the date in the realm's era and the month and season, the pages as
//   side tabs (an icon and a title each), and Close at the foot;
// - right, the page: its title, a note and a rule, then its cards.
//
// The pages (CityPage, each its own module): Overview, Population, Goods, Finances and Heritage.
// A milestone adds its page (M9's Ratings, M11's Labour and Food) by writing a CityPage and
// listing it in PAGES, the order the tabs show them.
//
// It's built afresh when it opens, and again while open once a game month passes, the window's
// size changes or the tax level or wage do (the Finances page's figures follow them): a page
// reads the city (crate::city::CityReport) and its history when it's built. The page shown and the
// Heritage page's tab are kept outside the widgets (STATE), so a rebuild keeps them.
//
// CityManagement is its dialog kind. PopulationManagement, ResourcesManagement and
// FinancesManagement open it on their page (redirect), and only CityManagement shows as open.
//
// Paths: CityManagement/tab_bar (value: the page shown) and a tab per page
// (CityManagement/Finances); the header's CityManagement/city, date, month; CityManagement/Close;
// a page's widgets under its title (CityManagement/Finances/Tax level).

use common::{Color, Rect};
use engine::ui::{UiLabelLook, UiPainter, UiTextAnchor};
use serde_json::json;

use super::*;
use super::screen::{Scales, left_group, spacer, text_line, tight};
use crate::{
    city::{self, CityReport},
    realm,
};

mod finances;
mod goods;
mod heritage;
mod overview;
mod population;

// ----------------------------------------------
// Layout (points on the generator's 1620 × 960 panel, scaled with it)
// ----------------------------------------------

const PANEL: Vec2 = Vec2::new(1620.0, 960.0);
// Where the contents go, in from the panel's edges (left, top, right, bottom): at least the
// panel's own content insets.
const PADDING: [f32; 4] = [64.0, 60.0, 84.0, 76.0];

const TABS_WIDTH: f32 = 282.0; // The tabs' column (the tab rects, from x 64).
const TABS_GAP: f32 = 74.0; // To the page, the rule in its middle.
const TAB_HEIGHT: f32 = 58.0;
const TAB_SPACING: f32 = 12.0;
const TAB_TEXT_SIZE: f32 = 24.0;
const TAB_ICON: f32 = 30.0;
const HEADER_HEIGHT: f32 = 166.0; // From the inner top to the first tab.
const CLOSE: Vec2 = Vec2::new(270.0, 56.0);

const CITY_SIZE: f32 = 38.0;
const DATE_SIZE: f32 = 18.0;

// A page: its title row, the rule under it, then its body from 186 points down the panel.
const TITLE_SIZE: f32 = 44.0;
const NOTE_SIZE: f32 = 19.0;
const PAGE_HEAD: f32 = 126.0; // From the inner top to the body.
pub(super) const BODY: Vec2 = Vec2::new(1116.0, 690.0);

// ----------------------------------------------
// State
// ----------------------------------------------

struct BookState {
    page: usize,         // In PAGES.
    heritage_tab: usize, // The Heritage page's tab.
    built: Option<BuildKey>,
}

static STATE: UiStaticVar<BookState> = UiStaticVar::new(BookState { page: 0, heritage_tab: 2, built: None });

// What the open book is built from: the window's size, the game's month, the tax level and wage.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
struct BuildKey {
    window: (u32, u32),
    month: u32,
    tax_level: usize,
    wage: u32,
}

fn build_key(context: &GameUiContext) -> BuildKey {
    let window = ui::screen_size(context.ui_sys);
    let finance = context.sim.finance();
    BuildKey {
        window: (window.x.round() as u32, window.y.round() as u32),
        month: city::history::current_month(context.sim),
        tax_level: finance.tax_level().index(),
        wage: finance.wage(),
    }
}

// Whether the open book must be built again: a month passed, the window's size changed, or the
// tax level or wage (the page shown and the Heritage tab are kept).
pub(super) fn needs_rebuild(context: &GameUiContext) -> bool {
    !in_home_menu() && STATE.as_ref().built != Some(build_key(context))
}

// The kinds that open the City book, and on which page: CityManagement on the one shown last.
pub(super) fn redirect(kind: DialogMenuKind) -> Option<Option<usize>> {
    let page = |id: &str| PAGES.iter().position(|page| page.id() == id);
    match kind {
        DialogMenuKind::CityManagement => Some(None),
        DialogMenuKind::PopulationManagement => Some(page("population")),
        DialogMenuKind::ResourcesManagement => Some(page("goods")),
        DialogMenuKind::FinancesManagement => Some(page("finances")),
        _ => None,
    }
}

// Sets the book up for opening on `page` (or the one shown last).
pub(super) fn prepare(page: Option<usize>) {
    let state = STATE.as_mut();
    if let Some(page) = page {
        state.page = page.min(PAGES.len() - 1);
    }
    state.built = None;
}

// ----------------------------------------------
// Pages
// ----------------------------------------------

// What a page builds from.
pub(super) struct PageContext<'a> {
    pub scales: Scales,
    pub report: &'a CityReport,
    pub colors: BookColors,
    pub realm: crate::realm::Realm,
}

impl PageContext<'_> {
    // Points for `px` of the mockup panel.
    pub fn pt(&self, px: f32) -> f32 {
        self.scales.pt(px)
    }
}

// A page of the book.
pub(super) trait CityPage: Sync {
    // Its id (automation, redirect): "overview".
    fn id(&self) -> &'static str;
    // Its tab's icon (skin::CITY_ICONS).
    fn icon(&self) -> &'static str;
    // The note at the right of its title.
    fn note(&self, context: &GameUiContext) -> String;
    // Its body, BODY points large at the page's scale.
    fn body(&self, context: &mut GameUiContext, page: &PageContext) -> UiWidgetGroup;

    // Its title: its City string `book.page.<id>`.
    fn title(&self) -> String {
        ui::text::get(UiTextCategory::City, &format!("book.page.{}", self.id())).to_string()
    }
}

// The pages, in the order the tabs show them.
static PAGES: [&dyn CityPage; 5] = [&overview::Overview, &population::Population, &goods::Goods, &finances::Finances, &heritage::Heritage];

// ----------------------------------------------
// CityManagement
// ----------------------------------------------

pub struct CityManagement {
    menu: UiMenuRcMut,
    tab_bar: UiMenuWidgetIndex,
}

implement_dialog_menu! { CityManagement, ["City Book"] }

impl CityManagement {
    pub fn new(context: &mut GameUiContext) -> Self {
        let kind = Self::KIND.to_string();
        let window = ui::screen_size(context.ui_sys);
        let frame = DialogFrame::from_skin(Self::KIND);
        let size = frame.size(window);
        let menu = UiMenu::new(kind).with_flags(frame.menu_flags()).with_size(size).with_widget_spacing(Vec2::zero());

        // The panel, and the contents inside its own insets and the mockup's margins.
        let scale = (size.x / PANEL.x).min(size.y / PANEL.y);
        let panel = skin::panel(context, "dialog");
        let own = match &panel {
            skin::SkinPanel::NineSlice { content, .. } => *content,
            skin::SkinPanel::Sprite { content, .. } => content.unwrap_or(UiMenuInsets::same(0.0)),
        };
        let insets = UiMenuInsets::points(
            own.left.max(PADDING[0] * scale).round(),
            own.top.max(PADDING[1] * scale).round(),
            own.right.max(PADDING[2] * scale).round(),
            own.bottom.max(PADDING[3] * scale).round(),
        );
        let mut menu = panel.apply_to_menu(menu).with_content_insets(insets);
        let inner_height = (size.y - insets.top - insets.bottom).max(1.0);

        // Only in game: the book reads the city. Built at boot for the home menu too, empty there.
        if in_home_menu() {
            STATE.as_mut().built = Some(build_key(context));
            let tab_bar = menu.add_widget(UiTabBar::new());
            return Self { menu: menu.into_rc(), tab_bar };
        }

        // The mockup's layout at the panel's scale, smaller where a realm's frame leaves less room
        // inside it.
        let inner_width = (size.x - insets.left - insets.right).max(1.0);
        let fit = (inner_width / (TABS_WIDTH + TABS_GAP + BODY.x)).min(inner_height / (PAGE_HEAD + BODY.y)).min(scale);
        let scales = Scales::with_size(fit);
        let report = CityReport::read(context.world, context.sim);
        let page_context = PageContext { scales, report: &report, colors: BookColors::read(), realm: skin::current_realm() };

        let look = side_look(&page_context);
        let mut tab_bar = UiTabBar::new()
            .with_selected(STATE.as_ref().page.min(PAGES.len() - 1))
            .on_tab_changed(|bar, _, _| STATE.as_mut().page = bar.selected_index());
        for page in PAGES.iter() {
            let contents = page_contents(context, *page, &page_context);
            let icon = skin::city_icon(context, page.icon());
            tab_bar = tab_bar.with_icon_tab_contents(page.title(), icon, contents);
        }
        let header = header(context, &page_context);
        let footer = footer(context, &page_context);
        let tab_bar = menu.add_widget(tab_bar.with_side(look, header, footer, inner_height));

        STATE.as_mut().built = Some(build_key(context));
        Self { menu: menu.into_rc(), tab_bar }
    }

    // Shows `page` (an index in PAGES) in the open book.
    pub(super) fn select_page(&mut self, page: usize) {
        STATE.as_mut().page = page;
        if let Some(tab_bar) = self.menu.widget_as_mut::<UiTabBar>(self.tab_bar)
            && page < tab_bar.tab_count()
        {
            tab_bar.set_selected(page);
        }
    }
}

// The side tabs' look: the selected one filled with the accent, the others ruled under.
fn side_look(page: &PageContext) -> UiSideTabLook {
    let c = &page.colors;
    UiSideTabLook {
        width: page.pt(TABS_WIDTH),
        height: page.pt(TAB_HEIGHT),
        spacing: page.pt(TAB_SPACING),
        gap: page.pt(TABS_GAP),
        padding: page.pt(12.0),
        icon_size: page.pt(TAB_ICON),
        icon_gap: page.pt(12.0),
        rounding: page.pt(6.0),
        text_size: em(page, TAB_TEXT_SIZE, true),
        heading_font: true,
        text: c.text,
        selected_text: c.on_accent,
        icon: c.accent,
        selected_icon: c.on_accent,
        selected_fill: c.accent,
        hover_fill: c.hover,
        rule: Some(c.rule),
        divider: Some((c.rule, page.pt(36.0))),
    }
}

// Above the tabs: the city's name, the year in the realm's era, the month and the season.
fn header(context: &mut GameUiContext, page: &PageContext) -> UiWidgetGroup {
    let scales = page.scales;
    let width = page.pt(TABS_WIDTH - 12.0);
    let realm = page.realm;
    let calendar = context.sim.calendar();
    let configs = &crate::config::GameConfigs::get().sim;
    let date = calendar.date(configs.days_per_month, configs.seconds_per_month);
    let year = realm::date_text(realm, calendar);
    let month = ui::text::format(
        UiTextCategory::City,
        "book.month_season",
        &[("month", &date.month.to_string()), ("season", &crate::menu::in_game::season_text(realm, calendar.season()))],
    );

    let city_font = scales.heading_font(CITY_SIZE);
    let date_font = scales.font(DATE_SIZE);
    let c = &page.colors;
    // Rows no taller than the mockup's, where a realm's font draws tall lines (Tampadesa's).
    let city = text_line(&city_name(), "city", city_font, c.title, width, UiTextAlign::Left).with_heading_font();
    let date_line = |text: &str, label: &str| tight(context, text_line(text, label, date_font, c.sub, width, UiTextAlign::Left), date_font, DATE_SIZE, scales);
    let group = left_group()
        .with_widget_spacing(Vec2::zero())
        .with_margins(page.pt(12.0), 0.0)
        .with_widget(spacer(page.pt(26.0)))
        .with_widget(tight(context, city, city_font, CITY_SIZE, scales))
        .with_widget(spacer(page.pt(4.0)))
        .with_widget(date_line(&year, "date"))
        .with_widget(date_line(&month, "month"));
    // As tall as the room above the first tab.
    let height = group.measure(context).y;
    group.with_widget(spacer((page.pt(HEADER_HEIGHT) - height).max(0.0)))
}

// The city's name: the mission's city, else the custom game's map, else "Your city".
fn city_name() -> String {
    if let Some(active) = crate::campaign::active_mission()
        && let Some(mission) = crate::campaign::config::CampaignConfigs::get().mission(active.campaign_id, active.mission_index)
        && let Some(city) = mission.city_display_name()
    {
        return city.to_string();
    }
    match crate::custom_game::active().and_then(|game| game.record.map.as_deref()).map(super::load_save::custom_map_display_name) {
        Some(map) => map,
        None => ui::text::get(UiTextCategory::City, "book.your_city").to_string(),
    }
}

// At the column's foot: Close.
fn footer(context: &mut GameUiContext, page: &PageContext) -> UiWidgetGroup {
    let close = make_dialog_close_button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Hud, "button.close"))
        .with_width(UiTextButtonWidth::Fixed(page.pt(CLOSE.x)))
        .with_min_height(page.pt(CLOSE.y));
    left_group().with_margins(page.pt(12.0), 0.0).with_widget(page.scales.button_text(close, 22.0))
}

// A page: its title and note, the rule, then its body.
fn page_contents(context: &mut GameUiContext, page: &dyn CityPage, page_context: &PageContext) -> UiWidgetGroup {
    let width = page_context.pt(BODY.x);
    let title = page.title();
    let note = page.note(context);
    let c = page_context.colors;
    let title_look = label_look(page_context, TITLE_SIZE, c.title, true);
    let note_look = label_look(page_context, NOTE_SIZE, c.sub, false);
    let head = UiCanvas::new(Vec2::new(width, page_context.pt(84.0)), move |_, painter, rect| {
        painter.label(Vec2::new(rect.min.x, rect.min.y + rect.height() * 0.81), UiTextAnchor::Start, &title, &title_look);
        painter.label(Vec2::new(rect.max.x, rect.min.y + rect.height() * 0.79), UiTextAnchor::End, &note, &note_look);
    })
    .with_label("title", Some(json!({ "title": page.title(), "note": page.note(context) })));
    let rule = skin::separator(context, "rule");
    let mut group = left_group().with_widget_spacing(Vec2::zero()).with_widget(head);
    let head_height = group.measure(context).y;
    let rule_height = rule.measure(context).y;
    group.add_widget(rule);
    group.add_widget(spacer((page_context.pt(PAGE_HEAD) - head_height - rule_height).max(0.0)));
    group.add_widget(page.body(context, page_context));
    group
}

// ----------------------------------------------
// Colours and painting
// ----------------------------------------------

// The book's colours (skin::city_color).
#[derive(Copy, Clone)]
pub(crate) struct BookColors {
    pub text: Color,
    pub sub: Color,
    pub title: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub rule: Color,
    pub card: Color,
    pub card_outline: Color,
    pub field: Color,
    pub field_outline: Color,
    pub good: Color,
    pub track: Color,
    pub hover: Color,
    pub bad: Color, // High risks and shortages (the inspector).
}

impl BookColors {
    pub(crate) fn read() -> Self {
        let c = skin::city_color;
        Self {
            text: c("text", "ink"),
            sub: c("sub", "sub"),
            title: c("title", "title"),
            accent: c("accent", "accent"),
            on_accent: c("on_accent", "on_accent"),
            rule: c("rule", "sub"),
            card: c("card", "light"),
            card_outline: c("card_outline", "sub"),
            field: c("field", "light"),
            field_outline: c("field_outline", "sub"),
            good: c("good", "accent"),
            track: c("track", "sub"),
            hover: c("hover", "light"),
            bad: c("bad", "accent"),
        }
    }
}

// The size UiPainter::label takes for text `px` large in the mockup (an em of the realm's font),
// at the page's scale.
pub(super) fn em(page: &PageContext, px: f32, heading: bool) -> f32 {
    page.pt(px) / skin::font_em(heading)
}

pub(super) fn label_look(page: &PageContext, px: f32, color: Color, heading: bool) -> UiLabelLook {
    UiLabelLook { size: em(page, px, heading), heading_font: heading, color, ..Default::default() }
}

// The mockup's numbers: "1,284", "+42", "−259" (a true minus).
pub(super) fn number(value: i64, signed: bool) -> String {
    let digits = group_digits(value.abs());
    match (value.signum(), signed) {
        (-1, _) => format!("−{digits}"),
        (1, true) => format!("+{digits}"),
        _ => digits,
    }
}

// Paints what a page draws, in the page's points from a rect's top left: `k` scales the mockup's
// px.
pub(crate) struct Paint<'a> {
    pub painter: &'a UiPainter,
    pub origin: Vec2,
    pub k: f32,
    pub c: BookColors,
    pub em_body: f32,
    pub em_heading: f32,
}

impl<'a> Paint<'a> {
    pub fn new(painter: &'a UiPainter, rect: Rect, k: f32, c: BookColors) -> Self {
        Self { painter, origin: rect.min, k, c, em_body: skin::font_em(false), em_heading: skin::font_em(true) }
    }

    pub fn at(&self, x: f32, y: f32) -> Vec2 {
        Vec2::new(self.origin.x + x * self.k, self.origin.y + y * self.k)
    }

    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_pos_and_size(self.at(x, y), Vec2::new(w * self.k, h * self.k))
    }

    pub fn look(&self, px: f32, color: Color, heading: bool) -> UiLabelLook {
        let em = if heading { self.em_heading } else { self.em_body };
        UiLabelLook { size: px * self.k / em, heading_font: heading, color, ..Default::default() }
    }

    // Text at a baseline point, `px` large.
    pub fn text(&self, x: f32, y: f32, text: &str, px: f32, color: Color, anchor: UiTextAnchor, heading: bool) -> Rect {
        self.painter.label(self.at(x, y), anchor, text, &self.look(px, color, heading))
    }

    // How wide `text` is, in mockup px.
    pub fn width(&self, text: &str, px: f32, heading: bool) -> f32 {
        self.painter.label_rect(Vec2::zero(), UiTextAnchor::Start, text, &self.look(px, Color::black(), heading)).width() / self.k
    }

    // A section title: upper case, spaced, in the accent.
    pub fn section(&self, x: f32, y: f32, text: &str) {
        let mut look = self.look(17.0, self.c.accent, true);
        look.letter_spacing = 2.0 * self.k;
        self.painter.label(self.at(x, y), UiTextAnchor::Start, &text.to_uppercase(), &look);
    }

    // A card: a soft fill with an outline, its title and note.
    pub fn card(&self, x: f32, y: f32, w: f32, h: f32, title: Option<&str>, note: Option<&str>) {
        let rect = self.rect(x, y, w, h);
        self.painter.rect_filled(rect, self.c.card, 6.0 * self.k);
        self.painter.rect_stroke(rect, 6.0 * self.k, 1.5 * self.k, self.c.card_outline);
        if let Some(title) = title {
            self.section(x + 22.0, y + 36.0, title);
        }
        if let Some(note) = note {
            self.text(x + w - 22.0, y + 36.0, note, 17.0, self.c.sub, UiTextAnchor::End, false);
        }
    }

    pub fn rule(&self, x0: f32, x1: f32, y: f32) {
        self.painter.line(self.at(x0, y), self.at(x1, y), 1.0 * self.k, self.c.rule);
    }

    // A bar: its track, and the accent up to `v` (at least a sliver).
    pub fn bar(&self, x: f32, y: f32, w: f32, h: f32, v: f32) {
        self.painter.rect_filled(self.rect(x, y, w, h), self.c.track, h * 0.5 * self.k);
        let fill = if v > 0.0 { w * v.clamp(0.04, 1.0) } else { 0.0 };
        if fill > 0.0 {
            self.painter.rect_filled(self.rect(x, y, fill, h), self.c.accent, h * 0.5 * self.k);
        }
    }

    // A monthly change, right-aligned at x: an arrow and the signed number; "steady" for none,
    // and nothing before the history has a month.
    pub fn change(&self, x: f32, y: f32, change: Option<i64>, px: f32) {
        let Some(change) = change else {
            return;
        };
        if change == 0 {
            self.text(x, y, &ui::text::get(UiTextCategory::City, "book.steady"), px, self.c.sub, UiTextAnchor::End, false);
            return;
        }
        let color = if change > 0 { self.c.good } else { self.c.accent };
        let text = number(change, true);
        let width = self.width(&text, px, false);
        self.text(x, y, &text, px, color, UiTextAnchor::End, false);
        self.arrow(x - width - 12.0, y - px * 0.32, change > 0, color);
    }

    // A small chevron up or down, centred on (x, y).
    pub fn arrow(&self, x: f32, y: f32, up: bool, color: Color) {
        let d = if up { -1.0 } else { 1.0 };
        let points = [self.at(x - 5.0, y - 3.0 * d), self.at(x, y + 3.5 * d), self.at(x + 5.0, y - 3.0 * d)];
        self.painter.polyline(&points, 2.2 * self.k, color, false);
    }

    // A goal's mark: a filled disc with a tick when met, else a ring.
    pub fn check(&self, x: f32, y: f32, met: bool) {
        if met {
            self.painter.circle_filled(self.at(x, y), 11.0 * self.k, self.c.accent);
            let points = [self.at(x - 5.5, y), self.at(x - 1.5, y + 4.5), self.at(x + 6.0, y - 5.0)];
            self.painter.polyline(&points, 2.6 * self.k, self.c.on_accent, false);
        } else {
            self.painter.circle_stroke(self.at(x, y), 11.0 * self.k, 2.0 * self.k, self.c.accent);
        }
    }

    // A line chart of `values` in the rect, its area washed in the accent, the last point marked;
    // with an axis of three values left of it, and month numbers under it from `first_month`.
    pub fn chart(&self, x: f32, y: f32, w: f32, h: f32, values: &[f32], axis: bool, months: Option<&[u32]>, width: f32) {
        if values.is_empty() {
            return;
        }
        let lo = values.iter().copied().fold(f32::MAX, f32::min);
        let hi = values.iter().copied().fold(f32::MIN, f32::max);
        let pad = ((hi - lo) * 0.15).max(1.0);
        // Never below nothing for a count that can't be (a population, stock).
        let floor = if lo >= 0.0 { 0.0 } else { f32::MIN };
        let (lo, hi) = ((lo - pad).max(floor), hi + pad);
        let step = if values.len() > 1 { w / (values.len() - 1) as f32 } else { 0.0 };
        let points: Vec<Vec2> = values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let px = if values.len() > 1 { x + i as f32 * step } else { x + w };
                self.at(px, y + h - (v - lo) / (hi - lo) * h)
            })
            .collect();

        if axis {
            // The axis's values, rounded to a step that keeps them apart.
            let step = 10f32.powf(((hi - lo) / 2.0).max(1.0).log10().floor());
            for k in 0..3 {
                let gy = y + h * k as f32 / 2.0;
                self.painter.dashed_polyline(&[self.at(x, gy), self.at(x + w, gy)], 1.0 * self.k, self.c.rule, 4.0 * self.k, 5.0 * self.k);
                let value = hi - (hi - lo) * k as f32 / 2.0;
                let rounded = ((value / step).round() * step) as i64;
                self.text(x - 10.0, gy + 6.0, &number(rounded, false), 15.0, self.c.sub, UiTextAnchor::End, false);
            }
        }

        // The area under the line, a strip per segment.
        let base = self.at(0.0, y + h).y;
        let wash = Color::new(self.c.accent.r, self.c.accent.g, self.c.accent.b, self.c.accent.a * 0.14);
        for pair in points.windows(2) {
            let quad = [Vec2::new(pair[0].x, base), pair[0], pair[1], Vec2::new(pair[1].x, base)];
            self.painter.convex_polygon(&quad, wash, 0.0, Color::transparent());
        }
        if points.len() > 1 {
            self.painter.polyline(&points, width * self.k, self.c.accent, false);
        }
        if let Some(last) = points.last() {
            self.painter.circle_filled(*last, (width + 2.0) * self.k, self.c.accent);
        }

        if let Some(months) = months {
            for (i, month) in months.iter().enumerate() {
                let px = if values.len() > 1 { x + i as f32 * step } else { x + w };
                self.text(px, y + h + 26.0, &month.to_string(), 15.0, self.c.sub, UiTextAnchor::Middle, false);
            }
        }
    }

    // A HUD line icon (skin::hud_icon's art, drawn in the accent) centred on (x, y), `size` px.
    pub fn icon(&self, texture: engine::render::texture::TextureHandle, x: f32, y: f32, size: f32) {
        let half = size * 0.5;
        self.painter.image(texture, self.rect(x - half, y - half, size, size), Color::white());
    }
}

// A canvas BODY wide (or `width` px of the panel), `height` px tall, painted by `paint` in the
// page's colours and scale; recorded as `label` with `value`.
pub(super) fn body_canvas<F>(page: &PageContext, width: f32, height: f32, label: &str, value: serde_json::Value, paint: F) -> UiCanvas
where
    F: Fn(&Paint) + 'static,
{
    let k = page.scales.size;
    let c = page.colors;
    UiCanvas::new(Vec2::new(page.pt(width), page.pt(height)), move |_, painter, rect| {
        paint(&Paint::new(painter, rect, k, c));
    })
    .with_label(label, Some(value))
}

// ----------------------------------------------
// PopulationManagement / ResourcesManagement / FinancesManagement: kinds that open the book
// ----------------------------------------------

// Never opened themselves: the dialog framework opens CityManagement on their page in their place.
pub struct PopulationManagement {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { PopulationManagement, ["Population"] }

impl PopulationManagement {
    pub fn new(_context: &mut GameUiContext) -> Self {
        Self { menu: UiMenu::new(Self::KIND.to_string()).into_rc() }
    }
}

pub struct ResourcesManagement {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { ResourcesManagement, ["Resources"] }

impl ResourcesManagement {
    pub fn new(_context: &mut GameUiContext) -> Self {
        Self { menu: UiMenu::new(Self::KIND.to_string()).into_rc() }
    }
}

pub struct FinancesManagement {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { FinancesManagement, ["Finances"] }

impl FinancesManagement {
    pub fn new(_context: &mut GameUiContext) -> Self {
        Self { menu: UiMenu::new(Self::KIND.to_string()).into_rc() }
    }
}
