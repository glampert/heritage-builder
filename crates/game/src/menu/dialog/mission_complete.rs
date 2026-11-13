// The debrief after a mission (docs/plans/realm-ui.md, Phase 3; intermission-<realm>.jpg): a
// full-screen dialog laid out as the mockup generator's screens.py intermission() draws it, on the
// skin's intermission art (skin::intermission):
// - left, the campaign's stele (crate::campaign::stele, drawn by menu::stele::SteleArt): its title,
//   a carved line per finished mission, the one just finished freshly carved, and an empty row for
//   each mission to come;
// - right, on the skin's big panel: "MISSION N COMPLETE", the mission's name, its city and the
//   year in the realm's era; the goals (target and achieved, a mark per goal) from its
//   requirements and the city's stats; the stats (population, treasury, years taken); the note
//   that its city joins the world map; and Keep playing and Continue.
//
// Keep playing closes it and the game goes on (the mission stays finished). Continue goes on with
// the campaign: to the world map, which offers the next mission (the A/B sites of a choice, or
// "Next: <mission>" and Begin), when it shows this campaign; to the next mission straight away
// when it doesn't (no world map, or another realm's look under a ui.realm override); and after the
// campaign's last mission, to the campaign-complete screen (CampaignComplete).
//
// It's built afresh each time it opens (prepare), from the mission being played
// (campaign::active_mission) and the world's stats, and again when the window's size changes (its
// layout follows the window, screen::Scales) or the skin or language does (dialog::rebuild_all).
// Opened without a mission (automation's dialog.open in another game), it shows "Mission complete"
// and the buttons only.
//
// Paths: MissionComplete/stele (kind canvas: title, lines, newest, rows_to_come);
// MissionComplete/panel/kicker, name, place, goals (kind table: Goals, Target, Achieved, a status
// per row), stats (kind key_values), note/joins, Keep playing, and the Continue button (Continue to
// the world map, Continue to the next mission, or Continue).

use super::*;
use super::screen::{Scales, left_group, spacer, text_line, tight, tight_row};
use crate::{
    campaign::{
        self,
        config::{CampaignConfigs, MissionGoal},
        stele::{self as stele_data, Stele},
    },
    menu::{skin::SkinPanel, stele::SteleArt},
    profile,
    realm,
    world_map::{self as world_map_data, WorldMapProvider},
};

// ----------------------------------------------
// Layout (points on the generator's 1920 × 1080 screen, scaled by the window: screen::Scales)
// ----------------------------------------------

const STELE_WIDTH: f32 = 860.0; // The left column: the stele on its niche.
const PANEL_RECT: [f32; 4] = [860.0, 60.0, 1020.0, 960.0];
const PANEL_PADDING: [f32; 4] = [56.0, 56.0, 56.0, 40.0];

// Text sizes (the generator's font sizes).
const KICKER_SIZE: f32 = 20.0;
const NAME_SIZE: f32 = 54.0;
const PLACE_SIZE: f32 = 22.0;
const HEADER_SIZE: f32 = 16.0;
const HEADER_LETTER_SPACING: f32 = 2.0; // The mockup's ls=2.
const GOAL_SIZE: f32 = 23.0;
const STAT_LABEL_SIZE: f32 = 19.0;
const STAT_VALUE_SIZE: f32 = 32.0;
const NOTE_SIZE: f32 = 22.0;
const BUTTON_TEXT_SIZE: f32 = 22.0;

// The goals: a row per goal 46 points apart, the target's column ending 204 points left of the
// achieved one's right edge.
const GOAL_ROW: f32 = 46.0;
const TARGET_WIDTH: f32 = 196.0;
const ACHIEVED_WIDTH: f32 = 196.0;
const GOAL_COLUMN_GAP: f32 = 8.0;
const KICKER_LETTER_SPACING: f32 = 3.0;
const STAT_GAP: f32 = 6.0;
const NOTE_HEIGHT: f32 = 64.0;
const NOTE_ICON: [f32; 2] = [36.0, 68.0]; // The map icon's centre and the text's left, in from the box's left.
const BUTTON_HEIGHT: f32 = 58.0;
const KEEP_WIDTH: f32 = 300.0;
const CONTINUE_WIDTH: f32 = 420.0;

