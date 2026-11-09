// Skins (docs/plans/realm-ui.md, Skins): everything realm-specific about the retail UI, as data.
// One file per realm, assets/configs/ui/skins/<realm>.json; a realm's file holds only what it
// changes and is merged over dailong.json (engine::config::merge_json: objects key by key, anything
// else replaced). A skin holds:
// - `style`: the realm's UI style (a UiStyleDef: fonts, text styles, colours, widget visuals,
//   spacing), loaded at boot as the theme named after the realm ("dailong", ...);
// - `text`: named text colours (ink, accent, on_accent, ...), which the other blocks name;
// - `headings`, `panels`, `buttons`, `separators`, `controls`, `emblems`, `icons`, `icon_sets`,
//   `backdrops`, `dialogs`: the sprites, 9-slice borders, insets and sizes the screens use;
// - `meters`, `rows`: the colours (`text` names or "#RRGGBB(AA)") of painted meters and step
//   selectors, and of list rows, key-value pairs and tables;
// - `sounds`: each UI sound role's file, applied with ui::sound::set_mapping.
//
// Every file is read once, at boot (the web build can read an asset only once), and kept; the
// textures a skin names are loaded through the texture cache, which keeps them. ui.reload_skin
// reads the files again (desktop; a changed image under the same path needs a restart). The skin
// shown follows realm::ui_realm_for(): follow() switches it, applies its sounds and bumps
// version(), which the dialogs (dialog::sync) and the HUD (InGameMenus) watch to rebuild
// themselves.
//
// The helpers below are how screens are built: dialog, button, panel, tooltip, heading, separator,
// popup, icon; and for the realm UI's widgets tab_bar, toggle, step_selector, menu_item,
// list_row_look, icon_button, framed_image, emblem, key_values, table, meter and rich_text. No
// sprite path is written in the menu code (see the test at the end).

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;
use strum::IntoEnumIterator;

use common::{Color, Vec2};
use engine::{
    config,
    file_sys::{self, paths::PathRef},
    log,
    render::texture::TextureHandle,
    sound::SoundSystem,
    ui::{
        DEV_UI_THEME,
        UiFontScale,
        UiNineSlice,
        UiNineSliceCenter,
        UiStyleSource,
        UiString,
        UiSystem,
        UiThemeId,
        UiTooltipPlacement,
        sound::{self, UiSoundMapping},
        widgets::*,
    },
};

use crate::{
    realm::Realm,
    tile::minimap::style::HexColor,
    ui_context::GameUiContext,
};

// The developer UI's style file, under assets/configs/.
const DEV_STYLE_FILE: &str = "ui/styles/dev.json";

// A skin file, under assets/configs/.
pub fn skin_file(realm: Realm) -> String {
    format!("ui/skins/{realm}.json")
}

// The UI theme of a realm's skin: its style, loaded under the realm's name.
pub fn theme_name(realm: Realm) -> String {
    realm.to_string()
}

// ----------------------------------------------
// Skin definition (the JSON)
// ----------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiSkinDef {
    pub sprite_scale: f32, // Pixels per point of the skin's art (2: drawn at 2×).
    pub style: Value,      // A UiStyleDef, checked by the engine when loaded.
    pub text: BTreeMap<String, HexColor>,
    pub headings: BTreeMap<String, SkinHeadingDef>,
    pub panels: BTreeMap<String, SkinPanelDef>,
    pub buttons: BTreeMap<String, SkinButtonDef>,
    pub icon_sets: BTreeMap<String, String>, // Sprite-button sets: name → folder of state sprites.
    pub icons: BTreeMap<String, String>,     // Single sprites by name.
    pub controls: SkinControlsDef,
    pub meters: SkinMetersDef,
    pub rows: SkinRowsDef,
    pub separators: BTreeMap<String, SkinSeparatorDef>,
    // TODO(realm-ui): Phase 1 draws the emblems (skin::emblem): the home menu's realm row, Custom
    // Game's realm rows and the save rows.
    pub emblems: BTreeMap<String, String>,   // Realm → stem (idle, locked, current states).
    pub backdrops: SkinBackdropsDef,
    pub dialogs: BTreeMap<String, SkinDialogDef>, // "default", then by dialog kind name.
    pub sounds: BTreeMap<String, Option<String>>,
}

impl Default for UiSkinDef {
    fn default() -> Self {
        Self {
            sprite_scale: 2.0,
            style: Value::Object(Default::default()),
            text: BTreeMap::new(),
            headings: BTreeMap::new(),
            panels: BTreeMap::new(),
            buttons: BTreeMap::new(),
            icon_sets: BTreeMap::new(),
            icons: BTreeMap::new(),
            controls: SkinControlsDef::default(),
            meters: SkinMetersDef::default(),
            rows: SkinRowsDef::default(),
            separators: BTreeMap::new(),
            emblems: BTreeMap::new(),
            backdrops: SkinBackdropsDef::default(),
            dialogs: BTreeMap::new(),
            sounds: BTreeMap::new(),
        }
    }
}

// How a 9-slice's middle and edges fill their room (UiNineSliceCenter).
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkinCenter {
    #[default]
    Stretch,
    Tile,
    Round,
}

impl SkinCenter {
    fn nine_slice(self) -> UiNineSliceCenter {
        match self {
            Self::Stretch => UiNineSliceCenter::Stretch,
            Self::Tile => UiNineSliceCenter::Tile,
            Self::Round => UiNineSliceCenter::Round,
        }
    }
}

// A panel: a 9-slice (`slice` given: borders in pixels, left, top, right, bottom) or a sprite
// stretched over the rect (no `slice`, the old HUD art). `content`: the contents' insets in points.
// `text`: the colour of text drawn on it, a `text` colour's name or "#RRGGBB" (the style's if
// none), e.g. light text on Tampadesa's dark lacquer tooltip.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinPanelDef {
    pub sprite: String,
    pub slice: Option<[f32; 4]>,
    pub center: SkinCenter,
    pub content: Option<[f32; 4]>,
    pub text: Option<String>,
}

// A button role. Framed (`sprite`: a stem with _idle, _hovered, _pressed and _disabled sprites,
// cut by `slice`), or plain text that shows the `hover` panel (grown by `hover_grow` points) or
// the `hover_sprite` underline while hovered. `font`: large, normal, small or extra_small.
// `text`, `text_hovered`: colours as panels name them. `min_width`: the narrowest a framed button
// gets, in points. The menu item role (UiMenuItem) also takes `marker`, a sprite at its left
// while hovered, and `subline`, its subline's colour; the icon role (UiIconButton) `selected`, the
// selected frame (cut by `slice`), and `size`, the button's in points.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinButtonDef {
    pub sprite: Option<String>,
    pub slice: [f32; 4],
    pub center: SkinCenter,
    pub padding: Option<[f32; 2]>,
    pub font: Option<SkinFontSize>,
    pub text: Option<String>,
    pub text_hovered: Option<String>,
    pub hover: Option<String>,
    pub hover_grow: [f32; 2],
    pub hover_sprite: Option<String>,
    pub min_width: Option<f32>,
    pub marker: Option<String>,
    pub subline: Option<String>,
    pub selected: Option<String>,
    pub size: Option<f32>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkinFontSize {
    Large,
    Normal,
    Small,
    ExtraSmall,
}

impl SkinFontSize {
    fn button_size(self) -> UiTextButtonSize {
        match self {
            Self::Large => UiTextButtonSize::Large,
            Self::Normal => UiTextButtonSize::Normal,
            Self::Small => UiTextButtonSize::Small,
            Self::ExtraSmall => UiTextButtonSize::ExtraSmall,
        }
    }
}

// A heading level: its lines' font scale, the separator under it, its margins and colour.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinHeadingDef {
    pub font_scale: f32,
    pub separator: Option<String>,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub color: Option<String>,
}

