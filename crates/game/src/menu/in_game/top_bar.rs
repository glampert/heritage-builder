// The HUD's top bar (docs/plans/realm-ui.md, Phase 5: P5b; hud-<realm>.jpg): one bar from the
// window's left edge to the build column, on the skin's `hud_bar` panel:
// - the goals button (its badge counts the goals met), which drops the goals panel down under it;
//   hidden in a game without goals;
// - the speed controls: pause, play (1×), fast (2×) and faster (4×), the one in use pressed;
// - the date in the UI realm's era words and the season;
// - population, treasury (the warning colour in debt) and the food in granaries and markets;
// - the four ratings as placeholder rings until M9 rates them;
// - the game menu's buttons: Game, Load / Save, Settings, City book (the city management dialog
//   until Phase 6's City book), Briefing (a campaign mission's only);
// - the realm's gap ornaments in the stretches between them.
// The stats are painted (UiCanvas, recorded with their values: `TopBar/date`, `population`,
// `treasury`, `food`, `ratings`); the buttons are icon tiles (`TopBar/Goals`, `Pause`, ...).

use std::{cell::RefCell, rc::Rc};

use common::{Color, Rect, Vec2};
use engine::ui::{self, UiLabelLook, UiString, UiTextAnchor, text::UiTextCategory, widgets::*};
use serde_json::json;

use super::layout;
use crate::{
    campaign::{self, config::CampaignConfigs},
    menu::{
        dialog::{self, DialogMenuKind, goal_line},
        skin::{self, HudGlyphGroup, HudTileKind, SkinPanel},
    },
    realm::{self, Realm},
    sim::{calendar::Season, resources::ResourceKind},
    ui_context::GameUiContext,
};

// The speeds the speed controls set: play, fast and faster.
const SPEEDS: [f32; 3] = [1.0, 2.0, 4.0];

const TILE_GAP: f32 = 6.0;
const DATE_WIDTH: f32 = 240.0;
const STAT_WIDTH: f32 = 134.0;
const RATING_STEP: f32 = 72.0;
const RATINGS_WIDTH: f32 = 4.0 * RATING_STEP + 20.0;

// ----------------------------------------------
// The stats the bar paints
// ----------------------------------------------

#[derive(Clone, Default, PartialEq)]
struct BarStats {
    date: String,
    season: String,
    population: u32,
    treasury: i64,
    food: u32,
    goals: Vec<GoalRow>,
}

#[derive(Clone, PartialEq)]
pub struct GoalRow {
    pub text: String,
    pub target: i64,
    pub achieved: i64,
    pub met: bool,
}

// The goals of the mission being played, read against the world's stats (the debrief's lines).
fn goal_rows(context: &GameUiContext) -> Vec<GoalRow> {
    let Some(active) = campaign::active_mission() else {
        return Vec::new();
    };
    let Some(mission) = CampaignConfigs::get().mission(active.campaign_id, active.mission_index) else {
        return Vec::new();
    };
    let stats = context.world.stats();
    mission
        .requirements
        .goals
        .iter()
        .map(|goal| {
            let (text, target, achieved) = goal_line(goal, stats);
            GoalRow { text, target, achieved, met: goal.is_met(stats) }
        })
        .collect()
}

// The food the city holds in its granaries and markets.
fn food_stored(context: &GameUiContext) -> u32 {
    let resources = &context.world.stats().resources;
    ResourceKind::foods().iter().map(|kind| resources.granaries.count(kind) + resources.markets.count(kind)).sum()
}

fn season_text(realm: Realm, season: Season) -> String {
    let key = match season {
        Season::Wet => format!("season.{realm}.wet"),
        Season::Dry => format!("season.{realm}.dry"),
    };
    ui::text::get(UiTextCategory::Realm, &key).to_string()
}

