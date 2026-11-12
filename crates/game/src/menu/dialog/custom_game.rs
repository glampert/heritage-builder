// The Custom Game dialog (docs/plans/realm-ui.md, Phase 1, Custom Game; menu-custom-game-<realm>.jpg):
// a game outside the campaigns (crate::custom_game), in three columns.
//
// - Realm: a row per realm, its emblem, name and its campaign's name; locked, with "Start
//   Campaign II to unlock", until the player has reached one of its campaigns (or the profile's
//   custom_realms unlocks it). Picking a realm picks the starting dates its eras give.
// - Map: a row per map in maps.json that can be played now (a save's map while the save is in
//   saves/), with its size. The Blank map takes a width, a height and a terrain, under the preview.
// - Preview: the minimap's bake of the map (custom_game::preview; a save's map shows the realm's
//   emblem instead), and its size, water, woodland, trade partners and hazards.
// - Options: the starting date (the realm's eras), goals (Sandbox only for now), the starting
//   treasury, the difficulty in five steps, and the switches: seasons and floods (disabled until
//   M7), fires and collapses (the session's CityRisks), raids (disabled) and fast construction.
// - Cancel and Start: Start replaces the session with the game (custom_game::start).
//
// What the player picked is kept outside the dialog (STATE): the dialog is built again when the
// realm changes (its rows, emblems and dates), when the Blank map is picked or left (its own
// options), or when the window's size changes how large it's drawn (dialog::sync).
//
// Paths: CustomGame/Realm (the realm list) and CustomGame/Realm/<realm>, CustomGame/Map and
// CustomGame/Map/<map>, CustomGame/Width|Height|Terrain (the Blank map's), CustomGame/preview,
// CustomGame/details, CustomGame/Starting date|Goals|Starting treasury (dropdowns),
// CustomGame/Difficulty, CustomGame/Seasons and floods|Fires and collapses|Raids and invasions|Fast
// construction (toggles), CustomGame/Cancel|Start.

use std::sync::atomic::{AtomicU32, Ordering};

use common::Size;
use engine::render::texture::TextureHandle;
use strum::IntoEnumIterator;

use super::*;
use crate::{
    campaign::config::CampaignConfigs,
    config::GameConfigs,
    custom_game::{
        self, BLANK_MAP_SIZES, BLANK_MAP_TERRAINS, ChosenMap, CustomGameSettings, Difficulty, MapConfigs, MapDef, MapSource,
        config::TreasuryDef,
        preview::{self, GridStats, PreviewGrid},
    },
    debug::preset_maps,
    menu::skin::EmblemState,
    profile,
    realm::{self, Realm},
    save_header,
    sim::calendar::GameCalendar,
    tile::{minimap, sets::PresetTiles},
};

// ----------------------------------------------
// Layout (points on a 1920 × 1080 window, scaled by the dialog's scale: skin::dialog_scale)
// ----------------------------------------------

// The columns' shares of the content width and the room between them, as in the mockup (500, 470
// and 450 points, 70 apart).
const COLUMN_SHARES: [f32; 3] = [500.0, 470.0, 450.0];
const COLUMN_SPACING: f32 = 70.0;
const PART_SPACING: f32 = 12.0;     // Between a column's parts.
const SECTION_GAP: f32 = 30.0;      // Above the Map section.
const REALM_ICON_SIZE: f32 = 64.0;
const REALM_ROW_PADDING: [f32; 2] = [14.0, 10.0];
const MAP_ROW_PADDING: [f32; 2] = [20.0, 12.0];
const ROW_SPACING: f32 = 8.0;
const PREVIEW_ASPECT: f32 = 330.0 / 470.0; // The preview's height over its width.
const PREVIEW_INSET: f32 = 2.0;     // The preview inside its well: past the well's thin border.
const EMBLEM_SCALE: f32 = 2.0;      // A save's map shows the realm's emblem this many times its size.
const CANCEL_WIDTH: f32 = 180.0;
const START_WIDTH: f32 = 270.0;
const BUTTON_SPACING: f32 = 20.0;
const COLUMNS_SLACK: f32 = 24.0;    // Room left under the columns for what measuring misses.
const STEP_SEGMENT_HEIGHT: f32 = 16.0;
const OPTION_GAPS: usize = 5;       // Above the options' parts: date, goals, treasury, difficulty, switches.
const OPTION_TOGGLES: usize = 4;
// Sizes on the reference window and on the smallest (1280 × 720), in proportion between them
// (by_window): the text size over the body font, the buttons' height and the toggles' sprites.
const TEXT_SCALES: (f32, f32) = (1.1, 0.8);
const BUTTON_HEIGHTS: (f32, f32) = (56.0, 36.0);
const CONTROL_SCALES: (f32, f32) = (1.0, 0.75);
const TOGGLE_HEIGHTS: (f32, f32) = (48.0, 32.0);
const SECTION_FONT_SCALE: f32 = 0.78; // "REALM", "MAP", over the text size.
const CAPTION_FONT_SCALE: f32 = 0.92; // The options' captions, over the text size.
const REALM_TITLE_SCALE: f32 = 1.12;
const ROW_SUBTITLE_SCALE: f32 = 0.8;
const DETAILS_FONT_SCALE: f32 = 0.9;
const DETAILS_ROW_PADDING: f32 = 5.0;