// A separator: a 9-slice rule (or a stretched sprite without `slice`), `height` points tall, with
// an optional ornament at its middle.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinSeparatorDef {
    pub sprite: String,
    pub slice: Option<[f32; 4]>,
    pub height: Option<f32>,
    pub ornament: Option<String>,
    pub ornament_size: [f32; 2],
}

// A 9-slice sprite of a control.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinSlicedSpriteDef {
    pub sprite: String,
    pub slice: [f32; 4],
}

// The controls' art. The toggle's draws UiToggle (skin::toggle); the others are checked by the
// tests but not drawn yet.
// TODO(realm-ui): skin the checkbox, slider, dropdown and scrollbar (egui's own widgets today)
// with these sprites, in place of the style's visuals.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinControlsDef {
    pub toggle: SkinToggleDef,
    pub checkbox: SkinCheckboxDef,
    pub slider: SkinSliderDef,
    pub dropdown: SkinDropdownDef,
    pub scrollbar: SkinScrollbarDef,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinToggleDef {
    pub off: String,
    pub on: String,
    pub knob: String,
    pub size: [f32; 2],         // The pill's, in points; the knob's is its sprite's.
    pub text: Option<String>,       // The label's colour.
    pub state_text: Option<String>, // The "On" / "Off" text's colour.
}

// The colours of painted meters (UiMeter), step selectors and toggles without sprites: the filled
// part, the empty part, outlines, a bar's goal tick and a ring's text.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinMetersDef {
    pub fill: Option<String>,
    pub track: Option<String>,
    pub outline: Option<String>,
    pub goal: Option<String>,
    pub text: Option<String>,
}

// The colours of rows: template list rows' titles (`selected`: a selected row's), subtitles and
// right-hand texts; key-value pairs' labels and values; table headers; the rules under rows; and
// a table's status marks (`done`, `pending`, `failed`: a goal met, still open, missed).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinRowsDef {
    pub title: Option<String>,
    pub selected: Option<String>,
    pub subtitle: Option<String>,
    pub label: Option<String>,
    pub value: Option<String>,
    pub header: Option<String>,
    pub rule: Option<String>,
    pub done: Option<String>,
    pub pending: Option<String>,
    pub failed: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinCheckboxDef {
    pub off: String,
    pub on: String,
    pub size: [f32; 2],
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinSliderDef {
    pub rail: SkinSlicedSpriteDef,
    pub fill: SkinSlicedSpriteDef,
    pub knob: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinDropdownDef {
    pub field: String, // A panel's name.
    pub arrow: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinScrollbarDef {
    pub track: SkinSlicedSpriteDef,
    pub thumb: SkinSlicedSpriteDef,
}

// The home menu's backdrop (one frame, or frames of an animation `home_frame_secs` apart, the last
// two looping) and the colour dialogs dim the screen with, in the menus and in game.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinBackdropsDef {
    pub home: Vec<String>,
    pub home_frame_secs: f32,
    pub dim_home: HexColor,
    pub dim_game: HexColor,
}

impl Default for SkinBackdropsDef {
    fn default() -> Self {
        Self { home: Vec::new(), home_frame_secs: 0.3, dim_home: HexColor::rgba(0), dim_game: HexColor::rgba(0x00000026) }
    }
}

// A dialog's window: its panel, size and placement, and whether the dialog under it shows.
// - `size`: [width, height] in points on a 1920 × 1080 window, scaled down to fit smaller ones
//   (dialog_size); a width or height of zero or less is the reference window's minus that much.
//   Or "fullscreen": the whole window, for full-screen screens (the world map, the intermission,
//   the comic viewer).
// - `align`: left (centred on the left), center or right; the menus mode's if none.
// - `show_below`: whether the dialog under this one stays visible, drawn inert under it (false if
//   none: the mockups hide Home's panel under Settings). A full-screen dialog hides it anyway.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinDialogDef {
    pub panel: Option<String>,
    pub size: Option<SkinDialogSize>,
    pub align: Option<SkinAlign>,
    pub show_below: Option<bool>,
}

#[derive(Copy, Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum SkinDialogSize {
    Points([f32; 2]),
    Mode(SkinDialogSizeMode),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkinDialogSizeMode {
    Fullscreen,
}

impl SkinDialogSize {
    // A dialog's size when neither its own entry nor `default` gives one: today's dialogs' width,
    // and the reference window's height minus 120 points.
    pub const DEFAULT: Self = Self::Points([550.0, -120.0]);

    #[inline]
    pub fn is_fullscreen(self) -> bool {
        self == Self::Mode(SkinDialogSizeMode::Fullscreen)
    }
}

// The window size, in points, that dialog sizes are given for.
pub const DIALOG_REFERENCE_WINDOW: Vec2 = Vec2::new(1920.0, 1080.0);
// The smallest window the dialogs are made to fit: on a smaller one they scale no further (and are
// cut to the window).
pub const DIALOG_MIN_WINDOW: Vec2 = Vec2::new(1280.0, 720.0);

// The size in points of a dialog of `size` on a window of `window` points: its size on the
// reference window, scaled down by as much as the window is smaller than it (the smaller of the
// width and height ratios, so its proportions stay), but no further than on the minimum window,
// and never larger than the window. Whole points, so its contents stay on the pixel grid.
// TODO(realm-ui): Phase 1's UI scale setting (Settings, Game tab) scales the sizes too: with it the
// factor is the window's ratio at 100% scale, so a dialog keeps its share of the screen while its
// contents grow.
pub fn dialog_size(size: SkinDialogSize, window: Vec2) -> Vec2 {
    let window = Vec2::new(window.x.max(1.0), window.y.max(1.0));
    let [width, height] = match size {
        SkinDialogSize::Mode(SkinDialogSizeMode::Fullscreen) => return window,
        SkinDialogSize::Points(points) => points,
    };

    let reference = DIALOG_REFERENCE_WINDOW;
    let fit = |size: f32, reference: f32| if size > 0.0 { size } else { (reference + size).max(1.0) };
    let size = Vec2::new(fit(width, reference.x), fit(height, reference.y));

    let min_factor = (DIALOG_MIN_WINDOW.x / reference.x).min(DIALOG_MIN_WINDOW.y / reference.y);
    let factor = (window.x / reference.x).min(window.y / reference.y).clamp(min_factor, 1.0);

    Vec2::new((size.x * factor).min(window.x).round().max(1.0), (size.y * factor).min(window.y).round().max(1.0))
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkinAlign {
    Left,
    Center,
    Right,
}

impl SkinAlign {
    pub fn menu_flags(self) -> UiMenuFlags {
        match self {
            Self::Left => UiMenuFlags::AlignCenter | UiMenuFlags::AlignLeft,
            Self::Center => UiMenuFlags::AlignCenter,
            Self::Right => UiMenuFlags::AlignCenter | UiMenuFlags::AlignRight,
        }
    }
}

// ----------------------------------------------
// Loading and merging
// ----------------------------------------------

// A skin file's JSON.
fn parse_skin_file(realm: Realm, text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|err| format!("Invalid UI skin {}: {err}", skin_file(realm)))
}

fn read_skin_file(realm: Realm) -> Result<Value, String> {
    let path = config::configs_path().join(skin_file(realm));
    let text = file_sys::load_string(&path).map_err(|err| format!("Can't read UI skin {path}: {err}"))?;
    parse_skin_file(realm, &text)
}

// `realm`'s skin: its file merged over Đại Long's, given each realm's file as JSON.
pub fn merge_skin(realm: Realm, files: &impl Fn(Realm) -> Value) -> Result<UiSkinDef, String> {
    let mut value = files(Realm::DaiLong);
    if realm != Realm::DaiLong {
        config::merge_json(&mut value, files(realm));
    }
    serde_json::from_value(value).map_err(|err| format!("Invalid UI skin '{realm}': {err}"))
}

// The skin files as JSON, read from assets/configs/ (each call reads them again).
pub fn read_skin_files() -> Result<Vec<(Realm, Value)>, String> {
    Realm::iter().map(|realm| read_skin_file(realm).map(|value| (realm, value))).collect()
}

// Every realm's skin from the files' JSON, in Realm order.
fn merge_all(files: &[(Realm, Value)]) -> Result<Vec<UiSkinDef>, String> {
    let file = |realm: Realm| files.iter().find(|(file_realm, _)| *file_realm == realm).map(|(_, value)| value.clone()).unwrap_or_default();
    Realm::iter().map(|realm| merge_skin(realm, &file)).collect()
}

// The UI styles of `skins`: the developer UI's first (its file), then each realm skin's style.
fn style_sources_of(skins: &[UiSkinDef]) -> Vec<UiStyleSource> {
    let mut sources = vec![UiStyleSource::file(DEV_UI_THEME, DEV_STYLE_FILE)];
    for (realm, skin) in Realm::iter().zip(skins) {
        sources.push(UiStyleSource::json(theme_name(realm), skin.style.clone()));
    }
    sources
}

// The UI styles the game loads (UiSystem::load_styles): the developer UI's, then one per realm
// skin, named after the realm. The loaded skins' if load() ran, else read from the files (tests).
pub fn style_sources() -> Result<Vec<UiStyleSource>, String> {
    if SkinState::get().loaded {
        return Ok(style_sources_of(&SkinState::get().skins));
    }
    Ok(style_sources_of(&merge_all(&read_skin_files()?)?))
}

// The sound mapping of a skin's `sounds` block, problems logged.
fn sound_mapping(skin: &UiSkinDef, realm: Realm) -> UiSoundMapping {
    let names = skin.sounds.clone().into_iter().collect();
    let (mapping, problems) = sound::mapping_from_names(names);
    for problem in problems {
        log::error!(log::channel!("ui"), "UI skin '{realm}': {problem}");
    }
    mapping
}

// ----------------------------------------------
// SkinState: the loaded skins and the current one
// ----------------------------------------------

struct SkinState {
    loaded: bool,
    skins: Vec<UiSkinDef>, // By Realm, merged over Đại Long's, as read at boot (or reloaded).
    current: Realm,
    version: u32, // Goes up whenever the current skin changes (realm switch or reload).
}

impl SkinState {
    const fn new() -> Self {
        Self { loaded: false, skins: Vec::new(), current: Realm::DaiLong, version: 0 }
    }
}

common::singleton! { SKIN_STATE, SkinState }

// Reads the three skin files and loads every UI style (the developer UI's and each skin's). Once,
// at boot, before any menu is built; then apply_sounds().
pub fn load(ui_sys: &mut UiSystem, pixels_per_point: f32) -> Result<(), String> {
    let files = read_skin_files()?;
    install(files, ui_sys, pixels_per_point)?;
    let names: Vec<String> = Realm::iter().map(|realm| realm.to_string()).collect();
    log::info!(log::channel!("ui"), "Loaded UI skins: {}.", names.join(", "));
    Ok(())
}

// Reads the skin files again (ui.reload_skin), e.g. after editing one, and reloads the styles; then
// apply_sounds(). If anything fails, the skins stay as they were. The texture cache keeps what it
// loaded, so a sprite image changed under the same path shows only after a restart. On web, which
// can read an asset only once (the style and font files included), nothing can have changed: the
// skins as loaded at boot stay, and only the menus rebuild.
pub fn reload(ui_sys: &mut UiSystem) -> Result<(), String> {
    if cfg!(feature = "web") {
        let state = SkinState::get_mut();
        if !state.loaded {
            return Err("The UI skins aren't loaded.".into());
        }
        state.version += 1;
        log::info!(log::channel!("ui"), "Rebuilt the UI with the skins as loaded at boot (web).");
        return Ok(());
    }
    let files = read_skin_files()?;
    let pixels_per_point = ui_sys.pixels_per_point();
    install(files, ui_sys, pixels_per_point)?;
    log::info!(log::channel!("ui"), "Reloaded the UI skins.");
    Ok(())
}

fn install(files: Vec<(Realm, Value)>, ui_sys: &mut UiSystem, pixels_per_point: f32) -> Result<(), String> {
    let skins = merge_all(&files)?;
    ui_sys.load_styles(&style_sources_of(&skins), pixels_per_point)?;

    let state = SkinState::get_mut();
    state.skins = skins;
    state.loaded = true;
    state.version += 1;
    Ok(())
}

// Gives the UI sounds Đại Long's skin's mapping as the default and the current skin's over it.
pub fn apply_sounds(sound_sys: &mut SoundSystem) {
    let state = SkinState::get();
    if !state.loaded {
        return;
    }
    sound::set_default_mapping(sound_sys, sound_mapping(&state.skins[Realm::DaiLong as usize], Realm::DaiLong));
    sound::set_mapping(sound_sys, &sound_mapping(&state.skins[state.current as usize], state.current));
}

// The skin shown now, and its realm.
pub fn current() -> &'static UiSkinDef {
    let state = SkinState::get();
    debug_assert!(state.loaded, "skin::load must run first!");
    &state.skins[state.current as usize]
}

