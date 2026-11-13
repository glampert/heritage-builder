// The campaign picker (docs/plans/realm-ui.md, Phase 1, Home; campaign-picker-<realm>.jpg): what
// the home menu's Campaign opens once the player can play more than one campaign. A card per
// campaign in campaigns.json: its emblem, number, name and realm, a mark per mission (filled for
// each one finished), and where it stands: its next mission, finished, still to come, or what
// unlocks it. The player picks a card they can play, and Continue starts that campaign at its next
// mission. Built from the profile, as the home menu is (rebuild_profile_dialogs).
//
// Cards are labeled by the campaign's number (CampaignPicker/Campaign II), and so are their
// widgets' paths (CampaignPicker/Campaign II/...).

use engine::log;

use super::{
    home::{campaign_number, emblem_state, roman_numeral, start_campaign_mission, unlock_text},
    *,
};
use crate::{campaign::config::CampaignConfigs, profile};

// ----------------------------------------------
// Layout (points on a 1920 × 1080 window)
// ----------------------------------------------

const CARD_SPACING: f32 = 40.0;
const CARD_EMBLEM_SIZE: f32 = 112.0;   // Scaled with the dialog, as is the spacing below.
const CARD_PART_SPACING: f32 = 14.0;   // Between a card's parts.
// Sizes on the reference window and on the smallest (1280 × 720), and between them in proportion
// (by_window): the cards' least height, the buttons' height and the cards' text size (over the
// font scales below). The title keeps its size as the dialog shrinks, so the cards take the room
// it leaves.
const CARD_MIN_HEIGHTS: (f32, f32) = (665.0, 360.0);
const BUTTON_HEIGHTS: (f32, f32) = (56.0, 36.0);
const CARD_TEXT_SCALES: (f32, f32) = (1.3, 1.0);
// Room the cards leave free, so the dialog never scrolls: the title's measure leaves out the item
// spacing drawn around its separator, and rounding.
const CARDS_SLACK: f32 = 24.0;
const CARD_MARK_SIZE: f32 = 20.0;      // A mission's mark, and the gap after it: half that.
const CARD_NUMBER_FONT_SCALE: f32 = 0.85;
const CARD_NAME_FONT_SCALE: f32 = 1.4;
const CARD_REALM_FONT_SCALE: f32 = 0.95;
const CARD_STATUS_FONT_SCALE: f32 = 1.0;
const BUTTON_SPACING: f32 = 24.0;
const BACK_BUTTON_WIDTH: f32 = 200.0;
const REPLAY_BUTTON_WIDTH: f32 = 260.0;
const CONTINUE_BUTTON_WIDTH: f32 = 240.0;

// ----------------------------------------------
// CampaignPicker
// ----------------------------------------------

pub struct CampaignPicker {
    menu: UiMenuRcMut,
}

// The campaign picked, by index in campaigns.json, while the picker is open. Kept outside the
// dialog so a rebuild (the profile changed, or a resize changed the dialogs' scale), which builds
// the picker again and reopens it, keeps the player's pick; None once it closes, or once a session
// switch drops the dialogs (forget_pick).
static PICKED: UiStaticVar<Option<usize>> = UiStaticVar::new(None);

// The dialogs were dropped without closing (dialog::reset): the next open starts afresh.
pub(super) fn forget_pick() {
    PICKED.set(None);
}

// The picker's sizes on a window: points on the reference window scaled by the window
// (`window`: dialog_scale) and by as much as the player's UI scale grew the dialog (`growth`:
// dialog_ui_growth); text by the window and `fit` (dialog_text_fit), since the themes grow it by
// the UI scale already. So its cards and buttons fit it from 75% to 200%, down to 1280 × 720.
#[derive(Copy, Clone)]
struct PickerScale {
    window: f32,
    growth: f32,
    fit: f32,
}

impl PickerScale {
    fn new(kind: &str, window: Vec2) -> Self {
        Self { window: skin::dialog_scale(window), growth: skin::dialog_ui_growth(kind, window), fit: skin::dialog_text_fit(kind, window) }
    }

    fn points(self, points: f32) -> f32 {
        points * self.window * self.growth
    }

    // A size given on the reference and the smallest window (by_window).
    fn by_window(self, sizes: (f32, f32)) -> f32 {
        by_window(sizes, self.window) * self.growth
    }

    // A text scale given on the reference and the smallest window.
    fn text(self, scales: (f32, f32)) -> f32 {
        by_window(scales, self.window) * self.fit
    }
}

fn selected() -> usize {
    (*PICKED.as_ref()).unwrap_or_else(default_selection)
}

implement_dialog_menu! { CampaignPicker, ["Campaign"] }

