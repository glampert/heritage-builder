// The Settings dialog (docs/plans/realm-ui.md, Phase 1, Settings; menu-settings-<realm>.jpg): one
// dialog, MainSettings, with a tab per group of settings: Game, Sound, Graphics and Controls (the
// key bindings, read-only). The GameSettings, SoundSettings and GraphicsSettings kinds open it on
// their tab (redirect).
//
// Each setting is a row: its label at the left and its control (a toggle, a slider or a dropdown)
// at the right, with a rule under it. The controls edit pending values, kept outside the dialog
// (STATE), so a rebuild of the open dialog (a realm or language switch, ui.reload_skin) keeps them
// and the tab shown. Ok writes the changed ones into GameConfigs, applies them to the systems that
// can take them at once and saves the user configs (SettingsCategory::commit; not in a script, a
// replay or the smoke test, which keep them for the run); Cancel and Escape drop them; Restore
// defaults sets every setting's pending value to the configs' default (assets/configs), for Ok to
// commit. Settings the engine reads only when it starts say "(after a restart)".
//
// Paths: MainSettings/tab_bar and MainSettings/<Tab> (the tabs), MainSettings/<Tab>/<setting>,
// MainSettings/Controls/Key bindings (the table), MainSettings/Restore defaults|Cancel|Ok.

use common::{Size, time::Seconds};
use engine::{
    Engine,
    app::{ApplicationApi, ApplicationWindowMode},
    log,
    render::{RenderApi, texture::TextureFilter},
};
use strum::VariantArray;

use super::*;
use crate::{GameLoop, config::GameConfigs};

mod game;
mod sound;
mod graphics;
mod controls;

// ----------------------------------------------
// Layout (points on a 1920 × 1080 window, scaled by the dialog's scale: skin::dialog_scale)
// ----------------------------------------------

const DROPDOWN_WIDTH: f32 = 300.0;
const SLIDER_WIDTH: f32 = 420.0;      // The rail and the value at its left.
const TAB_MIN_WIDTH: f32 = 200.0;
const PART_SPACING: f32 = 14.0;       // Between the title, the tabs' contents and the buttons.
const RULE_THICKNESS: f32 = 1.0;
const SLACK: f32 = 24.0;              // Room left under the rows for what measuring misses.
const BUTTONS_GAP: f32 = 20.0;        // At least, between the last row and the buttons.
const RESTORE_WIDTH: f32 = 300.0;
const CANCEL_WIDTH: f32 = 200.0;
const OK_WIDTH: f32 = 200.0;
const BUTTON_SPACING: f32 = 20.0;
// Sizes on the reference window and on the smallest (1280 × 720), in proportion between them
// (by_window): a row's height (the rule under it included), its text over the body font, and the
// buttons' height.
const ROW_STEPS: (f32, f32) = (62.0, 40.0);
const TAB_HEIGHTS: (f32, f32) = (52.0, 34.0);
const TAB_TEXT_SCALES: (f32, f32) = (1.35, 1.1); // Over the skin's tab font.
const TEXT_SCALES: (f32, f32) = (1.15, 0.9);
const BUTTON_HEIGHTS: (f32, f32) = (56.0, 36.0);
const CONTROL_SCALES: (f32, f32) = (1.0, 0.75); // The toggles' and sliders' sprites.
const NOTE_FONT_SCALE: f32 = 0.85;    // The Controls tab's note, over the text size.

// ----------------------------------------------
// SettingsTab
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug, EnumCount, EnumIter)]
pub(super) enum SettingsTab {
    Game,
    Sound,
    Graphics,
    Controls,
}

impl SettingsTab {
    fn label(self) -> UiString {
        match self {
            Self::Game => UiString::key(UiTextCategory::Menu, "settings.tab.game"),
            Self::Sound => UiString::key(UiTextCategory::Menu, "settings.tab.sound"),
            Self::Graphics => UiString::key(UiTextCategory::Menu, "settings.tab.graphics"),
            Self::Controls => UiString::key(UiTextCategory::Menu, "settings.tab.controls"),
        }
    }

