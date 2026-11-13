// The finished stele at the end of a campaign (docs/plans/realm-ui.md, Phase 3: Campaign complete;
// campaign-complete-<realm>.jpg): a full-screen dialog laid out as the mockup generator's
// dialogs.py campaign_complete() draws it, in the debrief's style (skin::intermission):
// - left, the campaign's stele carved in full (crate::campaign::stele, drawn by
//   menu::stele::SteleArt on the skin's `intermission.complete` placement: wider than the
//   debrief's, its niche stretched to match), the last line freshly carved;
// - right, on the skin's big panel: "CAMPAIGN I COMPLETE", the campaign's name, the realm and the
//   date it was finished in, its closing line (the campaign's `stele_closing`), its missions, the
//   stats (missions finished, the last city and its population), the note that the next campaign
//   is open (or that every one is finished), Watch the epilogue when the campaign has one (it
//   plays over the screen), and Return to the menu, which plays the epilogue first while the
//   profile hasn't seen it.
//
// It's built afresh each time it opens (prepare), from the mission being played (the campaign's
// last: campaign::active_mission) and the profile, which has recorded it by then, and again when
// the window's size changes (its layout follows the window, screen::Scales) or the skin or the
// language does. Opened without a mission (automation's dialog.open in another game), it shows
// "Campaign complete" and its button only.
//
// The profile reached the next visible campaign when the last mission was finished
// (Profile::mission_finished), so the home menu Return to the menu leads to shows that campaign's
// realm.
//
// Paths: CampaignComplete/stele (kind canvas: title, lines, newest, rows_to_come);
// CampaignComplete/panel/kicker, name, place, closing, missions_title, missions (kind table: a row
// per mission, its number and name; missions_2 for a second column), stats (kind key_values),
// note/next, Watch the epilogue (only for a campaign with an epilogue), Return to the menu.

use common::{Color, Rect};

use super::*;
use super::screen::{BuiltOnOpen, Scales, left_group, spacer, text_line, tight, tight_row};
use crate::{
    GameLoop,
    campaign::{
        self,
        config::CampaignConfigs,
        stele::{self as stele_data, Stele},
    },
    menu::{
        skin::{EmblemState, SkinPanel},
        stele::SteleArt,
    },
    profile,
    realm::{self, Realm},
};

// ----------------------------------------------
// Layout (points on the generator's 1920 × 1080 screen, scaled by the window: screen::Scales)
// ----------------------------------------------

const PANEL_PADDING: [f32; 4] = [56.0, 56.0, 56.0, 40.0];

// Text sizes (the generator's font sizes).
const KICKER_SIZE: f32 = 20.0;
const KICKER_LETTER_SPACING: f32 = 3.0;
const NAME_SIZE: f32 = 52.0;
const PLACE_SIZE: f32 = 21.0;
const CLOSING_SIZE: f32 = 28.0;
const SECTION_SIZE: f32 = 16.0;
const SECTION_LETTER_SPACING: f32 = 2.0;
const MISSION_SIZE: f32 = 19.0;
const STAT_LABEL_SIZE: f32 = 19.0;
const STAT_VALUE_SIZE: f32 = 28.0;
const NOTE_SIZE: f32 = 21.0;
const BUTTON_TEXT_SIZE: f32 = 22.0;

// The missions: rows 36 points apart, five to a column, two columns at most (more rows then); the
// number's column.
const MISSION_ROW: f32 = 36.0;
const MISSIONS_PER_COLUMN: usize = 5;
const MISSION_NUMBER_WIDTH: f32 = 30.0;
const MISSION_COLUMN_GAP: f32 = 24.0;
const STAT_GAP: f32 = 6.0;
const NOTE_HEIGHT: f32 = 84.0;
const NOTE_EMBLEM: [f32; 3] = [46.0, 28.0, 90.0]; // The emblem's centre and radius, and the text's left, in from the box's left.
const BUTTON_HEIGHT: f32 = 58.0;
const RETURN_WIDTH: f32 = 340.0;
const EPILOGUE_WIDTH: f32 = 300.0;

