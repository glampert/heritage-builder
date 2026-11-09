use std::any::Any;
use egui::{Color32, Sense};

use common::Vec2;

use super::{UiWidget, UiWidgetContext};
use crate::{
    file_sys::paths::PathRef,
    render::texture::TextureHandle,
    ui::{immediate::{egui_vec2, uv_full}, internal, renderer::egui_texture},
};

// ----------------------------------------------
// UiSeparator
// ----------------------------------------------

// A line across the menu, or a blank spacer. Its size takes in the item spacing on both sides.
#[derive(Clone)]
pub struct UiSeparator {
    separator: Option<TextureHandle>,
    size: Option<Vec2>, // Across the menu if None.
    thickness: f32,
    vertical: bool,
}

impl UiWidget for UiSeparator {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let size = self.size.unwrap_or_else(|| internal::calc_separator_size(ui_sys, !self.vertical, self.thickness));

        ui_sys.with_ui(|ui| {
            // Empty space, optionally with a texture over it.
            let (rect, _) = ui.allocate_exact_size(egui_vec2(size), Sense::hover());

            if let Some(separator) = self.separator {
                ui.painter().image(egui_texture(separator), rect, uv_full(), Color32::WHITE);
            }
        });
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        self.size.unwrap_or_else(|| internal::calc_separator_size(context.ui_sys(), !self.vertical, self.thickness))
    }
}

impl Default for UiSeparator {
    fn default() -> Self {
        Self::new()
    }
}

impl UiSeparator {
    // A blank, one point thick line across the menu.
    pub fn new() -> Self {
        Self { separator: None, size: None, thickness: 1.0, vertical: false }
    }

    // A sprite stretched over the line.
    pub fn with_sprite<'a>(mut self, context: &mut dyn UiWidgetContext, sprite: impl Into<PathRef<'a>>) -> Self {
        self.separator = Some(context.load_texture(sprite.into()));
        self
    }

    // An exact size, in place of a line across the menu.
    pub fn with_size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    // The thickness of a line across the menu.
    pub fn with_thickness(mut self, thickness: f32) -> Self {
        debug_assert!(thickness >= 0.0);
        self.thickness = thickness;
        self
    }

    // A vertical line, for a side by side group.
    pub fn with_vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self
    }
}
