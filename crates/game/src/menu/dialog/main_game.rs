// The in-game Game menu (docs/plans/realm-ui.md, Phase 1, In-game Game menu), as the game-menu
// mockups draw it: the realm's crest, "Game", the mission or custom game being played, a rule, the
// menu items (Resume, Save, Load, Settings, Quit to menu, Exit game) with the realm's marker on the
// one hovered, and when the city was last saved. Quit to menu and Exit game ask first in a popup
// (game-quit mockups): Cancel, Save and quit (or Save and exit: the game saved in the autosave's
// slot, which Continue loads, then left once the save is written) and Quit (or Exit).
//
// The place and the last-saved line follow the game each frame the menu is on top (a save made
// through its own Save item, an autosave, minutes going by), since the menu is built once per skin
// and window and the game goes on under it. Its contents scale with the panel's content area
// (skin::dialog_content_scale), at any UI scale (its text at skin::dialog_content_text_scale), as
// Home's do, and the last-saved line sits at its bottom.

use super::*;
use crate::{
    GameLoop,
    campaign::{self, config::CampaignConfigs},
    custom_game,
    game_loop::AfterSaveAction,
};

// ----------------------------------------------
// Layout (points on a 1920 × 1080 window, scaled with the content area: dialog_content_scale)
// ----------------------------------------------

const GAME_MENU_WIDGET_SPACING: f32 = 10.0;  // Between the panel's parts.
const GAME_MENU_ITEM_SPACING: f32 = 12.0;    // Between the menu items.
const GAME_MENU_ITEM_SCALE: f32 = 1.15;      // The items' fonts, padding and marker, over the skin's menu_item role.
const GAME_MENU_CREST_HEIGHT: f32 = 72.0;    // The crest is drawn this tall at most, keeping its shape.
const GAME_MENU_PLACE_FONT_SCALE: f32 = 0.8; // The place line, over the skin's subtitle.
const GAME_MENU_SAVED_FONT_SCALE: f32 = 0.9; // "Saved 12 minutes ago".

// Automation labels of the lines filled in when the menu opens.
const PLACE_LABEL: &str = "place";
const LAST_SAVED_LABEL: &str = "last_saved";

// The popups' buttons, as the game-quit mockups draw them (points on a 1920 × 1080 window, scaled
// as the dialogs are: skin::dialog_scale).
const POPUP_BUTTON_WIDTH: f32 = 220.0;
const POPUP_BUTTON_HEIGHT: f32 = 56.0;

// Room the content area keeps free below the last-saved line, so rounding (more of it at a UI
// scale) never makes it scroll.
const GAME_MENU_SLACK: f32 = 12.0;

// The popups' labels (automation ids: "Quit Game Popup/Save and quit").
const QUIT_POPUP_LABEL: &str = "Quit Game Popup";
const EXIT_POPUP_LABEL: &str = "Exit Game Popup";

// ----------------------------------------------
// GameMenuItem
// ----------------------------------------------

// The menu's items, in order. Their labels are their names in widget paths (MainGame/Quit to menu).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum GameMenuItem {
    Resume,
    Save,
    Load,
    Settings,
    QuitToMenu,
    ExitGame,
}

impl GameMenuItem {
    const ALL: [Self; 6] = [Self::Resume, Self::Save, Self::Load, Self::Settings, Self::QuitToMenu, Self::ExitGame];

    fn label(self) -> UiString {
        match self {
            Self::Resume => UiString::key(UiTextCategory::Menu, "game_menu.resume"),
            Self::Save => UiString::key(UiTextCategory::Menu, "game_menu.save"),
            Self::Load => UiString::key(UiTextCategory::Menu, "game_menu.load"),
            Self::Settings => UiString::key(UiTextCategory::Menu, "game_menu.settings"),
            Self::QuitToMenu => UiString::key(UiTextCategory::Menu, "game_menu.quit_to_menu"),
            Self::ExitGame => UiString::key(UiTextCategory::Menu, "game_menu.exit_game"),
        }
    }

