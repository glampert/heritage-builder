// The Load / Save dialog (docs/plans/realm-ui.md, Phase 1, Load / Save; menu-load-save-<realm>.jpg):
// one dialog, LoadOrSaveGame, for loading and saving. The LoadGame and SaveGame kinds open it, in
// load or save mode (redirect): its primary button and what it starts on differ.
//
// - At the left, a row per save, newest first (its header's save time; saves without one after,
//   by name): the realm's emblem, its name, where it was played ("Campaign I · mission 2 · <city>",
//   or a custom game), the time it was saved and its in-game date as its realm writes it. Old saves,
//   without a header, show "—".
// - At the right, the preview of the save the "Save as" field names: its thumbnail and its mission,
//   campaign, date, population, treasury and time played.
// - Under the list, the "Save as" field: picking a row puts its name there, and typing a save's
//   name picks its row. Delete (confirmed in a popup), Cancel, Save (an existing name asks first,
//   in the Overwrite popup) and Load.
//
// The saves are read (their headers only: save_header::read_head) when the dialog opens, and again
// when a save, a thumbnail or a deletion changes them (save_header::revision) or the window's size
// changes how large the dialog is drawn: dialog::sync builds the dialog again then. What the player
// picked and typed is kept outside the dialog (STATE), so a rebuild keeps it.
//
// Paths: LoadOrSaveGame/item_list (its value the picked save), LoadOrSaveGame/<save>,
// LoadOrSaveGame/Save as (the field), LoadOrSaveGame/preview, LoadOrSaveGame/details,
// LoadOrSaveGame/Delete|Cancel|Save|Load; the popups' Overwrite Save Game Popup/Yes|No and
// Delete Save Game Popup/Delete|Cancel.

use std::sync::atomic::{AtomicU32, Ordering};

use common::Size;
use engine::{
    file_sys::paths::PathRef,
    log,
    render::texture::TextureHandle,
    save::storage,
};

use super::*;
use crate::{
    GameLoop,
    campaign::config::CampaignConfigs,
    config::GameConfigs,
    menu::skin::EmblemState,
    realm::Realm,
    save_header::{self, SaveHeader},
    session::{self, SaveSummary},
};

// ----------------------------------------------
// Layout (points on a 1920 × 1080 window, scaled by the dialog's scale: skin::dialog_scale)
// ----------------------------------------------

const PREVIEW_WIDTH: f32 = 440.0;     // The preview column's.
const COLUMN_SPACING: f32 = 56.0;
const PART_SPACING: f32 = 12.0;       // Between the parts of a column, and the columns and buttons.
const ROW_ICON_SIZE: f32 = 52.0;
const ROW_PADDING: [f32; 2] = [14.0, 9.0];
const ROW_SPACING: f32 = 4.0;
const RIGHT_COLUMN: f32 = 0.6;        // Where a row's time and date start, in its text's width.
const LIST_SCROLL_BAR_WIDTH: f32 = 12.0; // About what the list's scroll bar takes from its rows.
const LIST_MIN_HEIGHT: f32 = 100.0;   // At least this tall, whatever the window.
const COLUMNS_SLACK: f32 = 24.0;      // Room left under the columns for what measuring misses.
const DELETE_WIDTH: f32 = 200.0;
const CANCEL_WIDTH: f32 = 180.0;
const SAVE_WIDTH: f32 = 180.0;
const LOAD_WIDTH: f32 = 170.0;
const BUTTON_SPACING: f32 = 18.0;
// Sizes on the reference window and on the smallest (1280 × 720), in proportion between them
// (by_window): the text size over the body font, and the buttons' height.
const TEXT_SCALES: (f32, f32) = (1.2, 0.95);
const BUTTON_HEIGHTS: (f32, f32) = (56.0, 36.0);
const SECTION_FONT_SCALE: f32 = 0.78; // "SAVED GAME", "SAVED", "SAVE AS", over the text size.
const ROW_SUBTITLE_SCALE: f32 = 0.8;
const DETAILS_FONT_SCALE: f32 = 0.85; // The preview's details, over the text size.
const DETAILS_ROW_PADDING: f32 = 4.0; // Above and below each detail's text.

// The thumbnail's JPEG fits whatever size it has into the preview: contained, on the well panel.
const PREVIEW_FIT: UiImageFit = UiImageFit::Contain;

// ----------------------------------------------
// Mode
// ----------------------------------------------

// What the dialog was opened for: its primary button, and what it starts on (load: the newest save;
// save: no save picked, a new name).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(super) enum LoadSaveMode {
    Load,
    Save,
}

// The dialog kinds that open this dialog, and in which mode: LoadGame and SaveGame are this dialog
// in load or save mode, and LoadOrSaveGame itself opens in load mode, as its mockup shows it.
pub(super) fn redirect(kind: DialogMenuKind) -> Option<LoadSaveMode> {
    match kind {
        DialogMenuKind::LoadGame | DialogMenuKind::LoadOrSaveGame => Some(LoadSaveMode::Load),
        DialogMenuKind::SaveGame => Some(LoadSaveMode::Save),
        _ => None,
    }
}