// The gaps between the panel's parts as the mockup spaces them (tightness 1), from the top: after
// the kicker, the name, the place, the rule, the closing line and the missions' title; and from
// the bottom, between the stats and the note and above the buttons. The room left over goes
// between the missions and the stats.
const GAPS: [f32; 8] = [14.0, 6.0, 10.0, 34.0, 26.0, 10.0, 20.0, 36.0];

// ----------------------------------------------
// State
// ----------------------------------------------

// Kept outside the widgets, as the debrief's (screen::BuiltOnOpen): whether it's prepared for
// opening, and the window it was built for.
static STATE: UiStaticVar<BuiltOnOpen> = UiStaticVar::new(BuiltOnOpen::new());

// Whether Watch the epilogue played it since the screen was prepared: Return to the menu then
// quits without playing it again (runtime only, reset when the screen is prepared).
static EPILOGUE_WATCHED: UiStaticVar<bool> = UiStaticVar::new(false);

// Sets the screen up for opening (dialog::open_dialog builds it afresh then).
pub(super) fn prepare() {
    STATE.as_mut().prepare();
    *EPILOGUE_WATCHED.as_mut() = false;
}

// Whether the open screen must be built again: the window's size changed.
pub(super) fn needs_rebuild(context: &GameUiContext) -> bool {
    STATE.as_ref().needs_rebuild(context)
}

// ----------------------------------------------
// CampaignComplete
// ----------------------------------------------

pub struct CampaignComplete {
    menu: UiMenuRcMut,
}

impl DialogMenuFactory for CampaignComplete {
    const KIND: DialogMenuKind = DialogMenuKind::CampaignComplete;
    const TITLE: &'static [&'static str] = &["Campaign Complete"];

    fn create(context: &mut GameUiContext) -> DialogMenuImpl {
        DialogMenuImpl::from(CampaignComplete::new(context))
    }
}

impl DialogMenu for CampaignComplete {
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
        self.menu_mut().close(context);
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
    closing: Option<String>,
    missions: Vec<(String, String)>, // Number, name.
    stats: Vec<(String, String)>,
    next: Option<(Option<Realm>, String)>, // The next campaign's realm (its emblem) and the note.
    stele: Option<Stele>,
}

