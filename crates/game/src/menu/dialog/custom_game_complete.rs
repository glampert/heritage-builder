// A custom game's short debrief (docs/plans/ratings.md, §7, Phase 5; no mockup): what opens when its
// goal set's goals are met (custom_game::tick, campaign's CustomGameComplete prompt). The failure
// screen's layout: a full-screen dialog on the skin's intermission backdrop, its panel in the middle,
// no stele and no world map:
// - "GOALS MET", the goal set's name, the map and the year in the realm's era;
// - the goals (the debrief's table), the four ratings as bars and the stats (population, treasury,
//   years taken);
// - Return to the menu and Keep playing, which closes it (so does Escape): the game goes on, its
//   goals met for good (CustomGameRecord::completed), so it doesn't open again.
//
// Nothing is recorded in the profile. It's built afresh each time it opens, and again when the
// window's size changes. Opened in another game (automation's dialog.open), it shows "GOALS MET"
// and the city's ratings and stats only.
//
// Paths: CustomGameComplete/panel/kicker, name, place, goals (kind table: Goals, Target, Achieved, a
// status per row), ratings_title and ratings (kind canvas: names, values), stats (kind key_values),
// Return to the menu, Keep playing.

use super::{
    mission_complete::{self, GoalRow},
    screen::{BuiltOnOpen, Scales, left_group, spacer, text_line, tight},
    *,
};
use crate::{GameLoop, campaign, custom_game, realm};

// ----------------------------------------------
// Layout (points on the generator's 1920 × 1080 screen, scaled by the window: screen::Scales)
// ----------------------------------------------

// The failure screen's panel, in the middle of the screen.
const PANEL_RECT: [f32; 4] = [450.0, 60.0, 1020.0, 960.0];
const PANEL_PADDING: [f32; 4] = [56.0, 56.0, 56.0, 40.0];

const KICKER_SIZE: f32 = 20.0;
const KICKER_LETTER_SPACING: f32 = 3.0;
const NAME_SIZE: f32 = 54.0;
const PLACE_SIZE: f32 = 22.0;
const BUTTON_TEXT_SIZE: f32 = 22.0;
const BUTTON_HEIGHT: f32 = 58.0;
const MENU_WIDTH: f32 = 280.0;
const KEEP_WIDTH: f32 = 300.0;

// The gaps from the top: after the kicker, the name, the place and the rule; then under the ratings
// and above the buttons.
const GAPS: [f32; 6] = [12.0, 4.0, 16.0, 24.0, 20.0, 36.0];

// ----------------------------------------------
// State
// ----------------------------------------------

static STATE: UiStaticVar<BuiltOnOpen> = UiStaticVar::new(BuiltOnOpen::new());

// Sets the screen up for opening (dialog::open_dialog builds it afresh then).
pub(super) fn prepare() {
    STATE.as_mut().prepare();
}

// Whether the open screen must be built again: the window's size changed.
pub(super) fn needs_rebuild(context: &GameUiContext) -> bool {
    STATE.as_ref().needs_rebuild(context)
}

// A custom game's heading, as its debrief and failure screen show it: its goal set's name and the
// line under it, its map and the year in `realm`'s era; for a Sandbox, its map's name and the year.
// None outside a custom game.
pub(super) fn heading(context: &GameUiContext, realm: realm::Realm) -> Option<(String, String)> {
    let game = custom_game::active()?;
    let map = game.record.map.as_deref().map(super::load_save::custom_map_display_name);
    let year = realm::date_text(realm, context.sim.calendar());
    Some(match (game.goal_set(), map) {
        (Some(set), Some(map)) => (
            set.display_name().to_string(),
            ui::text::format(UiTextCategory::Menu, "debrief.city_year", &[("city", &map), ("year", &year)]),
        ),
        (Some(set), None) => (set.display_name().to_string(), year),
        (None, map) => (map.unwrap_or_default(), year),
    })
}

// ----------------------------------------------
// CustomGameComplete
// ----------------------------------------------

pub struct CustomGameComplete {
    menu: UiMenuRcMut,
}

impl DialogMenuFactory for CustomGameComplete {
    const KIND: DialogMenuKind = DialogMenuKind::CustomGameComplete;
    const TITLE: &'static [&'static str] = &["Goals Met"];

    fn create(context: &mut GameUiContext) -> DialogMenuImpl {
        DialogMenuImpl::from(CustomGameComplete::new(context))
    }
}

impl DialogMenu for CustomGameComplete {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> DialogMenuKind {
        Self::KIND
    }

    fn menu(&self) -> &UiMenuRcMut {
        &self.menu
    }

    // Escape (or any close) keeps playing.
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

// What the panel shows, read when it's built.
struct Summary {
    kicker: String,
    name: Option<String>,
    place: Option<String>,
    goals: Vec<GoalRow>,
    ratings: Vec<(String, u32)>, // Each rating's name in the UI realm's words, and its value.
    stats: Vec<(String, String)>,
}

impl Summary {
    fn read(context: &GameUiContext) -> Self {
        let realm = realm::ui_realm_for(false).realm;
        let (name, place) = heading(context, realm).unzip();
        let goals = custom_game::active_requirements()
            .map(|requirements| mission_complete::goal_rows(requirements, context.world.stats(), realm))
            .unwrap_or_default();
        let ratings = crate::ratings::views(context.sim.ratings(), realm)
            .map(|views| views.into_iter().map(|view| (view.name, view.value)).collect())
            .unwrap_or_default();
        Self {
            kicker: ui::text::get(UiTextCategory::Menu, "custom_complete.kicker").to_uppercase(),
            name,
            place,
            goals,
            ratings,
            stats: mission_complete::stat_rows(context),
        }
    }
}

impl CustomGameComplete {
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
        if !STATE.as_ref().active {
            return Self { menu: menu.into_rc() };
        }

