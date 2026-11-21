// The trade limits dialog (docs/plans/trade.md, §3), TradeLimits: what the player lets a partner trade.
// No mockup: it's laid out in the City book's Goods page style (its colours and painted rows).
//
// Its title names the partner; a line under it gives the route and whether it's open. Then a section
// per side: the goods the partner sells (They sell: imports) and those it buys (They buy: exports), a
// row per good: its name, what the city holds (yards and granaries), the partner's price and how
// many it still sells or buys this year, and a dropdown for the limit: Don't import, or Up to N in
// the city; Don't export, or Keep N in the city. The dropdowns set the limit at once
// (CityTrade::set_limit). Close at the bottom.
//
// The world map's side panel opens it for a partner whose route is open (open); dialog.open opens
// it for the game's first partner. Only in a game that trades. Built afresh when it opens, and
// again when the window's size, the partner, its route or the month changes (needs_rebuild), so
// the figures follow the city month by month, as the City book's do.
//
// Paths: TradeLimits/title (canvas: partner, name), TradeLimits/route (canvas: line, detail, open),
// TradeLimits/They sell and TradeLimits/They buy (canvas: the column titles), a row per good,
// TradeLimits/<Good> terms (canvas: good, held, price, left, side) and its dropdown
// TradeLimits/<Good>, and TradeLimits/Close.

use common::Color;
use engine::ui::UiTextAnchor;
use serde_json::json;

use super::{
    city_book::{BookColors, Paint},
    *,
};
use crate::{
    GameLoop,
    config::GameConfigs,
    sim::{resources::ResourceKind, trade::TradeLimit},
    trade::{self, GoodTerms},
};

// ----------------------------------------------
// Layout (points on a 1920 × 1080 window, scaled by the dialog's scale and the UI scale's growth)
// ----------------------------------------------

const TITLE_HEIGHT: f32 = 60.0;
const ROUTE_HEIGHT: f32 = 40.0;
const HEADER_HEIGHT: f32 = 50.0; // A section's title and column titles.
const ROW_HEIGHT: f32 = 46.0;
const SECTION_GAP: f32 = 18.0;
const DROPDOWN_WIDTH: f32 = 250.0;
const COLUMN_GAP: f32 = 16.0; // Between a row's figures and its dropdown.
const BUTTON_WIDTH: f32 = 220.0;
const BUTTON_HEIGHT: f32 = 56.0;
const SLACK: f32 = 24.0;

// The limits the dropdowns offer, besides none: what the city may hold of an import, and what it
// keeps of an export.
const IMPORT_STEPS: [u32; 7] = [10, 20, 40, 80, 120, 160, 320];
const EXPORT_STEPS: [u32; 8] = [0, 10, 20, 40, 80, 120, 160, 320];

// ----------------------------------------------
// State
// ----------------------------------------------

struct TradeLimitsState {
    partner: Option<String>, // The city whose limits it shows; None: the game's first partner.
    active: bool,            // Prepared for opening, until it closes.
    built: Option<BuildKey>,
}

#[derive(Clone, PartialEq, Debug)]
struct BuildKey {
    window: (u32, u32),
    partner: Option<String>,
    open: bool,
    month: u32,
}

static STATE: UiStaticVar<TradeLimitsState> =
    UiStaticVar::new(TradeLimitsState { partner: None, active: false, built: None });

// Opens the dialog for the partner `city`, over the dialogs open.
pub(super) fn open(context: &mut GameUiContext, city: &str) {
    STATE.as_mut().partner = Some(city.to_string());
    super::open(DialogMenuKind::TradeLimits, false, context);
}

// Sets it up for opening: the partner asked for (open), else the game's first.
pub(super) fn prepare() {
    let state = STATE.as_mut();
    state.active = true;
    state.built = None;
}

// The partner it shows: the one asked for if the game trades with it, else the game's first.
fn shown_partner(context: &GameUiContext) -> Option<String> {
    let trade = context.sim.trade();
    STATE
        .as_ref()
        .partner
        .clone()
        .filter(|city| trade.partner(city).is_some())
        .or_else(|| trade.partners().first().map(|partner| partner.city().to_string()))
}

fn build_key(context: &GameUiContext) -> BuildKey {
    let window = ui::screen_size(context.ui_sys);
    let partner = shown_partner(context);
    let open = partner.as_deref().and_then(|city| context.sim.trade().partner(city)).is_some_and(|partner| partner.open);
    let sim = &GameConfigs::get().sim;
    let month = context.sim.calendar().date(sim.days_per_month, sim.seconds_per_month).ordinal_month();
    BuildKey { window: (window.x.round() as u32, window.y.round() as u32), partner, open, month }
}

