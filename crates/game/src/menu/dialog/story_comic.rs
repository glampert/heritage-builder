// The comic viewer (docs/plans/realm-ui.md, Phase 4: Viewer; intro-<realm>.jpg): a story sequence's
// panels (crate::story), one at a time, full screen, laid out as the mockup generator's screens.py
// intro() draws it on its 1920 × 1080 screen (screen::Scales):
// - the panel's picture in the realm frame (skin::story's `frame`), in sepia for a flashback;
// - its narration on the skin's caption panel, where the panel's `narration_anchor` or the skin
//   puts it; its speech bubbles painted (a rounded box and its tail, by kind: speech, a whisper's
//   dashed outline, a group's stacked boxes, bold ink); a dynamic panel's list of the player's
//   stele lines (crate::campaign::stele);
// - the bottom bar on the skin's `bar` panel: the campaign line, the sequence's title, a dot per
//   panel (done, current, to come), "Panel N of M", Back, Skip and Next (Continue on the last).
//
// Keys (dialog::handle_key, from the menus): Space, Enter and Right go forward, Left goes back,
// Escape skips (the menus close the top dialog: closing ends the sequence). The simulation stays
// paused while it plays, as under every dialog.
//
// What plays is asked for with `play` (StoryRequest: the campaign, the sequence, the panel to start
// on, whether the profile records it as seen once it ends, and what runs once it's closed). It's
// kept outside the widgets (STATE): the dialog is built again for each panel and when the window's
// size changes. `dialog.open StoryComic` without a request plays the first sequence of the
// campaign the world map would show (world_map::default_campaign). Ending it (Skip, Escape,
// Continue on the last panel) closes it, records the sequence seen if asked (Skip counts too) and
// queues its `on_end`, which dialog::sync runs at the start of the next frame, told how it ended
// (StoryEnd: Continue on the last panel, or skipped). While an `on_end` follows, the simulation
// stays paused (what follows decides: the briefing, the world map, the next mission).
//
// The campaign flow plays a mission's sequence before its briefing (dialog::open_mission_start) and
// after its debrief (MissionComplete's Continue), and the campaign-complete screen its epilogue,
// each once per profile; the home menu's Replay the story, the briefing's Replay the story and the
// campaign-complete screen's Watch the epilogue play them again.
//
// Paths: StoryComic/panel (kind canvas: campaign, sequence, title, panel, panels, image, narration,
// bubbles (speaker, text, kind), sepia, stele_lines); StoryComic/bar/campaign, title, dots (kind
// canvas: current, count), count, Back, Skip, Next or Continue.

use common::{Color, Rect};
use engine::{
    app::input::InputKey,
    render::texture::TextureHandle,
    ui::{UiLabelLook, UiNineSlice, UiPainter, UiTextAnchor},
};
use serde_json::{Value, json};

use super::*;
use super::screen::{BuiltOnOpen, Scales, left_group, spacer, text_line, tight};
use crate::{
    campaign::{config::CampaignConfigs, stele as stele_data},
    menu::skin::{SkinMapTextDef, SkinPanel, SkinStoryBubbleDef},
    profile,
    story::{BubbleKind, PANEL_SIZE, PanelDef, StoryConfigs},
};

// ----------------------------------------------
// Layout (points on the generator's 1920 × 1080 screen, scaled by the window: screen::Scales)
// ----------------------------------------------

// The panel's top left; its size is story::PANEL_SIZE.
const PANEL_AT: Vec2 = Vec2::new(60.0, 56.0);
// What the panel's canvas covers, down to the bottom bar.
const CANVAS_HEIGHT: f32 = 958.0;
const BAR: [f32; 4] = [40.0, 958.0, 1840.0, 96.0];

// The bar's parts, in screen points: the texts' left edge; the dots' centre, row and spacing; the
// count's baseline; the buttons (x, width) and their height.
const BAR_TEXT_X: f32 = 80.0;
const DOTS_CENTRE: f32 = 960.0;
const DOT_SPACING: f32 = 34.0;
// The dots' and the count's column, centred on DOTS_CENTRE and ending where Back starts.
const DOTS_WIDTH: f32 = 320.0;
const BACK_BUTTON: [f32; 2] = [1120.0, 160.0];
const SKIP_BUTTON: [f32; 2] = [1300.0, 220.0];
const NEXT_BUTTON: [f32; 2] = [1540.0, 300.0];
const BUTTON_HEIGHT: f32 = 56.0;
// The bar is laid out as fixed columns, the gaps between these parts among them: a negative gap
// would be clamped to 0 and push everything after it right.
const _: () = assert!(
    BAR_TEXT_X >= BAR[0]
        && DOTS_CENTRE - DOTS_WIDTH * 0.5 >= BAR_TEXT_X
        && BACK_BUTTON[0] >= DOTS_CENTRE + DOTS_WIDTH * 0.5
        && SKIP_BUTTON[0] >= BACK_BUTTON[0] + BACK_BUTTON[1]
        && NEXT_BUTTON[0] >= SKIP_BUTTON[0] + SKIP_BUTTON[1]
        && BAR[0] + BAR[2] >= NEXT_BUTTON[0] + NEXT_BUTTON[1]
);