    fn from_index(index: usize) -> Self {
        Self::iter().nth(index).unwrap_or(Self::Game)
    }
}

// The dialog kinds that open the Settings dialog, and on which tab: MainSettings on Game, the
// other three on theirs.
pub(super) fn redirect(kind: DialogMenuKind) -> Option<SettingsTab> {
    match kind {
        DialogMenuKind::MainSettings | DialogMenuKind::GameSettings => Some(SettingsTab::Game),
        DialogMenuKind::SoundSettings => Some(SettingsTab::Sound),
        DialogMenuKind::GraphicsSettings => Some(SettingsTab::Graphics),
        _ => None,
    }
}

// ----------------------------------------------
// Settings
// ----------------------------------------------

// A setting's value as its control holds it: a toggle's, a slider's, or a dropdown's choice (an
// index in its items).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum SettingValue {
    Bool(bool),
    Number(u32),
    Choice(usize),
}

impl SettingValue {
    fn as_bool(self) -> bool {
        matches!(self, Self::Bool(true))
    }

    fn as_number(self) -> u32 {
        match self {
            Self::Number(value) => value,
            _ => 0,
        }
    }

    fn as_choice(self) -> usize {
        match self {
            Self::Choice(index) => index,
            _ => 0,
        }
    }
}

enum SettingControl {
    Toggle,
    Slider { range: std::ops::RangeInclusive<u32>, suffix: &'static str },
    Dropdown { items: Vec<String> },
}

// A setting: its label, its control, how it reads its value from GameConfigs (the current ones, or
// the defaults for Restore defaults) and writes it back. `restart`: the engine reads it only when
// it starts, which its label says.
struct SettingDef {
    tab: SettingsTab,
    label: UiString,
    control: SettingControl,
    restart: bool,
    read: fn(&GameConfigs) -> SettingValue,
    write: fn(&mut GameConfigs, SettingValue),
}

impl SettingDef {
    // The label as the row shows it (and its path).
    fn label_text(&self) -> String {
        let label = self.label.resolve();
        if self.restart {
            ui::text::format(UiTextCategory::Menu, "settings.after_restart", &[("setting", label)])
        } else {
            label.to_string()
        }
    }
}

// Every setting, in the order the tabs show them. The pending values (STATE) are kept by index in
// this list.
fn all_settings() -> Vec<SettingDef> {
    let mut settings = game::settings();
    settings.extend(sound::settings());
    settings.extend(graphics::settings());
    settings
}

// The index in `values` of the one nearest `value`.
fn nearest_choice<T: Copy + Into<f64>>(values: &[T], value: T) -> usize {
    let value: f64 = value.into();
    values
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let a: f64 = (**a).into();
            let b: f64 = (**b).into();
            (a - value).abs().total_cmp(&(b - value).abs())
        })
        .map_or(0, |(index, _)| index)
}

// ----------------------------------------------
// State, kept across rebuilds
// ----------------------------------------------

struct SettingsState {
    tab: SettingsTab,
    pending: Vec<SettingValue>, // By index in all_settings().
    built: (u32, u32, u32),     // build_key() when the dialog was last built.
}

static STATE: UiStaticVar<SettingsState> = UiStaticVar::new(SettingsState { tab: SettingsTab::Game, pending: Vec::new(), built: (0, 0, 0) });

// What the dialog's layout reads from the window: its scale (in hundredths) and the dialog's size.
fn build_key(context: &GameUiContext) -> (u32, u32, u32) {
    let window = ui::screen_size(context.ui_sys);
    let size = skin::dialog_def(&MainSettings::KIND.to_string()).size.unwrap_or(skin::SkinDialogSize::DEFAULT);
    let size = skin::dialog_size(size, window);
    ((skin::dialog_scale(window) * 100.0).round() as u32, size.x as u32, size.y as u32)
}

// Whether the open dialog must be built again: the window's size changed its layout (a resize, the
// window mode). Its pending values and tab are kept (STATE).
pub(super) fn needs_rebuild(context: &GameUiContext) -> bool {
    STATE.as_ref().built != build_key(context)
}