    fn on_pressed(self, context: &mut GameUiContext) -> bool {
        const CLOSE_ALL_OTHERS: bool = false;
        match self {
            Self::Resume => super::close_current(context),
            Self::Save => super::open(DialogMenuKind::SaveGame, CLOSE_ALL_OTHERS, context),
            Self::Load => super::open(DialogMenuKind::LoadGame, CLOSE_ALL_OTHERS, context),
            Self::Settings => super::open(DialogMenuKind::MainSettings, CLOSE_ALL_OTHERS, context),
            Self::QuitToMenu => open_leave_popup(context, Leave::QuitToMenu),
            Self::ExitGame => open_leave_popup(context, Leave::ExitGame),
        }
    }
}

// ----------------------------------------------
// MainGame
// ----------------------------------------------

pub struct MainGame {
    menu: UiMenuRcMut,
    shown: (String, String), // The place and last-saved lines as drawn.
}

impl DialogMenuFactory for MainGame {
    const KIND: DialogMenuKind = DialogMenuKind::MainGame;
    const TITLE: &'static [&'static str] = &["Game"];

    fn create(context: &mut GameUiContext) -> DialogMenuImpl {
        DialogMenuImpl::from(MainGame::new(context))
    }
}

impl DialogMenu for MainGame {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn kind(&self) -> DialogMenuKind {
        Self::KIND
    }

    fn menu(&self) -> &UiMenuRcMut {
        &self.menu
    }

    fn draw_on_top(&mut self, context: &mut GameUiContext) {
        self.refresh_lines();
        self.menu_mut().draw(context);
    }

    fn draw_below(&mut self, context: &mut GameUiContext) {
        self.refresh_lines();
        let menu = self.menu_mut();
        let flags = menu.flags() & !UiMenuFlags::Modal;
        menu.draw_with_flags(context, flags);
    }
}

impl MainGame {
    pub fn new(context: &mut GameUiContext) -> Self {
        let kind = Self::KIND.to_string();
        let window = ui::screen_size(context.ui_sys);
        let scale = skin::dialog_content_scale(&kind, window);
        let text = skin::dialog_content_text_scale(&kind, window);
        let spacing = (GAME_MENU_WIDGET_SPACING * scale).round();
        let flags = DialogFrame::from_skin(Self::KIND).menu_flags();
        let mut menu = skin::dialog(context, &kind, flags).with_widget_spacing(Vec2::new(0.0, spacing));

        let shown = (place_text(), last_saved_text());
        // The parts, measured as they're added: the last-saved line goes at the bottom of the
        // content area, as the mockups pin it, a blank filler taking the room the rest leaves.
        let mut parts_height = 0.0;
        if let Some(crest) = super::home::crest(context, GAME_MENU_CREST_HEIGHT, scale) {
            parts_height += crest.measure(context).y + spacing;
            menu.add_widget(crest);
        }
        let title = skin::heading_scaled(context, [UiString::key(UiTextCategory::Menu, "game_menu.title")], HeadingLevel::Title, scale, text);
        parts_height += title.measure(context).y + spacing;
        menu.add_widget(title);
        let place = line_text(&shown.0, PLACE_LABEL, "accent", subtitle_font_scale() * GAME_MENU_PLACE_FONT_SCALE * text);
        parts_height += place.measure(context).y + spacing;
        menu.add_widget(place);
        let rule = skin::separator(context, "rule");
        parts_height += rule.measure(context).y + spacing;
        menu.add_widget(rule);
        let items = menu_items(context, scale, text);
        parts_height += items.measure(context).y + spacing;
        menu.add_widget(items);
        let saved = line_text(&shown.1, LAST_SAVED_LABEL, "sub", GAME_MENU_SAVED_FONT_SCALE * text);
        parts_height += saved.measure(context).y;

        let content = skin::dialog_content_size(&kind, window);
        let room = (content.y - parts_height - spacing - GAME_MENU_SLACK * scale).floor();
        if room > 0.0 {
            menu.add_widget(UiSeparator::new().with_size(Vec2::new(1.0, room)));
        }
        menu.add_widget(saved);

        Self { menu: menu.into_rc(), shown }
    }