pub fn current_realm() -> Realm {
    SkinState::get().current
}

// Goes up whenever the skin shown changes: a realm switch or a reload.
pub fn version() -> u32 {
    SkinState::get().version
}

pub fn is_loaded() -> bool {
    SkinState::get().loaded
}

// Shows `realm`'s skin if it doesn't already: its sounds, and a new version() for the menus to
// rebuild with. Retail menus call it as they're built and at the start of every frame, with
// realm::ui_realm_for() of their menus mode; apply_theme() switches the UI theme.
pub fn follow(context: &mut GameUiContext, realm: Realm) {
    let state = SkinState::get_mut();
    if !state.loaded || state.current == realm {
        return;
    }
    state.current = realm;
    state.version += 1;
    let mapping = sound_mapping(&state.skins[realm as usize], realm);
    sound::set_mapping(&mut context.sound_sys(), &mapping);
    log::info!(log::channel!("ui"), "UI skin: {realm}.");
}

// The current skin's theme: its realm's style.
pub fn theme(ui_sys: &UiSystem) -> UiThemeId {
    ui_sys.theme_id(&theme_name(current_realm())).unwrap_or(UiThemeId::DEV)
}

// Switches the UI to the current skin's theme (cheap when it's already on).
pub fn apply_theme(ui_sys: &UiSystem) {
    ui_sys.set_ui_theme(theme(ui_sys));
}

// ----------------------------------------------
// Colours
// ----------------------------------------------

// A colour as skins write them: a `text` colour's name, or "#RRGGBB(AA)". None if neither.
pub fn color(name: &str) -> Option<Color> {
    if name.starts_with('#') {
        return HexColor::parse(name).map(HexColor::color);
    }
    current().text.get(name).map(|color| color.color())
}

fn opt_color(name: &Option<String>) -> Option<Color> {
    name.as_deref().and_then(color)
}

// ----------------------------------------------
// Panels
// ----------------------------------------------

// A panel ready for a widget: a 9-slice or a stretched sprite, with its content insets and the
// colour of the text on it.
#[derive(Copy, Clone, Debug)]
pub enum SkinPanel {
    NineSlice { slice: UiNineSlice, content: UiMenuInsets, text_color: Option<Color> },
    Sprite { texture: engine::render::texture::TextureHandle, content: Option<UiMenuInsets>, text_color: Option<Color> },
}

impl SkinPanel {
    pub fn text_color(&self) -> Option<Color> {
        match self {
            Self::NineSlice { text_color, .. } | Self::Sprite { text_color, .. } => *text_color,
        }
    }

    // `menu` on this panel.
    pub fn apply_to_menu(self, menu: UiMenu) -> UiMenu {
        match self {
            Self::NineSlice { slice, content, .. } => menu.with_panel(slice, content),
            Self::Sprite { texture, content, .. } => {
                let menu = menu.with_background_texture(texture);
                match content {
                    Some(content) => menu.with_content_insets(content),
                    None => menu,
                }
            }
        }
    }