impl Summary {
    fn read(context: &GameUiContext) -> Self {
        let plain = || Self {
            kicker: ui::text::get(UiTextCategory::Menu, "campaign_complete.plain").to_uppercase(),
            name: None,
            place: None,
            closing: None,
            missions: Vec::new(),
            stats: Vec::new(),
            next: None,
            stele: None,
        };
        let configs = CampaignConfigs::get();
        let Some(active) = campaign::preview::shown_mission() else {
            return plain();
        };
        let Some(campaign) = configs.campaign(active.campaign_id) else {
            return plain();
        };

        let number = super::home::campaign_number(active.campaign_id);
        let kicker = ui::text::format(UiTextCategory::Menu, "campaign_complete.kicker", &[("campaign", &number)]).to_uppercase();
        let ui_realm = realm::ui_realm_for(false).realm;
        let date = realm::date_text(ui_realm, context.sim.calendar());
        // TODO(realm-ui): the mockup's span of years ("23 years, from year 1 to year 24 of the Thái
        // Ninh era") needs the campaign's first mission's start date kept in the profile.
        let place = ui::text::format(UiTextCategory::Menu, "campaign_complete.place", &[("realm", campaign.realm.display_name()), ("date", &date)]);
        let closing = campaign
            .stele_closing
            .as_deref()
            .map(|key| ui::text::format(UiTextCategory::Menu, "campaign_complete.closing", &[("line", ui::text::get(UiTextCategory::Story, key))]));

        // A preview's (campaign::preview): as if the campaign had been played up to its mission.
        let profile = campaign::preview::shown_profile();
        let stele = stele_data::stele(configs, profile, active.campaign_id, None);

        // A row per step: the mission that finished it (a site's, once picked), else the step's own.
        let steps = campaign.steps();
        let finished_by = |step: usize| {
            stele.as_ref().and_then(|stele| {
                stele.lines.iter().find(|line| line.distinction.is_none() && campaign.step_of(line.mission_index) == step).map(|line| line.mission_index)
            })
        };
        let missions = steps
            .iter()
            .map(|&step| {
                let mission = finished_by(step).unwrap_or(step);
                (campaign.mission_number(mission).to_string(), campaign.mission_display_name(mission).unwrap_or_default().to_string())
            })
            .collect();

        // TODO(realm-ui): M9's distinctions earned ("8 of 12", a mark per distinction beside each
        // mission) and the largest city the campaign founded (its population kept per mission in
        // the profile) go here, as the mockup shows them.
        let done = steps.iter().filter(|&&step| finished_by(step).is_some()).count();
        let mut stats = vec![(
            ui::text::get(UiTextCategory::Menu, "campaign_complete.stat.missions").to_string(),
            ui::text::format(UiTextCategory::Menu, "campaign_complete.of", &[("done", &done.to_string()), ("total", &steps.len().to_string())]),
        )];
        if let Some(city) = configs.mission(active.campaign_id, active.mission_index).and_then(|mission| mission.city_display_name()) {
            let population = super::load_save::group_digits(i64::from(context.world.stats().population.total));
            stats.push((
                ui::text::get(UiTextCategory::Menu, "campaign_complete.stat.city").to_string(),
                ui::text::format(UiTextCategory::Menu, "campaign_complete.city", &[("city", city), ("population", &population)]),
            ));
        }

        // The next campaign, reached now: open, or coming soon while it has no missions yet (the
        // home menu and the picker can't start it then).
        let next = Some(match configs.next_visible_after(active.campaign_id).and_then(|index| configs.campaign(index).map(|next| (index, next))) {
            Some((index, next)) => {
                let number = super::home::campaign_number(index);
                let args = [("campaign", number.as_str()), ("name", next.display_name()), ("realm", next.realm.display_name())];
                let note = if next.missions.is_empty() {
                    ui::text::format(UiTextCategory::Menu, "campaign_complete.next.coming_soon", &args)
                } else {
                    ui::text::format(UiTextCategory::Menu, "campaign_complete.next", &args)
                };
                (Some(next.realm), note)
            }
            None => (None, ui::text::get(UiTextCategory::Menu, "campaign_complete.next.none").to_string()),
        });

        Self {
            kicker,
            name: Some(campaign.display_name().to_string()),
            place: Some(place),
            closing,
            missions,
            stats,
            next,
            stele,
        }
    }
}

impl CampaignComplete {
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
        let summary = Summary::read(context);
        let [panel_x, panel_y, panel_width, _] = art.complete.panel;

        menu.add_widget(spacer(scales.origin.y));
        let mut columns = UiColumns::new().with_spacing(0.0);
        columns.add_column(UiColumnWidth::Fixed(scales.origin.x), UiWidgetGroup::new());
        columns.add_column(UiColumnWidth::Fixed(scales.pt(panel_x)), left_group().with_widget(stele_canvas(context, scales, panel_x, summary.stele.clone())));
        let panel = left_group().with_widget_spacing(Vec2::zero()).with_widget(spacer(scales.pt(panel_y))).with_widget(panel(context, scales, &summary));
        columns.add_column(UiColumnWidth::Fixed(scales.pt(panel_width)), panel);
        columns.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());
        menu.add_widget(columns);

        STATE.as_mut().built(context);
        Self { menu: menu.into_rc() }
    }

    // Back to the home menu, which shows the realm of the campaign the profile reached, after the
    // campaign's epilogue if the profile hasn't seen it (it plays once, recorded once it ends, Skip
    // included). An epilogue Watch the epilogue played on this screen isn't played again: it's
    // recorded as seen now, its first viewing. Quitting also resets the campaign
    // (cmd_quit_to_main_menu).
    // A preview's only closes the screen (campaign::preview).
    fn on_return(context: &mut GameUiContext) {
        if campaign::preview::is_active() {
            super::close(DialogMenuKind::CampaignComplete, context);
            return;
        }
        if let Some(campaign_index) = epilogue_campaign()
            && let Some(epilogue) = crate::story::epilogue(campaign_index)
            && !epilogue.is_seen(profile::get())
        {
            if *EPILOGUE_WATCHED.as_ref() {
                profile::on_sequence_seen(campaign_index, &epilogue.sequence.id);
                GameLoop::get_mut().quit_to_main_menu();
                return;
            }
            let request = StoryRequest {
                campaign_index,
                sequence_id: epilogue.sequence.id.clone(),
                panel: 0,
                records_seen: true,
                on_end: Some(|_, _| GameLoop::get_mut().quit_to_main_menu()),
            };
            if super::play_story(request, context) {
                return;
            }
        }
        GameLoop::get_mut().quit_to_main_menu();
    }

    // Watch the epilogue: plays it over the screen, which shows again once it ends. A replay, so
    // it records nothing; Return to the menu then records an unseen epilogue without playing it
    // again (EPILOGUE_WATCHED).
    fn on_watch_epilogue(context: &mut GameUiContext) {
        let Some(campaign_index) = epilogue_campaign() else {
            return;
        };
        let Some(epilogue) = crate::story::epilogue(campaign_index) else {
            return;
        };
        let request = StoryRequest { campaign_index, sequence_id: epilogue.sequence.id.clone(), panel: 0, records_seen: false, on_end: None };
        if super::play_story(request, context) {
            *EPILOGUE_WATCHED.as_mut() = true;
        }
    }
}