// The gaps between the panel's parts as the mockup spaces them (tightness 1), from the top: after
// the kicker, the name, the place and the rule; and from the bottom, between the stats and the
// note and above the buttons. The room left over goes between the goals and the stats, where M9's
// distinctions and ratings will go.
const GAPS: [f32; 6] = [12.0, 4.0, 16.0, 24.0, 26.0, 36.0];

// ----------------------------------------------
// State
// ----------------------------------------------

// Kept outside the widgets: whether it's prepared for opening (only then is it built in full: its
// textures load when needed), and the window it was built for.
struct DebriefState {
    active: bool,
    built_window: Option<(u32, u32)>,
}

static STATE: UiStaticVar<DebriefState> = UiStaticVar::new(DebriefState { active: false, built_window: None });

// Sets the debrief up for opening (dialog::open_dialog builds it afresh then).
pub(super) fn prepare() {
    let state = STATE.as_mut();
    state.active = true;
    state.built_window = None;
}

// Whether the open debrief must be built again: the window's size changed.
pub(super) fn needs_rebuild(context: &GameUiContext) -> bool {
    STATE.as_ref().built_window != Some(window_key(context))
}

fn window_key(context: &GameUiContext) -> (u32, u32) {
    let window = ui::screen_size(context.ui_sys);
    (window.x.round() as u32, window.y.round() as u32)
}

// Where Continue goes.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum ContinueTo {
    WorldMap,         // The world map shows the campaign and offers its next mission.
    NextMission,      // No world map to show it: the next mission starts.
    CampaignComplete, // The campaign's last mission: the campaign-complete screen.
    Close,            // No mission being played.
}

fn continue_to() -> ContinueTo {
    let Some(active) = campaign::active_mission() else {
        return ContinueTo::Close;
    };
    let Some(campaign) = CampaignConfigs::get().campaign(active.campaign_id) else {
        return ContinueTo::Close;
    };
    if campaign.is_last_step(active.mission_index) {
        return ContinueTo::CampaignComplete;
    }
    let map_offers_next = super::world_map::shows_campaign_in_game(active.campaign_id)
        && world_map_data::provider(active.campaign_id).is_some_and(|provider| provider.next_mission().is_some());
    if map_offers_next { ContinueTo::WorldMap } else { ContinueTo::NextMission }
}

// ----------------------------------------------
// MissionComplete
// ----------------------------------------------

pub struct MissionComplete {
    menu: UiMenuRcMut,
}

impl DialogMenuFactory for MissionComplete {
    const KIND: DialogMenuKind = DialogMenuKind::MissionComplete;
    const TITLE: &'static [&'static str] = &["Mission Complete"];

    fn create(context: &mut GameUiContext) -> DialogMenuImpl {
        DialogMenuImpl::from(MissionComplete::new(context))
    }
}

impl DialogMenu for MissionComplete {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> DialogMenuKind {
        Self::KIND
    }

