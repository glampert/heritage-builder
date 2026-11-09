use std::any::Any;
use egui::{Color32, Sense, Stroke, StrokeKind, vec2};

use common::{RectTexCoords, Vec2, bitflags_with_display, time::Seconds};

use super::{UiTooltipText, UiWidget, UiWidgetContext, tex_coords_uv};
use crate::{
    file_sys::paths::PathRef,
    render::texture::TextureHandle,
    ui::{UiSystem, immediate::{egui_vec2, uv_full}, internal, renderer::egui_texture},
};

// ----------------------------------------------
// UiSpriteIcon
// ----------------------------------------------

pub struct UiSpriteIcon {
    sprite: Option<TextureHandle>,
    tex_coords: RectTexCoords,
    size: Vec2,
    margin_top: f32,
    margin_bottom: f32,
    tooltip: Option<UiTooltipText>,
    unclipped_draw_size: Option<Vec2>, // Drawn at this size, past the menu's bounds, if set.
    outline: bool,
}

impl UiWidget for UiSpriteIcon {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let screen_rect = ui_sys.ctx().content_rect();

        let hovered = ui_sys.with_ui(|ui| {
            if self.margin_top != 0.0 {
                ui.add_space(self.margin_top); // NOTE: May be negative.
            }

            // Hover detection for the tooltip.
            let (rect, response) = ui.allocate_exact_size(egui_vec2(self.size), Sense::hover());

            // Render the actual sprite icon:
            if let Some(sprite) = self.sprite {
                let uv = tex_coords_uv(&self.tex_coords);
                let outline_stroke = Stroke::new(1.0, Color32::BLACK);

                // Unclipped, the icon can overflow the parent window's bounds.
                let (painter, icon_rect) = match self.unclipped_draw_size {
                    None => (ui.painter().clone(), rect),
                    Some(draw_size) => {
                        (ui.painter().with_clip_rect(screen_rect), egui::Rect::from_min_size(rect.min, egui_vec2(draw_size)))
                    }
                };

                painter.image(egui_texture(sprite), icon_rect, uv, Color32::WHITE);

                if self.outline {
                    painter.rect_stroke(rect.expand(2.0), 2.0, outline_stroke, StrokeKind::Middle);
                }
            }

            if self.margin_bottom > 0.0 {
                ui.add_space(self.margin_bottom);
            }

            ui_sys.set_last_response(&response);
            response.hovered()
        });

        if let Some(tooltip) = &self.tooltip
            && hovered
        {
            tooltip.draw(context);
        }
    }

    fn measure(&self, _context: &dyn UiWidgetContext) -> Vec2 {
        self.size
    }
}

impl UiSpriteIcon {
    // `size` is the room the icon takes; its sprite is given here or later, with set_sprite.
    pub fn new(size: Vec2) -> Self {
        debug_assert!(size.x > 0.0 && size.y > 0.0);

        Self {
            sprite: None,
            tex_coords: RectTexCoords::default(),
            size,
            margin_top: 0.0,
            margin_bottom: 0.0,
            tooltip: None,
            unclipped_draw_size: None,
            outline: false,
        }
    }

    pub fn with_sprite<'a>(mut self, context: &mut dyn UiWidgetContext, sprite: impl Into<PathRef<'a>>) -> Self {
        self.sprite = Some(context.load_texture(sprite.into()));
        self
    }

    // The part of the sprite's texture to draw, e.g. a tile of an atlas.
    pub fn with_tex_coords(mut self, tex_coords: RectTexCoords) -> Self {
        self.tex_coords = tex_coords;
        self
    }

    // Room above the icon; negative pulls it up.
    pub fn with_margin_top(mut self, margin: f32) -> Self {
        self.margin_top = margin;
        self
    }

    pub fn with_margin_bottom(mut self, margin: f32) -> Self {
        debug_assert!(margin >= 0.0);
        self.margin_bottom = margin;
        self
    }

    pub fn with_tooltip(mut self, tooltip: UiTooltipText) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    // Draws the sprite at this size from the icon's top left corner, past the menu's bounds, while
    // the icon takes `size` in the layout.
    pub fn with_unclipped_draw_size(mut self, draw_size: Vec2) -> Self {
        debug_assert!(draw_size.x > 0.0 && draw_size.y > 0.0);
        self.unclipped_draw_size = Some(draw_size);
        self
    }

    // A thin black frame around the icon.
    pub fn with_outline(mut self, outline: bool) -> Self {
        self.outline = outline;
        self
    }

    pub fn set_sprite(&mut self, sprite: TextureHandle) {
        self.sprite = Some(sprite);
    }

    pub fn set_tex_coords(&mut self, tex_coords: RectTexCoords) {
        self.tex_coords = tex_coords;
    }

    pub fn set_size(&mut self, size: Vec2) {
        self.size = size;
    }
}

// ----------------------------------------------
// UiSlideshowFlags
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Default)]
    pub struct UiSlideshowFlags: u8 {
        const Fullscreen = 1 << 0;
        const PlayedOnce = 1 << 1; // Finished playing at least once.
        const Looping    = 1 << 2; // Started playing again with one of UiSlideshowLoopMode.
    }
}

// ----------------------------------------------
// UiSlideshowLoopMode
// ----------------------------------------------

#[derive(Copy, Clone, Default)]
pub enum UiSlideshowLoopMode {
    #[default]
    None,               // Doesn't loop.
    WholeAnim,          // Loop whole anim from start to finish.
    FramesFromEnd(u32), // Loop between these many frames from the end (frame count - N).
}

// ----------------------------------------------
// UiSlideshow
// ----------------------------------------------

// Frames shown in turn: an animated background, or a still with one frame.
pub struct UiSlideshow {
    flags: UiSlideshowFlags,
    loop_mode: UiSlideshowLoopMode,