// Text sizes (the generator's font sizes).
const CAMPAIGN_SIZE: f32 = 18.0;
const CAMPAIGN_LETTER_SPACING: f32 = 3.0;
const TITLE_SIZE: f32 = 32.0;
const COUNT_SIZE: f32 = 18.0;
const BUTTON_TEXT_SIZE: f32 = 22.0;
// The gap between the campaign line and the title, and between the dots and the count.
const BAR_GAP: f32 = 4.0;
const DOTS_HEIGHT: f32 = 22.0;

// The dots: the current one's radius and ring, the others' radius and ring.
const DOT_CURRENT: [f32; 2] = [9.0, 2.0];
const DOT_OTHER: [f32; 2] = [6.0, 1.5];

// A dynamic panel's stele lines: their box's width and top (panel points), its padding, the gap
// between lines, and between the title and the first line.
const STELE_BOX: [f32; 2] = [1080.0, 150.0];
const STELE_PADDING: f32 = 44.0;
const STELE_LINE_GAP: f32 = 14.0;
const STELE_TITLE_GAP: f32 = 26.0;

// ----------------------------------------------
// State
// ----------------------------------------------

// What to play: the sequence (its campaign by index in campaigns.json, its id), the panel to start
// on (from 0), whether the profile records it as seen once it ends, and what runs once it has
// closed (at the start of the next frame: dialog::sync), e.g. the briefing it comes before.
pub struct StoryRequest {
    pub campaign_index: usize,
    pub sequence_id: String,
    pub panel: usize,
    pub records_seen: bool,
    pub on_end: Option<StoryOnEnd>,
}

// How a sequence ended: Continue on its last panel (Finished), or Skip or Escape (Skipped).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum StoryEnd {
    Finished,
    Skipped,
}

// What runs once a sequence has ended and its viewer closed.
pub type StoryOnEnd = fn(&mut GameUiContext, StoryEnd);

struct State {
    built: BuiltOnOpen,
    request: Option<StoryRequest>, // Asked for, not open yet.
    playing: Option<StoryRequest>, // Open: its `panel` is the one showing.
    built_panel: Option<usize>,    // The panel the open viewer was built for.
    reached_end: bool,             // Closing now ends it past its last panel (not a skip).
    finished: Option<(StoryOnEnd, StoryEnd)>,
}

static STATE: UiStaticVar<State> = UiStaticVar::new(State {
    built: BuiltOnOpen::new(),
    request: None,
    playing: None,
    built_panel: None,
    reached_end: false,
    finished: None,
});

// Asks for `request` to play: dialog::play_story opens the viewer with it.
pub(super) fn request(request: StoryRequest) {
    STATE.as_mut().request = Some(request);
}

// Sets the viewer up for opening (dialog::open_dialog builds it afresh then): the sequence asked
// for, else the first of the campaign the world map would show.
pub(super) fn prepare() {
    let state = STATE.as_mut();
    state.built.prepare();
    state.built_panel = None;
    state.playing = state.request.take().or_else(default_request).filter(|request| sequence_panels(request).is_some());
    if let Some(playing) = &mut state.playing {
        playing.panel = playing.panel.min(sequence_panels(playing).unwrap_or(1).saturating_sub(1));
    }
}

fn default_request() -> Option<StoryRequest> {
    let campaign_index = crate::world_map::default_campaign();
    let sequences = StoryConfigs::get().sequences_in_order(CampaignConfigs::get(), campaign_index);
    let sequence_id = sequences.first()?.sequence.id.clone();
    Some(StoryRequest { campaign_index, sequence_id, panel: 0, records_seen: false, on_end: None })
}

// How many panels the request's sequence has. None: no such sequence.
fn sequence_panels(request: &StoryRequest) -> Option<usize> {
    let campaign = CampaignConfigs::get().campaign(request.campaign_index)?;
    let count = StoryConfigs::get().stories.get(&campaign.id)?.sequence(&request.sequence_id)?.panels.len();
    (count > 0).then_some(count)
}

// Whether the open viewer must be built again: another panel shows, or the window's size changed.
pub(super) fn needs_rebuild(context: &GameUiContext) -> bool {
    let state = STATE.as_ref();
    state.built.needs_rebuild(context) || state.built_panel != state.playing.as_ref().map(|playing| playing.panel)
}

// What runs now that a sequence has ended and its viewer closed (dialog::sync), and how it ended.
pub(super) fn take_finished() -> Option<(StoryOnEnd, StoryEnd)> {
    STATE.as_mut().finished.take()
}

