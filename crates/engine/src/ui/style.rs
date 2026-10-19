// UI styles: the colors, spacing, frames and fonts of each UiTheme, loaded from JSON files under
// assets/configs/ui/styles/ (UiSystem::load_styles). A style names its own fonts, files under
// assets/fonts/, and gives each text style (body, heading, ...) a font and a size. Every field has
// a default, so a file only needs the ones it changes; unknown fields are errors.
//
// egui has one set of fonts per context, so every style's fonts go into one FontDefinitions, under
// names prefixed with the style's ("dev/mono"), and each text style becomes a font family
// ("dev/body") holding its font and fallbacks. egui's own Proportional and Monospace families get
// the developer style's body and monospace fonts, for the few places egui uses them itself.

use std::{collections::BTreeMap, sync::Arc};

use egui::{
    Color32,
    CornerRadius,
    FontData,
    FontDefinitions,
    FontFamily,
    FontId,
    Margin,
    Shadow,
    Stroke,
    TextStyle,
    epaint::text::FontTweak,
};
use serde::Deserialize;

use super::UiTheme;
use crate::{
    config,
    file_sys::{self, paths::PathRef},
};

// ----------------------------------------------
// UiStyle / UiStyles
// ----------------------------------------------

// A loaded style, ready for egui.
pub(super) struct UiStyle {
    pub egui: Arc<egui::Style>,
    pub icon_font: FontId,
    pub modal_backdrop: Color32,
}

impl UiStyle {
    // The font of `text_style`, e.g. TextStyle::Body for plain text.
    pub fn font(&self, text_style: &TextStyle) -> FontId {
        self.egui.text_styles.get(text_style).cloned().unwrap_or_default()
    }
}

// Every theme's style, and the fonts they use.
pub(super) struct UiStyles {
    styles: [UiStyle; UiTheme::COUNT],
    pub fonts: FontDefinitions,
}

impl UiStyles {
    // `paths`: a style file per theme, relative to assets/configs/.
    pub fn load(paths: &[(UiTheme, PathRef); UiTheme::COUNT]) -> Result<Self, String> {
        let mut fonts = FontDefinitions::empty();
        let mut font_files = BTreeMap::<String, Arc<Vec<u8>>>::new(); // Shared by the styles that use a file.
        let mut loaded = Vec::with_capacity(UiTheme::COUNT);

        for (index, (theme, path)) in paths.iter().enumerate() {
            debug_assert_eq!(*theme as usize, index, "Style paths must be in UiTheme order!");

            let file_path = config::configs_path().join(*path);
            let json = file_sys::load_string(&file_path).map_err(|err| format!("Can't read UI style {file_path}: {err}"))?;
            let def: UiStyleDef =
                serde_json::from_str(&json).map_err(|err| format!("Invalid UI style {file_path}: {err}"))?;

            let style = def.build(theme.style_name(), &mut fonts, &mut font_files).map_err(|err| format!("UI style {file_path}: {err}"))?;
            loaded.push(style);
        }

        // egui's own families, for the places egui uses them itself.
        let dev = UiTheme::Dev.style_name();
        let dev_family = |text_style: &str| fonts.families[&family(dev, text_style)].clone();
        let proportional = dev_family("body");
        let monospace = dev_family("monospace");
        fonts.families.insert(FontFamily::Proportional, proportional);
        fonts.families.insert(FontFamily::Monospace, monospace);

        let styles: [UiStyle; UiTheme::COUNT] =
            loaded.try_into().unwrap_or_else(|_| unreachable!("One style per theme"));

        Ok(Self { styles, fonts })
    }

    #[inline]
    pub fn style(&self, theme: UiTheme) -> &UiStyle {
        &self.styles[theme as usize]
    }
}

fn family(style_name: &str, text_style: &str) -> FontFamily {
    FontFamily::Name(format!("{style_name}/{text_style}").into())
}