impl SettingsState {
    fn value(&self, index: usize) -> SettingValue {
        self.pending.get(index).copied().unwrap_or(SettingValue::Bool(false))
    }

    fn set(&mut self, index: usize, value: SettingValue) {
        if let Some(pending) = self.pending.get_mut(index) {
            *pending = value;
        }
    }
}

// Reads every setting from the configs, for the dialog about to open on `tab`: what it shows until
// it's closed, rebuilt or not.
pub(super) fn prepare(tab: SettingsTab) {
    let configs = GameConfigs::get();
    let state = STATE.as_mut();
    state.tab = tab;
    state.pending = all_settings().iter().map(|setting| (setting.read)(configs)).collect();
}

// ----------------------------------------------
// SettingsCategory: Restore defaults and Ok
// ----------------------------------------------

struct SettingsCategory;

impl SettingsCategory {
    // Every pending value back to its default: the configs as assets/configs has them.
    fn restore_defaults() {
        let defaults = GameConfigs::defaults();
        let state = STATE.as_mut();
        state.pending = all_settings().iter().map(|setting| (setting.read)(&defaults)).collect();
    }

    // Writes the pending values that differ from the configs into them, applies them, and saves the
    // user configs if any changed. True if any did.
    fn commit() -> bool {
        let configs = GameConfigs::get_mut();
        let state = STATE.as_ref();
        let mut any_changed = false;

        for (index, setting) in all_settings().iter().enumerate() {
            let value = state.value(index);
            if (setting.read)(configs) != value {
                (setting.write)(configs, value);
                any_changed = true;
            }
        }

        if any_changed {
            Self::apply(configs);
            if GameLoop::get().saves_user_configs() {
                // Leaves out runtime changes made by automation commands (config.set).
                crate::automation::save_game_configs();
                log::info!(log::channel!("settings"), "Settings saved.");
            } else {
                log::info!(log::channel!("settings"), "Settings changed for this run only (script, replay or smoke test).");
            }
        }
        any_changed
    }

    // Gives the systems that can take them at once the settings now in the configs. The others
    // read the configs where they use them (the camera, a new game's speed), at the next start
    // (the window, the renderer) or not yet (TODO(realm-ui) at their configs).
    fn apply(configs: &GameConfigs) {
        let engine = Engine::get_mut();

        // The volumes only: the sound system's other settings (fades) may have been changed live
        // in the developer UI.
        let sound_sys = engine.sound_system_mut();
        let mut sound_settings = sound_sys.current_sound_settings();
        let volumes = &configs.engine.sound_settings;
        sound_settings.master_volume = volumes.master_volume;
        sound_settings.music_master_volume = volumes.music_master_volume;
        sound_settings.sfx_master_volume = volumes.sfx_master_volume;
        sound_settings.ambience_master_volume = volumes.ambience_master_volume;
        sound_settings.narration_master_volume = volumes.narration_master_volume;
        sound_settings.spatial_master_volume = volumes.spatial_master_volume;
        sound_sys.change_sound_settings(sound_settings);

        let tex_cache = engine.texture_cache_mut();
        if tex_cache.current_texture_settings() != configs.engine.texture_settings {
            tex_cache.change_texture_settings(configs.engine.texture_settings);
        }

        GameLoop::get_mut().apply_autosave_settings(configs.save.enable_autosave, configs.save.autosave_frequency_secs);

        // The dialogs and the HUD rebuild with the new strings (dialog::sync).
        if ui::text::current_language() != configs.engine.language
            && let Err(err) = ui::text::set_language(&configs.engine.language)
        {
            log::error!(log::channel!("settings"), "Can't switch the language: {err}");
        }

        // The UI scale: GameLoop applies it at the start of the next frame (apply_display_settings),
        // since it changes the styles every widget measures with.
    }
}

// ----------------------------------------------
// GameSettings / SoundSettings / GraphicsSettings: kinds that open MainSettings
// ----------------------------------------------

