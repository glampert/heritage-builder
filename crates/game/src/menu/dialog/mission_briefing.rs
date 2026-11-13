// The briefing before a mission (docs/plans/realm-ui.md, Phase 3: Briefing; briefing-<realm>.jpg):
// the realm's scroll or tablet (skin::briefing's panel) over the city, laid out as the mockup
// generator's dialogs.py briefing() draws it on its 1520 × 980 panel:
// - "CAMPAIGN I · MISSION 2", the mission's name, its city and the year in the realm's era, a rule;
// - left, an image of the mission's land (a placeholder until real art: the realm's world map
//   around the mission's city, its place ringed) with its caption, what's new in the mission and
//   its threats (the mission's `briefing_new` and `briefing_threats`, when it has them);
// - right, the lead-in (its `lead_in`) in quotes, the briefing's text (its
//   `briefing`, else the developers' `description`), the goals and their targets, and Begin.
//
// It opens when a campaign mission's map loads (campaign::take_pending_briefing, InGameMenus), the
// simulation paused until Begin closes it, and from the Game menu's Mission briefing during the
// mission (a stopgap until the HUD's button, Phase 5). A choice point started with no site picked
// (the home menu's Campaign resumes there: Profile::next_mission) offers its two sites in place of
// Begin, as the world map's choice box does, in the briefing that opens as the mission starts only:
// the site whose mission is this one begins it (and so does Escape), the other starts its own
// mission; the pick is recorded in the profile once its mission goes on (Profile::choice_made) or
// has started (Profile::mission_started records the variant's site).
//
// Its size is the skin's (`dialogs.MissionBriefing`: the mockup's panel, scaled to the window and
// grown by the UI scale), and its contents are laid out from the mockup's coordinates at the
// panel's scale, set smaller where a long text wouldn't fit. Built afresh each time it opens
// (prepare) and when the window's size changes. Only in a campaign mission: dialog.open fails in
// any other game.
//
// Paths: MissionBriefing/kicker, name, place, image (kind canvas: city, position), caption,
// section_new, new (kind table: a row per item), section_threats, threats (the line), lead_in, text_<n> (a
// paragraph each), goals (kind table: Goals, Target), Begin; for a choice, choice (its line) and a
// button per site, by its name (MissionBriefing/Vân Cảng).

use common::{Color, Rect};

use super::*;
use super::screen::{BuiltOnOpen, Scales, left_group, spacer, text_line, tight};
use crate::{
    campaign::{
        self,
        config::{CampaignConfigs, ChoiceSite},
    },
    menu::skin::SkinPanel,
    profile,
    realm,
    world_map::provider::step_choice,
};

// ----------------------------------------------
// Layout (points on the generator's 1520 × 980 panel, scaled with it)
// ----------------------------------------------

const PANEL: Vec2 = Vec2::new(1520.0, 980.0);
// Where its contents go, in from the panel's edges (left, top, right, bottom): at least the
// panel's own content insets.
const PADDING: [f32; 4] = [100.0, 92.0, 100.0, 60.0];

// Text sizes (the generator's font sizes).
const KICKER_SIZE: f32 = 17.0;
const LETTER_SPACING: f32 = 2.0;
const NAME_SIZE: f32 = 52.0;
const PLACE_SIZE: f32 = 22.0;
const CAPTION_SIZE: f32 = 17.0;
const SECTION_SIZE: f32 = 17.0;
const ITEM_SIZE: f32 = 20.0;
const LEAD_SIZE: f32 = 26.0;
const TEXT_SIZE: f32 = 21.0;
const GOAL_SIZE: f32 = 21.0;
const HEADER_SIZE: f32 = 15.0;
const BUTTON_TEXT_SIZE: f32 = 22.0;

const IMAGE: Vec2 = Vec2::new(560.0, 300.0);
const COLUMN_GAP: f32 = 60.0; // Between the image's column and the text's.
const ITEM_ROW: f32 = 36.0;
const GOAL_ROW: f32 = 42.0;
const TARGET_WIDTH: f32 = 220.0;
const BUTTON_HEIGHT: f32 = 58.0;
const BEGIN_WIDTH: f32 = 240.0;
const SITE_WIDTH: f32 = 300.0;
const BUTTON_GAP: f32 = 20.0;