impl BarStats {
    fn read(context: &GameUiContext) -> Self {
        let realm = skin::current_realm();
        let calendar = context.sim.calendar();
        Self {
            date: realm::date_text(realm, calendar),
            season: season_text(realm, calendar.season()),
            population: context.world.stats().population.total,
            treasury: context.sim.finance().balance(),
            food: food_stored(context),
            goals: goal_rows(context),
        }
    }

    fn goals_met(&self) -> usize {
        self.goals.iter().filter(|goal| goal.met).count()
    }
}

type SharedStats = Rc<RefCell<BarStats>>;

// ----------------------------------------------
// Looks
// ----------------------------------------------

fn look(size: f32, color: Color, heading: bool) -> UiLabelLook {
    UiLabelLook { size, heading_font: heading, color, ..Default::default() }
}

fn digits(value: i64) -> String {
    dialog::group_digits(value)
}

// The fourth rating is the realm's own: the Mandate, Divine favour, Merit.
fn rating_names(realm: Realm) -> [String; 4] {
    let get = |key: &str| ui::text::get(UiTextCategory::Hud, key).to_string();
    [
        get("rating.prosperity"),
        get("rating.culture"),
        get("rating.security"),
        ui::text::get(UiTextCategory::Realm, &format!("rating.{realm}.fourth")).to_string(),
    ]
}

// ----------------------------------------------
// TopBar
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum BarButton {
    Goals,
    Pause,
    Play,
    Fast,
    Faster,
    Game,
    LoadSave,
    Settings,
    CityBook,
    Briefing,
}

impl BarButton {
    // The tile's label (its automation path under TopBar/), glyph and tooltip's key.
    fn label(self) -> &'static str {
        match self {
            Self::Goals => "Goals",
            Self::Pause => "Pause",
            Self::Play => "Play",
            Self::Fast => "Fast",
            Self::Faster => "Faster",
            Self::Game => "Game",
            Self::LoadSave => "Load and save",
            Self::Settings => "Settings",
            Self::CityBook => "City book",
            Self::Briefing => "Briefing",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::Goals => "goals",
            Self::Pause => "pause",
            Self::Play => "play",
            Self::Fast => "fast",
            Self::Faster => "faster",
            Self::Game => "menu",
            Self::LoadSave => "save",
            Self::Settings => "gear",
            Self::CityBook => "book",
            Self::Briefing => "briefing",
        }
    }

    fn tooltip(self) -> &'static str {
        match self {
            Self::Goals => "button.goals",
            Self::Pause => "button.pause",
            Self::Play => "button.play",
            Self::Fast => "button.fast",
            Self::Faster => "button.faster",
            Self::Game => "button.game_menu",
            Self::LoadSave => "button.load_save",
            Self::Settings => "button.settings",
            Self::CityBook => "button.city_book",
            Self::Briefing => "button.briefing",
        }
    }

    fn on_pressed(self, context: &mut GameUiContext) {
        const CLOSE_ALL_OTHERS: bool = true;
        let speed = |context: &mut GameUiContext, index: usize| {
            context.sim.resume();
            context.sim.set_speed(SPEEDS[index]);
        };
        match self {
            Self::Goals => {} // TopBar::draw opens and closes the goals panel with the tile's state.
            Self::Pause => context.sim.pause(),
            Self::Play => speed(context, 0),
            Self::Fast => speed(context, 1),
            Self::Faster => speed(context, 2),
            Self::Game => {
                dialog::open(DialogMenuKind::MainGame, CLOSE_ALL_OTHERS, context);
            }
            Self::LoadSave => {
                dialog::open(DialogMenuKind::SaveGame, CLOSE_ALL_OTHERS, context);
            }
            Self::Settings => {
                dialog::open(DialogMenuKind::MainSettings, CLOSE_ALL_OTHERS, context);
            }
            // TODO(realm-ui): Phase 6's City book.
            Self::CityBook => {
                dialog::open(DialogMenuKind::CityManagement, CLOSE_ALL_OTHERS, context);
            }
            Self::Briefing => {
                dialog::open(DialogMenuKind::MissionBriefing, CLOSE_ALL_OTHERS, context);
            }
        }
    }
}

