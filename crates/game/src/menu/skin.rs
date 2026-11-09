// Skins (docs/plans/realm-ui.md, Skins): everything realm-specific about the retail UI, as data.
// One file per realm, assets/configs/ui/skins/<realm>.json; a realm's file holds only what it
// changes and is merged over dailong.json (engine::config::merge_json: objects key by key, anything
// else replaced). A skin holds:
// - `style`: the realm's UI style (a UiStyleDef: fonts, text styles, colours, widget visuals,
//   spacing), loaded at boot as the theme named after the realm ("dailong", ...);
// - `text`: named text colours (ink, accent, on_accent, ...), which the other blocks name;
// - `headings`, `panels`, `buttons`, `separators`, `controls`, `emblems`, `icons`, `icon_sets`,
//   `backdrops`, `dialogs`: the sprites, 9-slice borders, insets and sizes the screens use;
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
// popup, icon. No sprite path is written in the menu code (see the test at the end).

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;
use strum::IntoEnumIterator;

use common::{Color, Vec2};
use engine::{
    config,
    file_sys::{self, paths::PathRef},
    log,
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
    pub separators: BTreeMap<String, SkinSeparatorDef>,
    // TODO(realm-ui): Phase 1 draws the emblems: the home menu's campaign row and the save rows.
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
// `text`, `text_hovered`: colours as panels name them. `marker`, `selected`, `size`: for the menu
// item and icon button widgets to come.
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
    // TODO(realm-ui): Phase 0 Widgets draws these: the menu item's marker (UiMenuItem), the icon
    // button's selected frame and size (UiIconButton).
    pub marker: Option<String>,
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

// The controls' art, for the toggle, checkbox, slider, dropdown and scrollbar widgets to come
// (Phase 0, Widgets). Checked by the tests, not drawn yet.
// TODO(realm-ui): Phase 0 Widgets draws these (UiToggle, the checkbox, slider, dropdown and
// scrollbar skins) in place of egui's.
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
    pub size: [f32; 2],
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

// A dialog's panel, size and placement. `size` in points; a height (or width) of zero or less is
// the window's minus that much. `align`: left (centred on the left), center or right; the menus
// mode's if none.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkinDialogDef {
    pub panel: Option<String>,
    pub size: Option<[f32; 2]>,
    pub align: Option<SkinAlign>,
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
    let scale = current().sprite_scale;
    context
        .load_nine_slice(PathRef::from_str(sprite), slice_px)
        .with_scale(if scale > 0.0 { 1.0 / scale } else { 1.0 })
        .with_center(center.nine_slice())
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
    }
    def
}

// A dialog's menu: labeled `kind`, with `flags`, on its panel and at its size from the skin.
pub fn dialog(context: &mut GameUiContext, kind: &str, flags: UiMenuFlags) -> UiMenu {
    let def = dialog_def(kind);
    let viewport = Vec2::new(context.viewport_size.width as f32, context.viewport_size.height as f32);
    let [width, height] = def.size.unwrap_or([550.0, -120.0]);
    let fit = |size: f32, available: f32| if size > 0.0 { size } else { (available + size).max(1.0) };
    let size = Vec2::new(fit(width, viewport.x), fit(height, viewport.y));

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

    if let Some(stem) = &def.sprite {
        let scale = current().sprite_scale;
        let mut frame = UiTextButtonFrame::load(context, stem, def.slice)
            .with_scale(if scale > 0.0 { 1.0 / scale } else { 1.0 })
            .with_center(def.center.nine_slice());
        if let Some([x, y]) = def.padding {
            frame = frame.with_padding(Vec2::new(x, y));
        }
        if let Some(color) = opt_color(&def.text) {
            frame = frame.with_text_color(color);
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
