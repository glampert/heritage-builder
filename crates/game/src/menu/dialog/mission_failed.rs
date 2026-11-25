// The failure screen (docs/plans/ratings.md, §4, Phase 3; no mockup: decision Q8, the debrief's
// layout without the stele): a full-screen dialog on the skin's intermission backdrop, its panel in
// the middle:
// - "MISSION N FAILED", the mission's name, its city and the year in the realm's era;
// - why it was lost, in the realm's words ("The court has recalled you": `failure.<realm>.<reason>`)
//   and plainly under it ("The Mandate stayed below 10 for 12 months.");
// - the goals as they stood (the debrief's table) and the stats (population, treasury, years taken);
// - Return to the menu, Load a save and Try again (the mission from its start: its briefing opens,
//   the story the profile has seen doesn't play again).
//
// There's no Keep playing (decision Q7): Escape asks in `Return to Menu Popup` (Cancel, Return to
// the menu) instead of closing it. Nothing is recorded in the profile. campaign::request_failure_screen
// opens it when the mission is lost (ratings::failure::check_failure, or a lost game's save loaded),
// over any dialog open. It's built afresh each time it opens, and again when the window's size
// changes. A preview (the developer editor's Screens menu) shows it for a reason picked there, and
// its buttons only close it.
//
// Paths: MissionFailed/panel/kicker, name, place, reason, why (kind rich_text), goals (kind table:
// Goals, Target, Achieved, a status per row), stats (kind key_values), Return to the menu, Load a
// save, Try again.

use super::{
    mission_complete::{self, GoalRow},
    screen::{BuiltOnOpen, Scales, left_group, spacer, text_line, tight},
    *,
};
use crate::{
    GameLoop,
    campaign::{self, config::CampaignConfigs},
    config::GameConfigs,
    ratings::{self, FailureReason, RatingKind},
    realm,
};

// ----------------------------------------------
// Layout (points on the generator's 1920 × 1080 screen, scaled by the window: screen::Scales)
// ----------------------------------------------

// The debrief's panel, in the middle of the screen.
const PANEL_RECT: [f32; 4] = [450.0, 60.0, 1020.0, 960.0];
const PANEL_PADDING: [f32; 4] = [56.0, 56.0, 56.0, 40.0];

const KICKER_SIZE: f32 = 20.0;
const KICKER_LETTER_SPACING: f32 = 3.0;
const NAME_SIZE: f32 = 54.0;
const PLACE_SIZE: f32 = 22.0;
const REASON_SIZE: f32 = 36.0;
const WHY_SIZE: f32 = 22.0;
const BUTTON_TEXT_SIZE: f32 = 22.0;
const BUTTON_HEIGHT: f32 = 58.0;
const MENU_WIDTH: f32 = 280.0;
const LOAD_WIDTH: f32 = 260.0;
const TRY_AGAIN_WIDTH: f32 = 300.0;

// The gaps from the top: after the kicker, the name, the place, the rule, the reason, the why, and
// above the buttons.
const GAPS: [f32; 7] = [12.0, 4.0, 16.0, 28.0, 8.0, 28.0, 36.0];

pub const RETURN_POPUP_LABEL: &str = "Return to Menu Popup";

// ----------------------------------------------
// State
// ----------------------------------------------

struct FailedState {
    built: BuiltOnOpen,
    // A preview's reason (the Screens menu's), in place of the game's.
    preview_reason: Option<FailureReason>,
}

static STATE: UiStaticVar<FailedState> = UiStaticVar::new(FailedState { built: BuiltOnOpen::new(), preview_reason: None });

// Sets the screen up for opening (dialog::open_dialog builds it afresh then).
pub(super) fn prepare() {
    STATE.as_mut().built.prepare();
}

// Whether the open screen must be built again: the window's size changed.
pub(super) fn needs_rebuild(context: &GameUiContext) -> bool {
    STATE.as_ref().built.needs_rebuild(context)
}

// The reason a preview shows (dialog::preview), until the next.
pub(super) fn set_preview_reason(reason: Option<FailureReason>) {
    STATE.as_mut().preview_reason = reason;
}

// Why the game was lost: a preview's reason, else the game's (Debt if, opened by automation, it
// isn't lost).
fn shown_reason(context: &GameUiContext) -> FailureReason {
    if campaign::preview::is_active()
        && let Some(reason) = STATE.as_ref().preview_reason
    {
        return reason;
    }
    context.sim.ratings().failure().unwrap_or(FailureReason::Debt)
}

// ----------------------------------------------
// MissionFailed
// ----------------------------------------------

pub struct MissionFailed {
    menu: UiMenuRcMut,
}