// Whether something waits for a sequence that has just ended (its `on_end`, not run yet).
pub(super) fn is_ending() -> bool {
    STATE.as_ref().finished.is_some()
}

// Drops the sequence playing, and what follows one that has just ended, without ending it: closing
// the viewer then records nothing and runs no `on_end` (dialog::stop_story).
pub(super) fn drop_playing() {
    let state = STATE.as_mut();
    state.playing = None;
    state.reached_end = false;
    state.finished = None;
}

// The sequence showing: its campaign's index, its id and the panel (from 0). None: none.
pub fn playing() -> Option<(usize, String, usize)> {
    STATE.as_ref().playing.as_ref().map(|playing| (playing.campaign_index, playing.sequence_id.clone(), playing.panel))
}

// The viewer's keys: Space, Enter and Right go forward, Left goes back (Escape is the menus':
// closing the viewer skips the sequence). True if it took the key.
pub(super) fn on_key(context: &mut GameUiContext, key: InputKey) -> bool {
    match key {
        InputKey::Space | InputKey::Enter | InputKey::KpEnter | InputKey::Right => {
            step(context, 1);
            true
        }
        InputKey::Left => {
            step(context, -1);
            true
        }
        _ => false,
    }
}

// Shows the panel `delta` away; past the last one, the sequence ends. Not before the first.
fn step(context: &mut GameUiContext, delta: i32) {
    let Some(playing) = STATE.as_mut().playing.as_mut() else {
        return;
    };
    let count = sequence_panels(playing).unwrap_or(1);
    let next = playing.panel as i64 + i64::from(delta);
    if next >= count as i64 {
        STATE.as_mut().reached_end = true;
        super::close(DialogMenuKind::StoryComic, context);
        STATE.as_mut().reached_end = false;
    } else {
        playing.panel = next.max(0) as usize;
    }
}

// ----------------------------------------------
// StoryComic
// ----------------------------------------------

pub struct StoryComic {
    menu: UiMenuRcMut,
}

impl DialogMenuFactory for StoryComic {
    const KIND: DialogMenuKind = DialogMenuKind::StoryComic;
    const TITLE: &'static [&'static str] = &["Story"];

    fn create(context: &mut GameUiContext) -> DialogMenuImpl {
        DialogMenuImpl::from(StoryComic::new(context))
    }
}

impl DialogMenu for StoryComic {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> DialogMenuKind {
        Self::KIND
    }

    fn menu(&self) -> &UiMenuRcMut {
        &self.menu
    }

    // Only with a sequence to play.
    fn open(&mut self, context: &mut GameUiContext) -> bool {
        if STATE.as_ref().playing.is_none() {
            STATE.as_mut().built.active = false;
            return false;
        }
        self.menu_mut().open(context);
        true
    }

    // Ends the sequence (Skip, Escape, Continue on its last panel): the profile records it seen if
    // asked, and its `on_end` runs at the start of the next frame. The simulation stays paused
    // until then: closing the viewer resumes it (PauseSimIfOpen), and a key that ends it is handled
    // before the frame's simulation step (the briefing that follows must find it unstarted).
    fn close(&mut self, context: &mut GameUiContext) -> bool {
        self.menu_mut().close(context);
        let state = STATE.as_mut();
        state.built.active = false;
        state.built_panel = None;
        if let Some(playing) = state.playing.take() {
            if playing.records_seen {
                profile::on_sequence_seen(playing.campaign_index, &playing.sequence_id);
            }
            let end = if state.reached_end { StoryEnd::Finished } else { StoryEnd::Skipped };
            state.finished = playing.on_end.map(|on_end| (on_end, end));
            if state.finished.is_some() {
                context.sim.pause();
            }
        }
        true
    }

    // A session switch drops it without ending it: it isn't recorded as seen (a mission started
    // again plays its sequence again), and neither its `on_end` nor that of a sequence that ended
    // this frame, or one asked for and not open yet, runs in the new session: what was to follow
    // belonged to the session left (its briefing, its debrief's way on).
    fn reset(&mut self) {
        let menu = self.menu_mut();
        menu.reset_message_box();
        menu.set_flags(UiMenuFlags::IsOpen, false);
        let state = STATE.as_mut();
        state.built.active = false;
        state.request = None;
        state.playing = None;
        state.built_panel = None;
        state.reached_end = false;
        state.finished = None;
    }
}

// What the viewer shows of the panel playing, read when it's built.
struct Shown {
    campaign_line: String,
    title: String,
    panel: usize,
    count: usize,
    value: Value,
    art: PanelArt,
}

