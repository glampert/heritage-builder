// An important event's popup (docs/plans/messages.md, §2, M11 Phase 1; event-<realm>.jpg): on the
// dialogs' frame over the city, laid out as the mockup generator's dialogs.py event() draws it on
// its 920 × 810 panel:
// - the picture (the skin's `event.picture` cut to it about its `focus`: the realm's home backdrop
//   until there's art of its own), the event's line icon in a disc over its middle;
// - the kicker ("FLOOD") and, at the right, the month it came in ("Month 7 of year 17 · the wet
//   season", in the realm's era);
// - the title, the paragraph, and up to three lines of what it brings, each under a rule with its
//   line icon;
// - its message's place as a button (Show on map for a cell or building, Open the goods page for a
//   page, Open the world map) and Ok, which closes it (so does Escape).
//
// What it shows is its message (crate::messages: Message::popup), kept outside the widgets
// (STATE): the in-game menus open the queued popups one at a time once no other dialog is open
// (dialog::open_event_popup, messages::take_popup), and a click on an event's line in the HUD's log
// opens it again. Built afresh each time it opens, and again when the window's size changes.
// dialog.open fails without a message to show.
//
// Paths: EventPopup/picture (kind canvas: icon), kicker, date, title, text, line_<n> (a line's text,
// from 1) and line_<n>_icon (kind canvas: icon), Show on map | Open the <page> page | Open the world
// map, Ok.

use common::{Color, Rect};
use engine::render::texture::TextureHandle;

use super::{
    screen::{BuiltOnOpen, Scales, left_group, spacer, text_line, tight},
    *,
};
use crate::{
    menu::{in_game::message_log, skin::SkinPanel},
    messages::{EventPopup as PopupDef, Message, MessageTarget},
    realm,
};

// ----------------------------------------------
// Layout (points on the generator's 920 × 810 panel, scaled with it)
// ----------------------------------------------

const PANEL: Vec2 = Vec2::new(920.0, 810.0);
// Where its contents go, in from the panel's edges (left, top, right, bottom): at least the
// panel's own content insets.
const PADDING: [f32; 4] = [90.0, 76.0, 90.0, 70.0];

const PICTURE_HEIGHT: f32 = 230.0;
const PICTURE_DIM: f32 = 0.25; // The mockup's wash over the picture.
const DISC_RADIUS: f32 = 58.0;
const DISC_STROKE: f32 = 4.0;
const DISC_ICON: f32 = 64.0; // The icon's glyph in the disc.

const KICKER_SIZE: f32 = 17.0;
const LETTER_SPACING: f32 = 2.0;
const DATE_SIZE: f32 = 17.0;
const TITLE_SIZE: f32 = 42.0;
const TEXT_SIZE: f32 = 21.0;
const LINE_SIZE: f32 = 20.0;
const LINE_ROW: f32 = 42.0;
const LINE_ICON: f32 = 22.0; // A line icon's glyph.
const LINE_TEXT_X: f32 = 44.0;

const BUTTON_TEXT_SIZE: f32 = 22.0;
const BUTTON_HEIGHT: f32 = 56.0;
const PLACE_WIDTH: f32 = 300.0;
const OK_WIDTH: f32 = 200.0;
const BUTTON_GAP: f32 = 20.0;

// The gaps from the top: after the picture, the kicker's row, the title, the paragraph; the least
// room above the buttons.
const GAPS: [f32; 5] = [40.0, 10.0, 14.0, 14.0, 24.0];

// How far the contents may be set smaller to fit (a long paragraph), and each step.
const MIN_FIT: f32 = 0.6;
const FIT_STEP: f32 = 0.94;

// ----------------------------------------------
// State
// ----------------------------------------------

struct PopupState {
    built: BuiltOnOpen,
    message: Option<Message>, // The event shown.
}

static STATE: UiStaticVar<PopupState> = UiStaticVar::new(PopupState { built: BuiltOnOpen::new(), message: None });

// Opens `message`'s popup (it has one), in place of one open.
pub(super) fn open_message(message: Message, context: &mut GameUiContext) -> bool {
    if message.popup.is_none() {
        return false;
    }
    if super::is_open(DialogMenuKind::EventPopup) {
        super::close(DialogMenuKind::EventPopup, context);
    }
    STATE.as_mut().message = Some(message);
    let opened = super::open(DialogMenuKind::EventPopup, false, context);
    if !opened {
        STATE.as_mut().message = None;
    }
    opened
}