// The image: the world map's base this many pixels wide around the city (its height in the
// image's proportion), and the ring on the city's place.
const IMAGE_SPAN_PIXELS: f32 = 1000.0;
const CITY_RING: f32 = 14.0;

// The gaps as the mockup spaces them: after the kicker, the name, the place and the rule; on the
// left after the image, the caption, the list and before the threats' line; on the right after
// the lead-in, between paragraphs, before the goals and above the buttons.
const HEAD_GAPS: [f32; 4] = [8.0, 6.0, 14.0, 32.0];
const LEFT_GAPS: [f32; 4] = [12.0, 32.0, 10.0, 26.0];
const RIGHT_GAPS: [f32; 4] = [18.0, 14.0, 36.0, 30.0];

// How far the contents may be set smaller to fit (a long briefing), and each step.
const MIN_FIT: f32 = 0.6;
const FIT_STEP: f32 = 0.94;

// ----------------------------------------------
// State
// ----------------------------------------------

// Kept outside the widgets: whether it's prepared for opening and the window it was built for
// (screen::BuiltOnOpen), and whether it's the one that opened as the mission started (only that
// one offers a choice point's sites).
struct BriefingState {
    built: BuiltOnOpen,
    at_start: bool,
}

static STATE: UiStaticVar<BriefingState> = UiStaticVar::new(BriefingState { built: BuiltOnOpen::new(), at_start: false });

// Opens the briefing as the mission starts (InGameMenus, campaign::take_pending_briefing): a
// choice point with no site picked offers its sites then. Opened any other way (the Game menu's
// Mission briefing, dialog.open), it only shows Begin: picking the other site mid-mission would
// leave the city unsaved, which the world map asks about first.
pub(super) fn open_at_mission_start(context: &mut GameUiContext) -> bool {
    STATE.as_mut().at_start = true;
    let opened = super::open(DialogMenuKind::MissionBriefing, false, context);
    if !opened {
        STATE.as_mut().at_start = false;
    }
    opened
}

// Sets the briefing up for opening (dialog::open_dialog builds it afresh then).
pub(super) fn prepare() {
    STATE.as_mut().built.prepare();
}

// Whether the open briefing must be built again: the window's size changed.
pub(super) fn needs_rebuild(context: &GameUiContext) -> bool {
    STATE.as_ref().built.needs_rebuild(context)
}

// ----------------------------------------------
// MissionBriefing
// ----------------------------------------------

pub struct MissionBriefing {
    menu: UiMenuRcMut,
}

impl DialogMenuFactory for MissionBriefing {
    const KIND: DialogMenuKind = DialogMenuKind::MissionBriefing;
    const TITLE: &'static [&'static str] = &["Mission Briefing"];

    fn create(context: &mut GameUiContext) -> DialogMenuImpl {
        DialogMenuImpl::from(MissionBriefing::new(context))
    }
}

impl DialogMenu for MissionBriefing {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> DialogMenuKind {
        Self::KIND
    }

    fn menu(&self) -> &UiMenuRcMut {
        &self.menu
    }

    // Only in a campaign mission.
    fn open(&mut self, context: &mut GameUiContext) -> bool {
        if campaign::active_mission().is_none() {
            let state = STATE.as_mut();
            state.built.active = false;
            state.at_start = false;
            return false;
        }
        self.menu_mut().open(context);
        true
    }

    // Closed while it offers a choice point's sites (Escape), the mission being played goes on: its
    // site is the one picked, as its own button would record.
    fn close(&mut self, context: &mut GameUiContext) -> bool {
        if STATE.as_ref().at_start
            && let Some(active) = campaign::active_mission()
            && let Some(choice) = choice(active.campaign_id, active.mission_index)
            && let Some(&(site, _, _)) = choice.sites.iter().find(|(_, _, mission)| *mission == active.mission_index)
        {
            profile::on_choice_made(choice.campaign, choice.step, site);
        }
        self.menu_mut().close(context);
        let state = STATE.as_mut();
        state.built.active = false;
        state.at_start = false;
        true
    }