impl StoryComic {
    pub fn new(context: &mut GameUiContext) -> Self {
        let kind = Self::KIND.to_string();
        let window = ui::screen_size(context.ui_sys);
        let scales = Scales::new(window);
        let flags = DialogFrame::from_skin(Self::KIND).menu_flags();
        let mut menu = UiMenu::new(kind).with_flags(flags).with_size(window).with_content_insets(UiMenuInsets::same(0.0)).with_widget_spacing(Vec2::zero());

        // Built at boot and on every rebuild for every dialog kind, but shown only once prepared
        // for opening: until then an empty menu, so its textures load only when needed.
        if !STATE.as_ref().built.active {
            return Self { menu: menu.into_rc() };
        }
        let Some(shown) = read(context) else {
            return Self { menu: menu.into_rc() };
        };

        let art = skin::story();
        if !art.backdrop.is_empty() {
            menu = menu.with_background(context, PathRef::from_str(&art.backdrop));
        }

        let bar = bar(context, scales, &shown);
        let panel = shown.panel;
        menu.add_widget(spacer(scales.origin.y));
        menu.add_widget(super::screen::row(scales, 0.0, panel_canvas(scales, shown.art, shown.value)));
        menu.add_widget(super::screen::row(scales, BAR[0], bar));

        let state = STATE.as_mut();
        state.built.built(context);
        state.built_panel = Some(panel);
        Self { menu: menu.into_rc() }
    }
}

// The panel playing, as the viewer shows it. None: nothing plays.
fn read(context: &mut GameUiContext) -> Option<Shown> {
    let configs = CampaignConfigs::get();
    let (campaign_index, sequence_id, panel_index) = playing()?;
    let campaign = configs.campaign(campaign_index)?;
    let sequence = StoryConfigs::get().stories.get(&campaign.id)?.sequence(&sequence_id)?;
    let panel = sequence.panels.get(panel_index)?;

    // "CAMPAIGN I · THE DRAGON LEGACY"; a hidden campaign has no number, only its name.
    let campaign_line = match configs.campaign_number(campaign_index) {
        Some(_) => ui::text::format(
            UiTextCategory::Menu,
            "story.campaign",
            &[("campaign", &super::home::campaign_number(campaign_index)), ("name", campaign.display_name())],
        ),
        None => campaign.display_name().to_string(),
    }
    .to_uppercase();
    let title = ui::text::get(UiTextCategory::Story, &sequence.title).to_string();

    let stele = panel.stele_lines.then(|| {
        // A preview's as its other screens show it (campaign::preview::shown_profile).
        let stele = stele_data::stele(configs, crate::campaign::preview::shown_profile(), campaign_index, None);
        let title = stele.as_ref().map_or_else(|| campaign.display_name().to_string(), |stele| stele.title.clone());
        let lines: Vec<String> = stele.map_or_else(Vec::new, |stele| stele.lines.into_iter().map(|line| line.text).collect());
        (title, lines)
    });
    let art = PanelArt::load(context, panel, stele.clone());

    let value = json!({
        "campaign": campaign.id,
        "sequence": sequence.id,
        "title": title,
        "panel": panel_index + 1,
        "panels": sequence.panels.len(),
        "image": panel.image,
        "narration": art.caption.as_ref().map(|(text, _)| text.clone()),
        "bubbles": art.bubbles.iter().map(|bubble| json!({
            "speaker": bubble.speaker,
            "text": bubble.text,
            "kind": bubble.kind.to_string(),
        })).collect::<Vec<_>>(),
        "sepia": panel.sepia,
        "stele_lines": stele.map(|(title, lines)| json!({ "title": title, "lines": lines })),
    });

    Some(Shown { campaign_line, title, panel: panel_index, count: sequence.panels.len(), value, art })
}

// ----------------------------------------------
// The panel
// ----------------------------------------------

// A bubble as drawn: its text looked up.
struct BubbleArt {
    speaker: Option<String>,
    text: String,
    kind: BubbleKind,
    anchor: Vec2,
    width: Option<f32>,
    tail: Vec2,
}

// What the panel's canvas paints, ready at build time: textures, the caption's 9-slice, the texts.
struct PanelArt {
    image: Option<TextureHandle>,
    frame: Option<TextureHandle>,
    caption_panel: Option<UiNineSlice>,
    caption: Option<(String, Vec2)>, // The narration and its box's top left, in panel points.
    bubbles: Vec<BubbleArt>,
    sepia: bool,
    stele: Option<(String, Vec<String>)>,
}