// ----------------------------------------------
// State, kept across rebuilds
// ----------------------------------------------

// The preview shown, and what it was made for.
struct Preview {
    key: PreviewKey,
    texture: Option<(TextureHandle, Vec2)>,
    owned: bool,              // A texture made for it (released when replaced), not the emblem's.
    stats: Option<GridStats>, // None for a save's map, whose cells aren't read.
}

#[derive(Clone, PartialEq, Debug)]
struct PreviewKey {
    realm: Realm,
    map: usize,
    blank: (i32, i32, usize),
    pixels: (u32, u32), // The bake's size, the well's shape.
}

struct CustomGameDialogState {
    realm: Realm,
    maps: Vec<MapDef>,
    map_sizes: Vec<Option<Size>>, // Each map's size: a blank map's default, a save's read from its head.
    map: usize,
    blank_width: i32,
    blank_height: i32,
    blank_terrain: usize, // In BLANK_MAP_TERRAINS.
    date: usize,          // In custom_game::starting_dates(realm).
    goals: usize,
    treasury: usize,      // In the starting treasury choices.
    difficulty: Difficulty,
    fires_and_collapses: bool,
    fast_construction: bool,
    built: (u32, bool, Realm), // build_key() when the dialog was built.
    preview_pixels: (u32, u32), // The bake's size for the well as built (preview::preview_pixels).
    preview: Option<Preview>,
    retired: Vec<TextureHandle>, // Previews replaced, released before the next frame draws.
    synced: bool,                // The lists, the preview and the details follow the state.
}

impl CustomGameDialogState {
    fn selected_map(&self) -> Option<&MapDef> {
        self.maps.get(self.map)
    }

    fn is_blank(&self) -> bool {
        self.selected_map().is_some_and(MapDef::is_blank)
    }

    // The size of the map at `index`: the Blank map's as chosen.
    fn map_size(&self, index: usize) -> Option<Size> {
        match self.maps.get(index) {
            Some(map) if map.is_blank() => Some(Size::new(self.blank_width, self.blank_height)),
            Some(_) => self.map_sizes.get(index).copied().flatten(),
            None => None,
        }
    }

    fn preview_key(&self) -> PreviewKey {
        PreviewKey {
            realm: self.realm,
            map: self.map,
            blank: (self.blank_width, self.blank_height, self.blank_terrain),
            pixels: self.preview_pixels,
        }
    }
}

static STATE: UiStaticVar<CustomGameDialogState> = UiStaticVar::new(CustomGameDialogState {
    realm: Realm::DaiLong,
    maps: Vec::new(),
    map_sizes: Vec::new(),
    map: 0,
    blank_width: 128,
    blank_height: 128,
    blank_terrain: 0,
    date: 0,
    goals: 0,
    treasury: 0,
    difficulty: Difficulty::Normal,
    fires_and_collapses: true,
    fast_construction: false,
    built: (0, false, Realm::DaiLong),
    preview_pixels: preview::PREVIEW_PIXELS,
    preview: None,
    retired: Vec::new(),
    synced: false,
});

// What the dialog is built from, besides the skin and the strings: how large the window draws it
// (in hundredths), whether the Blank map is picked (its options) and the realm (its rows and dates).
fn build_key(context: &GameUiContext) -> (u32, bool, Realm) {
    let state = STATE.as_ref();
    let scale = skin::dialog_scale(ui::screen_size(context.ui_sys));
    ((scale * 100.0).round() as u32, state.is_blank(), state.realm)
}

// Whether `realm` can be played: one of its campaigns is playable (the first campaign always is),
// or the profile unlocked it for Custom Game.
fn is_unlocked(realm: Realm) -> bool {
    let configs = CampaignConfigs::get();
    let profile = profile::get();
    profile.custom_realms.contains(&realm)
        || configs.campaigns.iter().enumerate().any(|(index, campaign)| campaign.realm == realm && profile.is_playable(configs, index))
}

// The index of `realm`'s first campaign in campaigns.json.
fn realm_campaign(realm: Realm) -> Option<usize> {
    CampaignConfigs::get().campaigns.iter().position(|campaign| campaign.realm == realm)
}

// Reads the maps for the dialog about to open, and starts afresh: the UI's realm if it's unlocked
// (else the first that is), the first map, the default options.
pub(super) fn prepare(in_home_menu: bool) {
    let configs = MapConfigs::get();
    let state = STATE.as_mut();

    let ui_realm = realm::ui_realm_for(in_home_menu).realm;
    state.realm = if is_unlocked(ui_realm) { ui_realm } else { Realm::iter().find(|realm| is_unlocked(*realm)).unwrap_or_default() };

    state.maps = configs.available_maps();
    state.map_sizes = state
        .maps
        .iter()
        .map(|map| match &map.source {
            MapSource::Blank { width, height, .. } => Some(Size::new(*width, *height)),
            MapSource::Preset { preset_number } => preset_maps::preset_size(*preset_number),
            MapSource::SaveGame { save_file } => save_header::read_map_size(engine::file_sys::paths::PathRef::from_str(save_file)),
        })
        .collect();
    state.map = 0;
    let blank = state.maps.iter().find_map(|map| match &map.source {
        MapSource::Blank { width, height, terrain } => Some((*width, *height, terrain.clone())),
        _ => None,
    });
    let (width, height, terrain) = blank.unwrap_or((128, 128, "grass".to_string()));
    state.blank_width = width;
    state.blank_height = height;
    let terrain = custom_game::blank_terrain(&terrain);
    state.blank_terrain = BLANK_MAP_TERRAINS.iter().position(|choice| *choice == terrain).unwrap_or(0);

    state.date = 0;
    state.goals = 0;
    state.treasury = configs.default_treasury_index();
    state.difficulty = Difficulty::default();
    state.fires_and_collapses = GameConfigs::get().sim.risks.enabled;
    state.fast_construction = false;
    state.synced = false;
}