// What the bar was built for: it's built again when the game's goals or the window change.
#[derive(Clone, PartialEq)]
struct BuiltFor {
    window: Vec2,
    has_goals: bool,
    in_mission: bool,
}

pub struct TopBar {
    menu: UiMenuRcMut,
    row: UiMenuWidgetIndex,
    goals_panel: Option<GoalsPanel>,
    stats: SharedStats,
    built_for: BuiltFor,
    buttons: Vec<(BarButton, UiWidgetGroupWidgetIndex)>,
    canvases: Vec<(Option<Stat>, UiWidgetGroupWidgetIndex)>, // The date's (None) and the stats', their values updated each frame.
}

impl TopBar {
    pub fn new(context: &mut GameUiContext) -> Self {
        let stats: SharedStats = Rc::new(RefCell::new(BarStats::read(context)));
        let built_for = Self::built_for(context, &stats.borrow());
        let window = built_for.window;

        let panel = skin::panel(context, "hud_bar");
        let insets = match &panel {
            SkinPanel::NineSlice { content, .. } => [content.left, content.top, content.right, content.bottom],
            SkinPanel::Sprite { .. } => [8.0; 4],
        };
        let tile = skin::hud_tile_size(HudTileKind::Small);
        let width = (window.x - layout::column_width(context) - 2.0 * layout::MARGIN).max(640.0);
        let height = insets[1] + tile.y + insets[3];
        let inner_width = width - insets[0] - insets[2];

        let mut row = UiWidgetGroup::new().with_stack_vertically(false).with_center_horizontally(false).with_widget_spacing(Vec2::new(TILE_GAP, 0.0));
        let mut buttons = Vec::new();
        let mut add = |context: &mut GameUiContext, row: &mut UiWidgetGroup, button: BarButton| {
            let tile = skin::hud_tile(context, HudTileKind::Small, button.label(), HudGlyphGroup::Hud, button.glyph())
                .with_tooltip(skin::tooltip(context, UiString::key(UiTextCategory::Hud, button.tooltip())))
                .with_toggle(button == BarButton::Goals)
                .on_pressed(move |_, context| button.on_pressed(context_as_mut::<GameUiContext>(context)));
            buttons.push((button, row.add_widget(tile)));
        };

        let mut fixed = 0.0;
        if built_for.has_goals {
            add(context, &mut row, BarButton::Goals);
            fixed += tile.x + TILE_GAP;
        }
        for button in [BarButton::Pause, BarButton::Play, BarButton::Fast, BarButton::Faster] {
            add(context, &mut row, button);
            fixed += tile.x + TILE_GAP;
        }
        let menu_buttons: Vec<BarButton> = [BarButton::Game, BarButton::LoadSave, BarButton::Settings, BarButton::CityBook]
            .into_iter()
            .chain(built_for.in_mission.then_some(BarButton::Briefing))
            .collect();
        fixed += menu_buttons.len() as f32 * (tile.x + TILE_GAP);

        // The stats between the speed controls and the menu, their gaps split either side.
        let middle = DATE_WIDTH + 3.0 * STAT_WIDTH + RATINGS_WIDTH;
        let gaps = (inner_width - fixed - middle - 4.0 * TILE_GAP).max(0.0);
        let gap = gaps * 0.5;

        let mut canvases = vec![(None, row.add_widget(date_canvas(context, &stats, Vec2::new(DATE_WIDTH, tile.y))))];
        row.add_widget(ornament_canvas(context, "ornament_left", Vec2::new(gap, tile.y)));
        for stat in [Stat::Population, Stat::Treasury, Stat::Food] {
            canvases.push((Some(stat), row.add_widget(stat_canvas(context, &stats, stat, Vec2::new(STAT_WIDTH, tile.y)))));
        }
        row.add_widget(ratings_canvas(context, Vec2::new(RATINGS_WIDTH, tile.y)));
        row.add_widget(ornament_canvas(context, "ornament_right", Vec2::new(gap, tile.y)));
        for button in menu_buttons {
            add(context, &mut row, button);
        }

        let menu = UiMenu::new("TopBar")
            .with_flags(UiMenuFlags::IsOpen)
            .with_position(Vec2::new(layout::MARGIN, layout::MARGIN))
            .with_size(Vec2::new(width, height))
            .with_widget_spacing(Vec2::zero());
        let mut menu = panel.apply_to_menu(menu);
        let row = menu.add_widget(row);
        let menu = menu.into_rc();

        // The goals panel hangs under the goals button, the first tile.
        let goals_panel = built_for.has_goals.then(|| {
            let anchor = Vec2::new(layout::MARGIN + insets[0] + tile.x * 0.5, layout::MARGIN + height);
            GoalsPanel::new(context, &stats, anchor)
        });

        Self { menu, row, goals_panel, stats, built_for, buttons, canvases }
    }