// ----------------------------------------------
// State, kept across rebuilds
// ----------------------------------------------

// A save in the list: its name and what its head says (Err: it can't be read).
struct SaveEntry {
    name: String,
    summary: Result<SaveSummary, ()>,
}

impl SaveEntry {
    fn header(&self) -> Option<&SaveHeader> {
        self.summary.as_ref().ok().map(|summary| &summary.header)
    }

    fn loadable(&self) -> bool {
        self.summary.as_ref().is_ok_and(|summary| summary.loadable)
    }
}

// The thumbnail the preview shows, as a texture of its own.
struct Preview {
    save: String,
    texture: Option<(TextureHandle, Vec2)>, // None: the save has no thumbnail.
}

struct LoadSaveState {
    mode: LoadSaveMode,
    saves: Vec<SaveEntry>,
    field: String,           // The "Save as" field's text.
    built: (u32, u32),       // build_key() when the dialog was built.
    preview: Option<Preview>,
    preview_revision: u32,       // save_header::revision() its thumbnail was read at.
    retired: Vec<TextureHandle>, // Thumbnails replaced, released before the next frame draws.
    synced: bool,            // The list's selection, the preview and the buttons follow `field`.
}

impl LoadSaveState {
    // The save the field names, by its index in the list.
    fn selected(&self) -> Option<usize> {
        let name = self.field.trim();
        self.saves.iter().position(|entry| entry.name == name)
    }

    fn selected_entry(&self) -> Option<&SaveEntry> {
        self.selected().map(|index| &self.saves[index])
    }
}

static STATE: UiStaticVar<LoadSaveState> = UiStaticVar::new(LoadSaveState {
    mode: LoadSaveMode::Load,
    saves: Vec::new(),
    field: String::new(),
    built: (0, 0),
    preview: None,
    preview_revision: u32::MAX,
    retired: Vec::new(),
    synced: false,
});

// What the dialog is built from, besides the skin and the strings: the saves (their revision) and
// how large the window draws the dialog (in hundredths).
fn build_key(context: &GameUiContext) -> (u32, u32) {
    let scale = skin::dialog_scale(ui::screen_size(context.ui_sys));
    (save_header::revision(), (scale * 100.0).round() as u32)
}

// Reads the saves for the dialog about to open in `mode`, and picks what it starts on.
pub(super) fn prepare(mode: LoadSaveMode) {
    let state = STATE.as_mut();
    state.mode = mode;
    state.saves = read_saves();
    state.field = match mode {
        LoadSaveMode::Load => state.saves.first().map(|entry| entry.name.clone()).unwrap_or_default(),
        LoadSaveMode::Save => storage::DEFAULT_SAVE_FILE_NAME.as_str().to_string(),
    };
    state.synced = false;
}

// Whether the open dialog must be built again: the saves changed, or the window's size changed
// the dialog's. Reads the saves again if so, keeping what the field names.
pub(super) fn needs_rebuild(context: &GameUiContext) -> bool {
    let state = STATE.as_mut();
    if state.built == build_key(context) {
        return false;
    }
    if state.built.0 != save_header::revision() {
        state.saves = read_saves();
    }
    state.synced = false;
    true
}

// Every save in saves/, newest first by the time its header says it was saved; then those without
// one, by name.
fn read_saves() -> Vec<SaveEntry> {
    let mut saves: Vec<SaveEntry> = storage::list_save_files()
        .iter()
        .map(|file| {
            let name = file.to_string_lossy().into_owned();
            let summary = session::probe_save(PathRef::from_str(&name)).map_err(|err| {
                log::error!(log::channel!("session"), "Can't read the save '{name}': {err}");
            });
            SaveEntry { name, summary }
        })
        .collect();

    let saved_at = |entry: &SaveEntry| entry.header().and_then(|header| header.saved_at.clone());
    saves.sort_by(|a, b| match (saved_at(a), saved_at(b)) {
        (Some(a_time), Some(b_time)) => b_time.cmp(&a_time).then_with(|| a.name.cmp(&b.name)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.name.cmp(&b.name),
    });
    saves
}

// ----------------------------------------------
// Text
// ----------------------------------------------

fn none_text() -> String {
    ui::text::get(UiTextCategory::Menu, "load_save.none").to_string()
}

// 1234567 → "1,234,567", with the language's separator.
pub(crate) fn group_digits(value: i64) -> String {
    let separator = ui::text::get(UiTextCategory::Menu, "number.thousands_separator");
    let digits = value.unsigned_abs().to_string();
    let mut text = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            text.push_str(separator);
        }
        text.push(digit);
    }
    if value < 0 { format!("-{text}") } else { text }
}

// "4 h 12 min", "0 h 47 min": always with the hours, as the mockup writes it.
fn time_played_text(secs: f64) -> String {
    let minutes = (secs.max(0.0) / 60.0).floor() as u64;
    let (hours, minutes) = (minutes / 60, minutes % 60);
    ui::text::format(
        UiTextCategory::Menu,
        "load_save.time_played",
        &[("hours", &hours.to_string()), ("minutes", &format!("{minutes:02}"))],
    )
}