impl CampaignPicker {
    pub fn new(context: &mut GameUiContext) -> Self {
        let kind = Self::KIND.to_string();
        let window = ui::screen_size(context.ui_sys);
        let scale = PickerScale::new(&kind, window);
        let configs = CampaignConfigs::get();

        let title = skin::heading_at(context, [UiString::key(UiTextCategory::Menu, "campaign_picker.title")], HeadingLevel::Dialog, scale.fit);
        let buttons_row = buttons(context, scale);

        // The cards as tall as CARD_MIN_HEIGHTS says, or as the title and the buttons leave them.
        let spacing = DEFAULT_DIALOG_MENU_WIDGET_SPACING * 2.0;
        let room = skin::dialog_content_size(&kind, window).y - title.measure(context).y - buttons_row.measure(context).y - spacing.y * 2.0 - CARDS_SLACK;
        let card_height = scale.by_window(CARD_MIN_HEIGHTS).min(room).floor();

        let flags = DialogFrame::from_skin(Self::KIND).menu_flags();
        let mut menu = skin::dialog(context, &kind, flags).with_widget_spacing(spacing).with_widget(title);

        let row = UiCardRow::new();
        let mut cards = UiColumns::new().with_spacing(scale.points(CARD_SPACING));
        for (index, _) in configs.visible() {
            cards.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new().with_widget(card(context, index, scale, card_height, row.clone())));
        }
        menu.add_widget(cards);
        menu.add_widget(buttons_row);

        // Opening from the home menu, the pick starts on the latest campaign the player can play;
        // reopened by a rebuild, it stays, if that campaign can still be started.
        menu.set_open_close_callback(|_, _, is_open| {
            if is_open {
                if !(*PICKED.as_ref()).is_some_and(can_start) {
                    PICKED.set(Some(default_selection()));
                }
            } else {
                PICKED.set(None);
            }
        });

        Self { menu: menu.into_rc() }
    }
}

// The card a campaign can be started from: one the player can play that has a mission to start.
fn can_start(campaign_index: usize) -> bool {
    let configs = CampaignConfigs::get();
    let profile = profile::get();
    profile.is_playable(configs, campaign_index) && profile.next_mission(configs, campaign_index).is_some()
}

// The latest campaign the player can start (the first if none).
fn default_selection() -> usize {
    let visible: Vec<usize> = CampaignConfigs::get().visible().map(|(index, _)| index).collect();
    visible.iter().rev().copied().find(|&index| can_start(index)).unwrap_or(0)
}

fn on_continue() {
    let selected = selected();
    match profile::get().next_mission(CampaignConfigs::get(), selected) {
        Some(mission_index) if can_start(selected) => {
            // The session switch drops the dialogs, which forgets the pick (forget_pick).
            start_campaign_mission(selected, mission_index);
        }
        _ => log::error!(log::channel!("campaign"), "Campaign {selected} can't be started."),
    }
}

// A line of text centered in a card, `font_scale` times the body font (and CARD_TEXT_SCALES at
// `scale`), in the skin's colour `color`.
fn card_text(text: impl Into<UiString>, font_scale: f32, scale: PickerScale, color: &str) -> UiRichText {
    let font_scale = font_scale * scale.text(CARD_TEXT_SCALES);
    let text = skin::rich_text(text).with_align(UiTextAlign::Center).with_font_scale(UiFontScale(font_scale));
    match skin::color(color) {
        Some(color) => text.with_text_color(color),
        None => text,
    }
}