    fn built_for(context: &GameUiContext, stats: &BarStats) -> BuiltFor {
        BuiltFor {
            window: ui::screen_size(context.ui_sys),
            has_goals: !stats.goals.is_empty(),
            in_mission: campaign::active_mission().is_some(),
        }
    }

    pub fn draw(&mut self, context: &mut GameUiContext) {
        let stats = BarStats::read(context);
        if Self::built_for(context, &stats) != self.built_for {
            *self = Self::new(context);
        } else if *self.stats.borrow() != stats {
            *self.stats.borrow_mut() = stats;
        }

        // The tiles' states: the speed in use, the goals badge, the goals panel open with its button.
        let paused = context.sim.is_paused();
        let speed = context.sim.speed();
        let speed_index = SPEEDS.iter().rposition(|step| speed >= *step - 0.01).unwrap_or(0);
        let goals_badge = {
            let stats = self.stats.borrow();
            format!("{}/{}", stats.goals_met(), stats.goals.len())
        };
        let mut goals_open = false;
        if let Some(row) = self.menu.widget_as_mut::<UiWidgetGroup>(self.row) {
            let stats = self.stats.borrow();
            for (stat, index) in &self.canvases {
                if let Some(canvas) = row.widget_as_mut::<UiCanvas>(*index) {
                    canvas.set_value(Some(match stat {
                        None => json!({ "date": stats.date, "season": stats.season }),
                        Some(stat) => json!(stat.value(&stats)),
                    }));
                }
            }
            drop(stats);
            for (button, index) in &self.buttons {
                let Some(tile) = row.widget_as_mut::<UiIconTile>(*index) else { continue };
                match button {
                    BarButton::Goals => {
                        tile.set_badge_text(Some(&goals_badge));
                        goals_open = tile.is_selected();
                    }
                    BarButton::Pause => tile.set_selected(paused),
                    BarButton::Play => tile.set_selected(!paused && speed_index == 0),
                    BarButton::Fast => tile.set_selected(!paused && speed_index == 1),
                    BarButton::Faster => tile.set_selected(!paused && speed_index == 2),
                    _ => {}
                }
            }
        }

        self.menu.draw(context);
        if let Some(panel) = &mut self.goals_panel
            && goals_open
        {
            panel.draw(context, &self.stats.borrow());
        }
    }

    // Whether the goals panel is open (TopBar/Goals selected).
    pub fn goals_open(&mut self) -> bool {
        let Some(row) = self.menu.widget_as_mut::<UiWidgetGroup>(self.row) else { return false };
        self.buttons
            .iter()
            .find(|(button, _)| *button == BarButton::Goals)
            .and_then(|(_, index)| row.widget_as_mut::<UiIconTile>(*index))
            .is_some_and(|tile| tile.is_selected())
    }
}