// ----------------------------------------------
// UiStyleDef (the JSON file)
// ----------------------------------------------

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UiStyleDef {
    fonts: BTreeMap<String, UiFontDef>, // By name, for text_styles to refer to.
    text_styles: UiTextStylesDef,
    colors: UiColorsDef,
    widgets: UiWidgetsDef,
    spacing: UiSpacingDef,
    window: UiFrameDef,
    popup: UiPopupDef,
    animation_time: f32, // Seconds, e.g. for collapsing headers. 0 is instant.
    tooltip_delay: f32,  // Seconds the pointer rests on a widget before its tooltip shows.
    // How far from a resizable window's edges and corners the mouse grabs them, in points. The
    // zones straddle the edge, so keep them small: a window under the menu bar must not take
    // the bar's clicks.
    resize_grab_radius_side: f32,
    resize_grab_radius_corner: f32,
}

impl Default for UiStyleDef {
    fn default() -> Self {
        Self {
            fonts: BTreeMap::new(),
            text_styles: UiTextStylesDef::default(),
            colors: UiColorsDef::default(),
            widgets: UiWidgetsDef::default(),
            spacing: UiSpacingDef::default(),
            window: UiFrameDef { corner_radius: 6.0, stroke_width: 1.0 },
            popup: UiPopupDef { corner_radius: 4.0 },
            animation_time: 0.0,
            tooltip_delay: 0.0,
            resize_grab_radius_side: 5.0, // egui's defaults.
            resize_grab_radius_corner: 10.0,
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UiFontDef {
    file: String, // Under assets/fonts/.

    // The font draws this much larger or smaller than its text style's size. egui sizes a font by
    // its em, so a font whose ascent minus descent is larger than its em looks bigger than another
    // at the same size; units_per_em / (ascent - descent) sizes it by that height instead.
    scale: f32,

    y_offset_factor: f32, // Moves glyphs down by this fraction of the font size.
}

impl Default for UiFontDef {
    fn default() -> Self {
        Self { file: String::new(), scale: 1.0, y_offset_factor: 0.0 }
    }
}

// The six text styles every style has. `icon` is for FontAwesome icons (ui::icons).
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UiTextStylesDef {
    body: UiTextStyleDef,
    heading: UiTextStyleDef,
    button: UiTextStyleDef,
    small: UiTextStyleDef,
    monospace: UiTextStyleDef,
    icon: UiTextStyleDef,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UiTextStyleDef {
    font: String,           // A name from `fonts`.
    size: f32,              // In points.
    fallbacks: Vec<String>, // Fonts for the characters `font` lacks, in order.
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UiColorsDef {
    text: UiColor,
    text_disabled: UiColor,
    window_fill: UiColor, // Windows, and popups and tooltips too.
    panel_fill: UiColor,  // The main menu bar.
    border: UiColor,      // Windows and popups.
    separator: UiColor,
    faint_fill: UiColor,  // Striped rows.
    extreme_fill: UiColor, // Scroll bar tracks.
    text_edit_fill: UiColor,
    selection: UiColor,   // Selected text, items and the filled part of sliders.
    selection_text: UiColor,
    hyperlink: UiColor,
    warning: UiColor,
    error: UiColor,
    modal_backdrop: UiColor, // Dims the screen behind a modal.
}

impl Default for UiColorsDef {
    fn default() -> Self {
        let dark = egui::Visuals::dark();
        Self {
            text: UiColor(Color32::from_gray(220)),
            text_disabled: UiColor(Color32::from_gray(128)),
            window_fill: UiColor(dark.window_fill),
            panel_fill: UiColor(dark.panel_fill),
            border: UiColor(dark.window_stroke.color),
            separator: UiColor(dark.widgets.noninteractive.bg_stroke.color),
            faint_fill: UiColor(dark.faint_bg_color),
            extreme_fill: UiColor(dark.extreme_bg_color),
            text_edit_fill: UiColor(dark.extreme_bg_color),
            selection: UiColor(dark.selection.bg_fill),
            selection_text: UiColor(dark.selection.stroke.color),
            hyperlink: UiColor(dark.hyperlink_color),
            warning: UiColor(dark.warn_fg_color),
            error: UiColor(dark.error_fg_color),
            modal_backdrop: UiColor(Color32::from_black_alpha(40)),
        }
    }
}

// How widgets look in each state: text and labels (noninteractive), and widgets at rest
// (inactive), under the pointer (hovered), pressed (active) and with a popup open (open).
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UiWidgetsDef {
    noninteractive: UiWidgetDef,
    inactive: UiWidgetDef,
    hovered: UiWidgetDef,
    active: UiWidgetDef,
    open: UiWidgetDef,
}

impl Default for UiWidgetsDef {
    fn default() -> Self {
        let dark = egui::Visuals::dark().widgets;
        Self {
            noninteractive: UiWidgetDef::from_egui(&dark.noninteractive),
            inactive: UiWidgetDef::from_egui(&dark.inactive),
            hovered: UiWidgetDef::from_egui(&dark.hovered),
            active: UiWidgetDef::from_egui(&dark.active),
            open: UiWidgetDef::from_egui(&dark.open),
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UiWidgetDef {
    fill: UiColor,      // Checkboxes, sliders, scroll bar handles: widgets that always have a background.
    weak_fill: UiColor, // Buttons, combo boxes, selected tabs: widgets with an optional background.
    stroke: UiColor,    // Their outline.
    stroke_width: f32,
    text: UiColor,      // Their text, check marks and arrows.
    text_width: f32,    // The line width of check marks and arrows.
    corner_radius: f32,
    expansion: f32,     // How much larger than its rect it's drawn.
}

impl UiWidgetDef {
    fn from_egui(visuals: &egui::style::WidgetVisuals) -> Self {
        Self {
            fill: UiColor(visuals.bg_fill),
            weak_fill: UiColor(visuals.weak_bg_fill),
            stroke: UiColor(visuals.bg_stroke.color),
            stroke_width: visuals.bg_stroke.width,
            text: UiColor(visuals.fg_stroke.color),
            text_width: visuals.fg_stroke.width,
            corner_radius: f32::from(visuals.corner_radius.nw),
            expansion: visuals.expansion,
        }
    }

    fn to_egui(&self) -> egui::style::WidgetVisuals {
        egui::style::WidgetVisuals {
            bg_fill: self.fill.0,
            weak_bg_fill: self.weak_fill.0,
            bg_stroke: Stroke::new(self.stroke_width, self.stroke.0),
            corner_radius: corner_radius(self.corner_radius),
            fg_stroke: Stroke::new(self.text_width, self.text.0),
            expansion: self.expansion,
        }
    }
}

impl Default for UiWidgetDef {
    fn default() -> Self {
        Self::from_egui(&egui::Visuals::dark().widgets.inactive)
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UiSpacingDef {
    item_spacing: [f32; 2],   // Between widgets.
    button_padding: [f32; 2], // Between a framed widget's border and its text.
    window_margin: f32,       // Between a window's border and its contents.
    menu_margin: f32,         // Between a popup's border and its contents.
    indent: f32,
    interact_size: [f32; 2],  // The smallest a widget is.
    icon_width: f32,          // Check boxes and radio buttons.
    icon_spacing: f32,        // Between such an icon and its label.
    slider_width: f32,
    slider_rail_height: f32, // The bar the handle slides on.
    combo_width: f32,
    text_edit_width: f32,
    scroll_bar_width: f32,
}

impl Default for UiSpacingDef {
    fn default() -> Self {
        let spacing = egui::style::Spacing::default();
        Self {
            item_spacing: spacing.item_spacing.into(),
            button_padding: spacing.button_padding.into(),
            window_margin: f32::from(spacing.window_margin.left),
            menu_margin: f32::from(spacing.menu_margin.left),
            indent: spacing.indent,
            interact_size: spacing.interact_size.into(),
            icon_width: spacing.icon_width,
            icon_spacing: spacing.icon_spacing,
            slider_width: spacing.slider_width,
            slider_rail_height: spacing.slider_rail_height,
            combo_width: spacing.combo_width,
            text_edit_width: spacing.text_edit_width,
            scroll_bar_width: spacing.scroll.bar_width,
        }
    }
}

// Windows: their corner radius and border width.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UiFrameDef {
    corner_radius: f32,
    stroke_width: f32,
}

// Popups and tooltips: their corner radius. They take the window's border and fill.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UiPopupDef {
    corner_radius: f32,
}

impl UiStyleDef {
    // Adds this style's fonts to `fonts` under `name/` and returns the egui style.
    fn build(
        &self,
        name: &str,
        fonts: &mut FontDefinitions,
        font_files: &mut BTreeMap<String, Arc<Vec<u8>>>,
    ) -> Result<UiStyle, String> {
        if self.fonts.is_empty() {
            return Err("It has no fonts.".into());
        }

        for (font_name, font) in &self.fonts {
            let bytes = match font_files.get(&font.file) {
                Some(bytes) => bytes.clone(),
                None => {
                    let path = crate::file_sys::paths::assets_path().join("fonts").join(&font.file);
                    let bytes = Arc::new(file_sys::load_bytes(&path).map_err(|err| format!("Can't read font {path}: {err}"))?);
                    font_files.insert(font.file.clone(), bytes.clone());
                    bytes
                }
            };

            let tweak = FontTweak { scale: font.scale, y_offset_factor: font.y_offset_factor, ..Default::default() };
            let data = FontData::from_blob(bytes, 0).tweak(tweak);
            fonts.font_data.insert(format!("{name}/{font_name}"), Arc::new(data));
        }

        let text_styles = &self.text_styles;
        let mut font_ids = BTreeMap::new();

        for (key, text_style) in [
            ("body", TextStyle::Body),
            ("heading", TextStyle::Heading),
            ("button", TextStyle::Button),
            ("small", TextStyle::Small),
            ("monospace", TextStyle::Monospace),
            ("icon", TextStyle::Name("icon".into())),
        ] {
            let def = match key {
                "body" => &text_styles.body,
                "heading" => &text_styles.heading,
                "button" => &text_styles.button,
                "small" => &text_styles.small,
                "monospace" => &text_styles.monospace,
                _ => &text_styles.icon,
            };

            if def.size <= 0.0 {
                return Err(format!("Text style '{key}' needs a size."));
            }

            let mut family_fonts = Vec::with_capacity(1 + def.fallbacks.len());
            for font_name in std::iter::once(&def.font).chain(&def.fallbacks) {
                if !self.fonts.contains_key(font_name) {
                    return Err(format!("Text style '{key}' uses font '{font_name}', which isn't in its fonts."));
                }
                family_fonts.push(format!("{name}/{font_name}"));
            }

            fonts.families.insert(family(name, key), family_fonts);
            font_ids.insert(text_style, FontId::new(def.size, family(name, key)));
        }

        let icon_font = font_ids[&TextStyle::Name("icon".into())].clone();
        let modal_backdrop = self.colors.modal_backdrop.0;

        Ok(UiStyle { egui: Arc::new(self.to_egui(font_ids)), icon_font, modal_backdrop })
    }

    fn to_egui(&self, text_styles: BTreeMap<TextStyle, FontId>) -> egui::Style {
        let colors = &self.colors;
        let spacing = &self.spacing;

        let mut style = egui::Style { text_styles, animation_time: self.animation_time, ..Default::default() };

        // Text doesn't wrap unless a widget asks for it, and windows grow to fit it.
        style.wrap_mode = Some(egui::TextWrapMode::Extend);

        let visuals = &mut style.visuals;
        *visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(colors.text.0);
        visuals.weak_text_color = Some(colors.text_disabled.0);
        visuals.widgets.noninteractive = self.widgets.noninteractive.to_egui();
        visuals.widgets.noninteractive.bg_stroke.color = colors.separator.0; // Separators draw with it.
        visuals.widgets.inactive = self.widgets.inactive.to_egui();
        visuals.widgets.hovered = self.widgets.hovered.to_egui();
        visuals.widgets.active = self.widgets.active.to_egui();
        visuals.widgets.open = self.widgets.open.to_egui();
        visuals.selection = egui::style::Selection {
            bg_fill: colors.selection.0,
            stroke: Stroke::new(1.0, colors.selection_text.0),
        };
        visuals.hyperlink_color = colors.hyperlink.0;
        visuals.faint_bg_color = colors.faint_fill.0;
        visuals.extreme_bg_color = colors.extreme_fill.0;
        visuals.text_edit_bg_color = Some(colors.text_edit_fill.0);
        visuals.warn_fg_color = colors.warning.0;
        visuals.error_fg_color = colors.error.0;
        visuals.window_fill = colors.window_fill.0;
        visuals.window_stroke = Stroke::new(self.window.stroke_width, colors.border.0);
        visuals.window_corner_radius = corner_radius(self.window.corner_radius);
        visuals.window_shadow = Shadow::NONE;
        visuals.window_highlight_topmost = false;
        visuals.panel_fill = colors.panel_fill.0;
        visuals.menu_corner_radius = corner_radius(self.popup.corner_radius);
        visuals.popup_shadow = Shadow::NONE;
        visuals.button_frame = true;
        visuals.collapsing_header_frame = true;
        visuals.indent_has_left_vline = false;
        visuals.slider_trailing_fill = true;
        visuals.disabled_alpha = 0.5;

        let egui_spacing = &mut style.spacing;
        egui_spacing.item_spacing = spacing.item_spacing.into();
        egui_spacing.button_padding = spacing.button_padding.into();
        egui_spacing.window_margin = Margin::same(spacing.window_margin.round() as i8);
        egui_spacing.menu_margin = Margin::same(spacing.menu_margin.round() as i8);
        egui_spacing.indent = spacing.indent;
        egui_spacing.interact_size = spacing.interact_size.into();
        egui_spacing.icon_width = spacing.icon_width;
        egui_spacing.icon_width_inner = spacing.icon_width * 0.5;
        egui_spacing.icon_spacing = spacing.icon_spacing;
        egui_spacing.slider_width = spacing.slider_width;
        egui_spacing.slider_rail_height = spacing.slider_rail_height;
        egui_spacing.combo_width = spacing.combo_width;
        egui_spacing.text_edit_width = spacing.text_edit_width;
        egui_spacing.scroll = egui::style::ScrollStyle { bar_width: spacing.scroll_bar_width, ..egui::style::ScrollStyle::solid() };

        style.interaction.tooltip_delay = self.tooltip_delay;
        style.interaction.resize_grab_radius_side = self.resize_grab_radius_side;
        style.interaction.resize_grab_radius_corner = self.resize_grab_radius_corner;
        style.interaction.show_tooltips_only_when_still = false;
        style.interaction.selectable_labels = false; // Labels don't take clicks, as in the game's UI so far.
        style.explanation_tooltips = false;
        style.url_in_tooltip = false;

        style
    }
}

#[inline]
fn corner_radius(radius: f32) -> CornerRadius {
    CornerRadius::same(radius.round().clamp(0.0, 255.0) as u8)
}

// ----------------------------------------------
// UiColor
// ----------------------------------------------

// A color in a style file: "#RRGGBB", or "#RRGGBBAA" with straight alpha.
#[derive(Copy, Clone, Default)]
struct UiColor(Color32);

impl<'de> Deserialize<'de> for UiColor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        parse_hex_color(&text).map(UiColor).ok_or_else(|| {
            serde::de::Error::custom(format!("invalid color '{text}': expected \"#RRGGBB\" or \"#RRGGBBAA\""))
        })
    }
}

fn parse_hex_color(text: &str) -> Option<Color32> {
    let hex = text.strip_prefix('#')?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.is_ascii() {
        return None;
    }

    let channel = |index: usize| u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok();
    let alpha = if hex.len() == 8 { channel(3)? } else { u8::MAX };
    Some(Color32::from_rgba_unmultiplied(channel(0)?, channel(1)?, channel(2)?, alpha))
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_colors() {
        assert_eq!(parse_hex_color("#FF8000"), Some(Color32::from_rgb(255, 128, 0)));
        assert_eq!(parse_hex_color("#ff800080"), Some(Color32::from_rgba_unmultiplied(255, 128, 0, 128)));
        assert_eq!(parse_hex_color("FF8000"), None);
        assert_eq!(parse_hex_color("#FF80"), None);
        assert_eq!(parse_hex_color("#GG8000"), None);
    }

    #[test]
    fn rejects_unknown_fields_and_missing_fonts() {
        assert!(serde_json::from_str::<UiStyleDef>(r#"{ "colour": {} }"#).is_err());

        let def: UiStyleDef = serde_json::from_str(r#"{ "animation_time": 0.1 }"#).unwrap();
        let error = def.build("test", &mut FontDefinitions::empty(), &mut BTreeMap::new()).err();
        assert_eq!(error.as_deref(), Some("It has no fonts."));
    }
}