// Whether the open dialog must be built again: the realm changed, the Blank map was picked or
// left, or the window's size changed the dialog's.
pub(super) fn needs_rebuild(context: &GameUiContext) -> bool {
    let state = STATE.as_mut();
    if state.built == build_key(context) {
        return false;
    }
    state.synced = false;
    true
}

// What the dialog chose, as the game starts with it. None without a map.
fn chosen_settings() -> Option<CustomGameSettings> {
    let state = STATE.as_ref();
    let map = state.selected_map()?;
    let chosen = match &map.source {
        MapSource::Blank { .. } => ChosenMap::Blank {
            width: state.blank_width,
            height: state.blank_height,
            terrain: BLANK_MAP_TERRAINS.get(state.blank_terrain).copied().unwrap_or_default(),
        },
        MapSource::Preset { preset_number } => ChosenMap::Preset { preset_number: *preset_number },
        MapSource::SaveGame { save_file } => ChosenMap::SaveGame { save_file: save_file.clone() },
    };
    let dates = custom_game::starting_dates(state.realm);
    let treasury = treasury_choices();
    Some(CustomGameSettings {
        realm: state.realm,
        map_id: map.id.clone(),
        map: chosen,
        calendar: dates.get(state.date).or(dates.first()).cloned().unwrap_or_default(),
        starting_gold: treasury.choices.get(state.treasury).copied().unwrap_or(treasury.default),
        difficulty: state.difficulty,
        fires_and_collapses: state.fires_and_collapses,
        fast_construction: state.fast_construction,
        seasons_and_floods: false,
        raids: false,
    })
}

// ----------------------------------------------
// Text
// ----------------------------------------------

fn size_text(size: Option<Size>) -> String {
    match size {
        Some(size) => ui::text::format(
            UiTextCategory::Menu,
            "custom_game.map_size",
            &[("width", &size.width.to_string()), ("height", &size.height.to_string())],
        ),
        None => ui::text::get(UiTextCategory::Menu, "load_save.none").to_string(),
    }
}

fn terrain_label(terrain: PresetTiles) -> UiString {
    match terrain {
        PresetTiles::Dirt => UiString::key(UiTextCategory::Menu, "custom_game.terrain.dirt"),
        PresetTiles::Water => UiString::key(UiTextCategory::Menu, "custom_game.terrain.water"),
        _ => UiString::key(UiTextCategory::Menu, "custom_game.terrain.grass"),
    }
}

// The starting dates of `realm`, as its calendar writes them: "Long Thái era, year 1".
fn date_texts(realm: Realm) -> Vec<String> {
    custom_game::starting_dates(realm).into_iter().map(|settings| realm::date_text(realm, &GameCalendar::new(settings))).collect()
}

// The starting treasury's choices: maps.json's, or the defaults before it's loaded (the dialogs are
// built at startup, and in tests).
fn treasury_choices() -> TreasuryDef {
    if MapConfigs::is_initialized() { MapConfigs::get().starting_treasury.clone() } else { TreasuryDef::default() }
}

fn treasury_texts() -> Vec<String> {
    treasury_choices()
        .choices
        .iter()
        .map(|gold| ui::text::format(UiTextCategory::Menu, "custom_game.gold", &[("gold", &super::load_save::group_digits(i64::from(*gold)))]))
        .collect()
}

// The preview's details for the state's map.
fn details(state: &CustomGameDialogState) -> Vec<(UiString, UiString)> {
    let none = ui::text::get(UiTextCategory::Menu, "load_save.none").to_string();
    let stats = state.preview.as_ref().and_then(|preview| preview.stats);
    let percent = |value: Option<u32>| {
        value.map_or_else(|| none.clone(), |value| ui::text::format(UiTextCategory::Menu, "custom_game.percent", &[("value", &value.to_string())]))
    };
    let hazards = if state.fires_and_collapses {
        ui::text::get(UiTextCategory::Menu, "custom_game.hazards.fire").to_string()
    } else {
        ui::text::get(UiTextCategory::Menu, "custom_game.hazards.none").to_string()
    };
    vec![
        (UiString::key(UiTextCategory::Menu, "custom_game.details.size"), UiString::from(size_text(state.map_size(state.map)))),
        (UiString::key(UiTextCategory::Menu, "custom_game.details.water"), UiString::from(percent(stats.map(|stats| stats.water_percent)))),
        (UiString::key(UiTextCategory::Menu, "custom_game.details.woodland"), UiString::from(percent(stats.map(|stats| stats.woodland_percent)))),
        // TODO(realm-ui): the map's trade partners, once M10 brings trade.
        (UiString::key(UiTextCategory::Menu, "custom_game.details.trade_partners"), UiString::from(none.clone())),
        (UiString::key(UiTextCategory::Menu, "custom_game.details.hazards"), UiString::from(hazards)),
    ]
}