impl PanelArt {
    fn load(context: &mut GameUiContext, panel: &PanelDef, stele: Option<(String, Vec<String>)>) -> Self {
        let art = skin::story();
        let image = (!panel.image.is_empty()).then(|| context.load_texture(PathRef::from_str(&panel.image)));
        let frame = (!art.frame.is_empty()).then(|| context.load_texture(PathRef::from_str(&art.frame)));
        let caption_panel = match skin::panel(context, &art.caption.panel) {
            SkinPanel::NineSlice { slice, .. } => Some(slice),
            SkinPanel::Sprite { .. } => None,
        };
        let caption = panel.narration.as_deref().map(|key| {
            let at = panel.narration_anchor.unwrap_or(art.caption.at);
            (ui::text::get(UiTextCategory::Story, key).to_string(), Vec2::new(at[0], at[1]))
        });
        let bubbles = panel
            .bubbles
            .iter()
            .map(|bubble| BubbleArt {
                speaker: bubble.speaker.as_deref().map(|key| ui::text::get(UiTextCategory::Realm, key).to_string()),
                text: ui::text::get(UiTextCategory::Story, &bubble.text).to_string(),
                kind: bubble.kind,
                anchor: Vec2::new(bubble.anchor[0], bubble.anchor[1]),
                width: bubble.width,
                tail: Vec2::new(bubble.tail[0], bubble.tail[1]),
            })
            .collect();
        Self { image, frame, caption_panel, caption, bubbles, sepia: panel.sepia, stele }
    }
}

// The panel, its frame, caption and bubbles, over the screen down to the bar.
fn panel_canvas(scales: Scales, art: PanelArt, value: Value) -> UiCanvas {
    let k = scales.size;
    UiCanvas::new(Vec2::new(scales.pt(super::screen::REFERENCE.x), scales.pt(CANVAS_HEIGHT)), move |_, painter, rect| {
        let story = skin::story();
        let panel = Rect::from_pos_and_size(rect.min + PANEL_AT * k, Vec2::new(PANEL_SIZE[0], PANEL_SIZE[1]) * k);
        let at = |point: Vec2| panel.min + point * k;

        // The picture, in sepia for a flashback.
        if let Some(image) = art.image {
            let tint = if art.sepia { skin::color(&story.sepia_tint).unwrap_or(Color::white()) } else { Color::white() };
            painter.image(image, panel, tint);
            if art.sepia
                && let Some(wash) = skin::color(&story.sepia_wash)
            {
                painter.rect_filled(panel, wash, 0.0);
            }
        }

        // The frame round it, its ornaments over the picture's edges.
        if let Some(frame) = art.frame {
            let [left, top, right, bottom] = story.frame_bleed;
            let frame_rect = Rect::from_extents(panel.min - Vec2::new(left, top) * k, panel.max + Vec2::new(right, bottom) * k);
            painter.image(frame, frame_rect, Color::white());
        }

        if let Some((title, lines)) = &art.stele {
            paint_stele_lines(painter, panel, k, art.caption_panel.as_ref(), title, lines);
        }
        if let Some((text, anchor)) = &art.caption {
            paint_caption(painter, at(*anchor), k, art.caption_panel.as_ref(), text);
        }
        for bubble in &art.bubbles {
            paint_bubble(painter, bubble, &at, k);
        }
    })
    .with_label("panel", Some(value))
}

// A label look of the skin's `def` at the screen's scale, `factor` times as large.
fn look(def: &SkinMapTextDef, k: f32, factor: f32) -> UiLabelLook {
    skin::label_look(def, def.size * factor, k)
}

fn cased(def: &SkinMapTextDef, text: &str) -> String {
    if def.capitals { text.to_uppercase() } else { text.to_string() }
}

// The narration's box, its top left at `top_left`.
fn paint_caption(painter: &UiPainter, top_left: Vec2, k: f32, panel: Option<&UiNineSlice>, text: &str) {
    let caption = &skin::story().caption;
    let look = look(&caption.look, k, 1.0);
    let lines = painter.wrap_label(&cased(&caption.look, text), &look, (caption.width - caption.inset * 2.0) * k);
    let height = caption.baseline + caption.line_height * lines.len().saturating_sub(1) as f32 + caption.bottom;
    let rect = Rect::from_pos_and_size(top_left, Vec2::new(caption.width, height) * k);
    paint_box(painter, rect, k, panel);
    for (index, line) in lines.iter().enumerate() {
        let baseline = top_left + Vec2::new(caption.inset, caption.baseline + caption.line_height * index as f32) * k;
        painter.label(baseline, UiTextAnchor::Start, line, &look);
    }
}

// The caption panel over `rect` at the screen's scale, else a plain box in the bubble's colours.
fn paint_box(painter: &UiPainter, rect: Rect, k: f32, panel: Option<&UiNineSlice>) {
    match panel {
        Some(panel) => {
            let mut panel = *panel;
            panel.scale *= k;
            painter.nine_slice(rect, &panel, Color::white());
        }
        None => {
            let bubble = &skin::story().bubble;
            painter.rect_filled(rect, skin::color(&bubble.fill).unwrap_or(Color::white()), 0.0);
            painter.rect_stroke(rect, 0.0, bubble.stroke * k, skin::color(&bubble.ink).unwrap_or(Color::black()));
        }
    }
}