// When a save was made, as the list shows it: "Today, 21:14", "Yesterday, 23:40" or "4 Nov 2026",
// from its header's saved_at and today's date (None if unknown).
fn saved_text(saved_at: &str, today: Option<(i32, u32, u32)>) -> Option<String> {
    let (year, month, day, hour, minute) = save_header::parse_saved_at(saved_at)?;
    let time = format!("{hour:02}:{minute:02}");
    if let Some(today) = today {
        if today == (year, month, day) {
            return Some(ui::text::format(UiTextCategory::Menu, "load_save.saved.today", &[("time", &time)]));
        }
        if previous_day(today) == (year, month, day) {
            return Some(ui::text::format(UiTextCategory::Menu, "load_save.saved.yesterday", &[("time", &time)]));
        }
    }
    let month = month_name(month);
    Some(ui::text::format(
        UiTextCategory::Menu,
        "load_save.saved.date",
        &[("day", &day.to_string()), ("month", month), ("year", &year.to_string())],
    ))
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => ui::text::get(UiTextCategory::Menu, "month.short.1"),
        2 => ui::text::get(UiTextCategory::Menu, "month.short.2"),
        3 => ui::text::get(UiTextCategory::Menu, "month.short.3"),
        4 => ui::text::get(UiTextCategory::Menu, "month.short.4"),
        5 => ui::text::get(UiTextCategory::Menu, "month.short.5"),
        6 => ui::text::get(UiTextCategory::Menu, "month.short.6"),
        7 => ui::text::get(UiTextCategory::Menu, "month.short.7"),
        8 => ui::text::get(UiTextCategory::Menu, "month.short.8"),
        9 => ui::text::get(UiTextCategory::Menu, "month.short.9"),
        10 => ui::text::get(UiTextCategory::Menu, "month.short.10"),
        11 => ui::text::get(UiTextCategory::Menu, "month.short.11"),
        _ => ui::text::get(UiTextCategory::Menu, "month.short.12"),
    }
}

// The day before (year, month, day) in the Gregorian calendar.
fn previous_day((year, month, day): (i32, u32, u32)) -> (i32, u32, u32) {
    if day > 1 {
        return (year, month, day - 1);
    }
    let (year, month) = if month > 1 { (year, month - 1) } else { (year - 1, 12) };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (year, month, days)
}

fn today() -> Option<(i32, u32, u32)> {
    engine::platform::local_date_time().map(|now| (now.year, now.month, now.day))
}

// The campaign mission a header names: its campaign's number ("Campaign I"), the mission's number
// and its name. None outside a campaign, or for one the configs no longer have.
fn mission_of(entry: &SaveEntry) -> Option<(String, usize, String)> {
    let (campaign_index, mission_index) = entry.summary.as_ref().ok()?.mission()?;
    let campaign = CampaignConfigs::get().campaign(campaign_index)?;
    let name = campaign.mission_display_name(mission_index)?.to_string();
    Some((super::home::campaign_number(campaign_index), campaign.mission_number(mission_index), name))
}

// Where a save was played, under its name: "Campaign I · mission 2 · <city>" (the city the mission
// founds, or the mission's name for one that founds none), "Custom game · <map>", or why it can't
// be loaded. A game
// that's neither (a preset or new map from automation or the developer editor, or a mission the
// configs no longer have) shows its city's name, or "—".
fn place_text(entry: &SaveEntry) -> String {
    let Ok(summary) = &entry.summary else {
        return ui::text::get(UiTextCategory::Menu, "load_save.unreadable").to_string();
    };
    if !summary.loadable {
        return ui::text::get(UiTextCategory::Menu, "load_save.incompatible").to_string();
    }
    if summary.header == SaveHeader::default() {
        return none_text(); // A save from before headers.
    }
    let city = summary.header.city_name.clone();
    match mission_of(entry) {
        Some((campaign, mission, mission_name)) => ui::text::format(
            UiTextCategory::Menu,
            "load_save.campaign_place",
            &[("campaign", &campaign), ("mission", &mission.to_string()), ("place", &city.unwrap_or(mission_name))],
        ),
        // A custom game: "Custom game · <city>", or its map's name until custom games name their
        // cities.
        None if summary.header.custom_game.is_some() => match city.or_else(|| custom_map_name(&summary.header)) {
            Some(place) => ui::text::format(UiTextCategory::Menu, "load_save.custom_game_place", &[("place", &place)]),
            None => ui::text::get(UiTextCategory::Menu, "load_save.custom_game").to_string(),
        },
        None => city.unwrap_or_else(none_text),
    }
}

// The name of the map a custom game was started on, from its header: maps.json's, or its id for a
// map the configs no longer have.
fn custom_map_name(header: &SaveHeader) -> Option<String> {
    let id = header.custom_game.as_ref()?.map.as_deref()?;
    Some(custom_map_display_name(id))
}