// The campaign whose epilogue the screen offers: the mission being played's, if it has one.
fn epilogue_campaign() -> Option<usize> {
    let campaign_index = campaign::preview::shown_mission()?.campaign_id;
    crate::story::epilogue(campaign_index).map(|_| campaign_index)
}

// ----------------------------------------------
// The stele, left
// ----------------------------------------------

// The niche and the stele carved in full, stretched as the skin's `complete` places them, over the
// left column `width` points wide.
fn stele_canvas(context: &mut GameUiContext, scales: Scales, width: f32, stele: Option<Stele>) -> UiCanvas {
    let complete = &skin::intermission().complete;
    let art = SteleArt::load(context).stretched(complete.stele, complete.niche_stretch, complete.rule_clearance);
    let value = stele.as_ref().map(Stele::canvas_value);
    let scale = scales.size;
    UiCanvas::new(Vec2::new(scales.pt(width), scales.pt(super::screen::REFERENCE.y)), move |_, painter, rect| {
        art.draw_niche(painter, rect.min, scale);
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

fn panel(context: &mut GameUiContext, scales: Scales, summary: &Summary) -> UiFrame {
    let [_, _, width, height] = skin::intermission().complete.panel;
    let size = Vec2::new(scales.pt(width), scales.pt(height));
    let padding = PANEL_PADDING.map(|side| scales.pt(side));
    let inner = Vec2::new(size.x - padding[0] - padding[2], size.y - padding[1] - padding[3]);

    let buttons = buttons(context, scales, inner.x);
    let buttons_height = buttons.measure(context).y;

    // The parts above the buttons as the mockup spaces them; closer together where the realm's
    // font's tall lines (Tampadesa's) would push the buttons off the panel.
    let room_for_parts = inner.y - buttons_height - scales.pt(6.0);
    let (mut top, mut bottom) = (Vec::new(), Vec::new());
    let mut parts_height = 0.0;
    for tightness in [1.0, 0.6, 0.3, 0.0] {
        (top, bottom) = panel_parts(context, scales, summary, inner.x, tightness);
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

// The panel's parts above its button, `width` points wide, their gaps at `tightness` (1: as the
// mockup spaces them; 0: closest): the top ones (the heading, the closing line, the missions) and
// the bottom ones (the stats, the note), which keep to the button.
fn panel_parts(context: &mut GameUiContext, scales: Scales, summary: &Summary, width: f32, tightness: f32) -> (Vec<UiWidgetImpl>, Vec<UiWidgetImpl>) {
    let color = skin::intermission_color;
    let text = color("text", "ink");
    let sub = color("sub", "sub");
    let accent = color("accent", "accent");
    let gap = |index: usize| spacer(scales.pt(GAPS[index]) * tightness);
    let mut parts: Vec<UiWidgetImpl> = Vec::new();

    let kicker_font = scales.heading_font(KICKER_SIZE);
    let kicker = text_line(&summary.kicker, "kicker", kicker_font, accent, width, UiTextAlign::Left)
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
        // In italics, as the mockup sets it (egui slants the realm font).
        let place = UiRichText::from_spans([UiTextSpan::emphasized(place.clone())]).with_label("place").with_font_scale(font).with_width(width).with_text_color(sub);
        parts.push(tight(context, place, font, PLACE_SIZE, scales).into());
    }
    parts.push(gap(2).into());
    parts.push(skin::separator(context, "rule").into());

    if let Some(closing) = &summary.closing {
        parts.push(gap(3).into());
        let font = scales.font(CLOSING_SIZE);
        let closing = UiRichText::from_spans([UiTextSpan::emphasized(closing.clone())])
            .with_label("closing")
            .with_font_scale(font)
            .with_width(width - scales.pt(48.0))
            .with_align(UiTextAlign::Center)
            .with_text_color(accent);
        parts.push(UiWidgetGroup::new().with_widget(tight(context, closing, font, CLOSING_SIZE, scales)).into());
    }

    if !summary.missions.is_empty() {
        parts.push(gap(4).into());
        let font = scales.heading_font(SECTION_SIZE);
        let title = text_line(&ui::text::get(UiTextCategory::Menu, "campaign_complete.missions").to_uppercase(), "missions_title", font, color("section", "accent"), width, UiTextAlign::Left)
            .with_heading_font()
            .with_letter_spacing(scales.pt(SECTION_LETTER_SPACING));
        parts.push(tight(context, title, font, SECTION_SIZE, scales).into());
        parts.push(gap(5).into());
        parts.push(missions(context, scales, summary, width, tightness).into());
    }

    let mut bottom: Vec<UiWidgetImpl> = Vec::new();
    if !summary.stats.is_empty() {
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
                max_lines: Some([tight_row(STAT_LABEL_SIZE, scales), tight_row(STAT_VALUE_SIZE, scales)]),
            });
        for (label, value) in &summary.stats {
            stats = stats.with_pair(label.clone(), value.clone());
        }
        bottom.push(stats.into());
    }

    if let Some((realm, next)) = &summary.next {
        bottom.push(gap(6).into());
        bottom.push(note_box(context, scales, *realm, next, width).into());
    }
    bottom.push(gap(7).into());
    (parts, bottom)
}

// The missions, five to a column (two columns at most): each its number and name.
fn missions(context: &GameUiContext, scales: Scales, summary: &Summary, width: f32, tightness: f32) -> UiColumns {
    let color = skin::intermission_color;
    let font = scales.font(MISSION_SIZE);
    let line = context.calc_text_size(font, "Ag").y;
    let mut look = skin::row_look();
    look.label_color = Some(color("text", "ink"));
    look.rule_color = None;
    look.padding = ((scales.pt(MISSION_ROW) - line) * 0.5 * (0.5 + 0.5 * tightness)).max(scales.pt(1.0));

    let count = summary.missions.len();
    let per_column = if count > MISSIONS_PER_COLUMN * 2 { count.div_ceil(2) } else { MISSIONS_PER_COLUMN };
    let chunks: Vec<_> = summary.missions.chunks(per_column).collect();
    let gap = scales.pt(MISSION_COLUMN_GAP);
    let column_width = ((width - gap) / 2.0).floor();

    let mut columns = UiColumns::new().with_spacing(gap).with_width(width);
    for index in 0..2 {
        let Some(chunk) = chunks.get(index) else {
            columns.add_column(UiColumnWidth::Fixed(column_width), UiWidgetGroup::new());
            continue;
        };
        let label = if index == 0 { "missions".to_string() } else { format!("missions_{}", index + 1) };
        let mut table = skin::table()
            .with_label(label)
            .with_look(look)
            .with_width(column_width)
            .with_font_scale(font)
            .with_column_spacing(0.0)
            .with_header(UiTableHeader { hidden: true, ..UiTableHeader::default() })
            .with_column(UiTableColumn::new("", UiColumnWidth::Fixed(scales.pt(MISSION_NUMBER_WIDTH)), UiColumnAlign::Left).with_color(color("sub", "sub")))
            .with_column(UiTableColumn::new("", UiColumnWidth::Fraction(1.0), UiColumnAlign::Left));
        for (number, name) in chunk.iter() {
            table = table.with_row(UiTableRow::new([number.clone(), name.clone()]));
        }
        columns.add_column(UiColumnWidth::Fixed(column_width), left_group().with_widget(table));
    }
    columns
}

// "Campaign II is open: ...": a dashed box with the next campaign's emblem in a ring (a star when
// there's none to come).
fn note_box(context: &mut GameUiContext, scales: Scales, realm: Option<Realm>, next: &str, width: f32) -> UiFrame {
    let color = skin::intermission_color;
    let border = color("note_border", "accent");
    let font = scales.font(NOTE_SIZE);
    let icon_width = scales.pt(NOTE_EMBLEM[2]);
    let text = text_line(next, "next", font, color("text", "ink"), width - icon_width - scales.pt(12.0), UiTextAlign::Left);
    let text_height = text.measure(context).y;
    let height = scales.pt(NOTE_HEIGHT).max(text_height + scales.pt(12.0)).ceil();

    let emblem = realm.and_then(|realm| skin::emblem(context, realm, EmblemState::Idle));
    let k = scales.size;
    let icon = UiCanvas::new(Vec2::new(icon_width, height), move |_, painter, rect| {
        let centre = Vec2::new(rect.min.x + NOTE_EMBLEM[0] * k, rect.center().y);
        let radius = NOTE_EMBLEM[1] * k;
        painter.circle_stroke(centre, radius, (1.6 * k).max(1.0), border);
        match emblem {
            Some((texture, size)) => {
                // The emblem fitted inside the ring.
                let fit = radius * 1.3 / size.x.max(size.y).max(1.0);
                let size = size * fit;
                painter.image(texture, Rect::from_pos_and_size(centre - size * 0.5, size), Color::white());
            }
            None => {
                // A five-pointed star, as the generator draws one for the last campaign.
                let points: Vec<Vec2> = (0..=10)
                    .map(|index| {
                        let angle = std::f32::consts::PI * (index as f32 / 5.0 - 0.5);
                        let reach = if index % 2 == 0 { radius * 0.62 } else { radius * 0.26 };
                        centre + Vec2::new(angle.cos(), angle.sin()) * reach
                    })
                    .collect();
                for pair in points.windows(2) {
                    painter.line(pair[0], pair[1], (1.6 * k).max(1.0), border);
                }
            }
        }
    })
    .with_label("icon", None);

    // The text centred on the box's height.
    let drop = ((height - text_height) * 0.5).floor();
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

// Watch the epilogue at the left, when the campaign has one (hidden otherwise), and Return to the
// menu at the right.
fn buttons(context: &mut GameUiContext, scales: Scales, width: f32) -> UiColumns {
    let back = skin::button(context, ButtonRole::Primary, UiString::key(UiTextCategory::Menu, "campaign_complete.return"), |_, context| {
        CampaignComplete::on_return(context_as_mut::<GameUiContext>(context));
    })
    .with_width(UiTextButtonWidth::Fixed(scales.pt(RETURN_WIDTH)))
    .with_min_height(scales.pt(BUTTON_HEIGHT));
    let mut columns = UiColumns::new().with_spacing(0.0).with_width(width);
    if epilogue_campaign().is_some() {
        let watch = skin::button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "campaign_complete.epilogue"), |_, context| {
            CampaignComplete::on_watch_epilogue(context_as_mut::<GameUiContext>(context));
        })
        .with_width(UiTextButtonWidth::Fixed(scales.pt(EPILOGUE_WIDTH)))
        .with_min_height(scales.pt(BUTTON_HEIGHT));
        columns.add_column(UiColumnWidth::Fixed(scales.pt(EPILOGUE_WIDTH)), left_group().with_widget(scales.button_text(watch, BUTTON_TEXT_SIZE)));
    }
    columns.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());
    columns.add_column(UiColumnWidth::Fixed(scales.pt(RETURN_WIDTH)), left_group().with_widget(scales.button_text(back, BUTTON_TEXT_SIZE)));
    columns
}