    fn reset(&mut self) {
        let menu = self.menu_mut();
        menu.reset_message_box();
        menu.set_flags(UiMenuFlags::IsOpen, false);
        let state = STATE.as_mut();
        state.built.active = false;
        state.at_start = false;
    }
}

// A choice point's sites, offered in place of Begin.
#[derive(Clone)]
struct BriefingChoice {
    campaign: usize,
    step: usize,
    line: String,
    sites: Vec<(ChoiceSite, String, usize)>, // The site, its name and the mission it starts.
}

// What the briefing shows, read when it's built.
struct Briefing {
    mission: Option<usize>,
    kicker: String,
    name: String,
    place: String,
    city: Option<(String, [f32; 2])>, // The mission's city and its place on the world map.
    caption: String,
    new: Vec<String>,
    threats: Option<String>,
    lead_in: Option<String>,
    paragraphs: Vec<String>,
    goals: Vec<(String, String)>,
    choice: Option<BriefingChoice>,
}

impl Briefing {
    fn read(context: &GameUiContext) -> Self {
        let configs = CampaignConfigs::get();
        let plain = || Self {
            mission: None,
            kicker: String::new(),
            name: ui::text::get(UiTextCategory::Menu, "briefing.plain").to_string(),
            place: String::new(),
            city: None,
            caption: String::new(),
            new: Vec::new(),
            threats: None,
            lead_in: None,
            paragraphs: Vec::new(),
            goals: Vec::new(),
            choice: None,
        };
        let Some(active) = campaign::active_mission() else {
            return plain();
        };
        let (Some(campaign), Some(mission)) = (configs.campaign(active.campaign_id), configs.mission(active.campaign_id, active.mission_index)) else {
            return plain();
        };

        let number = campaign.mission_number(active.mission_index).to_string();
        let kicker = ui::text::format(
            UiTextCategory::Menu,
            "briefing.kicker",
            &[("campaign", &super::home::campaign_number(active.campaign_id)), ("number", &number)],
        )
        .to_uppercase();
        let name = campaign.mission_display_name(active.mission_index).unwrap_or_default().to_string();
        let year = realm::date_text(realm::ui_realm_for(false).realm, context.sim.calendar());
        let city_name = mission.city_display_name();
        let place = match city_name {
            Some(city) => ui::text::format(UiTextCategory::Menu, "debrief.city_year", &[("city", city), ("year", &year)]),
            None => year,
        };
        let city = mission.city.as_ref().map(|city| (text_of(UiTextCategory::Realm, &city.name), city.position));
        let caption = match city_name {
            Some(city) => ui::text::format(UiTextCategory::Menu, "briefing.image.caption", &[("city", city)]),
            None => name.clone(),
        };

        let story = |key: &String| ui::text::get(UiTextCategory::Story, key).to_string();
        let lead_in = mission.lead_in_text().map(|line| ui::text::format(UiTextCategory::Menu, "briefing.lead_in", &[("line", line)]));
        let paragraphs = mission.briefing_text().split("\n\n").map(str::trim).filter(|paragraph| !paragraph.is_empty()).map(str::to_string).collect();

        let stats = context.world.stats();
        let digits = super::load_save::group_digits;
        let goals = mission
            .requirements
            .goals
            .iter()
            .map(|goal| {
                let (text, target, _) = super::mission_complete::goal_line(goal, stats);
                (text, digits(target))
            })
            .collect();

        Self {
            mission: Some(active.mission_index),
            kicker,
            name,
            place,
            city,
            caption,
            new: mission.briefing_new.iter().map(story).collect(),
            threats: mission.briefing_threats.as_ref().map(story),
            lead_in,
            paragraphs,
            goals,
            choice: STATE.as_ref().at_start.then(|| choice(active.campaign_id, active.mission_index)).flatten(),
        }
    }
}