    // A popup on this panel.
    pub fn apply_to_message_box(self, message_box: UiMessageBox) -> UiMessageBox {
        match self {
            Self::NineSlice { slice, content, .. } => message_box.with_panel(slice, content),
            Self::Sprite { texture, content, .. } => {
                let message_box = message_box.with_background_texture(texture);
                match content {
                    Some(content) => message_box.with_content_insets(content),
                    None => message_box,
                }
            }
        }
    }
}

fn insets(content: Option<[f32; 4]>) -> Option<UiMenuInsets> {
    content.map(|[left, top, right, bottom]| UiMenuInsets::points(left, top, right, bottom))
}

// A 9-slice of a sprite under assets/ui/, at the skin's sprite scale.
fn nine_slice(context: &mut dyn UiWidgetContext, sprite: &str, slice_px: [f32; 4], center: SkinCenter) -> UiNineSlice {
    context.load_nine_slice(PathRef::from_str(sprite), slice_px).with_scale(art_scale()).with_center(center.nine_slice())
}

// The panel `name` of the current skin. A name the skin lacks logs an error and gives its
// `dialog` panel, so a skin error never stops the UI.
pub fn panel(context: &mut dyn UiWidgetContext, name: &str) -> SkinPanel {
    let skin = current();
    let def = match skin.panels.get(name) {
        Some(def) => def,
        None => {
            log::error!(log::channel!("ui"), "UI skin '{}' has no panel '{name}'.", current_realm());
            skin.panels.get("dialog").expect("Every skin has a dialog panel (Đại Long's).")
        }
    };

    let text_color = opt_color(&def.text);
    match def.slice {
        Some(slice_px) => SkinPanel::NineSlice {
            slice: nine_slice(context, &def.sprite, slice_px, def.center),
            content: insets(def.content).unwrap_or_default(),
            text_color,
        },
        None => SkinPanel::Sprite {
            texture: context.load_texture(PathRef::from_str(&def.sprite)),
            content: insets(def.content),
            text_color,
        },
    }
}

// ----------------------------------------------
// Dialogs and popups
// ----------------------------------------------

// The skin's settings for the dialog `kind` (a DialogMenuKind's name): its own over `default`.
pub fn dialog_def(kind: &str) -> SkinDialogDef {
    let skin = current();
    let mut def = skin.dialogs.get("default").cloned().unwrap_or_default();
    if let Some(own) = skin.dialogs.get(kind) {
        def.panel = own.panel.clone().or(def.panel);
        def.size = own.size.or(def.size);
        def.align = own.align.or(def.align);
        def.show_below = own.show_below.or(def.show_below);
    }
    def
}

// A dialog's menu: labeled `kind`, with `flags`, on its panel and at its size from the skin for the
// window as it is now (the dialog framework sets it again each frame it draws the dialog).
pub fn dialog(context: &mut GameUiContext, kind: &str, flags: UiMenuFlags) -> UiMenu {
    let def = dialog_def(kind);
    let size = dialog_size(def.size.unwrap_or(SkinDialogSize::DEFAULT), engine::ui::screen_size(context.ui_sys));

    let menu = UiMenu::new(kind).with_flags(flags).with_size(size);
    panel(context, def.panel.as_deref().unwrap_or("dialog")).apply_to_menu(menu)
}

// A popup message box of a dialog: `lines` as a popup heading on the skin's popup panel, buttons
// to add with with_buttons. `label` names it for automation (`<label>/<button>`); it isn't drawn.
pub fn popup(context: &mut dyn UiWidgetContext, label: &str, lines: impl IntoIterator<Item = UiString>) -> UiMessageBox {
    let heading = heading(context, lines, HeadingLevel::Popup);
    panel(context, "popup").apply_to_message_box(UiMessageBox::new(label)).with_content(heading)
}

// The colour the dialogs dim the screen with behind them, in the home menu or in game.
pub fn backdrop_dim(home_menu: bool) -> Color {
    let backdrops = &current().backdrops;
    if home_menu { backdrops.dim_home.color() } else { backdrops.dim_game.color() }
}

// The home menu's backdrop frames (one for a still), and the time between frames.
pub fn home_backdrop() -> (&'static [String], f32) {
    let backdrops = &current().backdrops;
    (&backdrops.home, backdrops.home_frame_secs)
}

// ----------------------------------------------
// Headings and separators
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum HeadingLevel {
    Dialog,  // A dialog's title.
    Popup,   // A popup's lines.
    Section, // A section of a dialog, e.g. the city pages' lists.
}

impl HeadingLevel {
    fn key(self) -> &'static str {
        match self {
            Self::Dialog => "dialog",
            Self::Popup => "popup",
            Self::Section => "section",
        }
    }

    fn default_font_scale(self) -> f32 {
        match self {
            Self::Dialog => 1.8,
            Self::Popup => 1.5,
            Self::Section => 1.2,
        }
    }
}

// A heading of `lines` at `level`: its font scale, colour, margins and separator from the skin.
pub fn heading(context: &mut dyn UiWidgetContext, lines: impl IntoIterator<Item = UiString>, level: HeadingLevel) -> UiMenuHeading {
    let def = current().headings.get(level.key()).cloned().unwrap_or_default();
    let font_scale = UiFontScale(if def.font_scale > 0.0 { def.font_scale } else { level.default_font_scale() });
    let color = opt_color(&def.color);

    let lines = lines.into_iter().map(|line| {
        let text = UiText::new(line, font_scale);
        match color {
            Some(color) => text.with_color(color),
            None => text,
        }
    });

    let heading = UiMenuHeading::new(lines).with_margin_top(def.margin_top).with_margin_bottom(def.margin_bottom);
    with_heading_separator(context, heading, level)
}

// `heading` with the separator of headings at `level`, for a heading built with its own lines
// (e.g. placeholders filled in later).
pub fn with_heading_separator(context: &mut dyn UiWidgetContext, heading: UiMenuHeading, level: HeadingLevel) -> UiMenuHeading {
    let Some(separator) = current().headings.get(level.key()).and_then(|def| def.separator.clone()) else {
        return heading;
    };
    match separator_look(context, &separator) {
        Some(SeparatorLook::Art(art)) => heading.with_separator_art(art),
        Some(SeparatorLook::Sprite(sprite)) => heading.with_separator(context, PathRef::from_str(&sprite)),
        None => heading,
    }
}

enum SeparatorLook {
    Art(UiSeparatorArt),
    Sprite(String), // Stretched over the line.
}

fn separator_look(context: &mut dyn UiWidgetContext, kind: &str) -> Option<SeparatorLook> {
    let Some(def) = current().separators.get(kind) else {
        log::error!(log::channel!("ui"), "UI skin '{}' has no separator '{kind}'.", current_realm());
        return None;
    };

    let Some(slice_px) = def.slice else {
        return Some(SeparatorLook::Sprite(def.sprite.clone()));
    };

    let rule = nine_slice(context, &def.sprite, slice_px, SkinCenter::Stretch);
    let height = def.height.unwrap_or_else(|| rule.texture_size.y * rule.scale);
    let mut art = UiSeparatorArt::new(rule, height);
    if let Some(ornament) = &def.ornament {
        let [width, height] = def.ornament_size;
        art = art.with_ornament(context.load_texture(PathRef::from_str(ornament)), Vec2::new(width, height));
    }
    Some(SeparatorLook::Art(art))
}

// A separator line of `kind` (rule, plain, or the HUD's hud and hud_vertical), across the menu.
pub fn separator(context: &mut dyn UiWidgetContext, kind: &str) -> UiSeparator {
    match separator_look(context, kind) {
        Some(SeparatorLook::Art(art)) => {
            let height = art.total_height();
            UiSeparator::new().with_art(art).with_thickness(height)
        }
        Some(SeparatorLook::Sprite(sprite)) => UiSeparator::new().with_sprite(context, PathRef::from_str(&sprite)),
        None => UiSeparator::new(),
    }
}