// ----------------------------------------------
// CustomGame
// ----------------------------------------------

// Where its widgets are, to follow the state each frame (sync).
#[derive(Copy, Clone)]
struct Parts {
    columns: UiMenuWidgetIndex,
    left: UiColumnsColumnIndex,
    middle: UiColumnsColumnIndex,
    realms: UiWidgetGroupWidgetIndex,
    maps: UiWidgetGroupWidgetIndex,
    image: UiWidgetGroupWidgetIndex,
    details: UiWidgetGroupWidgetIndex,
    buttons: UiMenuWidgetIndex,
    actions: UiColumnsColumnIndex,
}

pub struct CustomGame {
    menu: UiMenuRcMut,
    parts: Parts,
}

impl DialogMenuFactory for CustomGame {
    const KIND: DialogMenuKind = DialogMenuKind::CustomGame;
    const TITLE: &'static [&'static str] = &["Custom Game"];

    fn create(context: &mut GameUiContext) -> DialogMenuImpl {
        DialogMenuImpl::from(CustomGame::new(context))
    }
}

impl DialogMenu for CustomGame {
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

impl CustomGame {
    pub fn new(context: &mut GameUiContext) -> Self {
        // Built at startup too, before it's ever opened: then it reads the maps itself.
        if STATE.as_ref().maps.is_empty() && MapConfigs::is_initialized() {
            prepare(super::in_home_menu());
        }
        let state = STATE.as_mut();
        state.built = build_key(context);
        state.synced = false;

        let kind = Self::KIND.to_string();
        let window = ui::screen_size(context.ui_sys);
        // Sizes by the window, and smaller with the dialog where a UI scale under 100% shrinks it
        // (dialog_ui_growth below 1; the themes shrink the text as much). Text by the window too,
        // times as much as the dialog grew with the player's UI scale, over the UI scale the themes
        // give it (dialog_text_fit), so the columns and buttons still fit at 200% on a small window.
        let window_scale = skin::dialog_scale(window);
        let scale = window_scale * skin::dialog_ui_growth(&kind, window).min(1.0);
        let fit = skin::dialog_text_fit(&kind, window);
        let text_scale = by_window(TEXT_SCALES, window_scale) * fit;
        let spacing = (PART_SPACING * scale).round();

        let title = skin::heading_at(context, [UiString::key(UiTextCategory::Menu, "custom_game.title")], HeadingLevel::Dialog, fit);
        let (buttons_row, actions) = buttons(context, scale, fit);

        // The columns: their widths share the content's, and their height is what the title and
        // the buttons leave.
        let content = skin::dialog_content_size(&kind, window);
        let column_spacing = (COLUMN_SPACING * scale).round();
        let shares: f32 = COLUMN_SHARES.iter().sum();
        let widths = COLUMN_SHARES.map(|share| ((content.x - column_spacing * 2.0) * share / shares).floor());
        let columns_height = content.y - title.measure(context).y - buttons_row.measure(context).y - spacing * 2.0 - COLUMNS_SLACK * scale;

        let flags = DialogFrame::from_skin(Self::KIND).menu_flags();
        let mut menu = skin::dialog(context, &kind, flags).with_widget_spacing(Vec2::new(0.0, spacing));
        menu.add_widget(title);

        // Realm and Map.
        let mut left = UiWidgetGroup::new().with_center_horizontally(false).with_widget_spacing(Vec2::new(0.0, spacing));
        left.add_widget(section_label(UiString::key(UiTextCategory::Menu, "custom_game.section.realm"), text_scale, "realm_heading"));
        let realm_list = realm_list(context, scale, text_scale);
        let realms_height = realm_list.measure(context).y;
        let realms = left.add_widget(realm_list);
        left.add_widget(UiSeparator::new().with_size(Vec2::new(1.0, (SECTION_GAP * scale).round())));
        let map_heading = section_label(UiString::key(UiTextCategory::Menu, "custom_game.section.map"), text_scale, "map_heading");
        let maps_height = (columns_height - realms_height - map_heading.measure(context).y * 2.0 - spacing * 4.0 - SECTION_GAP * scale).floor();
        left.add_widget(map_heading);
        let maps = left.add_widget(map_list(context, scale, text_scale, maps_height.max(60.0 * scale)));

        // Preview: the image as large as the column's width, or as the heading, the details and
        // the Blank map's options leave under it.
        let mut middle = UiWidgetGroup::new().with_center_horizontally(false).with_widget_spacing(Vec2::new(0.0, spacing));
        let preview_heading = section_label(UiString::key(UiTextCategory::Menu, "custom_game.section.preview"), text_scale, "preview_heading");
        let look = UiRowLook { padding: DETAILS_ROW_PADDING * scale, ..skin::row_look() };
        let mut key_values = skin::key_values().with_label("details").with_look(look).with_font_scale(UiFontScale(text_scale * DETAILS_FONT_SCALE));
        key_values.set_pairs(details(state));
        let blank_options = if state.is_blank() { blank_map_options(text_scale, widths[1]) } else { Vec::new() };
        let mut below_image = preview_heading.measure(context).y + key_values.measure(context).y + spacing * 2.0;
        if !blank_options.is_empty() {
            // A separator, then the options, each a spacing below the widget before it.
            below_image += blank_options.iter().map(|widget| widget.measure(context).y + spacing).sum::<f32>() + spacing * 2.0;
        }
        // The frame round the image takes room of its own.
        let framed = |context: &mut GameUiContext, height: f32| {
            let image = UiImage::empty(Vec2::new(widths[1], height)).with_label("preview").with_fit(UiImageFit::Contain);
            skin::framed_image_inset(context, image, "well", PREVIEW_INSET)
        };
        let full_height = (widths[1] * PREVIEW_ASPECT).floor();
        let frame_height = framed(context, full_height).measure(context).y - full_height;
        let image_height = full_height.min(columns_height - below_image - frame_height).max(60.0 * scale).floor();
        // Baked at the shape of the image inside the well, so the sea fills it to the border.
        let inner_width = (widths[1] - PREVIEW_INSET * 2.0).max(1.0);
        let inner_height = (image_height - PREVIEW_INSET * 2.0).max(1.0);
        state.preview_pixels = preview::preview_pixels(inner_height / inner_width);
        middle.add_widget(preview_heading);
        let image = framed(context, image_height);
        let image = middle.add_widget(image);
        let details = middle.add_widget(key_values);
        if !blank_options.is_empty() {
            middle.add_widget(UiSeparator::new().with_size(Vec2::new(1.0, spacing)));
            for widget in blank_options {
                middle.add_widget(widget);
            }
        }

        // Options.
        // Its parts spread over the column's height, as far as a few spacings apart; where even
        // without gaps they'd overflow it, the switches' rows get lower.
        let mut toggle_height = by_window(TOGGLE_HEIGHTS, scale).round();
        let natural = options(context, scale, text_scale, widths[2], 0.0, toggle_height).measure(context).y;
        let excess = natural + OPTION_GAPS as f32 - columns_height;
        if excess > 0.0 {
            toggle_height = (toggle_height - (excess / OPTION_TOGGLES as f32).ceil()).max(1.0);
        }
        let natural = options(context, scale, text_scale, widths[2], 0.0, toggle_height).measure(context).y;
        let gap = ((columns_height - natural) / OPTION_GAPS as f32).clamp(1.0, spacing * 3.0).floor();
        let right = options(context, scale, text_scale, widths[2], gap, toggle_height);

        let mut columns = UiColumns::new().with_spacing(column_spacing);
        let left = columns.add_column(UiColumnWidth::Fixed(widths[0]), left);
        let middle = columns.add_column(UiColumnWidth::Fixed(widths[1]), middle);
        columns.add_column(UiColumnWidth::Fixed(widths[2]), right);
        let columns = menu.add_widget(columns);
        let buttons = menu.add_widget(buttons_row);

        // Closing keeps nothing for the next opening, which starts afresh (prepare).
        menu.set_open_close_callback(|_, _, is_open| {
            if !is_open {
                STATE.as_mut().synced = false;
            }
        });

        Self { menu: menu.into_rc(), parts: Parts { columns, left, middle, realms, maps, image, details, buttons, actions } }
    }