// A dynamic panel's list of the player's stele lines (the epilogue's): the stele's title and its
// lines, on the caption's panel in the panel's middle.
fn paint_stele_lines(painter: &UiPainter, panel: Rect, k: f32, box_panel: Option<&UiNineSlice>, title: &str, lines: &[String]) {
    let story = skin::story();
    let default_look = SkinMapTextDef::default();
    let title_def = story.stele.get("title").unwrap_or(&default_look);
    let line_def = story.stele.get("line").unwrap_or(&default_look);
    let title_look = look(title_def, k, 1.0);
    let line_look = look(line_def, k, 1.0);
    let width = (STELE_BOX[0] - STELE_PADDING * 2.0) * k;

    let title_lines = painter.wrap_label(&cased(title_def, title), &title_look, width);
    let wrapped: Vec<Vec<String>> = lines.iter().map(|line| painter.wrap_label(&cased(line_def, line), &line_look, width - 24.0 * k)).collect();
    let title_height = title_def.size * 1.3 * k;
    let line_height = line_def.size * 1.3 * k;
    let body: f32 = wrapped.iter().map(|lines| lines.len() as f32 * line_height + STELE_LINE_GAP * k).sum();
    let height = STELE_PADDING * 2.0 * k + title_lines.len() as f32 * title_height + STELE_TITLE_GAP * k + body;
    let height = height.min(panel.height() - STELE_BOX[1] * k);
    let rect = Rect::from_pos_and_size(Vec2::new(panel.center().x - STELE_BOX[0] * 0.5 * k, panel.min.y + STELE_BOX[1] * k), Vec2::new(STELE_BOX[0] * k, height));
    paint_box(painter, rect, k, box_panel);

    let mut y = rect.min.y + STELE_PADDING * k + title_def.size * k;
    for line in &title_lines {
        painter.label(Vec2::new(rect.center().x, y), UiTextAnchor::Middle, line, &title_look);
        y += title_height;
    }
    y += STELE_TITLE_GAP * k - title_height + line_height;
    let x = rect.min.x + STELE_PADDING * k;
    for lines in &wrapped {
        if y > rect.max.y - STELE_PADDING * k * 0.5 {
            break;
        }
        painter.label(Vec2::new(x, y), UiTextAnchor::Start, "·", &line_look);
        for line in lines {
            painter.label(Vec2::new(x + 24.0 * k, y), UiTextAnchor::Start, line, &line_look);
            y += line_height;
        }
        y += STELE_LINE_GAP * k;
    }
}

// A speech bubble and its tail, as the generator's speech() draws one, by kind.
fn paint_bubble(painter: &UiPainter, bubble: &BubbleArt, at: &impl Fn(Vec2) -> Vec2, k: f32) {
    let def = &skin::story().bubble;
    let fill = skin::color(&def.fill).unwrap_or(Color::white());
    let ink = skin::color(&def.ink).unwrap_or(Color::black());
    let bold = bubble.kind == BubbleKind::Bold;
    let stroke = def.stroke * k * if bold { def.bold_stroke } else { 1.0 };
    let mut text_look = look(&def.text, k, if bold { def.bold_scale } else { 1.0 });
    if bubble.kind == BubbleKind::Whisper
        && let Some(color) = skin::color(&def.whisper_text)
    {
        text_look.color = color;
    }
    let name_look = look(&def.name, k, 1.0);

    let width = bubble.width.unwrap_or(def.width);
    let lines = painter.wrap_label(&cased(&def.text, &bubble.text), &text_look, (width - def.inset * 2.0) * k);
    let first = if bubble.speaker.is_some() { def.text_baseline } else { def.name_baseline };
    let line_height = def.line_height * if bold { def.bold_scale } else { 1.0 };
    let height = first + line_height * lines.len().saturating_sub(1) as f32 + def.bottom;
    let rect = Rect::from_pos_and_size(at(bubble.anchor), Vec2::new(width, height) * k);
    let radius = (def.radius * k).min(rect.height() * 0.5);
    let tip = at(bubble.tail);
    let dash = (bubble.kind == BubbleKind::Whisper).then_some([def.whisper_dash[0] * k, def.whisper_dash[1] * k]);

    // A group's: a second box behind, up and to the right, as a crowd's stacked voices.
    if bubble.kind == BubbleKind::Group {
        let offset = Vec2::new(def.group_offset, -def.group_offset) * k;
        let behind = rect.translated(offset);
        painter.rect_filled(behind, fill, radius);
        painter.rect_stroke(behind, radius, stroke, ink);
    }

    // The tail from the edge facing its tip, then the box over its base, then the base's line
    // painted out so the tail opens into the box.
    let (base_a, base_b) = tail_base(rect, radius, def, k, tip);
    painter.convex_polygon(&[base_a, tip, base_b], fill, 0.0, ink);
    outline(painter, &[base_a, tip, base_b], stroke, ink, dash, false);
    painter.rect_filled(rect, fill, radius);
    match dash {
        Some(_) => outline(painter, &rounded_rect(rect, radius), stroke, ink, dash, true),
        None => painter.rect_stroke(rect, radius, stroke, ink),
    }
    let inward = Vec2::new(stroke, 0.0) * if base_a.x < base_b.x { 1.0 } else { -1.0 };
    painter.line(base_a + inward, base_b - inward, stroke + 1.5, fill);

    let x = rect.min.x + def.inset * k;
    if let Some(speaker) = &bubble.speaker {
        painter.label(Vec2::new(x, rect.min.y + def.name_baseline * k), UiTextAnchor::Start, &cased(&def.name, speaker), &name_look);
    }
    for (index, line) in lines.iter().enumerate() {
        let y = rect.min.y + (first + line_height * index as f32) * k;
        painter.label(Vec2::new(x, y), UiTextAnchor::Start, line, &text_look);
    }
}