// Whether the open dialog must be built again: the window's size, the partner, its route or the
// month changed.
pub(super) fn needs_rebuild(context: &GameUiContext) -> bool {
    STATE.as_ref().built.as_ref() != Some(&build_key(context))
}

// ----------------------------------------------
// TradeLimits
// ----------------------------------------------

pub struct TradeLimits {
    menu: UiMenuRcMut,
}

impl DialogMenuFactory for TradeLimits {
    const KIND: DialogMenuKind = DialogMenuKind::TradeLimits;
    const TITLE: &'static [&'static str] = &["Trade Limits"];

    fn create(context: &mut GameUiContext) -> DialogMenuImpl {
        DialogMenuImpl::from(TradeLimits::new(context))
    }
}

impl DialogMenu for TradeLimits {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> DialogMenuKind {
        Self::KIND
    }

    fn menu(&self) -> &UiMenuRcMut {
        &self.menu
    }

    // Only in a game that trades.
    fn open(&mut self, context: &mut GameUiContext) -> bool {
        let in_game = !super::in_home_menu() && GameLoop::is_initialized() && GameLoop::get().is_in_game();
        if !in_game || !STATE.as_ref().active || shown_partner(context).is_none() {
            STATE.as_mut().active = false;
            return false;
        }
        self.menu_mut().open(context);
        true
    }

    fn close(&mut self, context: &mut GameUiContext) -> bool {
        self.menu_mut().close(context);
        let state = STATE.as_mut();
        state.active = false;
        state.partner = None;
        true
    }
}

impl TradeLimits {
    pub fn new(context: &mut GameUiContext) -> Self {
        let kind = Self::KIND.to_string();
        let flags = DialogFrame::from_skin(Self::KIND).menu_flags();
        STATE.as_mut().built = Some(build_key(context));

        // Built at boot and on every rebuild for every dialog kind, but filled only once prepared
        // for opening, in a game that trades.
        let partner = shown_partner(context);
        let (Some(city), true) = (partner, STATE.as_ref().active) else {
            return Self { menu: skin::dialog(context, &kind, flags).into_rc() };
        };
        let Some(terms) = trade::terms(context.sim.trade(), &city) else {
            return Self { menu: skin::dialog(context, &kind, flags).into_rc() };
        };

        let window = ui::screen_size(context.ui_sys);
        let k = skin::dialog_scale(window) * skin::dialog_ui_growth(&kind, window);
        let fit = skin::dialog_text_fit(&kind, window);
        let points = |size: f32| (size * k).round();
        let content = skin::dialog_content_size(&kind, window);
        let colors = BookColors::read();
        let name = trade::partner_name(context.sim.trade(), &city);
        let holdings = {
            let sim_context = context.new_sim_context();
            trade::holdings(&sim_context)
        };

        let mut menu = skin::dialog(context, &kind, flags).with_widget_spacing(Vec2::zero());
        // The height of what's added, so Close goes at the bottom.
        let mut used = 0.0;
        let width = content.x;
        let px = move |size: f32| size * fit;

        // The title and the route.
        let title = ui::text::format(UiTextCategory::Menu, "trade_limits.title", &[("partner", &name)]);
        let title_value = json!({ "partner": city, "name": name });
        let title_text = title.clone();
        used += points(TITLE_HEIGHT) + points(ROUTE_HEIGHT);
        menu.add_widget(
            UiCanvas::new(Vec2::new(width, points(TITLE_HEIGHT)), move |_, painter, rect| {
                let p = Paint::new(painter, rect, k, colors);
                p.text(0.0, 40.0, &title_text, px(34.0), p.c.title, UiTextAnchor::Start, true);
            })
            .with_label("title", Some(title_value)),
        );
        let trade_state = context.sim.trade();
        let line = trade::route_line(trade_state, &city).unwrap_or_default();
        let detail = trade::route_detail(trade_state, &city).unwrap_or_default();
        let open = trade_state.partner(&city).is_some_and(|partner| partner.open);
        let route_value = json!({ "line": line, "detail": detail, "open": open });
        let route_text = if detail.is_empty() { line.clone() } else { format!("{line} · {detail}") };
        menu.add_widget(
            UiCanvas::new(Vec2::new(width, points(ROUTE_HEIGHT)), move |_, painter, rect| {
                let p = Paint::new(painter, rect, k, colors);
                p.text(0.0, 24.0, &route_text, px(20.0), p.c.sub, UiTextAnchor::Start, false);
            })
            .with_label("route", Some(route_value)),
        );

        // A section per side.
        let row_width = (width - points(DROPDOWN_WIDTH) - points(COLUMN_GAP)).max(1.0);
        let partner = trade_state.partner(&city).cloned();
        let faith_scale = context.sim.faith().trade_scale();
        for (side, goods) in [(Side::Import, &terms.sells), (Side::Export, &terms.buys)] {
            if goods.is_empty() {
                continue;
            }
            used += points(SECTION_GAP) + points(HEADER_HEIGHT);
            menu.add_widget(spacer_line(points(SECTION_GAP)));
            menu.add_widget(section_header(side, width, row_width, points(HEADER_HEIGHT), k, fit, colors));
            for good in goods {
                let traded = partner.as_ref().map_or(0, |partner| match side {
                    Side::Import => partner.sold[good.good.index()],
                    Side::Export => partner.bought[good.good.index()],
                });
                let price = partner.as_ref().map_or(good.price, |partner| match side {
                    Side::Import => trade::import_price(faith_scale, partner, good),
                    Side::Export => trade::export_price(faith_scale, partner, good),
                });
                let row = TermsRow {
                    good: *good,
                    side,
                    held: holdings[good.good.index()],
                    price,
                    left: good.per_year.saturating_sub(traded),
                };
                let current = partner.as_ref().map_or(TradeLimit::Off, |partner| partner.limit(good.good));
                let line = UiWidgetGroup::new()
                    .with_stack_vertically(false)
                    .with_widget_spacing(Vec2::new(points(COLUMN_GAP), 0.0))
                    .with_widget(row.canvas(row_width, points(ROW_HEIGHT), k, fit, colors))
                    .with_widget(limit_dropdown(context, &city, good.good, side, current, points(DROPDOWN_WIDTH), fit));
                used += line.measure(context).y;
                menu.add_widget(line);
            }
        }

        // Close, at the bottom.
        let close =
            make_dialog_close_button(context, ButtonRole::Primary, UiString::key(UiTextCategory::Hud, "button.close"))
                .with_width(UiTextButtonWidth::Fixed(points(BUTTON_WIDTH)))
                .with_min_height(points(BUTTON_HEIGHT));
        let font_scale = close.font_scale().0 * fit;
        let close = close.with_font_scale(UiFontScale(font_scale));
        let room = (content.y - used - close.measure(context).y - SLACK * k).floor();
        if room > 0.0 {
            menu.add_widget(spacer_line(room));
        }
        menu.add_widget(close);

        Self { menu: menu.into_rc() }
    }
}