// The name of the map with id `id` in maps.json, or the id for a map the configs don't have. The
// Game menu names the custom game's map with it too.
pub(super) fn custom_map_display_name(id: &str) -> String {
    let configs = crate::custom_game::MapConfigs::is_initialized().then(crate::custom_game::MapConfigs::get);
    let map = configs.and_then(|configs| configs.maps.iter().find(|map| map.id == id));
    map.map_or_else(|| id.to_string(), |map| map.display_name().to_string())
}

// The preview's label and value pairs for `entry` (all "—" for none).
fn details(entry: Option<&SaveEntry>) -> Vec<(UiString, UiString)> {
    let header = entry.and_then(SaveEntry::header);
    let mission = entry.and_then(mission_of);
    let none = none_text();
    let custom_game = header.is_some_and(|header| header.custom_game.is_some()) && mission.is_none();

    let mission_name = match &mission {
        Some((_, _, name)) => name.clone(),
        None if custom_game => ui::text::get(UiTextCategory::Menu, "load_save.custom_game").to_string(),
        None => none.clone(),
    };
    let campaign = match &mission {
        Some((campaign, number, _)) => {
            ui::text::format(UiTextCategory::Menu, "load_save.campaign", &[("campaign", campaign), ("mission", &number.to_string())])
        }
        // A custom game's map ("Delta Flood Plain", as the mockup shows it).
        None => header.and_then(custom_map_name).unwrap_or_else(|| none.clone()),
    };
    let date = header.and_then(|header| header.date_text.clone()).unwrap_or_else(|| none.clone());
    let population = header.and_then(|header| header.population).map_or_else(|| none.clone(), |value| group_digits(i64::from(value)));
    let treasury = header.and_then(|header| header.treasury).map_or_else(|| none.clone(), group_digits);
    let played = header.and_then(|header| header.time_played_secs).map_or_else(|| none.clone(), time_played_text);

    vec![
        (UiString::key(UiTextCategory::Menu, "load_save.details.mission"), UiString::from(mission_name)),
        (UiString::key(UiTextCategory::Menu, "load_save.details.campaign"), UiString::from(campaign)),
        (UiString::key(UiTextCategory::Menu, "load_save.details.date"), UiString::from(date)),
        (UiString::key(UiTextCategory::Menu, "load_save.details.population"), UiString::from(population)),
        (UiString::key(UiTextCategory::Menu, "load_save.details.treasury"), UiString::from(treasury)),
        (UiString::key(UiTextCategory::Menu, "load_save.details.time_played"), UiString::from(played)),
    ]
}

// ----------------------------------------------
// LoadGame / SaveGame: kinds that open LoadOrSaveGame
// ----------------------------------------------

// Never opened themselves: the dialog framework opens LoadOrSaveGame in their place (redirect).
pub struct LoadGame {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { LoadGame, ["Load Game"] }

impl LoadGame {
    pub fn new(_context: &mut GameUiContext) -> Self {
        Self { menu: UiMenu::new(Self::KIND.to_string()).into_rc() }
    }
}

pub struct SaveGame {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { SaveGame, ["Save Game"] }

impl SaveGame {
    pub fn new(_context: &mut GameUiContext) -> Self {
        Self { menu: UiMenu::new(Self::KIND.to_string()).into_rc() }
    }
}

// ----------------------------------------------
// LoadOrSaveGame
// ----------------------------------------------

// Where its widgets are, to follow the field each frame (sync).
#[derive(Copy, Clone)]
struct Parts {
    columns: UiMenuWidgetIndex,
    left: UiColumnsColumnIndex,
    right: UiColumnsColumnIndex,
    list: UiWidgetGroupWidgetIndex,
    image: UiWidgetGroupWidgetIndex,
    details: UiWidgetGroupWidgetIndex,
    buttons: UiMenuWidgetIndex,
    delete_column: UiColumnsColumnIndex,
    actions_column: UiColumnsColumnIndex,
}

pub struct LoadOrSaveGame {
    menu: UiMenuRcMut,
    parts: Parts,
}

impl DialogMenuFactory for LoadOrSaveGame {
    const KIND: DialogMenuKind = DialogMenuKind::LoadOrSaveGame;
    const TITLE: &'static [&'static str] = &["Load / Save"];

    fn create(context: &mut GameUiContext) -> DialogMenuImpl {
        DialogMenuImpl::from(LoadOrSaveGame::new(context))
    }
}

impl DialogMenu for LoadOrSaveGame {
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
        self.sync(context);
        self.menu_mut().draw(context);
    }
}