// The card of the campaign at `campaign_index`: its emblem (the current one ringed: the latest
// the player can play), "CAMPAIGN II", its name and realm, its mission marks and where it stands; at least `min_height` tall.
fn card(context: &mut GameUiContext, campaign_index: usize, scale: PickerScale, min_height: f32, row: UiCardRow) -> UiCard {
    let configs = CampaignConfigs::get();
    let profile = profile::get();
    let campaign = &configs.campaigns[campaign_index];
    let playable = profile.is_playable(configs, campaign_index);
    let missions = campaign.mission_count();
    let record = profile.record(&campaign.id);
    let finished_missions = record.map_or(&[][..], |record| record.missions_finished.as_slice());
    let finished = campaign.steps().into_iter().filter(|&step| campaign.is_step_finished(step, finished_missions)).count();

    let mut contents = UiWidgetGroup::new().with_widget_spacing(Vec2::new(0.0, scale.points(CARD_PART_SPACING)));

    if let Some((texture, size)) = skin::emblem(context, campaign.realm, emblem_state(campaign_index)) {
        let emblem_size = Vec2::new(scale.points(CARD_EMBLEM_SIZE), scale.points(CARD_EMBLEM_SIZE));
        contents.add_widget(UiWidgetGroup::new().with_widget(UiImage::from_texture(texture, size, emblem_size).with_label("emblem")));
    }

    let number = configs.campaign_number(campaign_index).unwrap_or(campaign_index + 1);
    let number = ui::text::format(UiTextCategory::Menu, "campaign_picker.number", &[("number", &roman_numeral(number))]);
    contents.add_widget(card_text(number, CARD_NUMBER_FONT_SCALE, scale, "accent").with_label("number"));
    contents.add_widget(card_text(campaign.display_name(), CARD_NAME_FONT_SCALE, scale, if playable { "ink" } else { "sub" }).with_label("name"));
    contents.add_widget(card_text(campaign.realm.display_name(), CARD_REALM_FONT_SCALE, scale, "sub").with_label("realm"));
    contents.add_widget(skin::separator(context, "plain"));

    // A mark per mission, filled for each finished.
    if missions > 0 {
        let width = CARD_MARK_SIZE * 1.5 * missions as f32 - CARD_MARK_SIZE * 0.5;
        let marks = skin::meter(UiMeterKind::Steps(missions as u32), Vec2::new(scale.points(width), scale.points(CARD_MARK_SIZE)))
            .with_label("missions")
            .with_value(finished as f32, missions as f32);
        contents.add_widget(UiWidgetGroup::new().with_widget(marks));
    }

    // Where it stands.
    // TODO(realm-ui): "Last played today, 23:10" (the profile records no time yet) and the quote,
    // the newest line of the campaign's stele (Phase 3); the distinctions earned stay hidden until
    // M9 awards them (open decision 11).
    let (status, tooltip) = if !playable {
        let text = unlock_text(campaign_index);
        (text.clone(), Some(text))
    } else if missions == 0 {
        let text = ui::text::get(UiTextCategory::Menu, "home.coming_soon").to_string();
        (text.clone(), Some(text))
    } else if record.is_some_and(|record| record.finished) {
        (ui::text::get(UiTextCategory::Menu, "campaign_picker.finished").to_string(), None)
    } else {
        let next = profile.next_mission(configs, campaign_index).unwrap_or(0);
        let name = campaign.mission_display_name(next).unwrap_or("");
        let text = ui::text::format(
            UiTextCategory::Menu,
            "campaign_picker.mission",
            &[("mission", &campaign.mission_number(next).to_string()), ("count", &missions.to_string()), ("name", name)],
        );
        (text, None)
    };
    contents.add_widget(card_text(status, CARD_STATUS_FONT_SCALE, scale, if playable && missions > 0 { "ink" } else { "sub" }).with_label("status"));

    let mut card = skin::card(context, campaign_number(campaign_index), contents)
        .with_row(row)
        .with_min_height(min_height)
        .with_enabled(can_start(campaign_index))
        .on_read_selected(move |_, _| selected() == campaign_index)
        .on_pressed(move |_, _| PICKED.set(Some(campaign_index)));
    if let Some(tooltip) = tooltip {
        card = card.with_tooltip(skin::tooltip(context, tooltip));
    }
    card
}

// Back at the left; Replay a mission and Continue at the right, as tall as BUTTON_HEIGHTS says,
// their text at their role's size times `scale.fit`.
fn buttons(context: &mut GameUiContext, scale: PickerScale) -> UiColumns {
    let height = scale.by_window(BUTTON_HEIGHTS).round();
    let fitted = |button: UiTextButton| {
        let font_scale = button.font_scale().0 * scale.fit;
        button.with_font_scale(UiFontScale(font_scale))
    };
    let back = fitted(make_text_button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "button.back"), |_, context| {
        super::close_current(context_as_mut::<GameUiContext>(context));
    }))
    .with_width(UiTextButtonWidth::Fixed(BACK_BUTTON_WIDTH))
    .with_min_height(height);

    // TODO(realm-ui): a mission list, replaying any mission of a campaign already played; no phase
    // builds it yet (docs/plans/realm-ui.md, Later).
    let replay = fitted(make_text_button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "campaign_picker.replay"), |_, _| {}))
        .with_width(UiTextButtonWidth::Fixed(REPLAY_BUTTON_WIDTH))
        .with_min_height(height)
        .with_enabled(false)
        .with_tooltip(skin::tooltip(context, UiString::key(UiTextCategory::Menu, "home.coming_soon")));

    let start = fitted(make_text_button(context, ButtonRole::Primary, UiString::key(UiTextCategory::Menu, "campaign_picker.continue"), |_, _| on_continue()))
        .with_width(UiTextButtonWidth::Fixed(CONTINUE_BUTTON_WIDTH))
        .with_min_height(height);

    let right = UiWidgetGroup::new()
        .with_stack_vertically(false)
        .with_widget_spacing(Vec2::new(BUTTON_SPACING, 0.0))
        .with_widget(replay)
        .with_widget(start);

    UiColumns::new()
        .with_spacing(BUTTON_SPACING)
        .with_column(UiColumnWidth::Fixed(BACK_BUTTON_WIDTH), UiWidgetGroup::new().with_widget(back))
        .with_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new())
        .with_column(UiColumnWidth::Fixed(REPLAY_BUTTON_WIDTH + BUTTON_SPACING + CONTINUE_BUTTON_WIDTH), right)
}