// ----------------------------------------------
// The painted parts
// ----------------------------------------------

fn date_canvas(context: &mut GameUiContext, stats: &SharedStats, size: Vec2) -> UiCanvas {
    let text = skin::hud_color("text", "ink");
    let sub = skin::hud_color("sub", "sub");
    let rule = skin::hud_color("rule", "sub");
    let shared = stats.clone();
    let stats = stats.borrow();
    UiCanvas::new(size, move |_, painter, rect| {
        let stats = shared.borrow();
        painter.line(Vec2::new(rect.min.x, rect.min.y + 2.0), Vec2::new(rect.min.x, rect.max.y - 2.0), 2.0, rule);
        painter.label(Vec2::new(rect.min.x + 10.0, rect.min.y + 16.0), UiTextAnchor::Start, &stats.date, &look(16.0, text, false));
        painter.label(Vec2::new(rect.min.x + 10.0, rect.min.y + 33.0), UiTextAnchor::Start, &stats.season, &look(14.0, sub, false));
    })
    .with_label("date", Some(json!({ "date": stats.date, "season": stats.season })))
    .with_tooltip(skin::tooltip(context, stats.season.clone()))
}

// A gap ornament in the middle of a stretch, the wide one where it fits; none in a short gap.
fn ornament_canvas(context: &mut GameUiContext, label: &str, size: Vec2) -> UiCanvas {
    let wide = skin::hud_ornament(context, true);
    let narrow = skin::hud_ornament(context, false);
    UiCanvas::new(size, move |_, painter, rect| {
        let height = rect.height();
        let pick = [wide, narrow].into_iter().find(|(_, art)| {
            let scale = height / 36.0;
            art.x * scale <= rect.width() - 8.0
        });
        if let Some((texture, art)) = pick {
            let scale = height / 36.0;
            let drawn = art * scale;
            let center = rect.center();
            painter.image(texture, Rect::from_pos_and_size(center - drawn * 0.5, drawn), Color::white());
        }
    })
    .with_label(label, None)
}

#[derive(Copy, Clone)]
enum Stat {
    Population,
    Treasury,
    Food,
}

impl Stat {
    fn label(self) -> &'static str {
        match self {
            Self::Population => "population",
            Self::Treasury => "treasury",
            Self::Food => "food",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Population => "population",
            Self::Treasury => "coin",
            Self::Food => "food",
        }
    }

    fn tooltip(self) -> &'static str {
        match self {
            Self::Population => "stat.population",
            Self::Treasury => "stat.treasury",
            Self::Food => "stat.food",
        }
    }

    fn value(self, stats: &BarStats) -> i64 {
        match self {
            Self::Population => i64::from(stats.population),
            Self::Treasury => stats.treasury,
            Self::Food => i64::from(stats.food),
        }
    }
}

fn stat_canvas(context: &mut GameUiContext, stats: &SharedStats, stat: Stat, size: Vec2) -> UiCanvas {
    let (icon, icon_size) = skin::hud_icon(context, stat.icon());
    let bleed = skin::hud_icon_bleed();
    let text = skin::hud_color("text", "ink");
    let warning = skin::hud_color("warning", "accent");
    let shared = stats.clone();
    let value = stat.value(&stats.borrow());
    UiCanvas::new(size, move |_, painter, rect| {
        let value = stat.value(&shared.borrow());
        // The icon's 36-point drawing at 28 points, its bleed round it.
        let k = 28.0 / (icon_size.x - 2.0 * bleed).max(1.0);
        let drawn = icon_size * k;
        let center = Vec2::new(rect.min.x + 18.0, rect.center().y);
        painter.image(icon, Rect::from_pos_and_size(center - drawn * 0.5, drawn), Color::white());
        let color = if value < 0 { warning } else { text };
        painter.label(Vec2::new(rect.min.x + 38.0, rect.center().y + 7.0), UiTextAnchor::Start, &digits(value), &look(20.0, color, true));
    })
    .with_label(stat.label(), Some(json!(value)))
    .with_tooltip(skin::tooltip(context, UiString::key(UiTextCategory::Hud, stat.tooltip())))
}