// Never opened themselves: the dialog framework opens MainSettings on their tab in their place.
pub struct GameSettings {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { GameSettings, ["Game Settings"] }

impl GameSettings {
    pub fn new(_context: &mut GameUiContext) -> Self {
        Self { menu: UiMenu::new(Self::KIND.to_string()).into_rc() }
    }
}

pub struct SoundSettings {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { SoundSettings, ["Sound Settings"] }

impl SoundSettings {
    pub fn new(_context: &mut GameUiContext) -> Self {
        Self { menu: UiMenu::new(Self::KIND.to_string()).into_rc() }
    }
}

pub struct GraphicsSettings {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { GraphicsSettings, ["Graphics Settings"] }

impl GraphicsSettings {
    pub fn new(_context: &mut GameUiContext) -> Self {
        Self { menu: UiMenu::new(Self::KIND.to_string()).into_rc() }
    }
}

// ----------------------------------------------
// MainSettings
// ----------------------------------------------

pub struct MainSettings {
    menu: UiMenuRcMut,
    tab_bar: UiMenuWidgetIndex,
}

implement_dialog_menu! { MainSettings, ["Settings"] }

impl MainSettings {
    pub fn new(context: &mut GameUiContext) -> Self {
        // Built at startup too, before it's ever opened: then it reads the configs itself.
        if STATE.as_ref().pending.is_empty() {
            prepare(STATE.as_ref().tab);
        }

        // Sizes by the window (`scale`, by_window), times as much as the player's UI scale grew the
        // dialog (`growth`); the text, which the UI scale grows already, by as much less than it
        // as the window kept the dialog from growing (`fit`), so the rows and buttons still fit.
        STATE.as_mut().built = build_key(context);
        let window = ui::screen_size(context.ui_sys);
        let kind = Self::KIND.to_string();
        let scale = skin::dialog_scale(window);
        let growth = skin::dialog_ui_growth(&kind, window);
        let fit = (growth / skin::ui_scale()).min(1.0);
        let points = |size: f32| (size * scale * growth).round();
        let text_scale = by_window(TEXT_SCALES, scale) * fit;
        let spacing = points(PART_SPACING);

        let title = skin::heading_at(context, [UiString::key(UiTextCategory::Menu, "settings.title")], HeadingLevel::Dialog, fit);
        let buttons_row = buttons(context, scale, growth, fit);

        // The rows take the height the title, the tabs and the buttons leave, each tab's rows
        // sharing it, up to their height on the reference window.
        let settings = all_settings();
        let content = skin::dialog_content_size(&kind, window);
        let tab_size = Vec2::new(points(TAB_MIN_WIDTH), (by_window(TAB_HEIGHTS, scale) * growth).round());
        let tab_text_scale = by_window(TAB_TEXT_SCALES, scale) * fit;
        let tabs_height = skin::tab_bar_sized(context, tab_size, tab_text_scale).with_tab("Tab").measure(context).y;
        let buttons_gap = points(BUTTONS_GAP);
        let rows_height = (content.y - title.measure(context).y - tabs_height - buttons_row.measure(context).y - spacing * 3.0 - SLACK * scale * growth - buttons_gap).max(1.0);
        let max_row_step = by_window(ROW_STEPS, scale) * growth;
        let row_layout = |rows: usize| {
            let row_step = max_row_step.min(rows_height / rows.max(1) as f32).floor();
            RowLayout {
                row_height: (row_step - RULE_THICKNESS).max(1.0),
                text_scale,
                dropdown_width: points(DROPDOWN_WIDTH),
                slider_width: points(SLIDER_WIDTH),
                rule_color: skin::row_look().rule_color,
                controls_scale: by_window(CONTROL_SCALES, scale) * growth,
            }
        };

        let mut tab_bar = skin::tab_bar_sized(context, tab_size, tab_text_scale)
            .with_selected(STATE.as_ref().tab as usize)
            .on_tab_changed(|bar, _, _| STATE.as_mut().tab = SettingsTab::from_index(bar.selected_index()));
        // Every tab as tall as the room the rows have and the gap above the buttons, so the buttons
        // stay put whatever the tab: its contents, then a filler, with no spacing between them.
        let tabs_contents_height = rows_height + buttons_gap;
        for tab in SettingsTab::iter() {
            let contents = match tab {
                SettingsTab::Controls => controls::contents(context, text_scale, content.x, tabs_contents_height),
                _ => {
                    let rows = settings.iter().filter(|setting| setting.tab == tab).count();
                    setting_rows(context, &settings, tab, &row_layout(rows))
                }
            };
            let room = (tabs_contents_height - contents.measure(context).y).floor();
            let mut tab_contents = UiWidgetGroup::new().with_center_horizontally(false).with_widget(contents);
            if room > 0.0 {
                tab_contents.add_widget(UiSeparator::new().with_size(Vec2::new(1.0, room)));
            }
            tab_bar = tab_bar.with_tab_contents(tab.label(), tab_contents);
        }

        let flags = DialogFrame::from_skin(Self::KIND).menu_flags();
        let mut menu = skin::dialog(context, &kind, flags).with_widget_spacing(Vec2::new(0.0, spacing));
        menu.add_widget(title);
        let tab_bar = menu.add_widget(tab_bar);
        menu.add_widget(buttons_row);

        Self { menu: menu.into_rc(), tab_bar }
    }

