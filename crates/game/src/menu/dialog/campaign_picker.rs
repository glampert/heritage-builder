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
    home::{campaign_number, roman_numeral, start_campaign_mission},
    *,
};
use crate::{campaign::config::CampaignConfigs, menu::skin::EmblemState, profile};

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
// the picker again and reopens it, keeps the player's pick; None once it closes.
static PICKED: UiStaticVar<Option<usize>> = UiStaticVar::new(None);

fn selected() -> usize {
    (*PICKED.as_ref()).unwrap_or_else(default_selection)
}

implement_dialog_menu! { CampaignPicker, ["Campaign"] }

impl CampaignPicker {
    pub fn new(context: &mut GameUiContext) -> Self {
        let scale = skin::dialog_scale(ui::screen_size(context.ui_sys));
        let configs = CampaignConfigs::get();

        let title = skin::heading(context, [UiString::key(UiTextCategory::Menu, "campaign_picker.title")], HeadingLevel::Dialog);
        let flags = DialogFrame::from_skin(Self::KIND).menu_flags();
        let mut menu = skin::dialog(context, &Self::KIND.to_string(), flags)
            .with_widget_spacing(DEFAULT_DIALOG_MENU_WIDGET_SPACING * 2.0)
            .with_widget(title);

        let row = UiCardRow::new();
        let mut cards = UiColumns::new().with_spacing(CARD_SPACING * scale);
        for index in 0..configs.campaigns.len() {
            cards.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new().with_widget(card(context, index, scale, row.clone())));
        }
        menu.add_widget(cards);
        menu.add_widget(buttons(context, scale));

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
    let count = CampaignConfigs::get().campaigns.len();
    (0..count).rev().find(|&index| can_start(index)).unwrap_or(0)
}

fn on_continue() {
    let selected = selected();
    match profile::get().next_mission(CampaignConfigs::get(), selected) {
        Some(mission_index) if can_start(selected) => {
            // The session switch drops the dialogs without closing them.
            PICKED.set(None);
            start_campaign_mission(selected, mission_index);
        }
        _ => log::error!(log::channel!("campaign"), "Campaign {selected} can't be started."),
    }
}

// A line of text centered in a card, `font_scale` times the body font (and the window's
// CARD_TEXT_SCALES at a dialog scale of `scale`), in the skin's colour `color`.
fn card_text(text: impl Into<UiString>, font_scale: f32, scale: f32, color: &str) -> UiRichText {
    let font_scale = font_scale * by_window(CARD_TEXT_SCALES, scale);
    let text = skin::rich_text(text).with_align(UiTextAlign::Center).with_font_scale(UiFontScale(font_scale));
    match skin::color(color) {
        Some(color) => text.with_text_color(color),
        None => text,
    }
}

// The card of the campaign at `campaign_index`: its emblem (the current one ringed: the latest
// the player can play), "CAMPAIGN II", its name and realm, its mission marks and where it stands.
fn card(context: &mut GameUiContext, campaign_index: usize, scale: f32, row: UiCardRow) -> UiCard {
    let configs = CampaignConfigs::get();
    let profile = profile::get();
    let campaign = &configs.campaigns[campaign_index];
    let playable = profile.is_playable(configs, campaign_index);
    let current = profile.playable_campaigns(configs).last() == Some(&campaign_index);
    let missions = campaign.missions.len();
    let record = profile.record(&campaign.id);
    let finished = record.map_or(0, |record| record.missions_finished.len());

    let mut contents = UiWidgetGroup::new().with_widget_spacing(Vec2::new(0.0, CARD_PART_SPACING * scale));

    let state = match (playable, current) {
        (false, _) => EmblemState::Locked,
        (true, true) => EmblemState::Current,
        (true, false) => EmblemState::Idle,
    };
    if let Some((texture, size)) = skin::emblem(context, campaign.realm, state) {
        let emblem_size = Vec2::new(CARD_EMBLEM_SIZE, CARD_EMBLEM_SIZE) * scale;
        contents.add_widget(UiWidgetGroup::new().with_widget(UiImage::from_texture(texture, size, emblem_size).with_label("emblem")));
    }

    let number = ui::text::format(UiTextCategory::Menu, "campaign_picker.number", &[("number", &roman_numeral(campaign_index + 1))]);
    contents.add_widget(card_text(number, CARD_NUMBER_FONT_SCALE, scale, "accent").with_label("number"));
    contents.add_widget(card_text(campaign.display_name(), CARD_NAME_FONT_SCALE, scale, if playable { "ink" } else { "sub" }).with_label("name"));
    contents.add_widget(card_text(campaign.realm.display_name(), CARD_REALM_FONT_SCALE, scale, "sub").with_label("realm"));
    contents.add_widget(skin::separator(context, "plain"));

    // A mark per mission, filled for each finished.
    if missions > 0 {
        let width = CARD_MARK_SIZE * 1.5 * missions as f32 - CARD_MARK_SIZE * 0.5;
        let marks = skin::meter(UiMeterKind::Steps(missions as u32), Vec2::new(width, CARD_MARK_SIZE) * scale)
            .with_label("missions")
            .with_value(finished as f32, missions as f32);
        contents.add_widget(UiWidgetGroup::new().with_widget(marks));
    }

    // Where it stands.
    // TODO(realm-ui): "Last played today, 23:10" (the profile records no time yet) and the quote,
    // the newest line of the campaign's stele (Phase 3); the distinctions earned stay hidden until
    // M9 awards them (open decision 11).
    let (status, tooltip) = if !playable {
        let previous = campaign_number(campaign_index.saturating_sub(1));
        let text = ui::text::format(UiTextCategory::Menu, "campaign.unlock", &[("campaign", &previous)]);
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
            &[("mission", &(next + 1).to_string()), ("count", &missions.to_string()), ("name", name)],
        );
        (text, None)
    };
    contents.add_widget(card_text(status, CARD_STATUS_FONT_SCALE, scale, if playable && missions > 0 { "ink" } else { "sub" }).with_label("status"));

    let mut card = skin::card(context, campaign_number(campaign_index), contents)
        .with_row(row)
        .with_min_height(by_window(CARD_MIN_HEIGHTS, scale).round())
        .with_enabled(can_start(campaign_index))
        .on_read_selected(move |_, _| selected() == campaign_index)
        .on_pressed(move |_, _| PICKED.set(Some(campaign_index)));
    if let Some(tooltip) = tooltip {
        card = card.with_tooltip(skin::tooltip(context, tooltip));
    }
    card
}

// Back at the left; Replay a mission and Continue at the right, as tall as BUTTON_HEIGHTS says.
fn buttons(context: &mut GameUiContext, scale: f32) -> UiColumns {
    let height = by_window(BUTTON_HEIGHTS, scale).round();
    let back = make_text_button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "button.back"), |_, context| {
        super::close_current(context_as_mut::<GameUiContext>(context));
    })
    .with_width(UiTextButtonWidth::Fixed(BACK_BUTTON_WIDTH))
    .with_min_height(height);

    // TODO(realm-ui): Phase 3's mission list: replaying any mission of a campaign already played.
    let replay = make_text_button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "campaign_picker.replay"), |_, _| {})
        .with_width(UiTextButtonWidth::Fixed(REPLAY_BUTTON_WIDTH))
        .with_min_height(height)
        .with_enabled(false)
        .with_tooltip(skin::tooltip(context, UiString::key(UiTextCategory::Menu, "home.coming_soon")));

    let start = make_text_button(context, ButtonRole::Primary, UiString::key(UiTextCategory::Menu, "campaign_picker.continue"), |_, _| on_continue())
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