// Where a bubble's tail meets it: on its bottom edge when the tip is below its middle, else on its
// top, centred as near the tip as the rounded corners allow.
fn tail_base(rect: Rect, radius: f32, def: &SkinStoryBubbleDef, k: f32, tip: Vec2) -> (Vec2, Vec2) {
    let half = (def.tail_width * k * 0.5).min((rect.width() * 0.5 - radius).max(1.0));
    let low = rect.min.x + radius + half;
    let high = (rect.max.x - radius - half).max(low);
    let centre = tip.x.clamp(low, high);
    let y = if tip.y >= rect.center().y { rect.max.y } else { rect.min.y };
    (Vec2::new(centre - half, y), Vec2::new(centre + half, y))
}

// A polyline, in dashes when `dash` is given.
fn outline(painter: &UiPainter, points: &[Vec2], width: f32, color: Color, dash: Option<[f32; 2]>, closed: bool) {
    match dash {
        Some([dash, gap]) => {
            let mut points = points.to_vec();
            if closed && let Some(&first) = points.first() {
                points.push(first);
            }
            painter.dashed_polyline(&points, width, color, dash, gap);
        }
        None => painter.polyline(points, width, color, closed),
    }
}

// A rounded rect's outline as points, clockwise from its top left corner's end.
fn rounded_rect(rect: Rect, radius: f32) -> Vec<Vec2> {
    const STEPS: usize = 8;
    let corners = [
        (Vec2::new(rect.max.x - radius, rect.min.y + radius), -90.0f32),
        (Vec2::new(rect.max.x - radius, rect.max.y - radius), 0.0),
        (Vec2::new(rect.min.x + radius, rect.max.y - radius), 90.0),
        (Vec2::new(rect.min.x + radius, rect.min.y + radius), 180.0),
    ];
    let mut points = Vec::with_capacity(4 * (STEPS + 1));
    for (centre, start) in corners {
        for step in 0..=STEPS {
            let angle = (start + 90.0 * step as f32 / STEPS as f32).to_radians();
            points.push(centre + Vec2::new(angle.cos(), angle.sin()) * radius);
        }
    }
    points
}

// ----------------------------------------------
// The bottom bar
// ----------------------------------------------