        let art = skin::intermission();
        if !art.backdrop.is_empty() {
            menu = menu.with_background(context, PathRef::from_str(&art.backdrop));
        }
        let summary = Summary::read(context);

        menu.add_widget(spacer(scales.origin.y));
        let mut columns = UiColumns::new().with_spacing(0.0);
        columns.add_column(UiColumnWidth::Fixed(scales.origin.x + scales.pt(PANEL_RECT[0])), UiWidgetGroup::new());
        let size = Vec2::new(scales.pt(PANEL_RECT[2]), scales.pt(PANEL_RECT[3]));
        let panel = super::screen::fitted_panel(
            context,
            scales,
            size,
            PANEL_PADDING,
            |context, width| buttons(context, scales, width),
            |context, width, tightness| panel_parts(context, scales, &summary, width, tightness),
        );
        let panel =
            left_group().with_widget_spacing(Vec2::zero()).with_widget(spacer(scales.pt(PANEL_RECT[1]))).with_widget(panel);
        columns.add_column(UiColumnWidth::Fixed(scales.pt(PANEL_RECT[2])), panel);
        columns.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());
        menu.add_widget(columns);

        STATE.as_mut().built(context);
        Self { menu: menu.into_rc() }
    }

    // The game goes on, its goals met: nothing opens the debrief again.
    fn on_keep_playing(context: &mut GameUiContext) {
        campaign::continue_playing();
        super::close(DialogMenuKind::CustomGameComplete, context);
    }

    fn on_return_to_menu() {
        GameLoop::get_mut().quit_to_main_menu();
    }
}

// ----------------------------------------------
// The panel
// ----------------------------------------------

// The heading and the goals (top), and the ratings and stats (bottom, kept to the buttons).
fn panel_parts(
    context: &mut GameUiContext,
    scales: Scales,
    summary: &Summary,
    width: f32,
    tightness: f32,
) -> (Vec<UiWidgetImpl>, Vec<UiWidgetImpl>) {
    let color = skin::intermission_color;
    let gap = |index: usize| spacer(scales.pt(GAPS[index]) * tightness);
    let mut parts: Vec<UiWidgetImpl> = Vec::new();

    let kicker_font = scales.heading_font(KICKER_SIZE);
    let kicker = text_line(&summary.kicker, "kicker", kicker_font, color("accent", "accent"), width, UiTextAlign::Left)
        .with_heading_font()
        .with_letter_spacing(scales.pt(KICKER_LETTER_SPACING));
    parts.push(tight(context, kicker, kicker_font, KICKER_SIZE, scales).into());

    if let Some(name) = &summary.name {
        parts.push(gap(0).into());
        let font = scales.heading_font(NAME_SIZE);
        let name = text_line(name, "name", font, color("heading", "title"), width, UiTextAlign::Left).with_heading_font();
        parts.push(tight(context, name, font, NAME_SIZE, scales).into());
    }
    if let Some(place) = &summary.place {
        parts.push(gap(1).into());
        let font = scales.font(PLACE_SIZE);
        let place = UiRichText::from_spans([UiTextSpan::emphasized(place.clone())])
            .with_label("place")
            .with_font_scale(font)
            .with_width(width)
            .with_text_color(color("sub", "sub"));
        parts.push(tight(context, place, font, PLACE_SIZE, scales).into());
    }
    parts.push(gap(2).into());
    parts.push(skin::separator(context, "rule").into());

    if !summary.goals.is_empty() {
        parts.push(gap(3).into());
        parts.push(mission_complete::goals_table(context, scales, &summary.goals, width, tightness).into());
    }

    let mut bottom: Vec<UiWidgetImpl> = Vec::new();
    if !summary.ratings.is_empty() {
        bottom.push(mission_complete::section_title(context, scales, "ratings", "debrief.ratings", None, width).into());
        bottom.push(mission_complete::ratings_bars(scales, &summary.ratings, width, tightness).into());
        bottom.push(gap(4).into());
    }
    if !summary.stats.is_empty() {
        bottom.push(mission_complete::stats_values(scales, &summary.stats, width, tightness).into());
    }
    bottom.push(gap(5).into());
    (parts, bottom)
}

// Return to the menu at the left, Keep playing at the right.
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
        |_, _| CustomGameComplete::on_return_to_menu(),
    );
    let keep = skin::button(
        context,
        ButtonRole::Primary,
        UiString::key(UiTextCategory::Menu, "debrief.keep_playing"),
        |_, context| CustomGameComplete::on_keep_playing(context_as_mut::<GameUiContext>(context)),
    );
    let mut columns = UiColumns::new().with_spacing(0.0).with_width(width);
    columns.add_column(UiColumnWidth::Fixed(scales.pt(MENU_WIDTH)), left_group().with_widget(sized(menu, MENU_WIDTH)));
    columns.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());
    columns.add_column(UiColumnWidth::Fixed(scales.pt(KEEP_WIDTH)), left_group().with_widget(sized(keep, KEEP_WIDTH)));
    columns
}