fn text_of(category: UiTextCategory, key: &str) -> String {
    ui::text::get(category, key).to_string()
}

// The sites the mission being played offers, if it's a choice point the player hasn't picked a
// site of: the home menu's Campaign resumes a campaign at the choice point itself then
// (Profile::next_mission). Worded as the world map's choice box words them
// (world_map::provider::step_choice). Only the briefing that opens as the mission starts offers
// them (open_at_mission_start).
fn choice(campaign_index: usize, mission_index: usize) -> Option<BriefingChoice> {
    let campaign = CampaignConfigs::get().campaign(campaign_index)?;
    if profile::get().record(&campaign.id).and_then(|record| record.choice(mission_index)).is_some() {
        return None;
    }
    let choice = step_choice(campaign, mission_index)?;
    if choice.sites.len() < 2 {
        return None;
    }
    let sites = choice.sites.into_iter().map(|site| (site.site, site.name, site.mission_index)).collect();
    Some(BriefingChoice { campaign: campaign_index, step: mission_index, line: choice.line, sites })
}

impl MissionBriefing {
    pub fn new(context: &mut GameUiContext) -> Self {
        let kind = Self::KIND.to_string();
        let window = ui::screen_size(context.ui_sys);
        let frame = DialogFrame::from_skin(Self::KIND);
        let size = frame.size(window);
        let menu = UiMenu::new(kind).with_flags(frame.menu_flags()).with_size(size).with_widget_spacing(Vec2::zero());

        // Built at boot and on every rebuild for every dialog kind, but shown only once prepared
        // for opening: until then an empty menu, so its textures load only when needed.
        if !STATE.as_ref().built.active {
            return Self { menu: menu.into_rc() };
        }

        // The panel, and the contents inside its own insets and the mockup's margins.
        let scale = (size.x / PANEL.x).min(size.y / PANEL.y);
        let panel = skin::panel(context, &skin::briefing().panel);
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

        let briefing = Briefing::read(context);
        // The mockup's layout at the panel's scale, set smaller until it fits (a long text).
        let mut fit = 1.0;
        let parts = loop {
            let scales = Scales::with_size(scale * fit);
            let parts = contents(context, scales, &briefing, inner.x);
            let height: f32 = parts.iter().map(|part| part.measure(context).y).sum();
            if height <= inner.y - 2.0 || fit <= MIN_FIT {
                break parts;
            }
            fit = (fit * FIT_STEP).max(MIN_FIT);
        };
        for part in parts {
            menu.add_widget(part);
        }

        STATE.as_mut().built.built(context);
        Self { menu: menu.into_rc() }
    }

    // Begin: the briefing closes and the game goes on.
    fn on_begin(context: &mut GameUiContext) {
        super::close(DialogMenuKind::MissionBriefing, context);
    }

    // A site of the choice: its mission begins here if it's this one (closing records the pick),
    // else it starts (its map loads, and its own briefing opens). That's the other site's
    // variant, which records the pick once it has started (Profile::mission_started), so a map
    // that fails to load records nothing. If it can't start at all, nothing is recorded and this
    // mission goes on.
    fn on_site(context: &mut GameUiContext, choice: &BriefingChoice, mission: usize) {
        let current = campaign::active_mission().map(|active| active.mission_index);
        if current == Some(mission) {
            super::close(DialogMenuKind::MissionBriefing, context);
        } else if !super::home::start_campaign_mission(choice.campaign, mission) {
            STATE.as_mut().at_start = false;
            super::close(DialogMenuKind::MissionBriefing, context);
        }
    }
}

// ----------------------------------------------
// The contents
// ----------------------------------------------