// The message whose popup is open.
pub fn shown() -> Option<Message> {
    if !super::is_open(DialogMenuKind::EventPopup) {
        return None;
    }
    STATE.as_ref().message.clone()
}

// Sets the popup up for opening (dialog::open_dialog builds it afresh then).
pub(super) fn prepare() {
    STATE.as_mut().built.prepare();
}

// Whether the open popup must be built again: the window's size changed.
pub(super) fn needs_rebuild(context: &GameUiContext) -> bool {
    STATE.as_ref().built.needs_rebuild(context)
}

// ----------------------------------------------
// EventPopup
// ----------------------------------------------

pub struct EventPopup {
    menu: UiMenuRcMut,
}

impl DialogMenuFactory for EventPopup {
    const KIND: DialogMenuKind = DialogMenuKind::EventPopup;
    const TITLE: &'static [&'static str] = &["Event"];

    fn create(context: &mut GameUiContext) -> DialogMenuImpl {
        DialogMenuImpl::from(EventPopup::new(context))
    }
}

impl DialogMenu for EventPopup {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> DialogMenuKind {
        Self::KIND
    }

    fn menu(&self) -> &UiMenuRcMut {
        &self.menu
    }

    // Only with an event to show.
    fn open(&mut self, context: &mut GameUiContext) -> bool {
        if STATE.as_ref().message.as_ref().is_none_or(|message| message.popup.is_none()) {
            STATE.as_mut().built.active = false;
            return false;
        }
        self.menu_mut().open(context);
        true
    }

    fn close(&mut self, context: &mut GameUiContext) -> bool {
        self.menu_mut().close(context);
        let state = STATE.as_mut();
        state.built.active = false;
        state.message = None;
        true
    }

    fn reset(&mut self) {
        let menu = self.menu_mut();
        menu.reset_message_box();
        menu.set_flags(UiMenuFlags::IsOpen, false);
        let state = STATE.as_mut();
        state.built.active = false;
        state.message = None;
    }
}

impl EventPopup {
    pub fn new(context: &mut GameUiContext) -> Self {
        let kind = Self::KIND.to_string();
        let window = ui::screen_size(context.ui_sys);
        let frame = DialogFrame::from_skin(Self::KIND);
        let size = frame.size(window);
        let menu = UiMenu::new(kind).with_flags(frame.menu_flags()).with_size(size).with_widget_spacing(Vec2::zero());

        // Built at boot and on every rebuild for every dialog kind, but shown only once prepared
        // for opening with an event: until then an empty menu.
        let message = STATE.as_ref().message.clone();
        let (true, Some(message)) = (STATE.as_ref().built.active, message) else {
            return Self { menu: menu.into_rc() };
        };
        let Some(popup) = message.popup.as_deref() else {
            return Self { menu: menu.into_rc() };
        };

        // The panel, and the contents inside its own insets and the mockup's margins.
        let scale = (size.x / PANEL.x).min(size.y / PANEL.y);
        let panel_name = skin::dialog_def(&Self::KIND.to_string()).panel.unwrap_or_else(|| "dialog".into());
        let panel = skin::panel(context, &panel_name);
        let own = match &panel {
            SkinPanel::NineSlice { content, .. } => *content,
            SkinPanel::Sprite { content, .. } => content.unwrap_or(UiMenuInsets::same(0.0)),
        };
        let insets = UiMenuInsets::points(
            own.left.max(PADDING[0] * scale).round(),
            own.top.max(PADDING[1] * scale).round(),
            own.right.max(PADDING[2] * scale).round(),
            own.bottom.max(PADDING[3] * scale).round(),
        );
        let mut menu = panel.apply_to_menu(menu).with_content_insets(insets);
        let inner = Vec2::new(size.x - insets.left - insets.right, size.y - insets.top - insets.bottom);

        let date = date_text(context, message.month);
        // The mockup's layout at the panel's scale, set smaller until it fits (a long paragraph).
        let mut fit = 1.0;
        let (parts, buttons) = loop {
            let scales = Scales::with_size(scale * fit);
            let parts = contents(context, scales, popup, &date, inner.x);
            let buttons = buttons(context, scales, &message, inner.x);
            let height: f32 = parts.iter().map(|part| part.measure(context).y).sum::<f32>()
                + buttons.measure(context).y
                + scales.pt(GAPS[4]);
            if height <= inner.y - 2.0 || fit <= MIN_FIT {
                break (parts, buttons);
            }
            fit = (fit * FIT_STEP).max(MIN_FIT);
        };
        let parts_height: f32 = parts.iter().map(|part| part.measure(context).y).sum();
        let room = (inner.y - parts_height - buttons.measure(context).y - 2.0).floor().max(0.0);
        for part in parts {
            menu.add_widget(part);
        }
        menu.add_widget(spacer(room));
        menu.add_widget(buttons);

        STATE.as_mut().built.built(context);
        Self { menu: menu.into_rc() }
    }