impl LoadOrSaveGame {
    pub fn new(context: &mut GameUiContext) -> Self {
        let state = STATE.as_mut();
        state.built = build_key(context);
        state.synced = false;

        let window = ui::screen_size(context.ui_sys);
        let scale = skin::dialog_scale(window);
        let text_scale = by_window(TEXT_SCALES, scale);

        let title = skin::heading(context, [UiString::key(UiTextCategory::Menu, "load_save.title")], HeadingLevel::Dialog);
        let flags = DialogFrame::from_skin(Self::KIND).menu_flags();
        let mut menu = skin::dialog(context, &Self::KIND.to_string(), flags).with_widget_spacing(Vec2::new(0.0, PART_SPACING * scale));

        let buttons_row = buttons(context, scale);
        let field = save_as_field(context, text_scale);
        let headers = list_headers(
            section_label(UiString::key(UiTextCategory::Menu, "load_save.column.saved_game"), text_scale),
            section_label(UiString::key(UiTextCategory::Menu, "load_save.column.saved"), text_scale),
            scale,
        );
        let save_as = section_label(UiString::key(UiTextCategory::Menu, "load_save.save_as"), text_scale);

        // The columns take the content area's height the title and the buttons leave; in its
        // column, the list takes what its headers, the field and its label leave.
        let spacing = PART_SPACING * scale;
        let content = skin::dialog_content_size(&Self::KIND.to_string(), window);
        let columns_height = content.y - title.measure(context).y - buttons_row.measure(context).y - spacing * 2.0 - COLUMNS_SLACK * scale;
        let list_height = columns_height - headers.measure(context).y - save_as.measure(context).y - field.measure(context).y - spacing * 3.0;
        let list_height = list_height.floor().max(LIST_MIN_HEIGHT * scale);

        menu.add_widget(title);

        // The list column.
        let mut left_group = UiWidgetGroup::new().with_widget_spacing(Vec2::new(0.0, spacing));
        left_group.add_widget(headers);
        let list = left_group.add_widget(save_list(context, scale, text_scale, list_height));
        left_group.add_widget(save_as);
        left_group.add_widget(field);

        // The preview column.
        let preview_width = (PREVIEW_WIDTH * scale).round();
        let thumbnail = GameConfigs::get().save.thumbnail_size;
        let aspect = if thumbnail.is_valid() { thumbnail.height as f32 / thumbnail.width as f32 } else { 9.0 / 16.0 };
        let mut right_group = UiWidgetGroup::new().with_widget_spacing(Vec2::new(0.0, spacing * 1.5));
        let look = UiRowLook { padding: DETAILS_ROW_PADDING * scale, ..skin::row_look() };
        let mut key_values = skin::key_values().with_label("details").with_look(look).with_font_scale(UiFontScale(text_scale * DETAILS_FONT_SCALE));
        key_values.set_pairs(details(None));
        // The thumbnail as large as the column's width, or as the details leave under it.
        let details_height = key_values.measure(context).y;
        let image_height = (preview_width * aspect).min(columns_height - details_height - spacing * 1.5).max(1.0).floor();
        let image = UiImage::empty(Vec2::new(preview_width, image_height)).with_label("preview").with_fit(PREVIEW_FIT);
        let image = right_group.add_widget(skin::framed_image(context, image, "well"));
        let details = right_group.add_widget(key_values);

        let mut columns = UiColumns::new().with_spacing((COLUMN_SPACING * scale).round());
        let left = columns.add_column(UiColumnWidth::Fraction(1.0), left_group);
        let right = columns.add_column(UiColumnWidth::Fixed(preview_width), right_group);
        let columns = menu.add_widget(columns);

        let (buttons_row, delete_column, actions_column) = buttons_row.into_parts();
        let buttons = menu.add_widget(buttons_row);

        // Closing keeps nothing for the next opening, which starts afresh (prepare).
        menu.set_open_close_callback(|_, _, is_open| {
            if !is_open {
                STATE.as_mut().synced = false;
            }
        });

        Self { menu: menu.into_rc(), parts: Parts { columns, left, right, list, image, details, buttons, delete_column, actions_column } }
    }

    // Makes the list's selection, the preview and the buttons follow the field: the save it names.
    // Before the dialog draws, so the widgets aren't drawing while they change, and the thumbnail
    // texture a new preview replaces isn't released while a frame may still draw it.
    fn sync(&mut self, context: &mut GameUiContext) {
        let state = STATE.as_mut();

        // Thumbnails replaced last frame, drawn for the last time then.
        for mut texture in state.retired.drain(..) {
            context.render_sys().texture_cache_mut().release_texture(&mut texture);
        }

        let selected = state.selected();
        let parts = self.parts;
        let menu = &mut *self.menu;

        if !state.synced {
            state.synced = true;

            let preview = preview_for(context, state, selected);
            let columns = menu.widget_as_mut::<UiColumns>(parts.columns).unwrap();
            let left = columns.column_mut(parts.left);
            left.widget_as_mut::<UiItemList>(parts.list).unwrap().set_current_item(selected);

            let right = columns.column_mut(parts.right);
            let image = right.widget_as_mut::<UiImage>(parts.image).unwrap();
            match preview {
                Some((texture, size)) => image.set_texture(texture, size),
                None => image.clear_texture(),
            }
            let pairs = details(selected.map(|index| &state.saves[index]));
            right.widget_as_mut::<UiKeyValues>(parts.details).unwrap().set_pairs(pairs);
        }

        // The buttons: Delete and Load need a save picked (Load one this game can load), Save a
        // game to save and a name.
        let entry = selected.map(|index| &state.saves[index]);
        let can_delete = entry.is_some();
        let can_load = entry.is_some_and(SaveEntry::loadable);
        let can_save = !super::in_home_menu() && session::is_valid_save_name(state.field.trim());

        let buttons = menu.widget_as_mut::<UiColumns>(parts.buttons).unwrap();
        if let Some(delete) = buttons.column_mut(parts.delete_column).widgets_mut().first_mut() {
            enable_button(delete, can_delete);
        }
        let actions = buttons.column_mut(parts.actions_column);
        if let Some((_, group)) = actions.find_widget_of_type_mut::<UiWidgetGroup>() {
            for (button, enabled) in group.widgets_mut().iter_mut().zip([true, can_save, can_load]) {
                enable_button(button, enabled);
            }
        }
    }
}