    // Makes the lists, the preview and the details follow the state, before the dialog draws (so
    // the widgets aren't drawing while they change, and a preview texture a new one replaces isn't
    // released while a frame may still draw it). The Start button needs a map.
    fn sync(&mut self, context: &mut GameUiContext) {
        let state = STATE.as_mut();

        for mut texture in state.retired.drain(..) {
            context.render_sys().texture_cache_mut().release_texture(&mut texture);
        }

        let parts = self.parts;
        let menu = &mut *self.menu;

        if !state.synced {
            state.synced = true;
            update_preview(context, state);

            let columns = menu.widget_as_mut::<UiColumns>(parts.columns).unwrap();
            let left = columns.column_mut(parts.left);
            let realm_index = Realm::iter().position(|realm| realm == state.realm);
            left.widget_as_mut::<UiItemList>(parts.realms).unwrap().set_current_item(realm_index);
            let rows = map_rows(state);
            let maps = left.widget_as_mut::<UiItemList>(parts.maps).unwrap();
            maps.reset_rows(Some(state.map), rows);
            maps.set_current_item(Some(state.map)); // Scrolled into view.

            let middle = columns.column_mut(parts.middle);
            let image = middle.widget_as_mut::<UiImage>(parts.image).unwrap();
            match state.preview.as_ref().and_then(|preview| preview.texture) {
                Some((texture, size)) => image.set_texture(texture, size),
                None => image.clear_texture(),
            }
            // A bake fills the preview; an emblem shows at its size, in the middle.
            image.set_fit(if state.preview.as_ref().is_some_and(|preview| preview.owned) { UiImageFit::Contain } else { UiImageFit::Center });
            middle.widget_as_mut::<UiKeyValues>(parts.details).unwrap().set_pairs(details(state));
        }

        let can_start = state.selected_map().is_some() && is_unlocked(state.realm);
        let buttons = menu.widget_as_mut::<UiColumns>(parts.buttons).unwrap();
        let actions = buttons.column_mut(parts.actions);
        if let Some((_, group)) = actions.find_widget_of_type_mut::<UiWidgetGroup>()
            && let Some(start) = group.widgets_mut().last_mut()
            && let Some(button) = start.as_any_mut().downcast_mut::<UiTextButton>()
        {
            button.enable(can_start);
        }
    }
}

// Each preview texture gets a name of its own: the texture cache keeps names unique.
static PREVIEW_COUNTER: AtomicU32 = AtomicU32::new(0);

// Makes the preview for the state's realm and map if it isn't made yet: the map's bake in the
// realm's minimap style, or the realm's emblem for a save's map. The texture it replaces is
// released next frame.
fn update_preview(context: &mut GameUiContext, state: &mut CustomGameDialogState) {
    let key = state.preview_key();
    if state.preview.as_ref().is_some_and(|preview| preview.key == key) {
        return;
    }
    if let Some(Preview { texture: Some((texture, _)), owned: true, .. }) = state.preview.take() {
        state.retired.push(texture);
    }

    // Without making it the minimap's style: in game, the HUD's minimap keeps the game's realm.
    let style = &minimap::style::of_realm(state.realm);
    let grid = match state.selected_map().map(|map| &map.source) {
        Some(MapSource::Blank { .. }) => {
            let terrain = BLANK_MAP_TERRAINS.get(state.blank_terrain).copied().unwrap_or_default();
            Some(PreviewGrid::blank(state.blank_width, state.blank_height, terrain))
        }
        Some(MapSource::Preset { preset_number }) => PreviewGrid::preset(*preset_number, style),
        Some(MapSource::SaveGame { .. }) | None => None,
    };

    state.preview = Some(match grid {
        Some(grid) => {
            let (width, height) = state.preview_pixels;
            let pixels = grid.render(style, (width, height));
            let size = Size::new(width as i32, height as i32);
            let name = format!("custom_game_preview#{}", PREVIEW_COUNTER.fetch_add(1, Ordering::Relaxed));
            let texture = context.render_sys().texture_cache_mut().new_initialized_texture(&name, size, &pixels, Some(ui::texture_settings()));
            Preview { key, texture: Some((texture, Vec2::new(width as f32, height as f32))), owned: true, stats: Some(grid.stats()) }
        }
        // TODO(realm-ui): a save's map previewed from its tile map (custom_game::preview).
        None => {
            let emblem = skin::emblem(context, state.realm, EmblemState::Current).map(|(texture, size)| (texture, size * EMBLEM_SCALE));
            Preview { key, texture: emblem, owned: false, stats: None }
        }
    });
}

// ----------------------------------------------
// Widgets
// ----------------------------------------------

// A small label over a part, in capitals and the accent colour: "REALM", "PREVIEW".
fn section_label(text: UiString, text_scale: f32, label: &str) -> UiRichText {
    super::load_save::section_label(text, text_scale).with_label(label)
}

// A caption over an option's control: "Starting date".
fn caption(text: UiString, text_scale: f32, label: &str) -> UiRichText {
    let caption = skin::rich_text(text.resolve().to_string()).with_label(label).with_font_scale(UiFontScale(text_scale * CAPTION_FONT_SCALE));
    match skin::color("sub") {
        Some(color) => caption.with_text_color(color),
        None => caption,
    }
}

// A row per realm, its emblem, name and its campaign's name, or how to unlock it.
fn realm_list(context: &mut GameUiContext, scale: f32, text_scale: f32) -> UiItemList {
    let state = STATE.as_ref();
    let configs = CampaignConfigs::get();
    let rows = Realm::iter()
        .map(|realm| {
            let unlocked = is_unlocked(realm);
            let campaign = realm_campaign(realm);
            let subtitle = if unlocked {
                campaign.and_then(|index| configs.campaign(index)).map(|campaign| campaign.display_name().to_string()).unwrap_or_default()
            } else {
                let number = campaign.map(super::home::campaign_number).unwrap_or_default();
                ui::text::format(UiTextCategory::Menu, "custom_game.realm.unlock", &[("campaign", &number)])
            };
            let emblem_state = match (unlocked, realm == state.realm) {
                (false, _) => EmblemState::Locked,
                (true, true) => EmblemState::Current,
                (true, false) => EmblemState::Idle,
            };
            let mut row = UiListRow::new(realm.display_name()).with_subtitle(subtitle).with_enabled(unlocked);
            if let Some((texture, _)) = skin::emblem(context, realm, emblem_state) {
                row = row.with_icon(texture);
            }
            row
        })
        .collect();

    let mut look = skin::list_row_look(context);
    look.icon_size = REALM_ICON_SIZE * scale;
    look.padding = Vec2::new(REALM_ROW_PADDING[0], REALM_ROW_PADDING[1]) * scale;
    look.spacing = ROW_SPACING * scale;
    look.title_scale = REALM_TITLE_SCALE;
    look.subtitle_scale = ROW_SUBTITLE_SCALE;

    let font_scale = UiFontScale(text_scale);
    let count = Realm::iter().count() as f32;
    let height = (look.row_height(context.ui_sys, font_scale, true, true) * count + look.spacing * (count - 1.0)).ceil() + 2.0;
    let current = Realm::iter().position(|realm| realm == state.realm);

    let mut list = UiItemList::new()
        .with_hidden_label("Realm")
        .with_rows(rows)
        .with_row_look(look)
        .with_font_scale(font_scale)
        .with_size(Vec2::new(0.0, height))
        .on_selection_changed(|list, _| {
            let state = STATE.as_mut();
            if let Some(realm) = list.current_selection_index().and_then(|index| Realm::iter().nth(index)) {
                if realm != state.realm && is_unlocked(realm) {
                    state.realm = realm;
                    state.date = 0;
                    state.synced = false;
                }
            }
        });
    list.set_current_item(current);
    list
}

// A row per map, its name and size.
fn map_rows(state: &CustomGameDialogState) -> Vec<UiListRow> {
    state
        .maps
        .iter()
        .enumerate()
        .map(|(index, map)| UiListRow::new(map.display_name()).with_right(size_text(state.map_size(index)), None))
        .collect()
}

fn map_list(context: &mut GameUiContext, scale: f32, text_scale: f32, height: f32) -> UiItemList {
    let state = STATE.as_ref();
    let mut look = skin::list_row_look(context);
    look.padding = Vec2::new(MAP_ROW_PADDING[0], MAP_ROW_PADDING[1]) * scale;
    look.spacing = ROW_SPACING * scale;
    UiItemList::new()
        .with_hidden_label("Map")
        .with_rows(map_rows(state))
        .with_row_look(look)
        .with_font_scale(UiFontScale(text_scale))
        .with_size(Vec2::new(0.0, height))
        .with_flags(UiItemListFlags::Scrollable | UiItemListFlags::Scrollbars)
        .on_selection_changed(|list, _| {
            if let Some(index) = list.current_selection_index() {
                let state = STATE.as_mut();
                if index != state.map {
                    state.map = index;
                    state.synced = false;
                }
            }
        })
}

// The Blank map's width, height and terrain, under its preview.
fn blank_map_options(text_scale: f32, width: f32) -> Vec<UiWidgetImpl> {
    let state = STATE.as_ref();
    let font_scale = UiFontScale(text_scale * CAPTION_FONT_SCALE);
    let sizes = || BLANK_MAP_SIZES.map(|size| UiString::from(size.to_string()));
    let size_index = |size: i32| BLANK_MAP_SIZES.iter().position(|choice| *choice == size).unwrap_or(0);
    let selector = |label: UiString, steps: Vec<UiString>, current: usize| {
        skin::step_selector(label, steps).with_style(UiStepSelectorStyle::Arrows).with_font_scale(font_scale).with_width(width).with_current(current)
    };
    vec![
        UiWidgetImpl::from(
            selector(UiString::key(UiTextCategory::Menu, "custom_game.width"), sizes().to_vec(), size_index(state.blank_width)).on_changed(|selector, _, _| {
                let state = STATE.as_mut();
                state.blank_width = BLANK_MAP_SIZES[selector.current().min(BLANK_MAP_SIZES.len() - 1)];
                state.synced = false;
            }),
        ),
        UiWidgetImpl::from(
            selector(UiString::key(UiTextCategory::Menu, "custom_game.height"), sizes().to_vec(), size_index(state.blank_height)).on_changed(|selector, _, _| {
                let state = STATE.as_mut();
                state.blank_height = BLANK_MAP_SIZES[selector.current().min(BLANK_MAP_SIZES.len() - 1)];
                state.synced = false;
            }),
        ),
        UiWidgetImpl::from(
            selector(UiString::key(UiTextCategory::Menu, "custom_game.terrain"), BLANK_MAP_TERRAINS.map(terrain_label).to_vec(), state.blank_terrain).on_changed(
                |selector, _, _| {
                    let state = STATE.as_mut();
                    state.blank_terrain = selector.current().min(BLANK_MAP_TERRAINS.len() - 1);
                    state.synced = false;
                },
            ),
        ),
    ]
}

// The options column: the starting date, goals and treasury under their captions, the difficulty
// and the switches.
// `gap`: the room above each part (OPTION_GAPS of them); `toggle_height`: each switch's row's.
fn options(context: &mut GameUiContext, scale: f32, text_scale: f32, width: f32, gap: f32, toggle_height: f32) -> UiWidgetGroup {
    let state = STATE.as_ref();
    let spacing = (PART_SPACING * scale).round();
    let font_scale = UiFontScale(text_scale);
    let mut group = UiWidgetGroup::new().with_center_horizontally(false).with_widget_spacing(Vec2::new(0.0, spacing * 0.5));
    group.add_widget(section_label(UiString::key(UiTextCategory::Menu, "custom_game.section.options"), text_scale, "options_heading"));

    let gap = || UiSeparator::new().with_size(Vec2::new(1.0, gap.max(1.0)));

    // The starting date, in the realm's eras.
    group.add_widget(gap());
    group.add_widget(caption(UiString::key(UiTextCategory::Menu, "custom_game.starting_date"), text_scale, "starting_date_caption"));
    group.add_widget(
        skin::dropdown(context, "", date_texts(state.realm))
            .with_hidden_label(UiString::key(UiTextCategory::Menu, "custom_game.starting_date"))
            .with_width(width)
            .with_font_scale(font_scale)
            .on_get_current_selection(|_, _| STATE.as_ref().date)
            .on_selection_changed(|dropdown, _| STATE.as_mut().date = dropdown.current_selection_index()),
    );

    // Goals: a sandbox only.
    // TODO(realm-ui): goal sets (population, treasury, monuments) once M9 defines them in config.
    group.add_widget(gap());
    group.add_widget(caption(UiString::key(UiTextCategory::Menu, "custom_game.goals"), text_scale, "goals_caption"));
    group.add_widget(
        skin::dropdown(context, "", [ui::text::get(UiTextCategory::Menu, "custom_game.goals.sandbox")])
            .with_hidden_label(UiString::key(UiTextCategory::Menu, "custom_game.goals"))
            .with_width(width)
            .with_font_scale(font_scale)
            .on_get_current_selection(|_, _| STATE.as_ref().goals)
            .on_selection_changed(|dropdown, _| STATE.as_mut().goals = dropdown.current_selection_index()),
    );

    group.add_widget(gap());
    group.add_widget(caption(UiString::key(UiTextCategory::Menu, "custom_game.starting_treasury"), text_scale, "starting_treasury_caption"));
    group.add_widget(
        skin::dropdown(context, "", treasury_texts())
            .with_hidden_label(UiString::key(UiTextCategory::Menu, "custom_game.starting_treasury"))
            .with_width(width)
            .with_font_scale(font_scale)
            .on_get_current_selection(|_, _| STATE.as_ref().treasury)
            .on_selection_changed(|dropdown, _| STATE.as_mut().treasury = dropdown.current_selection_index()),
    );

    // The difficulty: stored with the game.
    // TODO(realm-ui): applied by M4 (the economy's balance) and M9 (ratings).
    group.add_widget(gap());
    let mut step_look = skin::step_look();
    step_look.segment_height = (STEP_SEGMENT_HEIGHT * scale).round();
    group.add_widget(
        skin::step_selector(UiString::key(UiTextCategory::Menu, "custom_game.difficulty"), Difficulty::iter().map(|step| UiString::from(step.label())))
            .with_look(step_look)
            .with_font_scale(UiFontScale(text_scale * CAPTION_FONT_SCALE))
            .with_width(width)
            .with_current(state.difficulty as usize)
            .on_changed(|selector, _, _| STATE.as_mut().difficulty = Difficulty::from_index(selector.current())),
    );

    // The switches.
    group.add_widget(gap());
    let controls_scale = by_window(CONTROL_SCALES, scale);
    let toggle = |context: &mut GameUiContext, key: &'static str| {
        skin::toggle_at(context, UiString::key(UiTextCategory::Menu, key), controls_scale)
            .with_font_scale(font_scale)
            .with_fill_width(true)
            .with_min_height(toggle_height)
    };
    // TODO(realm-ui): seasons and floods come with M7; until then off and disabled.
    group.add_widget(
        toggle(context, "custom_game.seasons_and_floods")
            .with_enabled(false)
            .with_tooltip(skin::tooltip(context, UiString::key(UiTextCategory::Menu, "custom_game.coming_in_m7")))
            .on_read_value(|_, _| false),
    );
    group.add_widget(
        toggle(context, "custom_game.fires_and_collapses")
            .on_read_value(|_, _| STATE.as_ref().fires_and_collapses)
            .on_update_value(|_, _, value| {
                let state = STATE.as_mut();
                state.fires_and_collapses = value;
                state.synced = false; // The hazards in the details.
            }),
    );
    // TODO(realm-ui): raids and invasions come with game Phase 2; until then off and disabled.
    group.add_widget(
        toggle(context, "custom_game.raids")
            .with_enabled(false)
            .with_tooltip(skin::tooltip(context, UiString::key(UiTextCategory::Menu, "custom_game.coming_later")))
            .on_read_value(|_, _| false),
    );
    group.add_widget(
        toggle(context, "custom_game.fast_construction")
            .on_read_value(|_, _| STATE.as_ref().fast_construction)
            .on_update_value(|_, _, value| STATE.as_mut().fast_construction = value),
    );
    group
}