// The heading, then the two columns, `width` points wide, at `scales`.
fn contents(context: &mut GameUiContext, scales: Scales, briefing: &Briefing, width: f32) -> Vec<UiWidgetImpl> {
    let color = skin::intermission_color;
    let accent = color("accent", "accent");
    let sub = color("sub", "sub");
    let gap = |points: f32| spacer(scales.pt(points));
    let mut parts: Vec<UiWidgetImpl> = Vec::new();

    if !briefing.kicker.is_empty() {
        let font = scales.heading_font(KICKER_SIZE);
        let kicker = text_line(&briefing.kicker, "kicker", font, accent, width, UiTextAlign::Left).with_heading_font().with_letter_spacing(scales.pt(LETTER_SPACING));
        parts.push(tight(context, kicker, font, KICKER_SIZE, scales).into());
        parts.push(gap(HEAD_GAPS[0]).into());
    }
    let font = scales.heading_font(NAME_SIZE);
    let name = text_line(&briefing.name, "name", font, color("heading", "title"), width, UiTextAlign::Left).with_heading_font();
    parts.push(tight(context, name, font, NAME_SIZE, scales).into());
    if !briefing.place.is_empty() {
        parts.push(gap(HEAD_GAPS[1]).into());
        let font = scales.font(PLACE_SIZE);
        let place = UiRichText::from_spans([UiTextSpan::emphasized(briefing.place.clone())]).with_label("place").with_font_scale(font).with_width(width).with_text_color(sub);
        parts.push(tight(context, place, font, PLACE_SIZE, scales).into());
    }
    parts.push(gap(HEAD_GAPS[2]).into());
    parts.push(skin::separator(context, "rule").into());
    if briefing.mission.is_none() {
        return parts;
    }
    parts.push(gap(HEAD_GAPS[3]).into());

    let left_width = scales.pt(IMAGE.x);
    let right_width = (width - left_width - scales.pt(COLUMN_GAP)).floor();
    let left = left_column(context, scales, briefing, left_width);
    let left_height = left.measure(context).y;
    let right = right_column(context, scales, briefing, right_width, left_height);
    let mut columns = UiColumns::new().with_spacing(scales.pt(COLUMN_GAP)).with_width(width);
    columns.add_column(UiColumnWidth::Fixed(left_width), left);
    columns.add_column(UiColumnWidth::Fraction(1.0), right);
    parts.push(columns.into());
    parts
}

// The image and its caption, what's new and the threats.
fn left_column(context: &mut GameUiContext, scales: Scales, briefing: &Briefing, width: f32) -> UiWidgetGroup {
    let color = skin::intermission_color;
    let gap = |points: f32| spacer(scales.pt(points));
    let mut group = left_group().with_widget_spacing(Vec2::zero());
    group.add_widget(image(context, scales, briefing));
    group.add_widget(gap(LEFT_GAPS[0]));
    let font = scales.font(CAPTION_SIZE);
    let caption = UiRichText::from_spans([UiTextSpan::emphasized(briefing.caption.clone())])
        .with_label("caption")
        .with_font_scale(font)
        .with_width(width)
        .with_align(UiTextAlign::Center)
        .with_text_color(color("sub", "sub"));
    group.add_widget(tight(context, caption, font, CAPTION_SIZE, scales));

    if !briefing.new.is_empty() {
        group.add_widget(gap(LEFT_GAPS[1]));
        group.add_widget(section_title(context, scales, "briefing.new", "section_new", width));
        group.add_widget(gap(LEFT_GAPS[2]));
        let def = skin::briefing();
        let font = scales.font(ITEM_SIZE);
        let line = context.calc_text_size(font, "Ag").y;
        let mut look = skin::row_look();
        look.label_color = Some(color("text", "ink"));
        look.rule_color = None;
        look.padding = ((scales.pt(ITEM_ROW) - line) * 0.5).max(scales.pt(1.0));
        let mut table = skin::table()
            .with_label("new")
            .with_look(look)
            .with_width(width)
            .with_font_scale(font)
            .with_header(UiTableHeader { hidden: true, ..UiTableHeader::default() })
            .with_bullets(def.bullet.bullet(), skin::color(&def.bullet_color).unwrap_or(color("accent", "accent")), scales.pt(10.0).max(3.0))
            .with_column(UiTableColumn::new("", UiColumnWidth::Fraction(1.0), UiColumnAlign::Left));
        for item in &briefing.new {
            table = table.with_row(UiTableRow::new([item.clone()]));
        }
        group.add_widget(table);
    }

    if let Some(threats) = &briefing.threats {
        group.add_widget(gap(LEFT_GAPS[3]));
        group.add_widget(section_title(context, scales, "briefing.threats", "section_threats", width));
        group.add_widget(gap(LEFT_GAPS[2]));
        group.add_widget(threats_line(context, scales, threats, width));
    }
    group
}