impl DialogMenuFactory for MissionFailed {
    const KIND: DialogMenuKind = DialogMenuKind::MissionFailed;
    const TITLE: &'static [&'static str] = &["Mission Failed"];

    fn create(context: &mut GameUiContext) -> DialogMenuImpl {
        DialogMenuImpl::from(MissionFailed::new(context))
    }
}

impl DialogMenu for MissionFailed {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> DialogMenuKind {
        Self::KIND
    }

    fn menu(&self) -> &UiMenuRcMut {
        &self.menu
    }

    // Escape (or any close) closes its popup first, and otherwise asks in it: the screen stays
    // until a button leaves the game. A preview's closes.
    fn close(&mut self, context: &mut GameUiContext) -> bool {
        let menu = self.menu_mut();
        if menu.is_message_box_open() {
            menu.close_message_box(context);
            return false;
        }
        if !campaign::preview::is_active() {
            open_return_popup(context, self.menu.clone());
            return false;
        }
        self.menu_mut().close(context);
        STATE.as_mut().built.active = false;
        true
    }

    fn reset(&mut self) {
        let menu = self.menu_mut();
        menu.reset_message_box();
        menu.set_flags(UiMenuFlags::IsOpen, false);
        STATE.as_mut().built.active = false;
    }
}

// What the panel shows, read when it's built.
struct Failure {
    kicker: String,
    name: Option<String>,
    place: Option<String>,
    reason: String,
    why: String,
    goals: Vec<GoalRow>,
    stats: Vec<(String, String)>,
}

impl Failure {
    fn read(context: &GameUiContext) -> Self {
        let reason = shown_reason(context);
        let realm = realm::ui_realm_for(false).realm;
        let rules = &GameConfigs::get().sim;
        let months = |count: u32| ui::text::format_count(UiTextCategory::Menu, "failed.months", i64::from(count), &[]);
        let configs = CampaignConfigs::get();
        let active = campaign::preview::shown_mission();
        let mission = active.as_ref().and_then(|active| configs.mission(active.campaign_id, active.mission_index));
        let why = match reason {
            FailureReason::Debt => ui::text::format(UiTextCategory::Menu, "failed.why.debt", &[(
                "months",
                &months(rules.finance.debt_limit_months),
            )]),
            FailureReason::Legitimacy => ui::text::format(UiTextCategory::Menu, "failed.why.legitimacy", &[
                ("rating", &ratings::rating_name(realm, RatingKind::Legitimacy)),
                ("below", &rules.ratings.recall_below.to_string()),
                ("months", &months(rules.ratings.recall_months)),
            ]),
            FailureReason::Time => {
                let limit = mission.and_then(|mission| mission.requirements.time_limit_months).unwrap_or_default();
                ui::text::format(UiTextCategory::Menu, "failed.why.time", &[("months", &months(limit))])
            }
        };
        let reason_text = ui::text::get(UiTextCategory::Realm, &format!("failure.{realm}.{reason}")).to_string();

        let campaign = active.as_ref().and_then(|active| configs.campaign(active.campaign_id));
        let (Some(active), Some(campaign), Some(mission)) = (active.as_ref(), campaign, mission) else {
            return Self {
                kicker: ui::text::get(UiTextCategory::Menu, "failed.kicker.plain").to_uppercase(),
                name: None,
                place: None,
                reason: reason_text,
                why,
                goals: Vec::new(),
                stats: mission_complete::stat_rows(context),
            };
        };
        let number = campaign.mission_number(active.mission_index).to_string();
        let kicker = ui::text::format(UiTextCategory::Menu, "failed.kicker", &[("number", &number)]).to_uppercase();
        let year = realm::date_text(realm, context.sim.calendar());
        let place = Some(match mission.city_display_name() {
            Some(city) => ui::text::format(UiTextCategory::Menu, "debrief.city_year", &[("city", city), ("year", &year)]),
            None => year,
        });
        Self {
            kicker,
            name: campaign.mission_display_name(active.mission_index).map(str::to_string),
            place,
            reason: reason_text,
            why,
            goals: mission_complete::goal_rows(&mission.requirements, context.world.stats(), realm),
            stats: mission_complete::stat_rows(context),
        }
    }
}