// The buttons' row: Cancel and Start at the right; and the column they're in.
fn buttons(context: &mut GameUiContext, scale: f32, fit: f32) -> (UiColumns, UiColumnsColumnIndex) {
    let height = by_window(BUTTON_HEIGHTS, scale).round();
    let width = |width: f32| UiTextButtonWidth::Fixed((width * scale).round());
    let fitted = |button: UiTextButton| {
        let font_scale = button.font_scale().0 * fit;
        button.with_font_scale(UiFontScale(font_scale))
    };
    let cancel = fitted(make_dialog_close_button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "button.cancel")))
        .with_width(width(CANCEL_WIDTH))
        .with_min_height(height);
    let start = fitted(make_text_button(context, ButtonRole::Primary, UiString::key(UiTextCategory::Menu, "custom_game.start"), |_, _| {
        if let Some(settings) = chosen_settings().filter(|settings| is_unlocked(settings.realm)) {
            custom_game::start(settings);
        }
    }))
    .with_width(width(START_WIDTH))
    .with_min_height(height);

    let spacing = (BUTTON_SPACING * scale).round();
    let actions_width = ((CANCEL_WIDTH + START_WIDTH) * scale).round() + spacing;
    let actions = UiWidgetGroup::new().with_stack_vertically(false).with_widget_spacing(Vec2::new(spacing, 0.0)).with_widget(cancel).with_widget(start);
    let mut columns = UiColumns::new().with_spacing(spacing);
    columns.add_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new());
    let actions = columns.add_column(UiColumnWidth::Fixed(actions_width), UiWidgetGroup::new().with_widget(actions));
    (columns, actions)
}