// The ratings: rings with each rating's initial, not rated ("—") until M9.
fn ratings_canvas(context: &mut GameUiContext, size: Vec2) -> UiCanvas {
    let realm = skin::current_realm();
    let names = rating_names(realm);
    let text = skin::hud_color("text", "ink");
    let sub = skin::hud_color("sub", "sub");
    let track = skin::hud_color("rating_track", "sub");
    let rule = skin::hud_color("rule", "sub");
    let initials: Vec<String> = names.iter().map(|name| rating_initial(name)).collect();
    let lines: Vec<String> = names
        .iter()
        .map(|name| ui::text::format(UiTextCategory::Hud, "rating.placeholder", &[("rating", name.as_str())]))
        .collect();
    UiCanvas::new(size, move |_, painter, rect| {
        painter.line(Vec2::new(rect.min.x, rect.min.y + 2.0), Vec2::new(rect.min.x, rect.max.y - 2.0), 2.0, rule);
        for (k, initial) in initials.iter().enumerate() {
            let center = Vec2::new(rect.min.x + 30.0 + k as f32 * RATING_STEP, rect.center().y);
            painter.circle_stroke(center, 15.0, 3.5, track);
            painter.label(center + Vec2::new(0.0, 5.5), UiTextAnchor::Middle, initial, &look(14.0, text, true));
            painter.label(center + Vec2::new(21.0, 5.5), UiTextAnchor::Start, "—", &look(14.0, sub, false));
        }
    })
    .with_label("ratings", Some(json!({ "placeholder": true, "names": names })))
    .with_tooltip(skin::tooltip(context, lines.join("\n")))
}

// A rating's initial in its ring: its name's first letter, an article left out ("The Mandate": M).
fn rating_initial(name: &str) -> String {
    let name = name.strip_prefix("The ").unwrap_or(name);
    name.chars().find(|c| c.is_alphabetic()).map(|c| c.to_uppercase().collect()).unwrap_or_default()
}

// ----------------------------------------------
// GoalsPanel
// ----------------------------------------------

const GOALS_WIDTH: f32 = 440.0;
const GOAL_ROW: f32 = 64.0;

// The mission's goals, dropped down under the goals button (`Goals`): a row per goal with its
// progress, on the skin's `hud_panel` panel, the notch pointing up at the button.
struct GoalsPanel {
    menu: UiMenuRcMut,
    canvas: UiMenuWidgetIndex,
}

fn goals_value(stats: &BarStats) -> serde_json::Value {
    json!(stats.goals.iter().map(|goal| json!({ "goal": goal.text, "target": goal.target, "achieved": goal.achieved, "met": goal.met })).collect::<Vec<_>>())
}