    // The button to the event's place: the popup closes, then the camera moves there or the page
    // opens.
    fn on_place(context: &mut GameUiContext, message: &Message) {
        super::close(DialogMenuKind::EventPopup, context);
        message_log::go_to_target(message, context);
    }
}

// "Month 7 of year 17 · the wet season": the ordinal month `month` in the UI realm's era and words.
fn date_text(context: &GameUiContext, month: u32) -> String {
    let realm = realm::ui_realm_for(false).realm;
    let calendar = context.sim.calendar();
    let (world_year, month) = (month / 12, month % 12 + 1);
    let year = calendar.era_at(world_year).map_or(world_year, |(_, year)| year);
    let season = crate::menu::in_game::season_text(realm, calendar.settings().season_of(month));
    let mut season_chars = season.chars();
    let season: String = match season_chars.next() {
        Some(first) => first.to_lowercase().chain(season_chars).collect(),
        None => String::new(),
    };
    ui::text::format(UiTextCategory::Menu, "event.when", &[
        ("month", &month.to_string()),
        ("year", &year.to_string()),
        ("season", &season),
    ])
}

// The button to a message's place, by its target's kind.
fn place_label(target: &MessageTarget) -> UiString {
    match target {
        MessageTarget::Cell(_) | MessageTarget::Building(..) => {
            UiString::key(UiTextCategory::Menu, "event.button.show_on_map")
        }
        MessageTarget::CityPage(page) => {
            let title = ui::text::get(UiTextCategory::City, &format!("book.page.{page}")).to_lowercase();
            ui::text::format(UiTextCategory::Menu, "event.button.open_page", &[("page", &title)]).into()
        }
        MessageTarget::WorldMap(_) => UiString::key(UiTextCategory::Menu, "event.button.world_map"),
    }
}

// ----------------------------------------------
// The contents
// ----------------------------------------------