impl MissionFailed {
    pub fn new(context: &mut GameUiContext) -> Self {
        let kind = Self::KIND.to_string();
        let window = ui::screen_size(context.ui_sys);
        let scales = Scales::new(window);
        let flags = DialogFrame::from_skin(Self::KIND).menu_flags();
        let mut menu = UiMenu::new(kind)
            .with_flags(flags)
            .with_size(window)
            .with_content_insets(UiMenuInsets::same(0.0))
            .with_widget_spacing(Vec2::zero());

        // Built at boot and on every rebuild for every dialog kind, but shown only once prepared
        // for opening: until then an empty menu, so its textures load only when needed.
        if !STATE.as_ref().built.active {
            return Self { menu: menu.into_rc() };
        }

        let art = skin::intermission();
        if !art.backdrop.is_empty() {
            menu = menu.with_background(context, PathRef::from_str(&art.backdrop));
        }
        let failure = Failure::read(context);

        menu.add_widget(spacer(scales.origin.y));
        let mut columns = UiColumns::new().with_spacing(0.0);
        columns.add_column(UiColumnWidth::Fixed(scales.origin.x + scales.pt(PANEL_RECT[0])), UiWidgetGroup::new());
        let panel = left_group()
            .with_widget_spacing(Vec2::zero())
            .with_widget(spacer(scales.pt(PANEL_RECT[1])))
            .with_widget(panel(context, scales, &failure));
        columns.add_column(UiColumnWidth::Fixed(scales.pt(PANEL_RECT[2])), panel);
        columns.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());
        menu.add_widget(columns);

        STATE.as_mut().built.built(context);
        Self { menu: menu.into_rc() }
    }

    // The mission again from its start: its map loads, and its briefing opens.
    fn on_try_again(context: &mut GameUiContext) {
        if campaign::preview::is_active() {
            super::close(DialogMenuKind::MissionFailed, context);
            return;
        }
        if let Some(active) = campaign::active_mission() {
            super::home::start_campaign_mission(active.campaign_id, active.mission_index);
        }
    }

    // Load / Save in load mode, over the screen: Cancel comes back to it.
    fn on_load(context: &mut GameUiContext) {
        if campaign::preview::is_active() {
            super::close(DialogMenuKind::MissionFailed, context);
            return;
        }
        super::open(DialogMenuKind::LoadGame, false, context);
    }

    fn on_return_to_menu(context: &mut GameUiContext) {
        if campaign::preview::is_active() {
            super::close(DialogMenuKind::MissionFailed, context);
            return;
        }
        GameLoop::get_mut().quit_to_main_menu();
    }
}

// Escape's popup: Cancel, or Return to the menu.
fn open_return_popup(context: &mut GameUiContext, mut menu: UiMenuRcMut) {
    if menu.is_message_box_open() {
        return;
    }
    menu.reset_message_box();
    let menu_weak_ref = menu.downgrade();
    menu.open_message_box(context, move |context| {
        let window = ui::screen_size(context.ui_sys());
        let sized = |button: UiTextButton| skin::size_popup_button(button, window);
        let body = [(UiString::key(UiTextCategory::Menu, "failed.popup.body"), "ink")];
        skin::popup_with_colored_body(
            context,
            RETURN_POPUP_LABEL,
            [UiString::key(UiTextCategory::Menu, "failed.popup.title")],
            body,
        )
        .with_buttons([
            sized(make_text_button(
                context,
                ButtonRole::Secondary,
                UiString::key(UiTextCategory::Menu, "button.cancel"),
                move |_, context| {
                    if let Some(mut menu) = menu_weak_ref.upgrade() {
                        menu.close_message_box(context);
                    }
                },
            )),
            sized(make_text_button(
                context,
                ButtonRole::Primary,
                UiString::key(UiTextCategory::Menu, "failed.return_to_menu"),
                |_, _| GameLoop::get_mut().quit_to_main_menu(),
            )),
        ])
    });
}

// ----------------------------------------------
// The panel
// ----------------------------------------------

fn panel(context: &mut GameUiContext, scales: Scales, failure: &Failure) -> UiFrame {
    let size = Vec2::new(scales.pt(PANEL_RECT[2]), scales.pt(PANEL_RECT[3]));
    let (panel, padding) = super::screen::intermission_panel(context, scales, PANEL_PADDING);
    let inner = Vec2::new(size.x - padding[0] - padding[2], size.y - padding[1] - padding[3]);

    let buttons = buttons(context, scales, inner.x);
    let buttons_height = buttons.measure(context).y;

    // Closer together where the realm's font's tall lines would push the buttons off the panel.
    let room_for_parts = inner.y - buttons_height - scales.pt(6.0);
    let (mut top, mut bottom) = (Vec::new(), Vec::new());
    let mut parts_height = 0.0;
    for tightness in [1.0, 0.6, 0.3, 0.0] {
        (top, bottom) = panel_parts(context, scales, failure, inner.x, tightness);
        parts_height = top.iter().chain(&bottom).map(|part| part.measure(context).y).sum();
        if parts_height <= room_for_parts {
            break;
        }
    }
    let room = (room_for_parts - parts_height).floor();

    let mut group = left_group().with_widget_spacing(Vec2::zero());
    for part in top {
        group.add_widget(part);
    }
    if room > 0.0 {
        group.add_widget(spacer(room));
    }
    for part in bottom {
        group.add_widget(part);
    }
    group.add_widget(buttons);

    let mut frame = UiFrame::new(group, size).with_label("panel").with_padding(padding);
    if let Some(panel) = panel {
        frame = frame.with_panel(panel);
    }
    frame
}