impl GoalsPanel {
    fn new(context: &mut GameUiContext, stats: &SharedStats, button: Vec2) -> Self {
        let panel = skin::panel(context, "hud_panel");
        let insets = match &panel {
            SkinPanel::NineSlice { content, .. } => [content.left, content.top, content.right, content.bottom],
            SkinPanel::Sprite { .. } => [16.0; 4],
        };
        let count = stats.borrow().goals.len().max(1);
        let inner = Vec2::new(GOALS_WIDTH - 52.0, 44.0 + count as f32 * GOAL_ROW);
        // A few points spare, so the window never scrolls its canvas.
        let size = Vec2::new(inner.x + insets[0] + insets[2], inner.y + insets[1] + insets[3] + 8.0);
        let position = Vec2::new((button.x - 60.0).max(layout::MARGIN), button.y + 18.0);

        let title = mission_name();
        let text = skin::hud_color("text", "ink");
        let sub = skin::hud_color("sub", "sub");
        let accent = skin::hud_color("accent", "accent");
        let track = skin::hud_color("rating_track", "sub");
        let notch = skin::hud_color("notch", "accent");
        let notch_outline = skin::hud_color("notch_outline", "sub");
        let shared = stats.clone();
        let value = goals_value(&stats.borrow());
        let notch_at = button.x - position.x;
        let canvas = UiCanvas::new(inner, move |_, painter, rect| {
            let stats = shared.borrow();
            // The notch on the panel's top edge, under the button, inside the window (its edge clips).
            let top = rect.min.y - insets[1] + 1.0;
            let x = rect.min.x - insets[0] + notch_at;
            painter.convex_polygon(&[Vec2::new(x - 12.0, top + 14.0), Vec2::new(x, top), Vec2::new(x + 12.0, top + 14.0)], notch, 1.5, notch_outline);
            let title_look = UiLabelLook { letter_spacing: 2.0, ..look(16.0, accent, true) };
            painter.label(Vec2::new(rect.min.x, rect.min.y + 20.0), UiTextAnchor::Start, &ui::text::get(UiTextCategory::Hud, "goals.title").to_uppercase(), &title_look);
            painter.label(Vec2::new(rect.max.x, rect.min.y + 20.0), UiTextAnchor::End, &title, &look(16.0, sub, false));
            for (k, goal) in stats.goals.iter().enumerate() {
                let y = rect.min.y + 44.0 + k as f32 * GOAL_ROW + 22.0;
                let mark = Vec2::new(rect.min.x + 11.0, y - 6.0);
                if goal.met {
                    painter.circle_filled(mark, 11.0, accent);
                    painter.polyline(&[mark + Vec2::new(-5.0, 0.0), mark + Vec2::new(-1.0, 4.0), mark + Vec2::new(6.0, -4.0)], 2.5, text, false);
                } else {
                    painter.circle_stroke(mark, 11.0, 2.0, accent);
                }
                painter.label(Vec2::new(rect.min.x + 34.0, y), UiTextAnchor::Start, &goal.text, &look(18.0, text, false));
                let progress = format!("{} / {}", digits(goal.achieved.min(goal.target)), digits(goal.target));
                painter.label(Vec2::new(rect.max.x, y), UiTextAnchor::End, &progress, &look(16.0, sub, false));
                let bar = Rect::from_pos_and_size(Vec2::new(rect.min.x + 34.0, y + 12.0), Vec2::new(rect.width() - 34.0, 8.0));
                painter.rect_filled(bar, track, 4.0);
                let share = if goal.target > 0 { (goal.achieved as f32 / goal.target as f32).clamp(0.04, 1.0) } else { 1.0 };
                painter.rect_filled(Rect::from_pos_and_size(bar.min, Vec2::new(bar.width() * share, bar.height())), accent, 4.0);
            }
        })
        .with_label("goals", Some(value));

        let menu = UiMenu::new("Goals").with_flags(UiMenuFlags::IsOpen).with_position(position).with_size(size).with_widget_spacing(Vec2::zero());
        let mut menu = panel.apply_to_menu(menu);
        let canvas = menu.add_widget(canvas);
        Self { menu: menu.into_rc(), canvas }
    }

    fn draw(&mut self, context: &mut GameUiContext, stats: &BarStats) {
        if let Some(canvas) = self.menu.widget_as_mut::<UiCanvas>(self.canvas) {
            canvas.set_value(Some(goals_value(stats)));
        }
        self.menu.draw(context);
    }
}

// The mission's name, the goals panel's subtitle.
fn mission_name() -> String {
    let Some(active) = campaign::active_mission() else {
        return String::new();
    };
    let configs = CampaignConfigs::get();
    configs.campaign(active.campaign_id).and_then(|campaign| campaign.mission_display_name(active.mission_index)).map(str::to_string).unwrap_or_default()
}
