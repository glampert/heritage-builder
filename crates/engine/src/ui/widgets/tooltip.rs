use super::UiWidgetContext;
use crate::{file_sys::paths::PathRef, render::texture::TextureHandle, ui::{UiFontScale, UiTooltipPlacement}};

// ----------------------------------------------
// UiTooltipText
// ----------------------------------------------

#[derive(Clone)]
pub struct UiTooltipText {
    pub(super) text: String,
    font_scale: UiFontScale,
    background: Option<TextureHandle>,
    placement: UiTooltipPlacement,
    opacity: f32,
}

impl UiTooltipText {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        debug_assert!(!text.is_empty());

        Self { text, font_scale: UiFontScale::default(), background: None, placement: UiTooltipPlacement::default(), opacity: 1.0 }
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    // A sprite behind the text, in place of the style's tooltip frame.
    pub fn with_background<'a>(mut self, context: &mut dyn UiWidgetContext, sprite: impl Into<PathRef<'a>>) -> Self {
        self.background = Some(context.load_texture(sprite.into()));
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

    // For the widget drawn last.
    pub(super) fn draw(&self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        crate::ui::custom_tooltip(context.ui_sys(), self.font_scale, self.background, self.placement, self.opacity, |ui_sys| {
            crate::ui::text(ui_sys, &self.text);
        });
    }
}
