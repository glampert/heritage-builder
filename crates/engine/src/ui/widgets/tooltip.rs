use common::Color;

use super::UiWidgetContext;
use crate::{
    file_sys::paths::PathRef,
    render::texture::TextureHandle,
    ui::{UiFontScale, UiNineSlice, UiString, UiTooltipBackground, UiTooltipPlacement},
};

// ----------------------------------------------
// UiTooltipText
// ----------------------------------------------

#[derive(Clone)]
pub struct UiTooltipText {
    pub(super) text: UiString,
    font_scale: UiFontScale,
    background: UiTooltipBackground,
    placement: UiTooltipPlacement,
    opacity: f32,
    text_color: Option<Color>, // The style's if None.
}

impl UiTooltipText {
    pub fn new(text: impl Into<UiString>) -> Self {
        let text = text.into();
        debug_assert!(!text.is_empty());

        Self { text, font_scale: UiFontScale::default(), background: UiTooltipBackground::Style, placement: UiTooltipPlacement::default(), opacity: 1.0, text_color: None }
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    // A sprite stretched behind the text, in place of the style's tooltip frame.
    pub fn with_background<'a>(mut self, context: &mut dyn UiWidgetContext, sprite: impl Into<PathRef<'a>>) -> Self {
        self.background = UiTooltipBackground::Sprite(context.load_texture(sprite.into()));
        self
    }

    // A loaded texture stretched behind the text, as with_background.
    pub fn with_background_texture(mut self, texture: TextureHandle) -> Self {
        self.background = UiTooltipBackground::Sprite(texture);
        self
    }

    // A 9-slice panel behind the text, in place of the style's tooltip frame. It covers the text
    // and the frame's margins: `insets` in points (left, top, right, bottom), so the text stays
    // inside the panel's border, or the style's popup margins if None.
    pub fn with_panel(mut self, panel: UiNineSlice, insets: Option<[f32; 4]>) -> Self {
        self.background = UiTooltipBackground::Panel(panel, insets);
        self
    }

    // Where it shows: at the pointer (the default), or beside its widget.
    pub fn with_placement(mut self, placement: UiTooltipPlacement) -> Self {
        self.placement = placement;
        self
    }

    // 1 is opaque; less lets what's under the tooltip show through, background and text alike.
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    // The text's color, e.g. light text on a dark panel; the style's by default.
    pub fn with_text_color(mut self, color: Option<Color>) -> Self {
        self.text_color = color;
        self
    }

    // For the widget drawn last.
    pub(super) fn draw(&self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        crate::ui::custom_tooltip(context.ui_sys(), self.font_scale, self.background, self.placement, self.opacity, |ui_sys| {
            match self.text_color {
                Some(color) => crate::ui::text_colored(ui_sys, color, self.text.resolve()),
                None => crate::ui::text(ui_sys, self.text.resolve()),
            }
        });
    }
}
