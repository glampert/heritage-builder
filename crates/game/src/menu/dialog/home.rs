// The home menu (docs/plans/realm-ui.md, Phase 1, Home), as the menu-home mockups draw it: a tall
// panel at the left with the realm's crest, the game's title and the realm's campaign under it;
// the campaign row, one emblem per campaign in campaigns.json (playable, the current one: the
// latest playable, or locked); the menu items with their sublines, the realm's marker on the one
// hovered; and the version. Continue loads the profile's last save; Campaign starts or resumes the
// campaign the player can play, at its next mission, or opens the campaign picker when they can
// play more than one.
//
// It shows the profile, the campaigns and the last save as they were when it was built. The
// dialogs that show them are built again when the profile changes, whenever the home menu opens
// (the last save may have changed under the same name, the autosave's) and when the window's size
// changes the dialogs' (rebuild_profile_dialogs). Built in game (every dialog is, with each skin),
// it leaves the last save unread and Continue disabled: it isn't shown there. Its contents scale
// with the window as its panel's content area does (skin::dialog_content_scale), so they fill the
// panel as the mockup's do, down to 1280 × 720, at any UI scale (its text at
// skin::dialog_content_text_scale, since the themes grow it by the UI scale already).

use std::path::PathBuf;

use engine::{file_sys::paths::PathRef, log, save::storage};

use super::*;
use crate::{
    GameLoop,
    campaign::{
        self,
        config::{CampaignConfigs, MissionMap},
    },
    menu::skin::EmblemState,
    profile,
    save_header,
    session,
};

// ----------------------------------------------
// Layout (points on a 1920 × 1080 window, scaled with the content area: dialog_content_scale)
// ----------------------------------------------

const HOME_WIDGET_SPACING: f32 = 6.0;  // Between the panel's parts.
const HOME_ITEM_SPACING: f32 = 4.0;    // Between the menu items.
const HOME_ITEM_SCALE: f32 = 1.1;      // The items' fonts, padding and marker, over the skin's menu_item role.
const HOME_EMBLEM_SIZE: f32 = 72.0;
const HOME_EMBLEM_LABEL_FONT_SCALE: f32 = 0.85;
const HOME_VERSION_FONT_SCALE: f32 = 0.8;
const HOME_CREST_HEIGHT: f32 = 76.0;   // The crest is drawn this tall at most, keeping its shape.

// ----------------------------------------------
// HomeItem
// ----------------------------------------------

// The menu's items, in order. Their labels are their names in widget paths (Home/Continue). New
// Game is gone: Custom Game's Blank map takes its options.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum HomeItem {
    Continue,
    Campaign,
    CustomGame,
    LoadGame,
    Settings,
    About,
    Quit,
}

impl HomeItem {
    const ALL: [Self; 7] = [Self::Continue, Self::Campaign, Self::CustomGame, Self::LoadGame, Self::Settings, Self::About, Self::Quit];

    fn label(self) -> UiString {
        match self {
            Self::Continue => UiString::key(UiTextCategory::Menu, "home.continue"),
            Self::Campaign => UiString::key(UiTextCategory::Menu, "home.campaign"),
            Self::CustomGame => UiString::key(UiTextCategory::Menu, "home.custom_game"),
            Self::LoadGame => UiString::key(UiTextCategory::Menu, "home.load_game"),
            Self::Settings => UiString::key(UiTextCategory::Menu, "home.settings"),
            Self::About => UiString::key(UiTextCategory::Menu, "home.about"),
            Self::Quit => UiString::key(UiTextCategory::Menu, "home.quit"),
        }
    }

    fn on_pressed(self, context: &mut GameUiContext) -> bool {
        const CLOSE_ALL_OTHERS: bool = false;
        match self {
            Self::Continue => on_continue(),
            Self::Campaign => on_campaign(context),
            Self::CustomGame => super::open(DialogMenuKind::CustomGame, CLOSE_ALL_OTHERS, context),
            Self::LoadGame => super::open(DialogMenuKind::LoadGame, CLOSE_ALL_OTHERS, context),
            Self::Settings => super::open(DialogMenuKind::MainSettings, CLOSE_ALL_OTHERS, context),
            Self::About => super::open(DialogMenuKind::About, CLOSE_ALL_OTHERS, context),
            Self::Quit => {
                GameLoop::get_mut().quit_game();
                true
            }
        }
    }
}