    // The place and last-saved lines anew, once what they say changed.
    fn refresh_lines(&mut self) {
        let place = place_text();
        if place != self.shown.0
            && let Some((_, line)) = self.menu.find_widget_with_label_mut::<UiRichText>(PLACE_LABEL)
        {
            line.set_spans([UiTextSpan::plain(place.as_str())]);
        }
        let saved = last_saved_text();
        if saved != self.shown.1
            && let Some((_, line)) = self.menu.find_widget_with_label_mut::<UiRichText>(LAST_SAVED_LABEL)
        {
            line.set_spans([UiTextSpan::plain(saved.as_str())]);
        }
        self.shown = (place, saved);
    }
}

// The menu's items, each pressing its action: sized at `scale`, their text at `text_scale`.
fn menu_items(context: &mut GameUiContext, scale: f32, text_scale: f32) -> UiWidgetGroup {
    let mut group = UiWidgetGroup::new().with_widget_spacing(Vec2::new(0.0, (GAME_MENU_ITEM_SPACING * scale).round()));
    for item_kind in GameMenuItem::ALL {
        let item = skin::menu_item_scaled(context, item_kind.label(), None, scale * GAME_MENU_ITEM_SCALE, text_scale * GAME_MENU_ITEM_SCALE).on_pressed(move |_, context| {
            item_kind.on_pressed(context_as_mut::<GameUiContext>(context));
        });
        group.add_widget(item);
    }
    group
}

// A centered line of plain text (a city's or map's name isn't markup) in the skin's colour
// `color`, labeled `label` for automation.
fn line_text(text: &str, label: &str, color: &str, font_scale: f32) -> UiRichText {
    let mut line = skin::rich_text("").with_label(label).with_align(UiTextAlign::Center).with_font_scale(UiFontScale(font_scale));
    line.set_spans([UiTextSpan::plain(text)]);
    match skin::color(color) {
        Some(color) => line.with_text_color(color),
        None => line,
    }
}

// The skin's subtitle font scale (the place line is a smaller subtitle).
fn subtitle_font_scale() -> f32 {
    skin::current().headings.get("subtitle").map_or(1.0, |def| if def.font_scale > 0.0 { def.font_scale } else { 1.0 })
}

// What is being played: "<mission> · Campaign I", "Custom game · <map>", or "Custom game" for a
// game that's neither (a preset, a new map from automation or the developer editor).
// TODO(realm-ui): the mockup's "<mission> · <city>" once missions and custom games name their
// cities (save_header's city_name).
fn place_text() -> String {
    if let Some(mission) = campaign::active_mission()
        && let Some(campaign) = CampaignConfigs::get().campaign(mission.campaign_id)
        && let Some(name) = campaign.mission_display_name(mission.mission_index)
    {
        let number = super::home::campaign_number(mission.campaign_id);
        return ui::text::format(UiTextCategory::Menu, "game_menu.place.mission", &[("mission", name), ("campaign", &number)]);
    }
    match custom_game::active().and_then(|game| game.record.map.as_deref()).map(super::load_save::custom_map_display_name) {
        Some(map) => ui::text::format(UiTextCategory::Menu, "load_save.custom_game_place", &[("place", &map)]),
        None => ui::text::get(UiTextCategory::Menu, "load_save.custom_game").to_string(),
    }
}

// When the city was last saved, in the time played since: "Saved 12 minutes ago", "Autosaved just
// now", or "Not saved yet". Empty while the game loop is being set up (the dialogs are first built then).
fn last_saved_text() -> String {
    if !GameLoop::is_initialized() {
        return String::new();
    }
    let (last_saved, time_played_secs) = GameLoop::get().last_saved();
    let Some(last_saved) = last_saved else {
        return ui::text::get(UiTextCategory::Menu, "game_menu.not_saved").to_string();
    };
    let minutes = ((time_played_secs - last_saved.time_played_secs).max(0.0) / 60.0).floor() as i64;
    match (last_saved.autosave, minutes) {
        (false, 0) => ui::text::get(UiTextCategory::Menu, "game_menu.saved.now").to_string(),
        (true, 0) => ui::text::get(UiTextCategory::Menu, "game_menu.autosaved.now").to_string(),
        (false, minutes) => ui::text::format_count(UiTextCategory::Menu, "game_menu.saved", minutes, &[]),
        (true, minutes) => ui::text::format_count(UiTextCategory::Menu, "game_menu.autosaved", minutes, &[]),
    }
}