fn bar(context: &mut GameUiContext, scales: Scales, shown: &Shown) -> UiFrame {
    let bar_text = skin::story_color("bar_text", "light");
    let bar_sub = skin::story_color("bar_sub", "light");
    let [bar_x, _, bar_width, bar_height] = BAR;

    // The campaign line over the sequence's title, at the left.
    let campaign_font = scales.heading_font(CAMPAIGN_SIZE);
    let title_font = scales.heading_font(TITLE_SIZE);
    let text_width = scales.pt(DOTS_CENTRE - DOTS_WIDTH * 0.5 - BAR_TEXT_X - 10.0);
    let campaign = text_line(&shown.campaign_line, "campaign", campaign_font, bar_sub, text_width, UiTextAlign::Left)
        .with_heading_font()
        .with_letter_spacing(scales.pt(CAMPAIGN_LETTER_SPACING));
    let title = text_line(&shown.title, "title", title_font, bar_text, text_width, UiTextAlign::Left).with_heading_font();
    let titles = left_group()
        .with_center_vertically(true)
        .with_widget_spacing(Vec2::new(0.0, scales.pt(BAR_GAP)))
        .with_widget(tight(context, campaign, campaign_font, CAMPAIGN_SIZE, scales))
        .with_widget(tight(context, title, title_font, TITLE_SIZE, scales));

    // The dots and the count, in the middle.
    let count_font = scales.font(COUNT_SIZE);
    let count_text = ui::text::format(
        UiTextCategory::Menu,
        "story.count",
        &[("panel", &(shown.panel + 1).to_string()), ("count", &shown.count.to_string())],
    );
    let count = text_line(&count_text, "count", count_font, bar_sub, scales.pt(DOTS_WIDTH), UiTextAlign::Center);
    let middle = UiWidgetGroup::new()
        .with_center_vertically(true)
        .with_widget_spacing(Vec2::new(0.0, scales.pt(BAR_GAP)))
        .with_widget(dots(scales, shown.panel, shown.count))
        .with_widget(tight(context, count, count_font, COUNT_SIZE, scales));

    // Back, Skip and Next (Continue on the last panel).
    let last = shown.panel + 1 >= shown.count;
    let back = skin::button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "story.back"), |_, context| {
        step(context_as_mut::<GameUiContext>(context), -1);
    })
    .with_enabled(shown.panel > 0);
    let skip = skin::button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "story.skip"), |_, context| {
        super::close(DialogMenuKind::StoryComic, context_as_mut::<GameUiContext>(context));
    });
    let next_key = if last { "story.continue" } else { "story.next" };
    let next = skin::button(context, ButtonRole::Primary, UiString::key(UiTextCategory::Menu, next_key), |_, context| {
        step(context_as_mut::<GameUiContext>(context), 1);
    });
    let sized = |button: UiTextButton, width: f32| {
        let button = button.with_width(UiTextButtonWidth::Fixed(scales.pt(width))).with_min_height(scales.pt(BUTTON_HEIGHT));
        left_group().with_center_vertically(true).with_widget(scales.button_text(button, BUTTON_TEXT_SIZE))
    };

    let mut columns = UiColumns::new().with_spacing(0.0);
    columns.add_column(UiColumnWidth::Fixed(scales.pt(BAR_TEXT_X - bar_x)), UiWidgetGroup::new());
    columns.add_column(UiColumnWidth::Fixed(scales.pt(DOTS_CENTRE - DOTS_WIDTH * 0.5 - BAR_TEXT_X)), titles);
    columns.add_column(UiColumnWidth::Fixed(scales.pt(DOTS_WIDTH)), middle);
    columns.add_column(UiColumnWidth::Fixed(scales.pt(BACK_BUTTON[0] - DOTS_CENTRE - DOTS_WIDTH * 0.5)), UiWidgetGroup::new());
    columns.add_column(UiColumnWidth::Fixed(scales.pt(BACK_BUTTON[1])), sized(back, BACK_BUTTON[1]));
    columns.add_column(UiColumnWidth::Fixed(scales.pt(SKIP_BUTTON[0] - BACK_BUTTON[0] - BACK_BUTTON[1])), UiWidgetGroup::new());
    columns.add_column(UiColumnWidth::Fixed(scales.pt(SKIP_BUTTON[1])), sized(skip, SKIP_BUTTON[1]));
    columns.add_column(UiColumnWidth::Fixed(scales.pt(NEXT_BUTTON[0] - SKIP_BUTTON[0] - SKIP_BUTTON[1])), UiWidgetGroup::new());
    columns.add_column(UiColumnWidth::Fixed(scales.pt(NEXT_BUTTON[1])), sized(next, NEXT_BUTTON[1]));
    columns.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());

    let mut frame = UiFrame::new(UiWidgetGroup::new().with_center_vertically(true).with_widget(columns), Vec2::new(scales.pt(bar_width), scales.pt(bar_height)))
        .with_label("bar");
    if let SkinPanel::NineSlice { slice, .. } = skin::panel(context, "bar") {
        frame = frame.with_panel(slice);
    }
    frame
}

// A dot per panel: the done ones filled, the current one larger in the accent with a ring, the
// ones to come as rings.
fn dots(scales: Scales, current: usize, count: usize) -> UiCanvas {
    let k = scales.size;
    let dot = skin::story_color("dot", "accent");
    let ring = skin::story_color("dot_ring", "light");
    let done = skin::story_color("dot_done", "light");
    let line = skin::story_color("dot_line", "light");
    UiCanvas::new(Vec2::new(scales.pt(DOTS_WIDTH), scales.pt(DOTS_HEIGHT)), move |_, painter, rect| {
        let centre = rect.center();
        // Closer together where many wouldn't fit.
        let spacing = (DOT_SPACING * k).min(rect.width() / count.max(1) as f32);
        let first = centre.x - spacing * (count.saturating_sub(1)) as f32 * 0.5;
        for index in 0..count {
            let at = Vec2::new(first + spacing * index as f32, centre.y);
            if index == current {
                painter.circle_filled(at, DOT_CURRENT[0] * k, dot);
                painter.circle_stroke(at, DOT_CURRENT[0] * k, (DOT_CURRENT[1] * k).max(1.0), ring);
            } else {
                if index < current {
                    painter.circle_filled(at, DOT_OTHER[0] * k, done);
                }
                painter.circle_stroke(at, DOT_OTHER[0] * k, (DOT_OTHER[1] * k).max(1.0), line);
            }
        }
    })
    .with_label("dots", Some(json!({ "current": current + 1, "count": count })))
}