// A section's title in capitals: "NEW IN THIS MISSION", "GOALS".
fn section_title(context: &GameUiContext, scales: Scales, key: &str, label: &str, width: f32) -> UiRichText {
    let font = scales.heading_font(SECTION_SIZE);
    let title = text_line(&ui::text::get(UiTextCategory::Menu, key).to_uppercase(), label, font, skin::intermission_color("section", "accent"), width, UiTextAlign::Left)
        .with_heading_font()
        .with_letter_spacing(scales.pt(LETTER_SPACING));
    tight(context, title, font, SECTION_SIZE, scales)
}

// The warning sign and the threats, in italics.
fn threats_line(context: &GameUiContext, scales: Scales, threats: &str, width: f32) -> UiWidgetGroup {
    let accent = skin::intermission_color("accent", "accent");
    let font = scales.font(ITEM_SIZE);
    let sign_width = scales.pt(36.0);
    let text = UiRichText::from_spans([UiTextSpan::emphasized(threats.to_string())])
        .with_label("threats")
        .with_font_scale(font)
        .with_width(width - sign_width)
        .with_text_color(skin::intermission_color("text", "ink"));
    let line = context.calc_text_size(font, "Ag").y;
    let k = scales.size;
    let sign = UiCanvas::new(Vec2::new(sign_width, line), move |_, painter, rect| {
        // The generator's "warn" glyph: a triangle with a mark in it.
        let centre = Vec2::new(rect.min.x + 12.0 * k, rect.min.y + line * 0.5);
        let at = |x: f32, y: f32| centre + Vec2::new(x * k, y * k);
        let stroke = (1.8 * k).max(1.0);
        let outline = [at(0.0, -11.0), at(11.0, 9.0), at(-11.0, 9.0), at(0.0, -11.0)];
        for pair in outline.windows(2) {
            painter.line(pair[0], pair[1], stroke, accent);
        }
        painter.line(at(0.0, -4.0), at(0.0, 3.0), stroke, accent);
        painter.circle_filled(at(0.0, 6.0), (1.3 * k).max(1.0), accent);
    })
    .with_label("sign", None);
    UiWidgetGroup::new()
        .with_stack_vertically(false)
        .with_center_horizontally(false)
        .with_center_vertically(false)
        .with_widget_spacing(Vec2::zero())
        .with_widget(sign)
        .with_widget(text)
}

// The lead-in, the briefing's text, the goals, and Begin (or the choice's sites), kept level with
// the bottom of the left column when it's the taller (`left_height`).
fn right_column(context: &mut GameUiContext, scales: Scales, briefing: &Briefing, width: f32, left_height: f32) -> UiWidgetGroup {
    let color = skin::intermission_color;
    let gap = |points: f32| spacer(scales.pt(points));
    let mut parts: Vec<UiWidgetImpl> = Vec::new();

    if let Some(lead_in) = &briefing.lead_in {
        let font = scales.font(LEAD_SIZE);
        let lead = UiRichText::from_spans([UiTextSpan::emphasized(lead_in.clone())])
            .with_label("lead_in")
            .with_font_scale(font)
            .with_width(width)
            .with_text_color(color("accent", "accent"));
        parts.push(lead.into());
        parts.push(gap(RIGHT_GAPS[0]).into());
    }
    let font = scales.font(TEXT_SIZE);
    for (index, paragraph) in briefing.paragraphs.iter().enumerate() {
        if index > 0 {
            parts.push(gap(RIGHT_GAPS[1]).into());
        }
        parts.push(text_line(paragraph, &format!("text_{}", index + 1), font, color("text", "ink"), width, UiTextAlign::Left).into());
    }
    if !briefing.goals.is_empty() {
        parts.push(gap(RIGHT_GAPS[2]).into());
        parts.push(goals_table(context, scales, briefing, width).into());
    }

    let buttons = buttons(context, scales, briefing, width);
    let parts_height: f32 = parts.iter().map(|part| part.measure(context).y).sum();
    let buttons_height = buttons.measure(context).y;
    let room = (left_height - parts_height - buttons_height).floor().max(scales.pt(RIGHT_GAPS[3]));

    let mut group = left_group().with_widget_spacing(Vec2::zero());
    for part in parts {
        group.add_widget(part);
    }
    group.add_widget(spacer(room));
    group.add_widget(buttons);
    group
}

