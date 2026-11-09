use std::any::Any;
use egui::{Color32, Sense, Stroke, StrokeKind, vec2};

use common::{Color, Rect, RectTexCoords, Vec2, bitflags_with_display, time::Seconds};

use super::{UiTooltipText, UiWidget, UiWidgetContext, tex_coords_uv};
use crate::{
    file_sys::paths::PathRef,
    render::texture::TextureHandle,
    ui::{
        UiNineSlice, UiString, UiSystem,
        immediate::{self, color32, egui_rect, egui_vec2, nine_slice_shape, to_rect, uv_full},
        internal,
        renderer::egui_texture,
    },
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

// ----------------------------------------------
// UiImageFit
// ----------------------------------------------

// How an image fills its rect.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum UiImageFit {
    #[default]
    Contain, // Whole, as large as fits, centered: bands on two sides if the shapes differ.
    Cover,   // Fills the rect, centered, its overflow cut.
    Stretch, // Fills the rect, stretched.
}

impl UiImageFit {
    pub fn name(self) -> &'static str {
        match self {
            Self::Contain => "contain",
            Self::Cover => "cover",
            Self::Stretch => "stretch",
        }
    }
}

// Where an image of `image_size` draws in `rect` with `fit`, and the part of the image (UVs, 0 to
// 1 from the top left) it shows there.
pub fn fit_image(rect: Rect, image_size: Vec2, fit: UiImageFit) -> (Rect, Rect) {
    let full_uv = Rect::from_pos_and_size(Vec2::zero(), Vec2::new(1.0, 1.0));
    let (width, height) = (rect.width(), rect.height());
    if image_size.x <= 0.0 || image_size.y <= 0.0 || width <= 0.0 || height <= 0.0 {
        return (rect, full_uv);
    }

    let scale_x = width / image_size.x;
    let scale_y = height / image_size.y;
    match fit {
        UiImageFit::Stretch => (rect, full_uv),
        UiImageFit::Contain => {
            let scale = scale_x.min(scale_y);
            let size = Vec2::new(image_size.x * scale, image_size.y * scale);
            let pos = rect.min + Vec2::new((width - size.x) * 0.5, (height - size.y) * 0.5);
            (Rect::from_pos_and_size(pos, size), full_uv)
        }
        UiImageFit::Cover => {
            let scale = scale_x.max(scale_y);
            // The part of the image the rect shows, in UVs, centered.
            let uv_size = Vec2::new(width / (image_size.x * scale), height / (image_size.y * scale));
            let uv_pos = Vec2::new((1.0 - uv_size.x) * 0.5, (1.0 - uv_size.y) * 0.5);
            (rect, Rect::from_pos_and_size(uv_pos, uv_size))
        }
    }
}

// ----------------------------------------------
// UiImage
// ----------------------------------------------

// A texture in a rect: a save's thumbnail, a map preview, an emblem, briefing art, a comic panel.
// Fit to its rect (contain, cover or stretch), optionally on a frame panel (a 9-slice drawn
// behind it, the image inset in it) and on a background colour. Recorded as kind "image", its
// value {"fit": ...}.
pub struct UiImage {
    label: UiString,
    texture: Option<TextureHandle>,
    texture_size: Vec2,
    size: Vec2, // A width of 0 is the menu's; a height of 0 follows the image's shape.
    fit: UiImageFit,
    tint: Color,
    background: Option<Color>,
    frame: Option<(UiNineSlice, f32)>, // Drawn over the whole rect, the image `f32` points in.
    tooltip: Option<UiTooltipText>,
}

impl UiWidget for UiImage {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let hovered = ui_sys.with_ui(|ui| {
            let available = immediate::fill_size(ui).x;
            let size = self.resolve_size(available);
            let (rect, response) = ui.allocate_exact_size(egui_vec2(size), Sense::hover());

            if ui.is_rect_visible(rect) {
                if let Some((frame, _)) = &self.frame {
                    ui.painter().add(nine_slice_shape(rect, frame, Color32::WHITE));
                }
                let inner = rect.shrink(self.frame.map_or(0.0, |(_, inset)| inset));
                if let Some(background) = self.background {
                    ui.painter().rect_filled(inner, 0.0, color32(background));
                }
                if let Some(texture) = self.texture {
                    let (draw_rect, uv) = fit_image(to_rect(inner), self.texture_size, self.fit);
                    ui.painter().image(egui_texture(texture), egui_rect(draw_rect), egui_rect(uv), color32(self.tint));
                }
            }

            let tooltip = self.tooltip.as_ref().map(|tooltip| tooltip.text.resolve());
            let value = serde_json::json!({ "fit": self.fit.name() });
            immediate::record(ui_sys, ui, &response, "image", self.label.resolve(), tooltip, true, Some(value));
            response.hovered()
        });

        if hovered && let Some(tooltip) = &self.tooltip {
            tooltip.draw(context);
        }
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        self.resolve_size(internal::available_size(context.ui_sys()).x)
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }
}