// ----------------------------------------------
// Quit to menu / Exit game popups
// ----------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Leave {
    QuitToMenu,
    ExitGame,
}

// Asks before leaving the game, over the Game menu: Cancel, save first, or leave.
fn open_leave_popup(context: &mut GameUiContext, leave: Leave) -> bool {
    let main_menu = super::current_as::<MainGame>().expect("Expected MainGame dialog to be open!");
    debug_assert!(main_menu.menu.is_open());
    if main_menu.menu.is_message_box_open() {
        return false;
    }

    // Built anew each time (a menu keeps the message box it built): the two popups share the slot,
    // and the last-saved line changes.
    main_menu.menu.reset_message_box();
    let menu_weak_ref = main_menu.menu.downgrade();
    main_menu.menu.open_message_box(context, move |context| {
        let (label, title, save, confirm) = match leave {
            Leave::QuitToMenu => (
                QUIT_POPUP_LABEL,
                UiString::key(UiTextCategory::Menu, "popup.quit.title"),
                UiString::key(UiTextCategory::Menu, "popup.quit.save_and_quit"),
                UiString::key(UiTextCategory::Menu, "popup.quit.quit"),
            ),
            Leave::ExitGame => (
                EXIT_POPUP_LABEL,
                UiString::key(UiTextCategory::Menu, "popup.exit.title"),
                UiString::key(UiTextCategory::Menu, "popup.exit.save_and_exit"),
                UiString::key(UiTextCategory::Menu, "popup.exit.exit"),
            ),
        };
        // What will be lost, in ink, and when the city was saved, in the skin's `sub` colour.
        let last_saved = ui::text::format(UiTextCategory::Menu, "popup.quit.last_saved", &[("when", &last_saved_text())]);
        let body = [(UiString::key(UiTextCategory::Menu, "popup.quit.body"), "ink"), (UiString::from(last_saved), "sub")];
        let after_save = match leave {
            Leave::QuitToMenu => AfterSaveAction::QuitToMainMenu,
            Leave::ExitGame => AfterSaveAction::ExitGame,
        };

        let button_scale = skin::dialog_scale(ui::screen_size(context.ui_sys()));
        let sized = |button: UiTextButton| {
            button
                .with_width(UiTextButtonWidth::Min((POPUP_BUTTON_WIDTH * button_scale).round()))
                .with_min_height((POPUP_BUTTON_HEIGHT * button_scale).round())
        };
        let cancel_menu = menu_weak_ref.clone();
        skin::popup_with_colored_body(context, label, [title], body).with_buttons([
            // Cancel also drops a save and quit still waiting for its save.
            sized(make_text_button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "button.cancel"), move |_, context| {
                GameLoop::get_mut().cancel_save_and_then();
                if let Some(mut main_menu) = cancel_menu.upgrade() {
                    main_menu.close_message_box(context);
                }
            })),
            // The popup closes at once: the game leaves a few frames later, once the save and its
            // thumbnail are written, with nothing left to press meanwhile but the Game menu's items.
            sized(make_text_button(context, ButtonRole::Secondary, save, move |_, context| {
                GameLoop::get_mut().save_and_then(after_save);
                if let Some(mut main_menu) = menu_weak_ref.upgrade() {
                    main_menu.close_message_box(context);
                }
            })),
            sized(make_text_button(context, ButtonRole::Primary, confirm, move |_, _| match leave {
                Leave::QuitToMenu => GameLoop::get_mut().quit_to_main_menu(),
                Leave::ExitGame => GameLoop::get_mut().quit_game(),
            })),
        ])
    });

    true
}