// The goals: a ring per goal, its text and its target.
fn goals_table(context: &GameUiContext, scales: Scales, briefing: &Briefing, width: f32) -> UiTable {
    let color = skin::intermission_color;
    let font = scales.font(GOAL_SIZE);
    let line = context.calc_text_size(font, "Ag").y;
    let mut look = skin::row_look();
    look.label_color = Some(color("text", "ink"));
    look.header_color = Some(color("sub", "sub"));
    look.rule_color = Some(color("rule", "sub"));
    look.padding = ((scales.pt(GOAL_ROW) - line) * 0.5).max(scales.pt(2.0));
    let accent = color("accent", "accent");
    let mut table = skin::table()
        .with_label("goals")
        .with_look(look)
        .with_width(width)
        .with_font_scale(font)
        .with_header(UiTableHeader {
            font_scale: Some(scales.heading_font(HEADER_SIZE)),
            heading_font: true,
            rule: false,
            over_status: true,
            letter_spacing: scales.pt(LETTER_SPACING),
            hidden: false,
        })
        .with_status_column(UiTableStatusLook::Badges { done: accent, mark: color("check_mark", "on_accent"), open: accent })
        .with_column(UiTableColumn::new(ui::text::get(UiTextCategory::Menu, "debrief.goals").to_uppercase(), UiColumnWidth::Fraction(1.0), UiColumnAlign::Left).with_title_color(color("section", "accent")))
        .with_column(UiTableColumn::new(ui::text::get(UiTextCategory::Menu, "debrief.target").to_uppercase(), UiColumnWidth::Fixed(scales.pt(TARGET_WIDTH)), UiColumnAlign::Right));
    for (goal, target) in &briefing.goals {
        table = table.with_row(UiTableRow::new([goal.clone(), target.clone()]).with_status(UiTableStatus::Pending));
    }
    table
}

// Begin at the right; for a choice, its line and a button per site.
fn buttons(context: &mut GameUiContext, scales: Scales, briefing: &Briefing, width: f32) -> UiWidgetGroup {
    // TODO(realm-ui): Phase 4's "Replay the story" button goes left of Begin, as the mockup has it,
    // once the story sequences exist.
    let sized = |button: UiTextButton, button_width: f32| {
        scales.button_text(button.with_width(UiTextButtonWidth::Fixed(scales.pt(button_width))).with_min_height(scales.pt(BUTTON_HEIGHT)), BUTTON_TEXT_SIZE)
    };
    let mut group = left_group().with_widget_spacing(Vec2::zero());
    let mut columns = UiColumns::new().with_spacing(scales.pt(BUTTON_GAP)).with_width(width);
    columns.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());
    match &briefing.choice {
        None => {
            let begin = skin::button(context, ButtonRole::Primary, UiString::key(UiTextCategory::Menu, "briefing.begin"), |_, context| {
                MissionBriefing::on_begin(context_as_mut::<GameUiContext>(context));
            });
            columns.add_column(UiColumnWidth::Fixed(scales.pt(BEGIN_WIDTH)), left_group().with_widget(sized(begin, BEGIN_WIDTH)));
        }
        Some(choice) => {
            let font = scales.font(TEXT_SIZE);
            let line = UiRichText::from_spans([UiTextSpan::emphasized(choice.line.clone())])
                .with_label("choice")
                .with_font_scale(font)
                .with_width(width)
                .with_align(UiTextAlign::Right)
                .with_text_color(skin::intermission_color("accent", "accent"));
            group.add_widget(tight(context, line, font, TEXT_SIZE, scales));
            group.add_widget(spacer(scales.pt(12.0)));
            for (_, name, mission) in choice.sites.clone() {
                let choice = choice.clone();
                let button = skin::button(context, ButtonRole::Primary, name, move |_, context| {
                    MissionBriefing::on_site(context_as_mut::<GameUiContext>(context), &choice, mission);
                });
                columns.add_column(UiColumnWidth::Fixed(scales.pt(SITE_WIDTH)), left_group().with_widget(sized(button, SITE_WIDTH)));
            }
        }
    }
    group.add_widget(columns);
    group
}