impl UiImage {
    // The sprite under assets/ui/ at `path`, `size` points large (see with_size).
    pub fn new<'a>(context: &mut dyn UiWidgetContext, path: impl Into<PathRef<'a>>, size: Vec2) -> Self {
        let texture = context.load_texture(path.into());
        let texture_size = context.render_sys().texture_cache().texture_size(texture);
        let texture_size = texture_size.map_or(Vec2::new(1.0, 1.0), |size| Vec2::new(size.width as f32, size.height as f32));
        Self::from_texture(texture, texture_size, size)
    }

    // A texture already loaded, e.g. a save's thumbnail, `texture_size` pixels large.
    pub fn from_texture(texture: TextureHandle, texture_size: Vec2, size: Vec2) -> Self {
        let mut image = Self::empty(size);
        image.set_texture(texture, texture_size);
        image
    }

    // No texture yet (set_texture), e.g. a preview filled in later: its frame and background only.
    pub fn empty(size: Vec2) -> Self {
        debug_assert!(size.x >= 0.0 && size.y >= 0.0);
        Self {
            label: UiString::default(),
            texture: None,
            texture_size: Vec2::new(1.0, 1.0),
            size,
            fit: UiImageFit::Contain,
            tint: Color::white(),
            background: None,
            frame: None,
            tooltip: None,
        }
    }

    // Names it in widget paths ("<menu>/image" without one).
    pub fn with_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self
    }

    // Its rect: a width of 0 takes the menu's width, a height of 0 follows the image's shape.
    pub fn with_size(mut self, size: Vec2) -> Self {
        self.size = size;
        self
    }

    pub fn with_fit(mut self, fit: UiImageFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn with_tint(mut self, tint: Color) -> Self {
        self.tint = tint;
        self
    }

    // Behind the image, inside the frame: the bands a contained image leaves.
    pub fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    // A 9-slice panel over the whole rect, behind the image, which sits `inset` points in.
    pub fn with_frame(mut self, frame: UiNineSlice, inset: f32) -> Self {
        debug_assert!(inset >= 0.0);
        self.frame = Some((frame, inset));
        self
    }

    pub fn with_tooltip(mut self, tooltip: UiTooltipText) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    pub fn set_texture(&mut self, texture: TextureHandle, texture_size: Vec2) {
        self.texture = Some(texture);
        self.texture_size = Vec2::new(texture_size.x.max(1.0), texture_size.y.max(1.0));
    }

    pub fn clear_texture(&mut self) {
        self.texture = None;
    }

    fn resolve_size(&self, available_width: f32) -> Vec2 {
        let width = if self.size.x > 0.0 { self.size.x } else { available_width.max(1.0) };
        let height = if self.size.y > 0.0 {
            self.size.y
        } else {
            let inset = self.frame.map_or(0.0, |(_, inset)| inset) * 2.0;
            (width - inset).max(1.0) * self.texture_size.y / self.texture_size.x + inset
        };
        Vec2::new(width, height)
    }
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::from_pos_and_size(Vec2::new(x, y), Vec2::new(width, height))
    }

    #[test]
    fn contain_centers_the_whole_image() {
        // A 2:1 image in a square: full width, half height, centered.
        let (draw, uv) = fit_image(rect(10.0, 20.0, 100.0, 100.0), Vec2::new(200.0, 100.0), UiImageFit::Contain);
        assert_eq!(draw, rect(10.0, 45.0, 100.0, 50.0));
        assert_eq!(uv, rect(0.0, 0.0, 1.0, 1.0));
    }

    #[test]
    fn cover_fills_and_cuts_the_overflow() {
        // A 2:1 image in a square: the middle half of its width.
        let (draw, uv) = fit_image(rect(10.0, 20.0, 100.0, 100.0), Vec2::new(200.0, 100.0), UiImageFit::Cover);
        assert_eq!(draw, rect(10.0, 20.0, 100.0, 100.0));
        assert_eq!(uv, rect(0.25, 0.0, 0.5, 1.0));
        // A tall image in a wide rect: the middle of its height.
        let (_, uv) = fit_image(rect(0.0, 0.0, 200.0, 100.0), Vec2::new(100.0, 400.0), UiImageFit::Cover);
        assert_eq!(uv, rect(0.0, 0.4375, 1.0, 0.125)); // Scaled to 200 × 800: 100 of its 800 points.
    }

    #[test]
    fn stretch_and_degenerate_sizes_fill_the_rect() {
        let target = rect(0.0, 0.0, 30.0, 40.0);
        assert_eq!(fit_image(target, Vec2::new(7.0, 3.0), UiImageFit::Stretch), (target, rect(0.0, 0.0, 1.0, 1.0)));
        assert_eq!(fit_image(target, Vec2::new(0.0, 3.0), UiImageFit::Contain), (target, rect(0.0, 0.0, 1.0, 1.0)));
    }
}