// ----------------------------------------------
// Rows
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Side {
    Import, // A good the partner sells.
    Export, // A good it buys.
}

impl Side {
    fn name(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Export => "export",
        }
    }
}

// The figures in a good's row.
struct TermsRow {
    good: GoodTerms,
    side: Side,
    held: u32,
    price: u32,
    left: u32,
}

// The x of each figure's right edge in a row, as a share of the row's width.
const HELD_AT: f32 = 0.52;
const PRICE_AT: f32 = 0.74;
const LEFT_AT: f32 = 0.98;

impl TermsRow {
    fn canvas(&self, width: f32, height: f32, k: f32, fit: f32, colors: BookColors) -> UiCanvas {
        let name = self.good.good.display_name().into_owned();
        let value = json!({
            "good": self.good.good.name(),
            "side": self.side.name(),
            "held": self.held,
            "price": self.price,
            "left": self.left,
        });
        let held = group_digits(i64::from(self.held));
        let price = ui::text::format(UiTextCategory::Menu, "trade_limits.price_value", &[("gold", &self.price.to_string())]);
        let left = group_digits(i64::from(self.left));
        let label = format!("{name} terms");
        let w = width / k;
        let size = move |px: f32| px * fit;
        UiCanvas::new(Vec2::new(width, height), move |_, painter, rect| {
            let p = Paint::new(painter, rect, k, colors);
            let y = 28.0;
            p.painter.rect_filled(p.rect(4.0, y - 12.0, 9.0, 9.0), p.c.accent, 1.0 * k);
            p.text(22.0, y, &name, size(20.0), p.c.text, UiTextAnchor::Start, false);
            p.text(w * HELD_AT, y, &held, size(19.0), p.c.text, UiTextAnchor::End, false);
            p.text(w * PRICE_AT, y, &price, size(19.0), p.c.text, UiTextAnchor::End, false);
            let left_color: Color = if left == "0" { p.c.sub } else { p.c.text };
            p.text(w * LEFT_AT, y, &left, size(19.0), left_color, UiTextAnchor::End, false);
            p.rule(0.0, w, 44.0);
        })
        .with_label(label, Some(value))
    }
}

