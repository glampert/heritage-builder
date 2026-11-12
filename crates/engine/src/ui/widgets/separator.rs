use std::any::Any;
use egui::{Color32, Sense};

use common::{Color, Vec2};

use super::{UiSeparatorArt, UiWidget, UiWidgetContext};
use crate::{
    file_sys::paths::PathRef,
    render::texture::TextureHandle,
    ui::{immediate::{color32, egui_vec2, uv_full}, internal, renderer::egui_texture},
};

// ----------------------------------------------
// UiSeparator
// ----------------------------------------------

// A line across the menu, or a blank spacer. Its size takes in the item spacing on both sides.
#[derive(Clone)]
pub struct UiSeparator {
    separator: Option<TextureHandle>,
    art: Option<UiSeparatorArt>, // In place of `separator`.
    color: Option<Color>,        // A plain line, `thickness` thick, in place of either.
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
        let size = self.size.unwrap_or_else(|| self.calc_size(ui_sys));

        ui_sys.with_ui(|ui| {
            // Empty space, optionally with a texture over it.
            let (rect, _) = ui.allocate_exact_size(egui_vec2(size), Sense::hover());

            if let Some(color) = self.color {
                let line = if self.vertical {
                    egui::Rect::from_center_size(rect.center(), egui::vec2(self.thickness, rect.height()))
                } else {
                    egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), self.thickness))
                };
                ui.painter().rect_filled(line, 0.0, color32(color));
            } else if let Some(art) = &self.art {
                art.paint(ui, rect);
            } else if let Some(separator) = self.separator {
                ui.painter().image(egui_texture(separator), rect, uv_full(), Color32::WHITE);
            }
        });
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        self.size.unwrap_or_else(|| self.calc_size(context.ui_sys()))
    }
}

impl UiSeparator {
    // Across the room it's in: a plain line just its thickness, the others with the item spacing
    // on both sides.
    fn calc_size(&self, ui_sys: &crate::ui::UiSystem) -> Vec2 {
        if self.color.is_some() {
            let room = internal::available_size(ui_sys);
            return if self.vertical { Vec2::new(self.thickness, room.y) } else { Vec2::new(room.x, self.thickness) };
        }
        internal::calc_separator_size(ui_sys, !self.vertical, self.thickness)
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
        Self { separator: None, art: None, color: None, size: None, thickness: 1.0, vertical: false }
    }

    // A sprite stretched over the line.
    pub fn with_sprite<'a>(mut self, context: &mut dyn UiWidgetContext, sprite: impl Into<PathRef<'a>>) -> Self {
        self.separator = Some(context.load_texture(sprite.into()));
        self
    }

    // A skin's rule and ornament over the line, in place of a sprite.
    pub fn with_art(mut self, art: UiSeparatorArt) -> Self {
        self.art = Some(art);
        self
    }

    // A plain line in `color`, `thickness` thick (with no spacing around it unless given a size).
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
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