// ----------------------------------------------
// Buttons and tooltips
// ----------------------------------------------

// The roles text buttons play, each looked up in the skin's `buttons`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ButtonRole {
    Primary,     // A dialog's main action: Ok, Start, Save, Yes.
    Secondary,   // Its other actions: Cancel, No.
    MenuItem,    // A dialog's list of choices: Home's, the Game menu's.
    Tab,
    TabSelected,
    HudItem,     // The HUD's text buttons (the palette's lists, the inspector), until Phase 5.
}

impl ButtonRole {
    pub fn key(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::MenuItem => "menu_item",
            Self::Tab => "tab",
            Self::TabSelected => "tab_selected",
            Self::HudItem => "hud_item",
        }
    }
}

// A text button of `role` labeled `label`, pressing which runs `on_pressed`.
pub fn button<F>(context: &mut dyn UiWidgetContext, role: ButtonRole, label: impl Into<UiString>, on_pressed: F) -> UiTextButton
where
    F: for<'a> Fn(&'a UiTextButton, &mut dyn UiWidgetContext) + 'static,
{
    let mut button = UiTextButton::new(label).on_pressed(on_pressed);
    let Some(def) = current().buttons.get(role.key()) else {
        log::error!(log::channel!("ui"), "UI skin '{}' has no button role '{}'.", current_realm(), role.key());
        return button;
    };

    if let Some(font) = def.font {
        button = button.with_size(font.button_size());
    }

    if let Some(frame) = button_frame(context, def) {
        if let Some(width) = def.min_width {
            button = button.with_width(UiTextButtonWidth::Min(width));
        }
        return button.with_frame(frame);
    }

    button = button.with_text_colors(opt_color(&def.text), opt_color(&def.text_hovered));
    if let Some(hover) = &def.hover {
        if let SkinPanel::NineSlice { slice, .. } = panel(context, hover) {
            let [x, y] = def.hover_grow;
            button = button.with_hover_panel(slice, Vec2::new(x, y));
        }
    } else if let Some(sprite) = &def.hover_sprite {
        button = button.with_hover_sprite(context, PathRef::from_str(sprite));
    }
    button
}

// A framed button role's frame: its state sprites, padding and text colour. None for a plain one.
fn button_frame(context: &mut dyn UiWidgetContext, def: &SkinButtonDef) -> Option<UiTextButtonFrame> {
    let stem = def.sprite.as_ref()?;
    let mut frame = UiTextButtonFrame::load(context, stem, def.slice).with_scale(art_scale()).with_center(def.center.nine_slice());
    if let Some([x, y]) = def.padding {
        frame = frame.with_padding(Vec2::new(x, y));
    }
    if let Some(color) = opt_color(&def.text) {
        frame = frame.with_text_color(color);
    }
    Some(frame)
}

// The button role `role`, or an empty one (and an error logged) if the skin lacks it.
fn button_def(role: &str) -> SkinButtonDef {
    current().buttons.get(role).cloned().unwrap_or_else(|| {
        log::error!(log::channel!("ui"), "UI skin '{}' has no button role '{role}'.", current_realm());
        SkinButtonDef::default()
    })
}

// Points per pixel of the skin's art.
fn art_scale() -> f32 {
    let scale = current().sprite_scale;
    if scale > 0.0 { 1.0 / scale } else { 1.0 }
}

// The retail tooltips' font scale.
pub const TOOLTIP_FONT_SCALE: UiFontScale = UiFontScale(0.8);

// A tooltip on the skin's tooltip panel, in its text colour.
pub fn tooltip(context: &mut dyn UiWidgetContext, text: impl Into<UiString>) -> UiTooltipText {
    let tooltip = UiTooltipText::new(text).with_font_scale(TOOLTIP_FONT_SCALE);
    match panel(context, "tooltip") {
        SkinPanel::NineSlice { slice, text_color, .. } => tooltip.with_panel(slice).with_text_color(text_color),
        SkinPanel::Sprite { texture, text_color, .. } => tooltip.with_background_texture(texture).with_text_color(text_color),
    }
}

// A tooltip beside its widget, for buttons along the screen's edges.
pub fn tooltip_beside(context: &mut dyn UiWidgetContext, text: impl Into<UiString>) -> UiTooltipText {
    tooltip(context, text).with_placement(UiTooltipPlacement::BesideWidget)
}

// ----------------------------------------------
// Icons and sprite buttons
// ----------------------------------------------

// The single sprite `name` (the HUD's icons), under assets/ui/. Empty if the skin lacks it.
pub fn icon(name: &str) -> &'static str {
    match current().icons.get(name) {
        Some(path) => path,
        None => {
            log::error!(log::channel!("ui"), "UI skin '{}' has no icon '{name}'.", current_realm());
            ""
        }
    }
}

// The state sprites' stem of sprite button `name` in icon set `set` (e.g. "palette", "services").
pub fn icon_stem(set: &str, name: &str) -> String {
    match current().icon_sets.get(set) {
        Some(folder) => format!("{folder}/{name}"),
        None => {
            log::error!(log::channel!("ui"), "UI skin '{}' has no icon set '{set}'.", current_realm());
            name.to_string()
        }
    }
}

// A sprite button labeled `label` ("settings": its name in widget paths) from icon set `set`.
pub fn sprite_button(context: &mut dyn UiWidgetContext, set: &str, label: &str, size: Vec2) -> UiSpriteButton {
    UiSpriteButton::with_sprite_stem(context, label, &icon_stem(set, label), size)
}

// ----------------------------------------------
// The realm UI's widgets
// ----------------------------------------------

// A sprite under assets/ui/ and its size in points, at the skin's sprite scale.
fn sprite_and_size(context: &mut dyn UiWidgetContext, sprite: &str) -> (TextureHandle, Vec2) {
    let texture = context.load_texture(PathRef::from_str(sprite));
    let size = context.render_sys().texture_cache().texture_size(texture);
    let size = size.map_or(Vec2::new(16.0, 16.0), |size| Vec2::new(size.width as f32, size.height as f32) * art_scale());
    (texture, size)
}

// A colour the skin names, else `fallback`'s if that's a name, else nothing.
fn color_or(name: &Option<String>, fallback: &str) -> Option<Color> {
    opt_color(name).or_else(|| color(fallback))
}

// A tab bar whose tabs are the skin's `tab` and `tab_selected` buttons, its tabs added with
// with_tab or with_tab_contents.
pub fn tab_bar(context: &mut dyn UiWidgetContext) -> UiTabBar {
    let tab = button_def(ButtonRole::Tab.key());
    let selected = button_def(ButtonRole::TabSelected.key());
    let font_scale = tab.font.map_or(UiFontScale::default(), |font| font.button_size().font_scale());
    let look = UiTabLook {
        tab: button_frame(context, &tab),
        selected: button_frame(context, &selected),
        font_scale,
        min_tab_width: tab.min_width,
        ..UiTabLook::default()
    };
    UiTabBar::new().with_look(look)
}

// The look of a toggle: the skin's controls.toggle sprites, or painted in its meter colours if it
// names none.
pub fn toggle_look(context: &mut dyn UiWidgetContext) -> UiToggleLook {
    let def = current().controls.toggle.clone();
    let [width, height] = def.size;
    let size = Vec2::new(if width > 0.0 { width } else { 62.0 }, if height > 0.0 { height } else { 30.0 });
    if def.off.is_empty() || def.on.is_empty() || def.knob.is_empty() {
        let meters = meter_look();
        return UiToggleLook::Painted {
            on: meters.fill,
            off: meters.track,
            knob: color("on_accent").unwrap_or(Color::white()),
            outline: meters.outline.unwrap_or(meters.goal),
            size,
        };
    }
    let (off, _) = sprite_and_size(context, &def.off);
    let (on, _) = sprite_and_size(context, &def.on);
    let (knob, knob_size) = sprite_and_size(context, &def.knob);
    UiToggleLook::Sprites { off, on, knob, size, knob_size }
}