fn enable_button(widget: &mut UiWidgetImpl, enabled: bool) {
    if let Some(button) = widget.as_any_mut().downcast_mut::<UiTextButton>() {
        button.enable(enabled);
    }
}

// The texture the preview shows for the save at `selected`: its thumbnail, loaded once per save
// picked. The texture it replaces is released next frame.
fn preview_for(context: &mut GameUiContext, state: &mut LoadSaveState, selected: Option<usize>) -> Option<(TextureHandle, Vec2)> {
    let save = selected.map(|index| state.saves[index].name.clone());
    let current = state.preview.as_ref().map(|preview| preview.save.clone());
    // A newer save or thumbnail under the same name is read again.
    let revision = save_header::revision();
    if save != current || state.preview_revision != revision {
        if let Some(Preview { texture: Some((texture, _)), .. }) = state.preview.take() {
            state.retired.push(texture);
        }
        state.preview = save.map(|save| {
            let texture = load_thumbnail(context, &save);
            Preview { save, texture }
        });
        state.preview_revision = revision;
    }
    state.preview.as_ref().and_then(|preview| preview.texture)
}

// Each thumbnail texture gets a name of its own: the texture cache keeps names unique.
static THUMBNAIL_COUNTER: AtomicU32 = AtomicU32::new(0);

// The save's thumbnail as a texture, and its size in pixels. None if it has none, or it can't be read.
fn load_thumbnail(context: &mut GameUiContext, save: &str) -> Option<(TextureHandle, Vec2)> {
    let bytes = storage::read_save_thumbnail(PathRef::from_str(save))?;
    let image = match image::load_from_memory(&bytes) {
        Ok(image) => image.into_rgba8(),
        Err(err) => {
            log::error!(log::channel!("session"), "Can't read the thumbnail of '{save}': {err}");
            return None;
        }
    };
    let size = Size::new(image.width() as i32, image.height() as i32);
    let name = format!("save_thumbnail#{}", THUMBNAIL_COUNTER.fetch_add(1, Ordering::Relaxed));
    let texture = context.render_sys().texture_cache_mut().new_initialized_texture(&name, size, image.as_raw(), Some(ui::texture_settings()));
    Some((texture, Vec2::new(size.width as f32, size.height as f32)))
}

// ----------------------------------------------
// Widgets
// ----------------------------------------------

// A small label over a part, in capitals and the accent colour: "SAVED GAME", "SAVE AS".
pub(super) fn section_label(text: UiString, text_scale: f32) -> UiRichText {
    let text = text.resolve().to_uppercase();
    let label = skin::rich_text(text).with_font_scale(UiFontScale(text_scale * SECTION_FONT_SCALE));
    match skin::color("accent") {
        Some(color) => label.with_text_color(color),
        None => label,
    }
}

// The list's column titles, over the rows' name and time columns.
fn list_headers(name: UiRichText, saved: UiRichText, scale: f32) -> UiColumns {
    // As the rows lay their texts out: after the emblem, and in the room the scroll bar leaves.
    let text_left = (ROW_PADDING[0] * 2.0 + ROW_ICON_SIZE) * scale;
    let text_right = ROW_PADDING[0] * scale + LIST_SCROLL_BAR_WIDTH;
    UiColumns::new()
        .with_spacing(0.0)
        .with_column(UiColumnWidth::Fixed(text_left), UiWidgetGroup::new())
        .with_column(UiColumnWidth::Fraction(RIGHT_COLUMN), UiWidgetGroup::new().with_widget(name))
        .with_column(UiColumnWidth::Fraction(1.0 - RIGHT_COLUMN), UiWidgetGroup::new().with_widget(saved))
        .with_column(UiColumnWidth::Fixed(text_right), UiWidgetGroup::new())
}