// ----------------------------------------------
// Continue: the last save
// ----------------------------------------------

// What the last save is, as Continue shows it.
#[derive(Clone)]
enum LastSave {
    None,                 // Nothing saved yet.
    Missing(String),      // The profile names a save that's gone.
    Unreadable(String),   // One that can't be read.
    Incompatible(String), // One of another save version.
    Loadable { save: String, mission: Option<(usize, usize)> }, // Its campaign and mission, if any.
}

// What Continue shows: the save it loads (None: it's disabled), its subline, and why it's disabled.
struct ContinueInfo {
    save: Option<String>,
    subline: Option<String>, // "<mission> · <campaign> · autosave", or the save's name.
    tooltip: Option<String>,
}

// The last save, kept between builds of the home menu, so a build doesn't read the save's head
// again. Read again when the profile or the saves change (a save deleted in Load / Save), or the
// home menu opens (forget_last_save). Its text is made at each build, in the language of the moment.
struct LastSaveCache {
    revision: (u32, u32), // The profile's and the saves' (save_header::revision) it was read at.
    last_save: Option<LastSave>,
}

static LAST_SAVE_CACHE: UiStaticVar<LastSaveCache> = UiStaticVar::new(LastSaveCache { revision: (0, 0), last_save: None });

// Reads the last save again when the home menu is next built: it may have changed under the same
// name (the autosave), which the profile doesn't see.
pub(super) fn forget_last_save() {
    LAST_SAVE_CACHE.as_mut().last_save = None;
}

fn last_save() -> LastSave {
    let cache = LAST_SAVE_CACHE.as_mut();
    let revision = (profile::revision(), save_header::revision());
    if cache.revision != revision || cache.last_save.is_none() {
        cache.revision = revision;
        cache.last_save = Some(read_last_save());
    }
    cache.last_save.clone().unwrap()
}

fn read_last_save() -> LastSave {
    let Some(save) = profile::get().last_save.clone() else {
        return LastSave::None;
    };
    // Its case aside: a file system that ignores case keeps a file's old name when it's saved over
    // under another case (session.save), and loads it under either.
    let listed = |file: &PathBuf| file.to_str().is_some_and(|file| file.eq_ignore_ascii_case(&save));
    if !storage::list_save_files().iter().any(listed) {
        return LastSave::Missing(save);
    }
    match session::probe_save(PathRef::from_str(&save)) {
        Err(err) => {
            log::error!(log::channel!("session"), "Can't read the last save '{save}': {err}");
            LastSave::Unreadable(save)
        }
        Ok(summary) if !summary.loadable => LastSave::Incompatible(save),
        Ok(summary) => LastSave::Loadable { save, mission: summary.mission() },
    }
}

// The save's name as the subline shows it: "autosave" for the autosave.
fn save_display_name(save: &str) -> String {
    if save == storage::AUTOSAVE_FILE_NAME.as_str() {
        ui::text::get(UiTextCategory::Menu, "home.continue.autosave").to_string()
    } else {
        save.to_string()
    }
}

fn continue_info() -> ContinueInfo {
    let disabled = |key: &'static str, save: &str| {
        let name = save_display_name(save);
        let tooltip = ui::text::format(UiTextCategory::Menu, key, &[("save", &name)]);
        ContinueInfo { save: None, subline: Some(name), tooltip: Some(tooltip) }
    };
    match last_save() {
        LastSave::None => {
            let tooltip = ui::text::get(UiTextCategory::Menu, "home.continue.no_save").to_string();
            ContinueInfo { save: None, subline: None, tooltip: Some(tooltip) }
        }
        LastSave::Missing(save) => disabled("home.continue.missing", &save),
        LastSave::Unreadable(save) => disabled("home.continue.unreadable", &save),
        LastSave::Incompatible(save) => disabled("home.continue.incompatible", &save),
        LastSave::Loadable { save, mission } => {
            let name = save_display_name(&save);
            let configs = CampaignConfigs::get();
            let mission = mission.and_then(|(campaign_index, mission_index)| {
                let mission = configs.campaign(campaign_index)?.mission_display_name(mission_index)?;
                Some((mission.to_string(), campaign_number(campaign_index)))
            });
            let subline = match mission {
                Some((mission, campaign)) => ui::text::format(
                    UiTextCategory::Menu,
                    "home.continue.subline",
                    &[("mission", &mission), ("campaign", &campaign), ("save", &name)],
                ),
                None => name,
            };
            ContinueInfo { save: Some(save), subline: Some(subline), tooltip: None }
        }
    }
}