    // Shows `tab` (a settings kind opened while the dialog is open), its pending values kept.
    pub(super) fn select_tab(&mut self, tab: SettingsTab) {
        STATE.as_mut().tab = tab;
        if let Some(tab_bar) = self.menu.widget_as_mut::<UiTabBar>(self.tab_bar) {
            tab_bar.set_selected(tab as usize);
        }
    }
}

// ----------------------------------------------
// Widgets
// ----------------------------------------------

struct RowLayout {
    row_height: f32,
    text_scale: f32,
    dropdown_width: f32,
    slider_width: f32,
    rule_color: Option<common::Color>,
    controls_scale: f32, // The toggles' and sliders' sprites.
}

// The rows of `tab`'s settings, each with a rule under it.
fn setting_rows(context: &mut GameUiContext, settings: &[SettingDef], tab: SettingsTab, layout: &RowLayout) -> UiWidgetGroup {
    let mut group = UiWidgetGroup::new().with_center_horizontally(false);
    for (index, setting) in settings.iter().enumerate().filter(|(_, setting)| setting.tab == tab) {
        group.add_widget(setting_row(context, index, setting, layout));
        let rule = UiSeparator::new().with_thickness(RULE_THICKNESS);
        group.add_widget(match layout.rule_color {
            Some(color) => rule.with_color(color),
            None => rule,
        });
    }
    group
}

// A setting's control, as a row: the label at the left, the control at the right edge.
fn setting_row(context: &mut GameUiContext, index: usize, setting: &SettingDef, layout: &RowLayout) -> UiWidgetImpl {
    let label = setting.label_text();
    let font_scale = UiFontScale(layout.text_scale);
    match &setting.control {
        SettingControl::Toggle => UiWidgetImpl::from(
            skin::toggle_at(context, label, layout.controls_scale)
                .with_font_scale(font_scale)
                .with_fill_width(true)
                .with_min_height(layout.row_height)
                .on_read_value(move |_, _| STATE.as_ref().value(index).as_bool())
                .on_update_value(move |_, _, value| STATE.as_mut().set(index, SettingValue::Bool(value))),
        ),
        SettingControl::Slider { range, suffix } => UiWidgetImpl::from(
            skin::slider_at(context, label, range.clone(), layout.controls_scale)
                .with_font_scale(font_scale)
                .with_value_suffix(*suffix)
                .with_row(UiControlRow::new(layout.slider_width, layout.row_height))
                .on_read_value(move |_, _| -> u32 { STATE.as_ref().value(index).as_number() })
                .on_update_value(move |_, _, value: u32| STATE.as_mut().set(index, SettingValue::Number(value))),
        ),
        SettingControl::Dropdown { items } => UiWidgetImpl::from(
            skin::dropdown(context, label, items)
                .with_font_scale(font_scale)
                .with_row(UiControlRow::new(layout.dropdown_width, layout.row_height))
                .on_get_current_selection(move |_, _| STATE.as_ref().value(index).as_choice())
                .on_selection_changed(move |dropdown, _| {
                    STATE.as_mut().set(index, SettingValue::Choice(dropdown.current_selection_index()));
                }),
        ),
    }
}

// The buttons' row: Restore defaults at the left; Cancel and Ok at the right.
// Their sizes by the window (`scale`, by_window) times the UI scale's `growth` of the dialog, and
// their text, which the UI scale grows already, by `fit` as the rows' is.
fn buttons(context: &mut GameUiContext, scale: f32, growth: f32, fit: f32) -> UiColumns {
    let height = (by_window(BUTTON_HEIGHTS, scale) * growth).round();
    let scale = scale * growth;
    let width = |width: f32| UiTextButtonWidth::Fixed((width * scale).round());
    let fitted = |button: UiTextButton| {
        let font_scale = button.font_scale().0 * fit;
        button.with_font_scale(UiFontScale(font_scale))
    };

    let restore = make_text_button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "settings.restore_defaults"), |_, _| {
        SettingsCategory::restore_defaults();
    })
    .with_width(width(RESTORE_WIDTH))
    .with_min_height(height);
    let restore = fitted(restore);
    let cancel = fitted(make_dialog_close_button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "button.cancel")))
        .with_width(width(CANCEL_WIDTH))
        .with_min_height(height);
    let ok = make_text_button(context, ButtonRole::Primary, UiString::key(UiTextCategory::Menu, "button.ok"), |_, context| {
        SettingsCategory::commit();
        super::close_current(context_as_mut::<GameUiContext>(context));
    })
    .with_width(width(OK_WIDTH))
    .with_min_height(height);
    let ok = fitted(ok);

    let spacing = (BUTTON_SPACING * scale).round();
    let actions_width = ((CANCEL_WIDTH + OK_WIDTH) * scale).round() + spacing;
    let actions = UiWidgetGroup::new().with_stack_vertically(false).with_widget_spacing(Vec2::new(spacing, 0.0)).with_widget(cancel).with_widget(ok);

    UiColumns::new()
        .with_spacing(spacing)
        .with_column(UiColumnWidth::Fixed((RESTORE_WIDTH * scale).round()), UiWidgetGroup::new().with_widget(restore))
        .with_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new())
        .with_column(UiColumnWidth::Fixed(actions_width), UiWidgetGroup::new().with_widget(actions))
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_choices() {
        let minutes = [1u32, 2, 5, 10, 15, 30];
        assert_eq!(nearest_choice(&minutes, 1), 0);
        assert_eq!(nearest_choice(&minutes, 5), 2);
        assert_eq!(nearest_choice(&minutes, 4), 2); // Ties and near misses pick the nearest.
        assert_eq!(nearest_choice(&minutes, 0), 0);
        assert_eq!(nearest_choice(&minutes, 600), 5);
        assert_eq!(nearest_choice::<u32>(&[], 3), 0);
    }

    #[test]
    fn tabs_and_redirects() {
        assert_eq!(redirect(DialogMenuKind::MainSettings), Some(SettingsTab::Game));
        assert_eq!(redirect(DialogMenuKind::GameSettings), Some(SettingsTab::Game));
        assert_eq!(redirect(DialogMenuKind::SoundSettings), Some(SettingsTab::Sound));
        assert_eq!(redirect(DialogMenuKind::GraphicsSettings), Some(SettingsTab::Graphics));
        assert_eq!(redirect(DialogMenuKind::Home), None);
        for (index, tab) in SettingsTab::iter().enumerate() {
            assert_eq!(SettingsTab::from_index(index), tab);
        }
        assert_eq!(SettingsTab::from_index(99), SettingsTab::Game);
    }
}