    // Ignored if UiSlideshowFlags::Fullscreen is set.
    size: Option<Vec2>,
    margin_left: f32,
    margin_right: f32,

    frames: Vec<TextureHandle>,
    frame_index: usize,
    frame_duration_secs: Seconds,
    frame_play_time_secs: Seconds,
}

impl UiWidget for UiSlideshow {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        self.update_anim(context.delta_time_secs());
        self.draw_current_frame(context);
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        internal::calc_child_window_size(ui_sys, self.requested_size(ui_sys))
    }
}

impl UiSlideshow {
    // The frames' sprites, in order. A single frame is a still; several play once, or again as
    // the loop mode says. In a menu, the slideshow fills the room left in it unless given a size.
    pub fn new<'a, I>(context: &mut dyn UiWidgetContext, frames: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<PathRef<'a>>,
    {
        let frames: Vec<TextureHandle> = frames.into_iter().map(|frame| context.load_texture(frame.into())).collect();
        debug_assert!(!frames.is_empty());

        // A still (single-frame) counts as already played once.
        let flags = if frames.len() <= 1 { UiSlideshowFlags::PlayedOnce } else { UiSlideshowFlags::empty() };

        Self {
            flags,
            loop_mode: UiSlideshowLoopMode::None,
            size: None,
            margin_left: 0.0,
            margin_right: 0.0,
            frames,
            frame_index: 0,
            frame_duration_secs: 0.0,
            frame_play_time_secs: 0.0,
        }
    }

    pub fn with_flags(mut self, flags: UiSlideshowFlags) -> Self {
        self.flags |= flags;
        self
    }

    pub fn with_loop_mode(mut self, loop_mode: UiSlideshowLoopMode) -> Self {
        self.loop_mode = loop_mode;
        self
    }

    // How long each frame shows; needed for more than one frame.
    pub fn with_frame_duration_secs(mut self, secs: Seconds) -> Self {
        debug_assert!(secs > 0.0);
        self.frame_duration_secs = secs;
        self
    }

    // Per axis: a positive size is fixed, zero takes the room left in the menu, and a negative one
    // takes that room less the value.
    pub fn with_size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    pub fn with_margins(mut self, left: f32, right: f32) -> Self {
        debug_assert!(left >= 0.0 && right >= 0.0);
        self.margin_left = left;
        self.margin_right = right;
        self
    }

    pub fn has_flags(&self, flags: UiSlideshowFlags) -> bool {
        self.flags.intersects(flags)
    }

    // ----------------------
    // Internal:
    // ----------------------

    // The size it asks for:
    //  > 0.0 -> fixed size
    //  = 0.0 -> use remaining host window size
    //  < 0.0 -> use remaining host window size minus abs(size)
    fn requested_size(&self, ui_sys: &UiSystem) -> Vec2 {
        let mut requested_size = self.size.unwrap_or(Vec2::zero());
        if self.margin_right > 0.0 {
            // NOTE: Decrement window padding from margin, so it is accurate.
            let padding = f32::from(ui_sys.egui_style().spacing.window_margin.left);
            requested_size.x -= self.margin_right - padding;
        }
        requested_size
    }

    fn update_anim(&mut self, delta_time_secs: Seconds) {
        if self.frames.len() <= 1 {
            // Static background (single-frame).
            return;
        }
        debug_assert!(self.frame_duration_secs > 0.0, "A slideshow with several frames needs a frame duration!");

        if self.has_flags(UiSlideshowFlags::PlayedOnce) && !self.has_flags(UiSlideshowFlags::Looping) {
            // Already played once and not looping. Early out.
            return;
        }

        // Advance animation:
        self.frame_play_time_secs += delta_time_secs;

        if self.frame_play_time_secs >= self.frame_duration_secs {
            if self.frame_index < self.frames.len() - 1 {
                // Move to next frame.
                self.frame_index += 1;
            } else {
                // Played the whole anim.
                self.flags.insert(UiSlideshowFlags::PlayedOnce);

                match self.loop_mode {
                    UiSlideshowLoopMode::WholeAnim => {
                        self.frame_index = 0; // Restart from beginning.
                        self.flags.insert(UiSlideshowFlags::Looping);
                    }
                    UiSlideshowLoopMode::FramesFromEnd(count) => {
                        self.frame_index = self.frames.len() - (count as usize); // Loop the last `count` frames.
                        self.flags.insert(UiSlideshowFlags::Looping);
                    }
                    UiSlideshowLoopMode::None => {} // Doesn't loop.
                }
            }

            // Reset the clock.
            self.frame_play_time_secs = 0.0;
        }
    }

    fn draw_current_frame(&mut self, context: &mut dyn UiWidgetContext) {
        let current_frame = self.frames[self.frame_index];
        let ui_sys = context.ui_sys();

        if context.is_inside_widget_window() && !self.has_flags(UiSlideshowFlags::Fullscreen) {
            // We are drawing inside a parent window: the frame takes its room in it.
            let size = internal::calc_child_window_size(ui_sys, self.requested_size(ui_sys));
            let margin_left = self.margin_left;

            ui_sys.with_ui(|ui| {
                let rect = egui::Rect::from_min_size(ui.cursor().min + vec2(margin_left, 0.0), egui_vec2(size));
                ui.painter().image(egui_texture(current_frame), rect, uv_full(), Color32::WHITE);
                ui.advance_cursor_after_rect(rect);
            });
        } else {
            // Draw full-screen rectangle with the anim frame texture, behind any other UI.
            let screen_rect = ui_sys.ctx().content_rect();
            let painter = ui_sys.ctx().layer_painter(egui::LayerId::background());
            painter.image(egui_texture(current_frame), screen_rect, uv_full(), Color32::WHITE);
        }
    }
}