// A toggle labeled `label`, its state texts "On" and "Off", in the skin's look and colours. Give
// it on_read_value and on_update_value; with_fill_width(true) for a settings row.
pub fn toggle(context: &mut dyn UiWidgetContext, label: impl Into<UiString>) -> UiToggle {
    // TODO(realm-ui): the toggle's (and step_selector's) font scale is the engine's default; take
    // it from the skin (a `font` on controls.toggle) once the Phase 1 settings dialog sizes them.
    let def = &current().controls.toggle;
    let colors = (color_or(&def.text, "ink"), color_or(&def.state_text, "sub"));
    UiToggle::new().with_label(label).with_look(toggle_look(context)).with_text_colors(colors.0, colors.1)
}

// The colours of the skin's painted meters.
pub fn meter_look() -> UiMeterLook {
    let def = &current().meters;
    let default = UiMeterLook::default();
    UiMeterLook {
        fill: color_or(&def.fill, "accent").unwrap_or(default.fill),
        track: color_or(&def.track, "light").unwrap_or(default.track),
        outline: color_or(&def.outline, "sub").or(default.outline),
        goal: color_or(&def.goal, "ink").unwrap_or(default.goal),
        text: color_or(&def.text, "ink"),
    }
}

// A meter of `kind`, `size` points large, in the skin's colours. Give it its value.
pub fn meter(kind: UiMeterKind, size: Vec2) -> UiMeter {
    UiMeter::new(kind, size).with_look(meter_look())
}

// A step selector labeled `label` over `steps`, in the skin's meter colours and text colours.
pub fn step_selector(label: impl Into<UiString>, steps: impl IntoIterator<Item = UiString>) -> UiStepSelector {
    let meters = meter_look();
    let look = UiStepLook { fill: meters.fill, track: meters.track, outline: meters.outline, ..UiStepLook::default() };
    let rows = &current().rows;
    UiStepSelector::new(label, steps).with_look(look).with_text_colors(color_or(&rows.title, "ink"), color_or(&rows.selected, "accent"))
}

// An entry of a dialog's list of choices (the home menu's), with an optional subline: the skin's
// menu_item role's font, colours, hover panel and marker.
pub fn menu_item(context: &mut dyn UiWidgetContext, label: impl Into<UiString>, subline: Option<UiString>) -> UiMenuItem {
    let def = button_def(ButtonRole::MenuItem.key());
    let font = def.font.map_or(UiTextButtonSize::Large, SkinFontSize::button_size).font_scale();
    let mut item = UiMenuItem::new(label)
        .with_font_scales(font, UiFontScale(font.0 * 0.55))
        .with_text_colors(opt_color(&def.text), opt_color(&def.text_hovered), color_or(&def.subline, "sub"));
    if let Some(subline) = subline {
        item = item.with_subline(subline);
    }
    if let Some(hover) = &def.hover
        && let SkinPanel::NineSlice { slice, .. } = panel(context, hover)
    {
        let [x, y] = def.hover_grow;
        item = item.with_hover_panel(slice, Vec2::new(x, y));
    }
    if let Some(marker) = &def.marker {
        let (texture, size) = sprite_and_size(context, marker);
        item = item.with_marker(texture, size);
    }
    item
}

// The look of template list rows (UiItemList::with_rows): the skin's `row` and `row_selected`
// panels and row colours.
pub fn list_row_look(context: &mut dyn UiWidgetContext) -> UiListRowLook {
    let nine_slice_of = |context: &mut dyn UiWidgetContext, name: &str| match current().panels.contains_key(name) {
        true => match panel(context, name) {
            SkinPanel::NineSlice { slice, .. } => Some(slice),
            SkinPanel::Sprite { .. } => None,
        },
        false => None,
    };
    let rows = &current().rows;
    UiListRowLook {
        panel: nine_slice_of(context, "row"),
        selected_panel: nine_slice_of(context, "row_selected"),
        title_color: color_or(&rows.title, "ink"),
        selected_title_color: color_or(&rows.selected, "accent"),
        subtitle_color: color_or(&rows.subtitle, "sub"),
        ..UiListRowLook::default()
    }
}

// The look of key-value pairs' and tables' rows: the skin's row colours.
pub fn row_look() -> UiRowLook {
    let rows = &current().rows;
    UiRowLook {
        label_color: color_or(&rows.label, "sub"),
        value_color: color_or(&rows.value, "ink"),
        header_color: color_or(&rows.header, "accent"),
        rule_color: opt_color(&rows.rule).or(UiRowLook::default().rule_color),
        ..UiRowLook::default()
    }
}

// Label and value pairs in the skin's row colours; add them with with_pair.
pub fn key_values() -> UiKeyValues {
    UiKeyValues::new().with_look(row_look())
}

// A table in the skin's row colours; add its columns and rows (a status column with
// table_status_look's).
pub fn table() -> UiTable {
    UiTable::new().with_look(row_look())
}

// The painted status marks of a table's status column, in the skin's `rows` colours (the engine's
// where it names none).
pub fn table_status_look() -> UiTableStatusLook {
    let rows = &current().rows;
    let UiTableStatusLook::Painted { done, pending, failed } = UiTableStatusLook::default() else {
        unreachable!("the default status look is painted");
    };
    UiTableStatusLook::Painted {
        done: opt_color(&rows.done).unwrap_or(done),
        pending: opt_color(&rows.pending).unwrap_or(pending),
        failed: opt_color(&rows.failed).unwrap_or(failed),
    }
}

// Rich text in the skin's ink, its emphasis in the accent colour, and every colour the skin's
// `text` block names usable in its markup ("[accent]...[/]").
pub fn rich_text(markup: impl Into<UiString>) -> UiRichText {
    let mut text = UiRichText::new(markup);
    if let Some(ink) = color("ink") {
        text = text.with_text_color(ink);
    }
    if let Some(accent) = color("accent") {
        text = text.with_emphasis_color(accent);
    }
    for (name, value) in &current().text {
        text = text.with_color(name.clone(), value.color());
    }
    text
}

// `image` on the skin's panel `panel_name` (e.g. "well" for a map preview), inset by the panel's
// left content inset.
pub fn framed_image(context: &mut dyn UiWidgetContext, image: UiImage, panel_name: &str) -> UiImage {
    match panel(context, panel_name) {
        SkinPanel::NineSlice { slice, content, .. } => image.with_frame(slice, if content.relative { 0.0 } else { content.left }),
        SkinPanel::Sprite { .. } => image,
    }
}

// The emblem states the skin draws.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EmblemState {
    Idle,
    Locked,
    Current,
}

impl EmblemState {
    fn suffix(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Locked => "locked",
            Self::Current => "current",
        }
    }
}

// The path under assets/ui/ of `realm`'s emblem in `state`; None if the skin has no emblem for it.
pub fn emblem_path(realm: Realm, state: EmblemState) -> Option<String> {
    current().emblems.get(&realm.to_string()).map(|stem| format!("{stem}_{}.png", state.suffix()))
}

// `realm`'s emblem in `state`, for a list row's icon or an image: its texture and size in points.
pub fn emblem(context: &mut dyn UiWidgetContext, realm: Realm, state: EmblemState) -> Option<(TextureHandle, Vec2)> {
    let path = emblem_path(realm, state);
    if path.is_none() {
        log::error!(log::channel!("ui"), "UI skin '{}' has no emblem for '{realm}'.", current_realm());
    }
    path.map(|path| sprite_and_size(context, &path))
}