// A row per save, picking one puts its name in the field. `height` as UiItemList::with_size takes it.
fn save_list(context: &mut GameUiContext, scale: f32, text_scale: f32, height: f32) -> UiItemList {
    let state = STATE.as_ref();
    let today = today();

    let rows = state
        .saves
        .iter()
        .map(|entry| {
            let header = entry.header();
            let realm = header.and_then(|header| header.realm).unwrap_or(Realm::DaiLong);
            let saved = header.and_then(|header| header.saved_at.as_deref()).and_then(|saved_at| saved_text(saved_at, today));
            let date = header.and_then(|header| header.date_text.clone());
            let mut row = UiListRow::new(entry.name.clone())
                .with_subtitle(place_text(entry))
                .with_right(saved.unwrap_or_else(none_text), Some(date.unwrap_or_else(none_text)));
            if let Some((texture, _)) = skin::emblem(context, realm, EmblemState::Idle) {
                row = row.with_icon(texture);
            }
            row
        })
        .collect();

    let mut look = skin::list_row_look(context);
    look.icon_size = ROW_ICON_SIZE * scale;
    look.padding = Vec2::new(ROW_PADDING[0], ROW_PADDING[1]) * scale;
    look.spacing = ROW_SPACING * scale;
    look.subtitle_scale = ROW_SUBTITLE_SCALE;
    look.right_column = Some(RIGHT_COLUMN);
    look.right_title_color = look.title_color;

    UiItemList::new()
        .with_rows(rows)
        .with_row_look(look)
        .with_font_scale(UiFontScale(text_scale))
        .with_size(Vec2::new(0.0, height))
        .with_flags(UiItemListFlags::Scrollable | UiItemListFlags::Scrollbars)
        .on_selection_changed(|list, _| {
            if let Some(name) = list.current_selection() {
                let state = STATE.as_mut();
                state.field = name.to_string();
                state.synced = false;
            }
        })
}

// The "Save as" field: the save the dialog loads, deletes or saves to.
fn save_as_field(context: &mut GameUiContext, text_scale: f32) -> UiTextInput {
    skin::text_input(context, UiString::key(UiTextCategory::Menu, "load_save.save_as"))
        .with_font_scale(UiFontScale(text_scale))
        .with_max_chars(session::MAX_SAVE_NAME_CHARS)
        .on_read_value(|_, _| STATE.as_ref().field.as_str())
        .on_update_value(|_, _, text| {
            let state = STATE.as_mut();
            if state.field != text {
                state.field = text.to_string();
                state.synced = false;
            }
        })
}

// The buttons' row, and its columns: Delete at the left; Cancel, Save and Load at the right.
struct ButtonsRow {
    columns: UiColumns,
    delete: UiColumnsColumnIndex,
    actions: UiColumnsColumnIndex,
}

impl ButtonsRow {
    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        self.columns.measure(context)
    }

    fn into_parts(self) -> (UiColumns, UiColumnsColumnIndex, UiColumnsColumnIndex) {
        (self.columns, self.delete, self.actions)
    }
}

fn buttons(context: &mut GameUiContext, scale: f32) -> ButtonsRow {
    let height = by_window(BUTTON_HEIGHTS, scale).round();
    let mode = STATE.as_ref().mode;
    let button = |context: &mut GameUiContext, role: ButtonRole, label: UiString, width: f32, on_pressed: fn(&mut GameUiContext)| {
        make_text_button(context, role, label, move |_, context| {
            on_pressed(context_as_mut::<GameUiContext>(context));
        })
        .with_width(UiTextButtonWidth::Fixed((width * scale).round()))
        .with_min_height(height)
    };
    let (save_role, load_role) = match mode {
        LoadSaveMode::Load => (ButtonRole::Secondary, ButtonRole::Primary),
        LoadSaveMode::Save => (ButtonRole::Primary, ButtonRole::Secondary),
    };

    let delete = button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "load_save.delete"), DELETE_WIDTH, on_delete);
    let cancel = button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "button.cancel"), CANCEL_WIDTH, |context| {
        super::close_current(context);
    });
    let mut save = button(context, save_role, UiString::key(UiTextCategory::Menu, "load_save.save"), SAVE_WIDTH, on_save);
    if super::in_home_menu() {
        save = save.with_tooltip(skin::tooltip(context, UiString::key(UiTextCategory::Menu, "load_save.no_game")));
    }
    let load = button(context, load_role, UiString::key(UiTextCategory::Menu, "load_save.load"), LOAD_WIDTH, on_load);

    let spacing = (BUTTON_SPACING * scale).round();
    let actions_width = ((CANCEL_WIDTH + SAVE_WIDTH + LOAD_WIDTH) * scale).round() + spacing * 2.0;
    let actions = UiWidgetGroup::new()
        .with_stack_vertically(false)
        .with_widget_spacing(Vec2::new(spacing, 0.0))
        .with_widget(cancel)
        .with_widget(save)
        .with_widget(load);

    let mut columns = UiColumns::new().with_spacing(spacing);
    let delete = columns.add_column(UiColumnWidth::Fixed((DELETE_WIDTH * scale).round()), UiWidgetGroup::new().with_widget(delete));
    columns.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());
    let actions = columns.add_column(UiColumnWidth::Fixed(actions_width), UiWidgetGroup::new().with_widget(actions));
    ButtonsRow { columns, delete, actions }
}