// The picture, the kicker and date, the title, the paragraph and the lines, `width` points wide.
fn contents(context: &mut GameUiContext, scales: Scales, popup: &PopupDef, date: &str, width: f32) -> Vec<UiWidgetImpl> {
    let color = skin::intermission_color;
    let gap = |index: usize| spacer(scales.pt(GAPS[index]));
    let mut parts: Vec<UiWidgetImpl> = Vec::new();

    parts.push(picture(context, scales, &popup.icon, width).into());
    parts.push(gap(0).into());

    // The kicker at the left, the date at the right.
    let kicker_font = scales.heading_font(KICKER_SIZE);
    let kicker =
        text_line(&popup.kicker.to_uppercase(), "kicker", kicker_font, color("accent", "accent"), 0.0, UiTextAlign::Left)
            .with_heading_font()
            .with_letter_spacing(scales.pt(LETTER_SPACING));
    let kicker = tight(context, kicker, kicker_font, KICKER_SIZE, scales);
    let date_font = scales.font(DATE_SIZE);
    let date = UiRichText::from_spans([UiTextSpan::emphasized(date.to_string())])
        .with_label("date")
        .with_font_scale(date_font)
        .with_align(UiTextAlign::Right)
        .with_text_color(color("sub", "sub"));
    let date = tight(context, date, date_font, DATE_SIZE, scales);
    let kicker_width = kicker.measure(context).x.ceil();
    let mut row = UiColumns::new().with_spacing(scales.pt(12.0)).with_width(width);
    row.add_column(UiColumnWidth::Fixed(kicker_width), left_group().with_widget(kicker));
    row.add_column(
        UiColumnWidth::Fraction(1.0),
        UiWidgetGroup::new().with_widget(date.with_width((width - kicker_width - scales.pt(12.0)).max(1.0))),
    );
    parts.push(row.into());
    parts.push(gap(1).into());

    let title_font = scales.heading_font(TITLE_SIZE);
    let title = text_line(&popup.title, "title", title_font, color("heading", "title"), width, UiTextAlign::Left)
        .with_heading_font();
    parts.push(tight(context, title, title_font, TITLE_SIZE, scales).into());

    if !popup.text.is_empty() {
        parts.push(gap(2).into());
        let font = scales.font(TEXT_SIZE);
        parts.push(text_line(&popup.text, "text", font, color("text", "ink"), width, UiTextAlign::Left).into());
    }

    if !popup.lines.is_empty() {
        parts.push(gap(3).into());
        let rule = color("rule", "sub");
        let font = scales.font(LINE_SIZE);
        let row_height = scales.pt(LINE_ROW);
        let text_x = scales.pt(LINE_TEXT_X);
        for (index, line) in popup.lines.iter().enumerate() {
            let number = index + 1;
            parts.push(UiSeparator::new().with_color(rule).with_size(Vec2::new(width, scales.pt(1.5).max(1.0))).into());
            let text = text_line(
                &line.text,
                &format!("line_{number}"),
                font,
                color("text", "ink"),
                width - text_x,
                UiTextAlign::Left,
            );
            let text_height = text.measure(context).y;
            let height = row_height.max(text_height + scales.pt(8.0));
            let pad = ((height - text_height) * 0.5).floor().max(0.0);
            let icon = line_icon(context, scales, &line.icon, Vec2::new(text_x, height), number);
            let mut columns = UiColumns::new().with_spacing(0.0).with_width(width);
            columns.add_column(UiColumnWidth::Fixed(text_x), left_group().with_widget(icon));
            columns.add_column(
                UiColumnWidth::Fraction(1.0),
                left_group().with_widget_spacing(Vec2::zero()).with_widget(spacer(pad)).with_widget(text),
            );
            parts.push(columns.into());
        }
    }
    parts
}

// A line icon (messages::popups::LINE_ICONS) and how to draw it: the HUD's line icons as they are
// (drawn in the accent), the City book's page icons tinted with the accent; the glyph's share of its
// sprite (the HUD icons' bleed).
fn icon_texture(context: &mut GameUiContext, name: &str) -> (TextureHandle, Color, f32) {
    if skin::HUD_ICONS.contains(&name) {
        let (texture, size) = skin::hud_icon(context, name);
        let glyph = ((size.x - 2.0 * skin::hud_icon_bleed()) / size.x.max(1.0)).clamp(0.1, 1.0);
        (texture, Color::white(), glyph)
    } else {
        let name = if skin::CITY_ICONS.contains(&name) { name } else { "star" };
        // The page icons: a 36-point box with 4 points of bleed (ui-art's page_icon).
        (skin::city_icon(context, name), skin::intermission_color("accent", "accent"), 28.0 / 36.0)
    }
}

// A line's icon, in a canvas `size` large, its glyph LINE_ICON points at the left.
fn line_icon(context: &mut GameUiContext, scales: Scales, name: &str, size: Vec2, number: usize) -> UiCanvas {
    let (texture, tint, glyph) = icon_texture(context, name);
    let drawn = scales.pt(LINE_ICON) / glyph;
    let x = scales.pt(14.0);
    UiCanvas::new(size, move |_, painter, rect| {
        let centre = Vec2::new(rect.min.x + x, rect.min.y + rect.height() * 0.5);
        painter.image(
            texture,
            Rect::from_pos_and_size(centre - Vec2::new(drawn, drawn) * 0.5, Vec2::new(drawn, drawn)),
            tint,
        );
    })
    .with_label(format!("line_{number}_icon"), Some(serde_json::json!({ "icon": name })))
}