// A square icon button labeled `label` (its name in widget paths) showing the skin's icon `icon`
// (see icon()), on the skin's icon frame and at its size, drawing its selected frame while selected.
pub fn icon_button(context: &mut dyn UiWidgetContext, label: &str, icon_name: &str) -> UiIconButton {
    let def = button_def("icon");
    let size = def.size.unwrap_or(28.0);
    let mut button = UiIconButton::new(label, Vec2::new(size, size));
    if let Some(frame) = button_frame(context, &def) {
        button = button.with_frame(frame);
    }
    if let Some(selected) = &def.selected {
        button = button.with_selected_panel(nine_slice(context, selected, def.slice, def.center));
    }
    let path = icon(icon_name);
    if !path.is_empty() {
        button = button.with_icon(context, PathRef::from_str(path), (size * 0.18).round());
    }
    button
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    fn repo_path(relative: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(relative)
    }

    fn files() -> Vec<(Realm, Value)> {
        Realm::iter()
            .map(|realm| {
                let path = repo_path(&format!("assets/configs/{}", skin_file(realm)));
                let text = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
                (realm, parse_skin_file(realm, &text).unwrap())
            })
            .collect()
    }

    fn skins() -> Vec<(Realm, UiSkinDef)> {
        Realm::iter().zip(merge_all(&files()).unwrap_or_else(|err| panic!("{err}"))).collect()
    }

    fn ui_asset_exists(path: &str) -> bool {
        repo_path("assets/ui").join(path).is_file()
    }

    fn stem_exists(stem: &str) -> bool {
        ["idle", "hovered", "pressed", "disabled"].iter().all(|state| ui_asset_exists(&format!("{stem}_{state}.png")))
    }

    // Every skin loads, and every style validates.
    #[test]
    fn every_skin_loads() {
        for (realm, skin) in skins() {
            UiStyleSource::json(theme_name(realm), skin.style.clone()).validate().unwrap_or_else(|err| panic!("{realm}: {err}"));
            assert!(skin.sprite_scale > 0.0, "{realm}");
            for name in ["dialog", "popup", "tooltip"] {
                assert!(skin.panels.contains_key(name), "{realm}: no panel '{name}'");
            }
            for role in ["primary", "secondary", "menu_item", "hud_item"] {
                assert!(skin.buttons.contains_key(role), "{realm}: no button role '{role}'");
            }
            for level in ["dialog", "popup", "section"] {
                assert!(skin.headings.contains_key(level), "{realm}: no heading '{level}'");
            }
            assert!(!skin.backdrops.home.is_empty(), "{realm}: no home backdrop");
        }
    }

    // A realm's file holds only what it changes: what it doesn't name is Đại Long's.
    #[test]
    fn realm_skins_inherit_dai_long() {
        let skins = skins();
        let dailong = &skins[0].1;
        for (realm, skin) in &skins[1..] {
            assert_eq!(skin.icons, dailong.icons, "{realm}");
            assert_eq!(skin.sounds, dailong.sounds, "{realm}");
            assert_eq!(skin.style["spacing"], dailong.style["spacing"], "{realm}");
            assert_ne!(skin.panels["dialog"].sprite, dailong.panels["dialog"].sprite, "{realm}");
        }
    }

    // Every sprite a skin names exists, and its 9-slice borders fit it.
    #[test]
    fn every_skin_sprite_exists() {
        for (realm, skin) in skins() {
            let png_size = |path: &str| -> (f32, f32) {
                let bytes = std::fs::read(repo_path("assets/ui").join(path)).unwrap();
                assert_eq!(&bytes[1..4], b"PNG", "{realm}: {path}");
                let be = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as f32;
                (be(16), be(20))
            };
            let check_slice = |path: &str, slice: [f32; 4]| {
                let (width, height) = png_size(path);
                assert!(slice[0] + slice[2] < width && slice[1] + slice[3] <= height, "{realm}: {path}: borders {slice:?} don't fit {width} × {height}");
            };

            for (name, panel) in &skin.panels {
                assert!(ui_asset_exists(&panel.sprite), "{realm}: panel '{name}': {} doesn't exist", panel.sprite);
                if let Some(slice) = panel.slice {
                    check_slice(&panel.sprite, slice);
                }
                if let Some(text) = &panel.text {
                    assert!(text.starts_with('#') || skin.text.contains_key(text), "{realm}: panel '{name}': no colour '{text}'");
                }
            }
            for (role, button) in &skin.buttons {
                if let Some(stem) = &button.sprite {
                    assert!(stem_exists(stem), "{realm}: button '{role}': {stem}_<state>.png don't all exist");
                    check_slice(&format!("{stem}_idle.png"), button.slice);
                }
                for path in [&button.hover_sprite, &button.marker, &button.selected].into_iter().flatten() {
                    assert!(ui_asset_exists(path), "{realm}: button '{role}': {path} doesn't exist");
                }
                if let Some(hover) = &button.hover {
                    assert!(skin.panels.get(hover).is_some_and(|panel| panel.slice.is_some()), "{realm}: button '{role}': no 9-slice panel '{hover}'");
                }
            }
            for (name, separator) in &skin.separators {
                assert!(ui_asset_exists(&separator.sprite), "{realm}: separator '{name}': {} doesn't exist", separator.sprite);
                if let Some(ornament) = &separator.ornament {
                    assert!(ui_asset_exists(ornament), "{realm}: separator '{name}': {ornament} doesn't exist");
                }
            }
            for (level, heading) in &skin.headings {
                if let Some(separator) = &heading.separator {
                    assert!(skin.separators.contains_key(separator), "{realm}: heading '{level}': no separator '{separator}'");
                }
            }
            for (set, folder) in &skin.icon_sets {
                assert!(repo_path("assets/ui").join(folder).is_dir(), "{realm}: icon set '{set}': {folder} isn't a folder");
            }
            for (name, path) in &skin.icons {
                assert!(ui_asset_exists(path), "{realm}: icon '{name}': {path} doesn't exist");
            }
            for (name, stem) in &skin.emblems {
                for state in ["idle", "locked", "current"] {
                    assert!(ui_asset_exists(&format!("{stem}_{state}.png")), "{realm}: emblem '{name}' has no {state} sprite");
                }
            }
            for path in &skin.backdrops.home {
                assert!(ui_asset_exists(path), "{realm}: home backdrop {path} doesn't exist");
            }
            let controls = &skin.controls;
            for path in [
                &controls.toggle.off, &controls.toggle.on, &controls.toggle.knob, &controls.checkbox.off, &controls.checkbox.on,
                &controls.slider.rail.sprite, &controls.slider.fill.sprite, &controls.slider.knob, &controls.dropdown.arrow,
                &controls.scrollbar.track.sprite, &controls.scrollbar.thumb.sprite,
            ] {
                assert!(ui_asset_exists(path), "{realm}: control sprite {path} doesn't exist");
            }
            assert!(skin.panels.contains_key(&controls.dropdown.field), "{realm}: no panel '{}' for the dropdown", controls.dropdown.field);
            for (kind, dialog) in &skin.dialogs {
                if let Some(panel) = &dialog.panel {
                    assert!(skin.panels.contains_key(panel), "{realm}: dialog '{kind}': no panel '{panel}'");
                }
            }
        }
    }

    // Every colour a skin's blocks name is a `text` colour or "#RRGGBB(AA)", and the widgets'
    // helpers find what they need: framed tabs, the icon frame, the toggle's sprites.
    #[test]
    fn every_skin_colour_and_widget_part_resolves() {
        for (realm, skin) in skins() {
            let resolves = |name: &str| if name.starts_with('#') { HexColor::parse(name).is_some() } else { skin.text.contains_key(name) };
            let meters = &skin.meters;
            let rows = &skin.rows;
            let toggle = &skin.controls.toggle;
            let names = [
                &meters.fill, &meters.track, &meters.outline, &meters.goal, &meters.text, &rows.title, &rows.selected, &rows.subtitle,
                &rows.label, &rows.value, &rows.header, &rows.rule, &rows.done, &rows.pending, &rows.failed, &toggle.text, &toggle.state_text,
            ];
            for name in names.into_iter().flatten() {
                assert!(resolves(name), "{realm}: no colour '{name}'");
            }
            for (role, button) in &skin.buttons {
                for name in [&button.text, &button.text_hovered, &button.subline].into_iter().flatten() {
                    assert!(resolves(name), "{realm}: button '{role}': no colour '{name}'");
                }
            }
            for name in ["ink", "accent", "sub", "light"] {
                assert!(skin.text.contains_key(name), "{realm}: no text colour '{name}', which the widget helpers fall back on");
            }

            for role in ["tab", "tab_selected", "icon"] {
                assert!(skin.buttons.get(role).is_some_and(|button| button.sprite.is_some()), "{realm}: button '{role}' isn't framed");
            }
            assert!(skin.buttons["icon"].selected.is_some() && skin.buttons["icon"].size.is_some_and(|size| size > 0.0), "{realm}: icon button");
            assert!(skin.buttons["menu_item"].marker.is_some(), "{realm}: no menu item marker");
            assert!(!toggle.off.is_empty() && !toggle.on.is_empty() && !toggle.knob.is_empty(), "{realm}: toggle sprites");
            assert!(toggle.size[0] > 0.0 && toggle.size[1] > 0.0, "{realm}: toggle size");
            for name in ["row", "row_selected", "well"] {
                assert!(skin.panels.get(name).is_some_and(|panel| panel.slice.is_some()), "{realm}: no 9-slice panel '{name}'");
            }
            for realm_name in Realm::iter() {
                assert!(skin.emblems.contains_key(&realm_name.to_string()), "{realm}: no emblem for {realm_name}");
            }
        }
    }

    // Every sound role a skin names exists, and Đại Long's names them all (the defaults).
    #[test]
    fn every_skin_sound_exists() {
        for (realm, skin) in skins() {
            let (mapping, problems) = sound::mapping_from_names(skin.sounds.clone().into_iter().collect());
            assert!(problems.is_empty(), "{realm}: {problems:?}");
            for role in sound::UiSoundKey::iter() {
                if realm == Realm::DaiLong {
                    assert!(mapping.get(&role).is_some_and(Option::is_some), "{realm}: no file for '{role}'");
                }
                if let Some(Some(file)) = mapping.get(&role) {
                    let path = repo_path("assets/sounds/sfx/ui").join(file);
                    assert!(path.is_file(), "{realm}: '{role}' names {}, which doesn't exist", path.display());
                }
            }
        }
    }

    // Dialog sizes are given for a 1920 × 1080 window and shrink with it, keeping their proportions,
    // down to the 1280 × 720 window's size, and never past the window.
    #[test]
    fn dialog_sizes_scale_to_the_window() {
        let size = SkinDialogSize::Points([1220.0, 920.0]);
        let at = |width: f32, height: f32| dialog_size(size, Vec2::new(width, height));

        assert_eq!(at(1920.0, 1080.0), Vec2::new(1220.0, 920.0));
        assert_eq!(at(3840.0, 2160.0), Vec2::new(1220.0, 920.0)); // Never larger.
        assert_eq!(at(2560.0, 1080.0), Vec2::new(1220.0, 920.0)); // The smaller ratio counts.
        assert_eq!(at(1440.0, 800.0), Vec2::new(904.0, 681.0)); // 800 / 1080 of it, in whole points.
        assert_eq!(at(1440.0, 1080.0), Vec2::new(915.0, 690.0)); // 1440 / 1920 of it.
        assert_eq!(at(1280.0, 720.0), Vec2::new(813.0, 613.0)); // Two thirds.
        assert_eq!(at(1024.0, 600.0), Vec2::new(813.0, 600.0)); // No smaller than at 1280 × 720, but cut to the window.

        // A width or height of zero or less is the reference window's minus that much, scaled.
        let from_edges = SkinDialogSize::Points([550.0, -120.0]);
        assert_eq!(dialog_size(from_edges, Vec2::new(1920.0, 1080.0)), Vec2::new(550.0, 960.0));
        assert_eq!(dialog_size(from_edges, Vec2::new(1440.0, 800.0)), Vec2::new(407.0, 711.0));

        let fullscreen = SkinDialogSize::Mode(SkinDialogSizeMode::Fullscreen);
        assert_eq!(dialog_size(fullscreen, Vec2::new(1440.0, 800.0)), Vec2::new(1440.0, 800.0));
        assert!(fullscreen.is_fullscreen() && !size.is_fullscreen());
    }

    // A dialog's size is [width, height] or "fullscreen"; its entry merges over `default`.
    #[test]
    fn dialog_defs_parse_and_merge() {
        let def: SkinDialogDef = serde_json::from_value(serde_json::json!({ "size": "fullscreen", "show_below": true })).unwrap();
        assert_eq!(def.size, Some(SkinDialogSize::Mode(SkinDialogSizeMode::Fullscreen)));
        assert_eq!(def.show_below, Some(true));
        let def: SkinDialogDef = serde_json::from_value(serde_json::json!({ "size": [600, 960], "align": "left" })).unwrap();
        assert_eq!(def.size, Some(SkinDialogSize::Points([600.0, 960.0])));
        assert!(serde_json::from_value::<SkinDialogDef>(serde_json::json!({ "size": "huge" })).is_err());
        assert!(serde_json::from_value::<SkinDialogDef>(serde_json::json!({ "size": [600] })).is_err());
    }

    // Every skin's `dialogs` entries name a dialog (or `default`), and their sizes fit the
    // reference window.
    #[test]
    fn every_skin_dialog_entry_names_a_dialog() {
        let kinds: Vec<String> = crate::menu::dialog::DialogMenuKind::iter().map(|kind| kind.to_string()).collect();
        for (realm, skin) in skins() {
            assert!(skin.dialogs.contains_key("default"), "{realm}: no default dialog");
            for (name, def) in &skin.dialogs {
                assert!(name == "default" || kinds.contains(name), "{realm}: no dialog '{name}'");
                if let Some(SkinDialogSize::Points([width, height])) = def.size {
                    let size = dialog_size(SkinDialogSize::Points([width, height]), DIALOG_REFERENCE_WINDOW);
                    assert!(size.x <= DIALOG_REFERENCE_WINDOW.x && size.y <= DIALOG_REFERENCE_WINDOW.y, "{realm}: '{name}' too large");
                    assert!(width.abs() < DIALOG_REFERENCE_WINDOW.x && height.abs() < DIALOG_REFERENCE_WINDOW.y, "{realm}: '{name}': {width} × {height}");
                }
            }
        }
    }

    // A one-key map in a realm's file adds to Đại Long's (merge_json), it doesn't replace it.
    #[test]
    fn one_key_maps_merge() {
        let base = serde_json::json!({ "dialogs": { "default": { "panel": "dialog" } }, "sounds": { "alert": "a.wav" } });
        let overlay = serde_json::json!({ "dialogs": { "Home": { "size": [380, 980] } }, "sounds": { "toggled": null } });
        let files = |realm: Realm| if realm == Realm::DaiLong { base.clone() } else { overlay.clone() };
        let skin = merge_skin(Realm::Kambara, &files).unwrap();
        assert_eq!(skin.dialogs.len(), 2);
        assert_eq!(skin.sounds.len(), 2);
        assert_eq!(skin.sounds["toggled"], None);
    }

    // No sprite path is written in the menu code: every one comes from the skin.
    #[test]
    fn menu_code_has_no_sprite_path_literals() {
        const PREFIXES: [&str; 5] = ["misc/", "buttons/", "icons/", "menu_bar/", "palette/"];
        let mut found = Vec::new();
        let mut stack = vec![repo_path("crates/game/src/menu")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|ext| ext != "rs") || path.ends_with("skin.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                for (number, line) in text.lines().enumerate() {
                    let code = line.split("//").next().unwrap_or_default();
                    for prefix in PREFIXES {
                        if code.contains(&format!("\"{prefix}")) {
                            found.push(format!("{}:{}: {}", path.display(), number + 1, line.trim()));
                        }
                    }
                }
            }
        }
        assert!(found.is_empty(), "Sprite paths in the menu code (take them from the skin):\n{}", found.join("\n"));
    }
}