fn on_continue() -> bool {
    match last_save() {
        LastSave::Loadable { save, .. } => {
            GameLoop::get_mut().load_save_game(PathRef::from_str(&save));
            true
        }
        _ => false,
    }
}

// ----------------------------------------------
// Campaign
// ----------------------------------------------

// "Campaign I": a campaign by its place among the visible campaigns in campaigns.json. A hidden
// (test) campaign goes by its name.
pub(super) fn campaign_number(campaign_index: usize) -> String {
    let configs = CampaignConfigs::get();
    match configs.campaign_number(campaign_index) {
        Some(number) => ui::text::format(UiTextCategory::Menu, "campaign.number", &[("number", &roman_numeral(number))]),
        None => configs.campaign(campaign_index).map_or_else(String::new, |campaign| campaign.display_name().to_string()),
    }
}

// 1 → "I", 4 → "IV", 12 → "XII".
pub(super) fn roman_numeral(mut number: usize) -> String {
    const NUMERALS: [(usize, &str); 13] = [
        (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I"),
    ];
    let mut text = String::new();
    for (value, numeral) in NUMERALS {
        while number >= value {
            text.push_str(numeral);
            number -= value;
        }
    }
    text
}

// The current campaign, which Campaign shows and the campaign row and the picker ring: the latest
// the player can play. None without campaigns.
pub(super) fn current_campaign() -> Option<usize> {
    profile::get().playable_campaigns(CampaignConfigs::get()).last().copied()
}

// The emblem of the campaign at `campaign_index`, in the campaign row and on its picker card:
// ringed for the current campaign, plain for another the player can play, else locked.
pub(super) fn emblem_state(campaign_index: usize) -> EmblemState {
    if !profile::get().is_playable(CampaignConfigs::get(), campaign_index) {
        EmblemState::Locked
    } else if current_campaign() == Some(campaign_index) {
        EmblemState::Current
    } else {
        EmblemState::Idle
    }
}

// What unlocks the campaign at `campaign_index`: "Finish Campaign I to unlock".
pub(super) fn unlock_text(campaign_index: usize) -> String {
    let configs = CampaignConfigs::get();
    let previous_index = configs.visible().map(|(index, _)| index).filter(|&index| index < campaign_index).last().unwrap_or(0);
    let previous = campaign_number(previous_index);
    ui::text::format(UiTextCategory::Menu, "campaign.unlock", &[("campaign", &previous)])
}

// What Campaign's subline says of the campaign at `campaign_index`: "Campaign I · mission 6 of 10",
// or that it has no missions yet.
fn campaign_progress_text(campaign_index: usize) -> String {
    let configs = CampaignConfigs::get();
    let number = campaign_number(campaign_index);
    let Some(campaign) = configs.campaign(campaign_index) else {
        return number;
    };
    let count = campaign.mission_count();
    match profile::get().next_mission(configs, campaign_index) {
        Some(mission) => ui::text::format(
            UiTextCategory::Menu,
            "home.campaign.subline",
            &[("campaign", &number), ("mission", &campaign.mission_number(mission).to_string()), ("count", &count.to_string())],
        ),
        None => ui::text::format(UiTextCategory::Menu, "home.campaign.subline.soon", &[("campaign", &number)]),
    }
}

// Starts the campaign at `campaign_index` at `mission_index`: loads its map, which leaves the home
// menu (GameSession::start_mission takes it from there). A mission whose save map this game can't
// load (missing, unreadable or of another version) starts nothing and leaves the campaign as it was;
// a load that fails all the same cancels the start (campaign::cancel_mission_start).
pub(super) fn start_campaign_mission(campaign_index: usize, mission_index: usize) -> bool {
    if let Some(MissionMap::SaveGame { save_file }) =
        CampaignConfigs::get().mission(campaign_index, mission_index).map(|mission| &mission.map)
    {
        match session::probe_save(PathRef::from_str(save_file)) {
            Ok(summary) if summary.loadable => {}
            Ok(_) => {
                log::error!(log::channel!("campaign"), "Can't start the mission: its map '{save_file}' is of another save version.");
                return false;
            }
            Err(err) => {
                log::error!(log::channel!("campaign"), "Can't start the mission: its map '{save_file}' doesn't load: {err}");
                return false;
            }
        }
    }
    match campaign::start_mission(campaign_index, mission_index) {
        Some(map) => {
            super::load_mission_map(&map);
            true
        }
        None => {
            log::error!(log::channel!("campaign"), "Campaign {campaign_index} has no mission {mission_index} to start.");
            false
        }
    }
}

// More than one campaign to play: the picker. One: its next mission (with none reached, the first
// campaign's first mission).
fn on_campaign(context: &mut GameUiContext) -> bool {
    let configs = CampaignConfigs::get();
    let playable = profile::get().playable_campaigns(configs);
    if playable.len() > 1 {
        return super::open(DialogMenuKind::CampaignPicker, false, context);
    }
    let Some(&campaign_index) = playable.first() else {
        log::error!(log::channel!("campaign"), "No campaign configured to start!");
        return false;
    };
    match profile::get().next_mission(configs, campaign_index) {
        Some(mission_index) => start_campaign_mission(campaign_index, mission_index),
        None => false,
    }
}

// ----------------------------------------------
// Home
// ----------------------------------------------

pub struct Home {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { Home, ["Heritage Builder"] }

impl Home {
    pub fn new(context: &mut GameUiContext) -> Self {
        // Sizes at the content area's scale; text at that over the UI scale the themes give it
        // (dialog_content_text_scale), so both grow as much as the panel does.
        let kind = Self::KIND.to_string();
        let window = ui::screen_size(context.ui_sys);
        let scale = skin::dialog_content_scale(&kind, window);
        let text = skin::dialog_content_text_scale(&kind, window);
        let flags = DialogFrame::from_skin(Self::KIND).menu_flags();
        let mut menu = skin::dialog(context, &kind, flags).with_widget_spacing(Vec2::new(0.0, HOME_WIDGET_SPACING * scale));

        if let Some(crest) = crest(context, HOME_CREST_HEIGHT, scale) {
            menu.add_widget(crest);
        }
        let title = UiString::key(UiTextCategory::Menu, "home.title");
        menu.add_widget(skin::heading_scaled(context, [title], HeadingLevel::Title, scale, text));
        menu.add_widget(skin::heading_scaled(context, [UiString::from(subtitle())], HeadingLevel::Subtitle, scale, text));
        menu.add_widget(skin::separator(context, "rule"));
        menu.add_widget(campaign_row(context, scale, text));
        menu.add_widget(menu_items(context, scale, text));
        menu.add_widget(version(text));

        Self { menu: menu.into_rc() }
    }
}

// The realm's crest over the title, if its skin has one: centered, at most `max_height` tall (times
// `scale`). About shows it too.
pub(super) fn crest(context: &mut GameUiContext, max_height: f32, scale: f32) -> Option<UiWidgetGroup> {
    let (texture, size) = skin::ornament(context, "crest")?;
    let height = size.y.min(max_height) * scale;
    let size = Vec2::new(size.x * height / size.y.max(1.0), height);
    Some(UiWidgetGroup::new().with_widget(UiImage::from_texture(texture, size, size).with_label("crest")))
}

// The line under the title: the campaign of the realm the menu shows. About shows it too.
pub(super) fn subtitle() -> String {
    let realm = skin::current_realm();
    CampaignConfigs::get()
        .visible()
        .map(|(_, campaign)| campaign)
        .find(|campaign| campaign.realm == realm)
        .map_or_else(|| realm.display_name().to_string(), |campaign| campaign.display_name().to_string())
}

// One emblem per campaign, side by side: the current campaign's (the latest the player can play)
// ringed, the others playable or locked, each with its realm's name under it ("Locked" if locked).
// Each emblem is labeled by its campaign's number (Home/Campaign II), its tooltip the campaign's
// name, or what unlocks it. Sizes at `scale`, the names at `text_scale`.
fn campaign_row(context: &mut GameUiContext, scale: f32, text_scale: f32) -> UiColumns {
    let configs = CampaignConfigs::get();
    let emblem_size = Vec2::new(HOME_EMBLEM_SIZE, HOME_EMBLEM_SIZE) * scale;
    let mut columns = UiColumns::new().with_spacing(8.0 * scale);

    for (index, campaign) in configs.visible() {
        let state = emblem_state(index);
        let (name, color, tooltip) = if state != EmblemState::Locked {
            (campaign.realm.display_name().to_string(), skin::color("ink"), campaign.display_name().to_string())
        } else {
            (ui::text::get(UiTextCategory::Menu, "home.locked").to_string(), skin::color("sub"), unlock_text(index))
        };

        let mut group = UiWidgetGroup::new().with_widget_spacing(Vec2::new(0.0, 4.0 * scale));
        if let Some((texture, size)) = skin::emblem(context, campaign.realm, state) {
            let image = UiImage::from_texture(texture, size, emblem_size).with_label(campaign_number(index)).with_tooltip(skin::tooltip(context, tooltip));
            group.add_widget(image);
        }
        let label = UiText::new(name, UiFontScale(HOME_EMBLEM_LABEL_FONT_SCALE * text_scale));
        let label = match color {
            Some(color) => label.with_color(color),
            None => label,
        };
        group.add_widget(UiMenuHeading::new([label]).with_margin_top(0.0).with_margin_bottom(0.0));
        columns.add_column(UiColumnWidth::Fraction(1.0), group);
    }
    columns
}

// The menu's items, Continue's and Campaign's with their sublines, the disabled ones with a tooltip
// that says why. Sized at `scale`, their text at `text_scale`.
fn menu_items(context: &mut GameUiContext, scale: f32, text_scale: f32) -> UiWidgetGroup {
    let mut group = UiWidgetGroup::new().with_widget_spacing(Vec2::new(0.0, HOME_ITEM_SPACING * scale));

    for item_kind in HomeItem::ALL {
        let (subline, enabled, tooltip) = match item_kind {
            HomeItem::Continue if !super::in_home_menu() => (None, false, None),
            HomeItem::Continue => {
                let info = continue_info();
                (info.subline, info.save.is_some(), info.tooltip)
            }
            HomeItem::Campaign => campaign_item_state(),
            _ => (None, true, None),
        };

        let mut item = skin::menu_item_scaled(context, item_kind.label(), subline.map(UiString::from), scale * HOME_ITEM_SCALE, text_scale * HOME_ITEM_SCALE)
            .with_enabled(enabled)
            .on_pressed(move |_, context| {
                item_kind.on_pressed(context_as_mut::<GameUiContext>(context));
            });
        if let Some(tooltip) = tooltip {
            item = item.with_tooltip(skin::tooltip(context, tooltip));
        }
        group.add_widget(item);
    }
    group
}

// Campaign's subline (the shown campaign's progress), whether it can be pressed, and why not.
fn campaign_item_state() -> (Option<String>, bool, Option<String>) {
    let configs = CampaignConfigs::get();
    let Some(shown) = current_campaign() else {
        return (None, false, Some(ui::text::get(UiTextCategory::Menu, "home.coming_soon").to_string()));
    };
    let playable = profile::get().playable_campaigns(configs).len();
    let startable = profile::get().next_mission(configs, shown).is_some();
    let tooltip = (!startable && playable < 2).then(|| ui::text::get(UiTextCategory::Menu, "home.coming_soon").to_string());
    (Some(campaign_progress_text(shown)), startable || playable > 1, tooltip)
}

// "Version 0.1.0", small, in the skin's `sub` colour.
fn version(scale: f32) -> UiMenuHeading {
    let text = ui::text::format(UiTextCategory::Menu, "home.version", &[("version", crate::version())]);
    let line = UiText::new(text, UiFontScale(HOME_VERSION_FONT_SCALE * scale));
    let line = match skin::color("sub") {
        Some(color) => line.with_color(color),
        None => line,
    };
    UiMenuHeading::new([line]).with_margin_top(0.0).with_margin_bottom(0.0)
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roman_numerals() {
        let cases = [(1, "I"), (2, "II"), (3, "III"), (4, "IV"), (9, "IX"), (10, "X"), (12, "XII"), (14, "XIV"), (40, "XL"), (1999, "MCMXCIX")];
        for (number, numeral) in cases {
            assert_eq!(roman_numeral(number), numeral);
        }
        assert_eq!(roman_numeral(0), "");
    }
}