// The picture: the skin's event picture cut to `width` × PICTURE_HEIGHT about its focus, washed
// darker, the event's icon in a disc over its middle, in the briefing image's outline.
fn picture(context: &mut GameUiContext, scales: Scales, icon: &str, width: f32) -> UiCanvas {
    let size = Vec2::new(width, scales.pt(PICTURE_HEIGHT));
    let def = skin::event();
    let texture = (!def.picture.is_empty()).then(|| context.load_texture(PathRef::from_str(&def.picture)));
    let texture_size = texture
        .and_then(|texture| context.render_sys().texture_cache().texture_size(texture))
        .map_or(Vec2::new(1.0, 1.0), |size| Vec2::new(size.width as f32, size.height as f32));
    // The part shown: as much of the picture as its shape allows, about the focus, kept on it.
    let aspect = size.x / size.y.max(1.0);
    let span = if texture_size.x / texture_size.y > aspect {
        Vec2::new(texture_size.y * aspect, texture_size.y)
    } else {
        Vec2::new(texture_size.x, texture_size.x / aspect)
    };
    let focus = Vec2::new(def.focus[0] * texture_size.x, def.focus[1] * texture_size.y);
    let min = Vec2::new(
        (focus.x - span.x * 0.5).clamp(0.0, (texture_size.x - span.x).max(0.0)),
        (focus.y - span.y * 0.5).clamp(0.0, (texture_size.y - span.y).max(0.0)),
    );
    let uv = Rect::from_pos_and_size(
        Vec2::new(min.x / texture_size.x, min.y / texture_size.y),
        Vec2::new(span.x / texture_size.x, span.y / texture_size.y),
    );

    let briefing = skin::briefing();
    let outline = skin::color(&briefing.image_outline).unwrap_or(Color::black());
    let outline_width = (briefing.image_outline_width * scales.size).max(1.0);
    let accent = skin::intermission_color("accent", "accent");
    let disc_fill = skin::intermission_color("note_fill", "panel");
    let (icon_texture, icon_tint, glyph) = icon_texture(context, icon);
    let k = scales.size;
    let icon_drawn = DISC_ICON * k / glyph;
    UiCanvas::new(size, move |_, painter, rect| {
        match texture {
            Some(texture) => painter.image_uv(texture, rect, uv, Color::white()),
            None => painter.rect_filled(rect, outline, 0.0),
        }
        painter.rect_filled(rect, Color::new(0.07, 0.04, 0.02, PICTURE_DIM), 0.0);
        let centre = rect.center();
        painter.circle_filled(centre, DISC_RADIUS * k, disc_fill);
        painter.circle_stroke(centre, DISC_RADIUS * k, (DISC_STROKE * k).max(1.5), accent);
        let drawn = Vec2::new(icon_drawn, icon_drawn);
        painter.image(icon_texture, Rect::from_pos_and_size(centre - drawn * 0.5, drawn), icon_tint);
        painter.rect_stroke(rect, 4.0 * k, outline_width, outline);
    })
    .with_label("picture", Some(serde_json::json!({ "icon": icon })))
}

// The button to the event's place (when its message has one), then Ok, at the right.
fn buttons(context: &mut GameUiContext, scales: Scales, message: &Message, width: f32) -> UiColumns {
    let sized = |button: UiTextButton, points: f32| {
        scales.button_text(
            button.with_width(UiTextButtonWidth::Fixed(scales.pt(points))).with_min_height(scales.pt(BUTTON_HEIGHT)),
            BUTTON_TEXT_SIZE,
        )
    };
    let mut columns = UiColumns::new().with_spacing(scales.pt(BUTTON_GAP)).with_width(width);
    columns.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());
    if let Some(target) = &message.target {
        let shown = message.clone();
        let place = skin::button(context, ButtonRole::Secondary, place_label(target), move |_, context| {
            EventPopup::on_place(context_as_mut::<GameUiContext>(context), &shown);
        });
        columns
            .add_column(UiColumnWidth::Fixed(scales.pt(PLACE_WIDTH)), left_group().with_widget(sized(place, PLACE_WIDTH)));
    }
    let ok = skin::button(context, ButtonRole::Primary, UiString::key(UiTextCategory::Menu, "button.ok"), |_, context| {
        super::close(DialogMenuKind::EventPopup, context_as_mut::<GameUiContext>(context));
    });
    columns.add_column(UiColumnWidth::Fixed(scales.pt(OK_WIDTH)), left_group().with_widget(sized(ok, OK_WIDTH)));
    columns
}