// A section's title and its column titles.
fn section_header(side: Side, width: f32, row_width: f32, height: f32, k: f32, fit: f32, colors: BookColors) -> UiCanvas {
    let get = |key: &str| ui::text::get(UiTextCategory::Menu, key).to_string();
    let title = match side {
        Side::Import => get("trade_limits.they_sell"),
        Side::Export => get("trade_limits.they_buy"),
    };
    let held = get("trade_limits.held");
    let price = get("trade_limits.price");
    let left = match side {
        Side::Import => get("trade_limits.left_sell"),
        Side::Export => get("trade_limits.left_buy"),
    };
    let limit = get("trade_limits.limit");
    let label = match side {
        Side::Import => "They sell",
        Side::Export => "They buy",
    };
    let value = json!({ "title": title, "columns": [held, price, left, limit] });
    let w = row_width / k;
    let full = width / k;
    let size = move |px: f32| px * fit;
    UiCanvas::new(Vec2::new(width, height), move |_, painter, rect| {
        let p = Paint::new(painter, rect, k, colors);
        p.section(0.0, 22.0, &title);
        let y = 44.0;
        p.text(w * HELD_AT, y, &held, size(16.0), p.c.sub, UiTextAnchor::End, false);
        p.text(w * PRICE_AT, y, &price, size(16.0), p.c.sub, UiTextAnchor::End, false);
        p.text(w * LEFT_AT, y, &left, size(16.0), p.c.sub, UiTextAnchor::End, false);
        p.text(full, y, &limit, size(16.0), p.c.sub, UiTextAnchor::End, false);
    })
    .with_label(label, Some(value))
}

// The limits a good's dropdown offers, the current one among them (a limit set by automation that
// isn't a step is offered too, in its place).
fn limit_choices(side: Side, current: TradeLimit) -> Vec<TradeLimit> {
    let mut choices = vec![TradeLimit::Off];
    match side {
        Side::Import => choices.extend(IMPORT_STEPS.iter().map(|&amount| TradeLimit::Import(amount))),
        Side::Export => choices.extend(EXPORT_STEPS.iter().map(|&amount| TradeLimit::Export(amount))),
    }
    let fits_side = matches!((side, current), (Side::Import, TradeLimit::Import(_)) | (Side::Export, TradeLimit::Export(_)));
    if fits_side && !choices.contains(&current) {
        let amount = |limit: &TradeLimit| match limit {
            TradeLimit::Import(amount) | TradeLimit::Export(amount) => Some(*amount),
            TradeLimit::Off => None,
        };
        let at = choices.iter().position(|choice| amount(choice) > amount(&current)).unwrap_or(choices.len());
        choices.insert(at, current);
    }
    choices
}

fn limit_text(limit: TradeLimit, side: Side) -> String {
    match (limit, side) {
        (TradeLimit::Off, Side::Import) => ui::text::get(UiTextCategory::Menu, "trade_limits.no_import").to_string(),
        (TradeLimit::Off, Side::Export) => ui::text::get(UiTextCategory::Menu, "trade_limits.no_export").to_string(),
        (TradeLimit::Import(amount), _) => {
            ui::text::format(UiTextCategory::Menu, "trade_limits.import_up_to", &[("count", &amount.to_string())])
        }
        (TradeLimit::Export(amount), _) => {
            ui::text::format(UiTextCategory::Menu, "trade_limits.keep", &[("count", &amount.to_string())])
        }
    }
}

// A good's limit: a dropdown labelled by the good (hidden), which reads the limit each frame and
// sets it when the player picks one.
fn limit_dropdown(
    context: &mut GameUiContext,
    city: &str,
    good: ResourceKind,
    side: Side,
    current: TradeLimit,
    width: f32,
    fit: f32,
) -> UiDropdown {
    let choices = limit_choices(side, current);
    let items: Vec<String> = choices.iter().map(|choice| limit_text(*choice, side)).collect();
    let dropdown = skin::dropdown(context, good.display_name().into_owned(), items);
    let font_scale = UiFontScale(dropdown.font_scale().0 * fit);
    let (read_city, read_choices) = (city.to_string(), choices.clone());
    let set_city = city.to_string();
    dropdown
        .with_hidden_label(good.display_name().into_owned())
        .with_width(width)
        .with_font_scale(font_scale)
        .on_get_current_selection(move |_, context| {
            let game = context_as_mut::<GameUiContext>(context);
            let limit = game.sim.trade().partner(&read_city).map_or(TradeLimit::Off, |partner| partner.limit(good));
            read_choices.iter().position(|choice| *choice == limit).unwrap_or(0)
        })
        .on_selection_changed(move |dropdown, context| {
            let limit = choices.get(dropdown.current_selection_index()).copied().unwrap_or(TradeLimit::Off);
            let game = context_as_mut::<GameUiContext>(context);
            game.sim.trade_mut().set_limit(&set_city, good, limit);
        })
}

// A blank line `height` points tall.
fn spacer_line(height: f32) -> UiSeparator {
    UiSeparator::new().with_size(Vec2::new(1.0, height.max(1.0)))
}