// ----------------------------------------------
// Actions
// ----------------------------------------------

fn on_load(_context: &mut GameUiContext) {
    let state = STATE.as_ref();
    if let Some(entry) = state.selected_entry().filter(|entry| entry.loadable()) {
        GameLoop::get_mut().load_save_game(PathRef::from_str(&entry.name));
    }
}

fn on_save(context: &mut GameUiContext) {
    let state = STATE.as_ref();
    let name = state.field.trim().to_string();
    if super::in_home_menu() || !session::is_valid_save_name(&name) {
        return;
    }
    // A save of that name, its case aside, as file systems that ignore case see it.
    if let Some(existing) = state.saves.iter().find(|entry| entry.name.eq_ignore_ascii_case(&name)) {
        // Asks first: Yes saves over it, under its own name, which a file system that ignores case
        // keeps for the file: the list, the file and the profile's last save then agree.
        let name = existing.name.clone();
        // Built afresh: the menu keeps one popup, the last built (Delete's or this one).
        let dialog = super::find::<LoadOrSaveGame>();
        dialog.menu.reset_message_box();
        dialog.menu.open_message_box(context, move |context| {
            let lines = [UiString::key(UiTextCategory::Menu, "popup.overwrite_save.title")];
            let name = name.clone();
            skin::popup(context, "Overwrite Save Game Popup", lines).with_buttons([
                make_text_button(context, ButtonRole::Primary, UiString::key(UiTextCategory::Menu, "button.yes"), move |_, context| {
                    GameLoop::get_mut().save_game(PathRef::from_str(&name));
                    super::find::<LoadOrSaveGame>().menu.close_message_box(context);
                }),
                make_text_button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "button.no"), |_, context| {
                    super::find::<LoadOrSaveGame>().menu.close_message_box(context);
                }),
            ])
        });
    } else {
        GameLoop::get_mut().save_game(PathRef::from_str(&name));
    }
}

fn on_delete(context: &mut GameUiContext) {
    let Some(entry) = STATE.as_ref().selected_entry() else {
        return;
    };
    let name = entry.name.clone();
    // Built afresh: the menu keeps one popup, the last built (Overwrite's or this one).
    let dialog = super::find::<LoadOrSaveGame>();
    dialog.menu.reset_message_box();
    dialog.menu.open_message_box(context, move |context| {
        let title = ui::text::format(UiTextCategory::Menu, "popup.delete_save.title", &[("save", &name)]);
        let body = [UiString::key(UiTextCategory::Menu, "popup.delete_save.body")];
        let name = name.clone();
        skin::popup_with_body(context, "Delete Save Game Popup", [UiString::from(title)], body).with_buttons([
            make_text_button(context, ButtonRole::Primary, UiString::key(UiTextCategory::Menu, "load_save.delete"), move |_, context| {
                // A failure is logged; the list is read again either way.
                let _ = session::delete_save(PathRef::from_str(&name));
                STATE.as_mut().field.clear();
                super::find::<LoadOrSaveGame>().menu.close_message_box(context);
            }),
            make_text_button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "button.cancel"), |_, context| {
                super::find::<LoadOrSaveGame>().menu.close_message_box(context);
            }),
        ])
    });
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_before() {
        assert_eq!(previous_day((2026, 11, 4)), (2026, 11, 3));
        assert_eq!(previous_day((2026, 11, 1)), (2026, 10, 31));
        assert_eq!(previous_day((2026, 1, 1)), (2025, 12, 31));
        assert_eq!(previous_day((2024, 3, 1)), (2024, 2, 29));
        assert_eq!(previous_day((2026, 3, 1)), (2026, 2, 28));
        assert_eq!(previous_day((2100, 3, 1)), (2100, 2, 28));
        assert_eq!(previous_day((2026, 5, 1)), (2026, 4, 30));
    }

    #[test]
    fn saved_times() {
        let today = Some((2026, 11, 11));
        assert_eq!(saved_text("2026-11-11 21:14:05", today).unwrap(), "Today, 21:14");
        assert_eq!(saved_text("2026-11-10 08:03:00", today).unwrap(), "Yesterday, 08:03");
        assert_eq!(saved_text("2026-11-04 23:40:00", today).unwrap(), "4 Nov 2026");
        assert_eq!(saved_text("2026-11-04 23:40:00", None).unwrap(), "4 Nov 2026");
        assert!(saved_text("not a time", today).is_none());
    }

    #[test]
    fn numbers_and_durations() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1120), "1,120");
        assert_eq!(group_digits(1234567), "1,234,567");
        assert_eq!(group_digits(-2410), "-2,410");
        assert_eq!(time_played_text(0.0), "0 h 00 min");
        assert_eq!(time_played_text(47.0 * 60.0 + 59.0), "0 h 47 min");
        assert_eq!(time_played_text((4.0 * 60.0 + 2.0) * 60.0), "4 h 02 min");
    }
}