// The heading, the reason and the goals (top), and the stats (bottom, kept to the buttons).
fn panel_parts(
    context: &mut GameUiContext,
    scales: Scales,
    failure: &Failure,
    width: f32,
    tightness: f32,
) -> (Vec<UiWidgetImpl>, Vec<UiWidgetImpl>) {
    let color = skin::intermission_color;
    let sub = color("sub", "sub");
    let gap = |index: usize| spacer(scales.pt(GAPS[index]) * tightness);
    let mut parts: Vec<UiWidgetImpl> = Vec::new();

    let kicker_font = scales.heading_font(KICKER_SIZE);
    let kicker = text_line(&failure.kicker, "kicker", kicker_font, color("accent", "accent"), width, UiTextAlign::Left)
        .with_heading_font()
        .with_letter_spacing(scales.pt(KICKER_LETTER_SPACING));
    parts.push(tight(context, kicker, kicker_font, KICKER_SIZE, scales).into());

    if let Some(name) = &failure.name {
        parts.push(gap(0).into());
        let font = scales.heading_font(NAME_SIZE);
        let name = text_line(name, "name", font, color("heading", "title"), width, UiTextAlign::Left).with_heading_font();
        parts.push(tight(context, name, font, NAME_SIZE, scales).into());
    }
    if let Some(place) = &failure.place {
        parts.push(gap(1).into());
        let font = scales.font(PLACE_SIZE);
        let place = UiRichText::from_spans([UiTextSpan::emphasized(place.clone())])
            .with_label("place")
            .with_font_scale(font)
            .with_width(width)
            .with_text_color(sub);
        parts.push(tight(context, place, font, PLACE_SIZE, scales).into());
    }
    parts.push(gap(2).into());
    parts.push(skin::separator(context, "rule").into());

    parts.push(gap(3).into());
    let font = scales.heading_font(REASON_SIZE);
    let reason =
        text_line(&failure.reason, "reason", font, color("accent", "accent"), width, UiTextAlign::Left).with_heading_font();
    parts.push(tight(context, reason, font, REASON_SIZE, scales).into());
    parts.push(gap(4).into());
    let font = scales.font(WHY_SIZE);
    parts.push(text_line(&failure.why, "why", font, color("text", "ink"), width, UiTextAlign::Left).into());

    if !failure.goals.is_empty() {
        parts.push(gap(5).into());
        parts.push(mission_complete::goals_table(context, scales, &failure.goals, width, tightness).into());
    }

    let mut bottom: Vec<UiWidgetImpl> = Vec::new();
    if !failure.stats.is_empty() {
        bottom.push(mission_complete::stats_values(scales, &failure.stats, width, tightness).into());
    }
    bottom.push(gap(6).into());
    (parts, bottom)
}

// Return to the menu and Load a save at the left, Try again at the right.
fn buttons(context: &mut GameUiContext, scales: Scales, width: f32) -> UiColumns {
    let sized = |button: UiTextButton, points: f32| {
        scales.button_text(
            button.with_width(UiTextButtonWidth::Fixed(scales.pt(points))).with_min_height(scales.pt(BUTTON_HEIGHT)),
            BUTTON_TEXT_SIZE,
        )
    };
    let menu = skin::button(
        context,
        ButtonRole::Secondary,
        UiString::key(UiTextCategory::Menu, "failed.return_to_menu"),
        |_, context| MissionFailed::on_return_to_menu(context_as_mut::<GameUiContext>(context)),
    );
    let load =
        skin::button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "failed.load"), |_, context| {
            MissionFailed::on_load(context_as_mut::<GameUiContext>(context))
        });
    let try_again =
        skin::button(context, ButtonRole::Primary, UiString::key(UiTextCategory::Menu, "failed.try_again"), |_, context| {
            MissionFailed::on_try_again(context_as_mut::<GameUiContext>(context))
        });
    let mut columns = UiColumns::new().with_spacing(0.0).with_width(width);
    columns.add_column(UiColumnWidth::Fixed(scales.pt(MENU_WIDTH)), left_group().with_widget(sized(menu, MENU_WIDTH)));
    columns.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());
    columns.add_column(UiColumnWidth::Fixed(scales.pt(LOAD_WIDTH)), left_group().with_widget(sized(load, LOAD_WIDTH)));
    columns.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());
    columns.add_column(
        UiColumnWidth::Fixed(scales.pt(TRY_AGAIN_WIDTH)),
        left_group().with_widget(sized(try_again, TRY_AGAIN_WIDTH)),
    );
    columns
}