    fn menu(&self) -> &UiMenuRcMut {
        &self.menu
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

// What the panel shows, read when it's built.
struct Debrief {
    kicker: String,
    name: Option<String>,
    place: Option<String>,
    goals: Vec<(String, String, String, bool)>, // Goal, target, achieved, met.
    stats: Vec<(String, String)>,
    joins: Option<String>,
    stele: Option<Stele>,
    continue_label: UiString,
}

impl Debrief {
    fn read(context: &GameUiContext) -> Self {
        let continue_label = match continue_to() {
            ContinueTo::WorldMap => UiString::key(UiTextCategory::Menu, "debrief.continue.world_map"),
            ContinueTo::NextMission => UiString::key(UiTextCategory::Menu, "debrief.continue.next_mission"),
            ContinueTo::CampaignComplete | ContinueTo::Close => UiString::key(UiTextCategory::Menu, "debrief.continue.campaign_complete"),
        };
        let plain = || Self {
            kicker: ui::text::get(UiTextCategory::Menu, "debrief.mission_complete.plain").to_uppercase(),
            name: None,
            place: None,
            goals: Vec::new(),
            stats: Vec::new(),
            joins: None,
            stele: None,
            continue_label: continue_label.clone(),
        };
        let configs = CampaignConfigs::get();
        let Some(active) = campaign::active_mission() else {
            return plain();
        };
        let (Some(campaign), Some(mission)) = (configs.campaign(active.campaign_id), configs.mission(active.campaign_id, active.mission_index)) else {
            return plain();
        };

        let number = campaign.mission_number(active.mission_index).to_string();
        let kicker = ui::text::format(UiTextCategory::Menu, "debrief.mission_complete", &[("number", &number)]).to_uppercase();
        let name = campaign.mission_display_name(active.mission_index).map(str::to_string);
        let year = realm::date_text(realm::ui_realm_for(false).realm, context.sim.calendar());
        let city = mission.city_display_name();
        let place = Some(match city {
            Some(city) => ui::text::format(UiTextCategory::Menu, "debrief.city_year", &[("city", city), ("year", &year)]),
            None => year,
        });

        let stats = context.world.stats();
        let digits = super::load_save::group_digits;
        let goals = mission
            .requirements
            .goals
            .iter()
            .map(|goal| {
                let (text, target, achieved) = match goal {
                    MissionGoal::Population { min } => {
                        (ui::text::get(UiTextCategory::Menu, "debrief.goal.population").to_string(), i64::from(*min), i64::from(stats.population.total))
                    }
                    MissionGoal::Employment { min_employed } => (
                        ui::text::get(UiTextCategory::Menu, "debrief.goal.employment").to_string(),
                        i64::from(*min_employed),
                        i64::from(stats.population.employed),
                    ),
                    MissionGoal::Treasury { min_gold } => {
                        (ui::text::get(UiTextCategory::Menu, "debrief.goal.treasury").to_string(), i64::from(*min_gold), stats.treasury.gold_units_total)
                    }
                    MissionGoal::Resource { kind, min } => (
                        ui::text::format(UiTextCategory::Menu, "debrief.goal.resource", &[("good", &kind.display_name())]),
                        i64::from(*min),
                        i64::from(stats.resources.all.count(*kind)),
                    ),
                };
                (text, digits(target), digits(achieved), goal.is_met(stats))
            })
            .collect();

        // The years the city was governed: its first year counts as one.
        let calendar = context.sim.calendar();
        let years = calendar.months_elapsed() / 12 + 1;
        let stats = vec![
            (ui::text::get(UiTextCategory::Menu, "debrief.stat.population").to_string(), digits(i64::from(stats.population.total))),
            (ui::text::get(UiTextCategory::Menu, "debrief.stat.treasury").to_string(), digits(context.sim.finance().balance())),
            (ui::text::get(UiTextCategory::Menu, "debrief.stat.years").to_string(), years.to_string()),
        ];
        let joins = city.map(|city| ui::text::format(UiTextCategory::Menu, "debrief.joins", &[("city", city)]));

        // The profile records the mission at the start of the next frame (GameLoop takes the
        // finished missions then), so the stele is carved from a copy that has it already.
        let mut profile = profile::get().clone();
        profile.mission_finished(configs, active.campaign_id, active.mission_index);
        let stele = stele_data::stele(configs, &profile, active.campaign_id, Some(active.mission_index));

        Self { kicker, name, place, goals, stats, joins, stele, continue_label }
    }
}

impl MissionComplete {
    pub fn new(context: &mut GameUiContext) -> Self {
        let kind = Self::KIND.to_string();
        let window = ui::screen_size(context.ui_sys);
        let scales = Scales::new(window);
        let flags = DialogFrame::from_skin(Self::KIND).menu_flags();
        let mut menu = UiMenu::new(kind).with_flags(flags).with_size(window).with_content_insets(UiMenuInsets::same(0.0)).with_widget_spacing(Vec2::zero());

        // Built at boot and on every rebuild for every dialog kind, but shown only once prepared
        // for opening: until then an empty menu, so its textures load only when needed.
        if !STATE.as_ref().active {
            return Self { menu: menu.into_rc() };
        }

        let art = skin::intermission();
        if !art.backdrop.is_empty() {
            menu = menu.with_background(context, PathRef::from_str(&art.backdrop));
        }
        let debrief = Debrief::read(context);

        menu.add_widget(spacer(scales.origin.y));
        let mut columns = UiColumns::new().with_spacing(0.0);
        columns.add_column(UiColumnWidth::Fixed(scales.origin.x), UiWidgetGroup::new());
        columns.add_column(UiColumnWidth::Fixed(scales.pt(STELE_WIDTH)), left_group().with_widget(stele_canvas(context, scales, debrief.stele.clone())));
        let panel = left_group().with_widget_spacing(Vec2::zero()).with_widget(spacer(scales.pt(PANEL_RECT[1]))).with_widget(panel(context, scales, &debrief));
        columns.add_column(UiColumnWidth::Fixed(scales.pt(PANEL_RECT[2])), panel);
        columns.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());
        menu.add_widget(columns);

        STATE.as_mut().built_window = Some(window_key(context));
        Self { menu: menu.into_rc() }
    }

    fn on_keep_playing(context: &mut GameUiContext) {
        campaign::continue_playing();
        super::close(DialogMenuKind::MissionComplete, context);
    }

    fn on_continue(context: &mut GameUiContext) {
        // TODO(realm-ui): Phase 4's story sequence after this mission plays here, before the world
        // map, the next mission or the campaign-complete screen (story-script.md: I-B after mission 5).
        match continue_to() {
            // The world map offers the next mission: its choice box's sites, or "Begin the
            // mission". The mission just finished stays the one being played until then.
            ContinueTo::WorldMap => {
                super::open(DialogMenuKind::WorldMap, true, context);
            }
            ContinueTo::NextMission => match campaign::advance_to_next_mission() {
                // Loading the next mission's map tears down and rebuilds the session, which
                // resets the dialog stack (InGameMenus::drop).
                Some(map) => super::load_mission_map(&map),
                None => {
                    super::close(DialogMenuKind::MissionComplete, context);
                }
            },
            // No next mission: advance_to_next_mission queues the CampaignComplete prompt, which
            // the in-game menus open once this closes.
            ContinueTo::CampaignComplete => {
                campaign::advance_to_next_mission();
                super::close(DialogMenuKind::MissionComplete, context);
            }
            ContinueTo::Close => {
                super::close(DialogMenuKind::MissionComplete, context);
            }
        }
    }
}

// ----------------------------------------------
// The stele, left
// ----------------------------------------------

// The niche and the stele with its carved text, over the left column, the composition's top left
// at the canvas's.
fn stele_canvas(context: &mut GameUiContext, scales: Scales, stele: Option<Stele>) -> UiCanvas {
    let art = SteleArt::load(context);
    let value = stele.as_ref().map(|stele| {
        serde_json::json!({
            "title": stele.title,
            "lines": stele.lines.iter().map(|line| line.text.clone()).collect::<Vec<_>>(),
            "newest": stele.lines.iter().find(|line| line.newest).map(|line| line.text.clone()),
            "rows_to_come": stele.rows_to_come,
        })
    });
    let scale = scales.size;
    UiCanvas::new(Vec2::new(scales.pt(STELE_WIDTH), scales.pt(super::screen::REFERENCE.y)), move |_, painter, rect| {
        art.draw_niche(painter, rect.min, scale);
        // Opened with no mission (automation): the slab, uncarved.
        match &stele {
            Some(stele) => art.draw_stele(painter, stele, rect.min, scale),
            None => art.draw_slab(painter, rect.min, scale),
        }
    })
    .with_label("stele", value)
}

// ----------------------------------------------
// The panel, right
// ----------------------------------------------

fn panel(context: &mut GameUiContext, scales: Scales, debrief: &Debrief) -> UiFrame {
    let size = Vec2::new(scales.pt(PANEL_RECT[2]), scales.pt(PANEL_RECT[3]));
    let padding = PANEL_PADDING.map(|side| scales.pt(side));
    let inner = Vec2::new(size.x - padding[0] - padding[2], size.y - padding[1] - padding[3]);

    let buttons = buttons(context, scales, debrief, inner.x);
    let buttons_height = buttons.measure(context).y;

    // The parts above the buttons as the mockup spaces them; closer together where the realm's
    // font's tall lines (Tampadesa's) would push the buttons off the panel.
    let room_for_parts = inner.y - buttons_height - scales.pt(6.0);
    let (mut top, mut bottom) = (Vec::new(), Vec::new());
    let mut parts_height = 0.0;
    for tightness in [1.0, 0.6, 0.3, 0.0] {
        (top, bottom) = panel_parts(context, scales, debrief, inner.x, tightness);
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
    if let SkinPanel::NineSlice { slice, .. } = skin::panel(context, &skin::intermission().panel) {
        frame = frame.with_panel(slice);
    }
    frame
}

// The panel's parts above its buttons, `width` points wide, their gaps at `tightness` (1: as the
// mockup spaces them; 0: closest): the top ones (the heading, the goals) and the bottom ones (the
// stats, the note), which keep to the buttons.
fn panel_parts(context: &mut GameUiContext, scales: Scales, debrief: &Debrief, width: f32, tightness: f32) -> (Vec<UiWidgetImpl>, Vec<UiWidgetImpl>) {
    let color = skin::intermission_color;
    let text = color("text", "ink");
    let sub = color("sub", "sub");
    let gap = |index: usize| spacer(scales.pt(GAPS[index]) * tightness);
    let mut parts: Vec<UiWidgetImpl> = Vec::new();

    let kicker_font = scales.heading_font(KICKER_SIZE);
    let kicker = text_line(&debrief.kicker, "kicker", kicker_font, color("accent", "accent"), width, UiTextAlign::Left)
        .with_heading_font()
        .with_letter_spacing(scales.pt(KICKER_LETTER_SPACING));
    parts.push(tight(context, kicker, kicker_font, KICKER_SIZE, scales).into());

    if let Some(name) = &debrief.name {
        parts.push(gap(0).into());
        let font = scales.heading_font(NAME_SIZE);
        let name = text_line(name, "name", font, color("heading", "title"), width, UiTextAlign::Left).with_heading_font();
        parts.push(tight(context, name, font, NAME_SIZE, scales).into());
    }
    if let Some(place) = &debrief.place {
        parts.push(gap(1).into());
        let font = scales.font(PLACE_SIZE);
        // In italics, as the mockup sets it (egui slants the realm font).
        let place = UiRichText::from_spans([UiTextSpan::emphasized(place.clone())])
            .with_label("place")
            .with_font_scale(font)
            .with_width(width)
            .with_text_color(sub);
        parts.push(tight(context, place, font, PLACE_SIZE, scales).into());
    }
    parts.push(gap(2).into());
    parts.push(skin::separator(context, "rule").into());

    if !debrief.goals.is_empty() {
        parts.push(gap(3).into());
        parts.push(goals_table(context, scales, debrief, width, tightness).into());
    }

    // TODO(realm-ui): M9's distinctions (each with its mark, "each one adds a line to the stele")
    // and the four ratings' meters go here, between the goals and the stats, as the mockup shows
    // them, once M9 brings them: the room left over is theirs (panel).

    let mut bottom: Vec<UiWidgetImpl> = Vec::new();
    if !debrief.stats.is_empty() {
        let mut look = skin::row_look();
        look.label_color = Some(sub);
        look.value_color = Some(text);
        look.rule_color = None;
        look.padding = 0.0;
        let mut stats = skin::key_values()
            .with_label("stats")
            .with_look(look)
            .with_width(width)
            .with_font_scale(scales.font(STAT_LABEL_SIZE))
            .with_columns(UiKeyValueColumns {
                value_font_scale: scales.heading_font(STAT_VALUE_SIZE),
                heading_font: true,
                gap: scales.pt(STAT_GAP) * tightness,
                // The mockup's rows (Tampadesa's Amita draws its own twice as tall as its letters).
                max_lines: Some([tight_row(STAT_LABEL_SIZE, scales), tight_row(STAT_VALUE_SIZE, scales)]),
            });
        for (label, value) in &debrief.stats {
            stats = stats.with_pair(label.clone(), value.clone());
        }
        bottom.push(stats.into());
    }

    if let Some(joins) = &debrief.joins {
        bottom.push(gap(4).into());
        bottom.push(note_box(context, scales, joins, width).into());
    }
    bottom.push(gap(5).into());
    (parts, bottom)
}

// The goals: a row per goal with its mark (met or not), its target and what the city achieved.
fn goals_table(context: &GameUiContext, scales: Scales, debrief: &Debrief, width: f32, tightness: f32) -> UiTable {
    let color = skin::intermission_color;
    let font = scales.font(GOAL_SIZE);
    let line = context.calc_text_size(font, "Ag").y;
    let mut look = skin::row_look();
    look.label_color = Some(color("text", "ink"));
    look.header_color = Some(color("sub", "sub"));
    look.rule_color = Some(color("rule", "sub"));
    look.padding = ((scales.pt(GOAL_ROW) - line) * 0.5 * (0.5 + 0.5 * tightness)).max(scales.pt(2.0));
    let mut table = skin::table()
        .with_label("goals")
        .with_look(look)
        .with_width(width)
        .with_font_scale(font)
        .with_column_spacing(scales.pt(GOAL_COLUMN_GAP))
        .with_header(UiTableHeader { font_scale: Some(scales.heading_font(HEADER_SIZE)), heading_font: true, rule: false, over_status: true, letter_spacing: scales.pt(HEADER_LETTER_SPACING) })
        .with_status_column(UiTableStatusLook::Badges {
            done: color("check", "accent"),
            mark: color("check_mark", "on_accent"),
            open: color("check_open", "sub"),
        })
        .with_column(UiTableColumn::new(ui::text::get(UiTextCategory::Menu, "debrief.goals").to_uppercase(), UiColumnWidth::Fraction(1.0), UiColumnAlign::Left).with_title_color(color("section", "accent")))
        .with_column(UiTableColumn::new(ui::text::get(UiTextCategory::Menu, "debrief.target").to_uppercase(), UiColumnWidth::Fixed(scales.pt(TARGET_WIDTH)), UiColumnAlign::Right).with_color(color("sub", "sub")))
        .with_column(UiTableColumn::new(ui::text::get(UiTextCategory::Menu, "debrief.achieved").to_uppercase(), UiColumnWidth::Fixed(scales.pt(ACHIEVED_WIDTH)), UiColumnAlign::Right));
    for (goal, target, achieved, met) in &debrief.goals {
        let status = if *met { UiTableStatus::Done } else { UiTableStatus::Pending };
        table = table.with_row(UiTableRow::new([goal.clone(), target.clone(), achieved.clone()]).with_status(status));
    }
    table
}

// "<City> joins the world map": a dashed box with a map's mark.
fn note_box(context: &GameUiContext, scales: Scales, joins: &str, width: f32) -> UiFrame {
    let color = skin::intermission_color;
    let border = color("note_border", "accent");
    let font = scales.font(NOTE_SIZE);
    let line = context.calc_text_size(font, "Ag").y;
    let height = scales.pt(NOTE_HEIGHT).max(line + scales.pt(8.0)).ceil();
    let icon_width = scales.pt(NOTE_ICON[1]);
    let k = scales.size;
    let icon = UiCanvas::new(Vec2::new(icon_width, height), move |_, painter, rect| {
        // The generator's "map" icon: a map folded in three.
        let centre = Vec2::new(rect.min.x + NOTE_ICON[0] * k, rect.center().y);
        let at = |x: f32, y: f32| centre + Vec2::new(x * k, y * k);
        let outline = [at(-16.0, -10.0), at(-6.0, -14.0), at(6.0, -10.0), at(16.0, -14.0), at(16.0, 10.0), at(6.0, 14.0), at(-6.0, 10.0), at(-16.0, 14.0), at(-16.0, -10.0)];
        let stroke = (1.6 * k).max(1.0);
        for pair in outline.windows(2) {
            painter.line(pair[0], pair[1], stroke, border);
        }
        painter.line(at(-6.0, -14.0), at(-6.0, 10.0), stroke, border);
        painter.line(at(6.0, -10.0), at(6.0, 14.0), stroke, border);
    })
    .with_label("icon", None);
    let text = text_line(joins, "joins", font, color("text", "ink"), width - icon_width - scales.pt(8.0), UiTextAlign::Left);
    // The text centred on the box's height.
    let drop = ((height - text.measure(context).y) * 0.5).floor();
    let text = left_group().with_widget_spacing(Vec2::zero()).with_widget(spacer(drop)).with_widget(text);
    let row = UiWidgetGroup::new()
        .with_stack_vertically(false)
        .with_center_horizontally(false)
        .with_center_vertically(false)
        .with_widget_spacing(Vec2::zero())
        .with_widget(icon)
        .with_widget(text);
    UiFrame::new(row, Vec2::new(width, height))
        .with_label("note")
        .with_fill(color("note_fill", "light"), 0.0)
        .with_outline(UiFrameOutline { color: border, width: scales.pt(1.5).max(1.0), inset: 0.0, dash: Some([scales.pt(6.0), scales.pt(3.0)]) })
}

// Keep playing at the left, Continue at the right.
fn buttons(context: &mut GameUiContext, scales: Scales, debrief: &Debrief, width: f32) -> UiColumns {
    let keep = skin::button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "debrief.keep_playing"), |_, context| {
        MissionComplete::on_keep_playing(context_as_mut::<GameUiContext>(context));
    })
    .with_width(UiTextButtonWidth::Fixed(scales.pt(KEEP_WIDTH)))
    .with_min_height(scales.pt(BUTTON_HEIGHT));
    let next = skin::button(context, ButtonRole::Primary, debrief.continue_label.clone(), |_, context| {
        MissionComplete::on_continue(context_as_mut::<GameUiContext>(context));
    })
    .with_width(UiTextButtonWidth::Fixed(scales.pt(CONTINUE_WIDTH)))
    .with_min_height(scales.pt(BUTTON_HEIGHT));
    let mut columns = UiColumns::new().with_spacing(0.0).with_width(width);
    columns.add_column(UiColumnWidth::Fixed(scales.pt(KEEP_WIDTH)), left_group().with_widget(scales.button_text(keep, BUTTON_TEXT_SIZE)));
    columns.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());
    columns.add_column(UiColumnWidth::Fixed(scales.pt(CONTINUE_WIDTH)), left_group().with_widget(scales.button_text(next, BUTTON_TEXT_SIZE)));
    columns
}