// ----------------------------------------------
// The image
// ----------------------------------------------

// The mission's land: a placeholder until each mission has its own picture, the realm's world map
// around the mission's city (the map's middle without one), its place ringed, in the skin's
// outline.
// TODO(realm-ui): a mission's own image (an `image` in its config, from the art pipeline) replaces
// the map once there is art for it.
fn image(context: &mut GameUiContext, scales: Scales, briefing: &Briefing) -> UiCanvas {
    let size = Vec2::new(scales.pt(IMAGE.x), scales.pt(IMAGE.y));
    let map = skin::world_map();
    let texture = (!map.base.is_empty()).then(|| context.load_texture(PathRef::from_str(&map.base)));
    let texture_size = texture
        .and_then(|texture| context.render_sys().texture_cache().texture_size(texture))
        .map_or(Vec2::new(1.0, 1.0), |size| Vec2::new(size.width as f32, size.height as f32));
    let map_size = map.base_map_size();
    let position = briefing.city.as_ref().map_or([map.extent[0] * 0.5, map.extent[1] * 0.5], |(_, position)| *position);
    let centre = map.base_pixel(position);

    // The part of the base shown, in its pixels, kept on the map.
    let span = Vec2::new(IMAGE_SPAN_PIXELS, IMAGE_SPAN_PIXELS * IMAGE.y / IMAGE.x);
    let span = Vec2::new(span.x.min(map_size.x), span.y.min(map_size.y));
    let min = Vec2::new((centre.x - span.x * 0.5).clamp(0.0, (map_size.x - span.x).max(0.0)), (centre.y - span.y * 0.5).clamp(0.0, (map_size.y - span.y).max(0.0)));
    let uv = Rect::from_pos_and_size(Vec2::new(min.x / texture_size.x, min.y / texture_size.y), Vec2::new(span.x / texture_size.x, span.y / texture_size.y));
    let ring = briefing.city.as_ref().map(|_| Vec2::new((centre.x - min.x) / span.x, (centre.y - min.y) / span.y));

    let def = skin::briefing();
    let outline = skin::color(&def.image_outline).unwrap_or(Color::black());
    let outline_width = (def.image_outline_width * scales.size).max(1.0);
    let accent = skin::intermission_color("accent", "accent");
    let k = scales.size;
    let value = briefing.city.as_ref().map(|(city, position)| serde_json::json!({ "city": city, "position": position }));
    UiCanvas::new(size, move |_, painter, rect| {
        match texture {
            Some(texture) => painter.image_uv(texture, rect, uv, Color::white()),
            None => painter.rect_filled(rect, outline, 0.0),
        }
        if let Some(ring) = ring {
            let at = rect.min + Vec2::new(ring.x * rect.width(), ring.y * rect.height());
            painter.circle_stroke(at, CITY_RING * k, (3.0 * k).max(1.5), accent);
            painter.circle_filled(at, (4.0 * k).max(2.0), accent);
        }
        painter.rect_stroke(rect, 2.0 * k, outline_width, outline);
    })
    .with_label("image", value)
}
