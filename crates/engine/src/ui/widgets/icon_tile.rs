use std::any::Any;
use smallbox::smallbox;
use egui::Sense;
use strum::EnumCount;

use common::{Color, Rect, Vec2};

use super::{UiSpriteButtonState, UiTooltipText, UiWidget, UiWidgetCallback, UiWidgetContext, fit_image, UiImageFit};
use crate::{
    render::texture::TextureHandle,
    ui::{
        UiDecorationAnchor, UiLabelLook, UiNineSlice, UiPainter, UiTextAnchor,
        decorated_panel::{overhang, union},
        immediate::{self, egui_rect, to_rect},
        sound::{UiSoundEvent, UiSoundKey, UiWidgetSounds},
    },
};

const STATE_COUNT: usize = UiSpriteButtonState::COUNT;

// ----------------------------------------------
// UiIconTileSprite
// ----------------------------------------------

// One state's frame of an icon tile: a 9-slice, or a texture stretched over the frame's rect.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum UiIconTileSprite {
    Panel(UiNineSlice),
    Stretched(TextureHandle),
}

impl From<UiNineSlice> for UiIconTileSprite {
    fn from(panel: UiNineSlice) -> Self {
        Self::Panel(panel)
    }
}

impl From<TextureHandle> for UiIconTileSprite {
    fn from(texture: TextureHandle) -> Self {
        Self::Stretched(texture)
    }
}

// ----------------------------------------------
// UiIconGlyph
// ----------------------------------------------

// An icon tile's pictogram: a texture, or a part of one (a glyph atlas), `uv` 0 to 1 from its top
// left. Fit into the tile at the shape of that part.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct UiIconGlyph {
    pub texture: TextureHandle,
    pub uv: Rect,
}

impl UiIconGlyph {
    // The whole texture.
    pub fn new(texture: TextureHandle) -> Self {
        Self { texture, uv: Rect::from_pos_and_size(Vec2::zero(), Vec2::new(1.0, 1.0)) }
    }

    // A part of an atlas, `uv` 0 to 1 from its top left.
    pub fn from_atlas(texture: TextureHandle, uv: Rect) -> Self {
        Self { texture, uv }
    }

    // A part of an atlas `atlas_size` pixels large, `rect_px` in its pixels.
    pub fn from_atlas_pixels(texture: TextureHandle, atlas_size: Vec2, rect_px: Rect) -> Self {
        let scale = |v: Vec2| Vec2::new(v.x / atlas_size.x.max(1.0), v.y / atlas_size.y.max(1.0));
        Self { texture, uv: Rect { min: scale(rect_px.min), max: scale(rect_px.max) } }
    }
}

// ----------------------------------------------
// UiIconTileBadge
// ----------------------------------------------

// How an icon tile draws its badge (a count, e.g. alerts, or "0/3" goals): its text in a disc
// `radius` points large, widening to a pill for longer text, centred `offset` points from the
// `anchor` point on the tile's rect; a fill, a ring `ring_width` points wide (0 for none), and its
// text's colour and size in points, in the theme's body or heading font.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct UiIconTileBadge {
    pub anchor: UiDecorationAnchor,
    pub offset: Vec2,
    pub radius: f32,
    pub fill: Color,
    pub ring: Color,
    pub ring_width: f32,
    pub text_color: Color,
    pub font_size: f32,
    pub heading_font: bool,
}

impl Default for UiIconTileBadge {
    fn default() -> Self {
        Self {
            anchor: UiDecorationAnchor::TopRight,
            offset: Vec2::new(-2.0, 2.0),
            radius: 8.0,
            fill: Color::new(0.75, 0.15, 0.1, 1.0),
            ring: Color::new(0.95, 0.85, 0.55, 1.0),
            ring_width: 1.0,
            text_color: Color::white(),
            font_size: 11.0,
            heading_font: false,
        }
    }
}

impl UiIconTileBadge {
    // The badge's rect on a tile over `tile_rect`, for text `text_width` points wide: a disc of
    // the radius, or as wide as the text plus a radius of room (half on each side).
    pub fn rect(&self, tile_rect: Rect, text_width: f32) -> Rect {
        let height = self.radius.max(0.0) * 2.0;
        let width = height.max(text_width + self.radius.max(0.0));
        let center = self.anchor.point(tile_rect) + self.offset;
        Rect::from_pos_and_size(center - Vec2::new(width, height) * 0.5, Vec2::new(width, height))
    }
}

// What a badge shows.
#[derive(Clone, PartialEq, Debug)]
enum UiIconTileBadgeValue {
    Count(u32),
    Text(String),
}

impl UiIconTileBadgeValue {
    fn text(&self) -> std::borrow::Cow<'_, str> {
        match self {
            Self::Count(count) => count.to_string().into(),
            Self::Text(text) => text.as_str().into(),
        }
    }

    fn json(&self) -> serde_json::Value {
        match self {
            Self::Count(count) => serde_json::Value::from(*count),
            Self::Text(text) => serde_json::Value::from(text.as_str()),
        }
    }
}

// ----------------------------------------------
// UiIconTileLook
// ----------------------------------------------

// How icon tiles of one kind look (a skin's palette tiles, its small HUD tiles): shared by every
// tile of that kind, so built once and cloned into each.
// - A frame per state (idle, hovered, pressed, disabled), each multiplied by a tint, drawn on the
//   tile's rect grown by `frame_bleed` points (left, top, right, bottom), so a crest can stand
//   over the tile: the bleed changes neither the tile's size nor where it takes the pointer.
// - Its glyph in the tile's rect shrunk by `glyph_inset` points (left, top, right, bottom), fit
//   whole, multiplied by a tint per state (hovered brighter, disabled faded), and moved
//   `pressed_offset` points down while pressed or selected.
// - Its badge's look.
// A selected tile (a toggle that's on, the tool in use) draws the pressed state.
#[derive(Clone, PartialEq, Debug)]
pub struct UiIconTileLook {
    frames: [Option<UiIconTileSprite>; STATE_COUNT], // By UiSpriteButtonState.
    frame_tints: [Color; STATE_COUNT],
    frame_bleed: [f32; 4],
    glyph_inset: [f32; 4],
    glyph_tints: [Color; STATE_COUNT],
    pressed_offset: f32,
    badge: UiIconTileBadge,
}

impl Default for UiIconTileLook {
    fn default() -> Self {
        let mut glyph_tints = [Color::white(); STATE_COUNT];
        glyph_tints[UiSpriteButtonState::Disabled as usize] = Color::new(0.6, 0.6, 0.6, 0.5);
        Self {
            frames: [None; STATE_COUNT],
            frame_tints: [Color::white(); STATE_COUNT],
            frame_bleed: [0.0; 4],
            glyph_inset: [4.0; 4],
            glyph_tints,
            pressed_offset: 1.0,
            badge: UiIconTileBadge::default(),
        }
    }
}

impl UiIconTileLook {
    // No frame, the glyph 4 points in, faded when disabled, 1 point lower when pressed.
    pub fn new() -> Self {
        Self::default()
    }

    // A frame per state.
    pub fn with_frames(
        mut self,
        idle: impl Into<UiIconTileSprite>,
        hovered: impl Into<UiIconTileSprite>,
        pressed: impl Into<UiIconTileSprite>,
        disabled: impl Into<UiIconTileSprite>,
    ) -> Self {
        self.frames[UiSpriteButtonState::Idle as usize] = Some(idle.into());
        self.frames[UiSpriteButtonState::Hovered as usize] = Some(hovered.into());
        self.frames[UiSpriteButtonState::Pressed as usize] = Some(pressed.into());
        self.frames[UiSpriteButtonState::Disabled as usize] = Some(disabled.into());
        self
    }

    // One frame for every state, told apart by the frame tints (with_frame_tints).
    pub fn with_frame(self, frame: impl Into<UiIconTileSprite>) -> Self {
        let frame = frame.into();
        self.with_frames(frame, frame, frame, frame)
    }

    // What each state's frame is multiplied by.
    pub fn with_frame_tints(mut self, idle: Color, hovered: Color, pressed: Color, disabled: Color) -> Self {
        self.frame_tints = state_array(idle, hovered, pressed, disabled);
        self
    }

    // How far the frame reaches past the tile's rect, in points: left, top, right, bottom.
    pub fn with_frame_bleed(mut self, bleed: [f32; 4]) -> Self {
        debug_assert!(bleed.iter().all(|side| *side >= 0.0));
        self.frame_bleed = bleed;
        self
    }

    // The glyph's room inside the tile's rect, in points: left, top, right, bottom.
    pub fn with_glyph_inset(mut self, inset: [f32; 4]) -> Self {
        self.glyph_inset = inset;
        self
    }

    // What the glyph is multiplied by in each state.
    pub fn with_glyph_tints(mut self, idle: Color, hovered: Color, pressed: Color, disabled: Color) -> Self {
        self.glyph_tints = state_array(idle, hovered, pressed, disabled);
        self
    }

    // How far the glyph moves down while pressed or selected, in points.
    pub fn with_pressed_offset(mut self, offset: f32) -> Self {
        self.pressed_offset = offset;
        self
    }

    pub fn with_badge(mut self, badge: UiIconTileBadge) -> Self {
        self.badge = badge;
        self
    }

    pub fn badge(&self) -> &UiIconTileBadge {
        &self.badge
    }

    // The frame's rect for a tile over `tile_rect`.
    pub fn frame_rect(&self, tile_rect: Rect) -> Rect {
        let [left, top, right, bottom] = self.frame_bleed;
        Rect { min: tile_rect.min - Vec2::new(left, top), max: tile_rect.max + Vec2::new(right, bottom) }
    }

    // Where a glyph of `glyph_size` (any unit: only its shape counts) draws on a tile over
    // `tile_rect` in `state`: in the inset rect, fit whole and centred, lower while pressed.
    pub fn glyph_rect(&self, tile_rect: Rect, glyph_size: Vec2, state: UiSpriteButtonState) -> Rect {
        let [left, top, right, bottom] = self.glyph_inset;
        let min = tile_rect.min + Vec2::new(left, top);
        let max = tile_rect.max - Vec2::new(right, bottom);
        let inner = Rect { min, max: Vec2::new(max.x.max(min.x), max.y.max(min.y)) };
        let (rect, _) = fit_image(inner, glyph_size, UiImageFit::Contain);
        if state == UiSpriteButtonState::Pressed { rect.translated(Vec2::new(0.0, self.pressed_offset)) } else { rect }
    }
}

// Four values by UiSpriteButtonState.
fn state_array<T: Copy>(idle: T, hovered: T, pressed: T, disabled: T) -> [T; STATE_COUNT] {
    let mut values = [idle; STATE_COUNT];
    values[UiSpriteButtonState::Hovered as usize] = hovered;
    values[UiSpriteButtonState::Pressed as usize] = pressed;
    values[UiSpriteButtonState::Disabled as usize] = disabled;
    values
}

// The state a tile draws: disabled, pressed (the pointer down on it, or selected), hovered or idle.
fn tile_state(enabled: bool, pointer_down: bool, selected: bool, hovered: bool) -> UiSpriteButtonState {
    if !enabled {
        UiSpriteButtonState::Disabled
    } else if pointer_down || selected {
        UiSpriteButtonState::Pressed
    } else if hovered {
        UiSpriteButtonState::Hovered
    } else {
        UiSpriteButtonState::Idle
    }
}

// ----------------------------------------------
// UiIconTile
// ----------------------------------------------

pub type UiIconTilePressed = UiWidgetCallback<dyn Fn(&UiIconTile, &mut dyn UiWidgetContext)>;

// A HUD button in a realm's craft: a framed tile with a pictogram glyph in it and an optional
// badge, its look (UiIconTileLook) shared by the tiles of its kind. Its frame and badge may reach
// past its rect (a crest, a badge on the corner): they're drawn within its container's clip rect
// grown by that much, and change neither its size nor where it takes the pointer. A glyph may have
// a pressed variant, drawn while pressed or selected. Toggling (with_toggle) flips its selection
// on each press. Recorded as kind "icon_button", as UiIconButton is, its value whether it's
// selected and its badge while it shows one.
pub struct UiIconTile {
    label: String,
    size: Vec2,
    look: UiIconTileLook,
    glyph: Option<UiIconGlyph>,
    pressed_glyph: Option<UiIconGlyph>,
    badge: Option<UiIconTileBadgeValue>,
    toggle: bool,
    selected: bool,
    enabled: bool,
    tooltip: Option<UiTooltipText>,
    sounds: UiWidgetSounds, // Hovered: nothing, Pressed: ButtonPressed.
    hovered: bool,
    on_pressed: UiIconTilePressed,
}

impl UiWidget for UiIconTile {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        // The glyphs' shapes, from their textures' sizes.
        let glyph_size = |glyph: Option<UiIconGlyph>, context: &mut dyn UiWidgetContext| {
            glyph.map(|glyph| {
                let size = context.render_sys().texture_cache().texture_size(glyph.texture);
                let size = size.map_or(Vec2::new(1.0, 1.0), |size| Vec2::new(size.width as f32, size.height as f32));
                Vec2::new(size.x * glyph.uv.width().abs(), size.y * glyph.uv.height().abs())
            })
        };
        let glyph_size_normal = glyph_size(self.glyph, context);
        let glyph_size_pressed = glyph_size(self.pressed_glyph, context);

        let ui_sys = context.ui_sys();
        let enabled = self.enabled;
        let (pressed, hovered) = ui_sys.with_ui(|ui| {
            let (rect, response) = ui.allocate_exact_size(immediate::egui_vec2(self.size), Sense::click());
            let tile_rect = to_rect(rect);
            let state = tile_state(enabled, response.is_pointer_button_down_on(), self.selected, response.hovered());

            // The badge's text and rect.
            let painter = UiPainter::from_egui(ui.painter().clone(), ui_sys);
            let badge = self.badge.as_ref().map(|badge| {
                let text = badge.text();
                let look = self.badge_label_look();
                let text_rect = painter.label_rect(Vec2::zero(), UiTextAnchor::Middle, &text, &look);
                (text, text_rect, self.look.badge.rect(tile_rect, text_rect.width()))
            });

            // Everything it draws, past its rect too.
            let frame_rect = self.look.frame_rect(tile_rect);
            let mut bounds = union(tile_rect, frame_rect);
            if let Some((_, _, badge_rect)) = &badge {
                bounds = union(bounds, *badge_rect);
            }

            if ui.is_rect_visible(egui_rect(bounds)) {
                let painter = UiPainter::from_egui(immediate::overhang_painter(ui, overhang(tile_rect, bounds)), ui_sys);

                if let Some(frame) = &self.look.frames[state as usize] {
                    let tint = self.look.frame_tints[state as usize];
                    match frame {
                        UiIconTileSprite::Panel(panel) => painter.nine_slice(frame_rect, panel, tint),
                        UiIconTileSprite::Stretched(texture) => painter.image(*texture, frame_rect, tint),
                    }
                }

                let pressed_look = state == UiSpriteButtonState::Pressed;
                let (glyph, size) = match (self.pressed_glyph, glyph_size_pressed) {
                    (Some(glyph), Some(size)) if pressed_look => (Some(glyph), Some(size)),
                    _ => (self.glyph, glyph_size_normal),
                };
                if let (Some(glyph), Some(size)) = (glyph, size) {
                    let at = self.look.glyph_rect(tile_rect, size, state);
                    painter.image_uv(glyph.texture, at, glyph.uv, self.look.glyph_tints[state as usize]);
                }

                if let Some((text, text_rect, badge_rect)) = &badge {
                    let badge_look = &self.look.badge;
                    let radius = badge_rect.height() * 0.5;
                    painter.rect_filled(*badge_rect, badge_look.fill, radius);
                    if badge_look.ring_width > 0.0 {
                        painter.rect_stroke(*badge_rect, radius, badge_look.ring_width, badge_look.ring);
                    }
                    // Centred on the badge: label() places text by its baseline, `text_rect` is its
                    // rect for a baseline at 0.
                    let center = badge_rect.center();
                    let baseline = center.y - text_rect.height() * 0.5 - text_rect.min.y;
                    painter.label(Vec2::new(center.x, baseline), UiTextAnchor::Middle, text, &self.badge_label_look());
                }
            }

            let tooltip = self.tooltip.as_ref().map(|tooltip| tooltip.text.resolve());
            let name = self.label.rsplit('/').next().unwrap_or(&self.label);
            let value = || {
                let mut value = serde_json::json!({ "selected": self.selected });
                if let Some(badge) = &self.badge {
                    value["badge"] = badge.json();
                }
                Some(value)
            };
            immediate::record_lazy(ui_sys, ui, &response, "icon_button", name, tooltip, enabled, value);
            (response.clicked(), response.hovered())
        });

        if hovered {
            if !self.hovered && enabled {
                self.sounds.play(context, UiSoundEvent::Hovered, None);
            }
            if let Some(tooltip) = &self.tooltip {
                tooltip.draw(context);
            }
        }
        self.hovered = hovered;

        if pressed && enabled {
            self.sounds.play(context, UiSoundEvent::Pressed, Some(UiSoundKey::ButtonPressed));
            if self.toggle {
                self.selected = !self.selected;
            }
            if let Some(on_pressed) = &self.on_pressed {
                on_pressed(self, context);
            }
        }
    }

    fn measure(&self, _context: &dyn UiWidgetContext) -> Vec2 {
        self.size
    }

    fn label(&self) -> &str {
        &self.label
    }
}

impl UiIconTile {
    // A tile `size` points large in `look`, named `label` in widget paths (its last '/' part),
    // with no glyph yet.
    pub fn new(label: impl Into<String>, size: Vec2, look: UiIconTileLook) -> Self {
        let label = label.into();
        debug_assert!(!label.is_empty());
        debug_assert!(size.x > 0.0 && size.y > 0.0);
        Self {
            label,
            size,
            look,
            glyph: None,
            pressed_glyph: None,
            badge: None,
            toggle: false,
            selected: false,
            enabled: true,
            tooltip: None,
            sounds: UiWidgetSounds::new(),
            hovered: false,
            on_pressed: None,
        }
    }

    // Its glyph, and the one drawn while pressed or selected (the normal one if None).
    pub fn with_glyph(mut self, glyph: UiIconGlyph, pressed: Option<UiIconGlyph>) -> Self {
        self.set_glyph(glyph, pressed);
        self
    }

    // Each press flips its selection, before on_pressed runs.
    pub fn with_toggle(mut self, toggle: bool) -> Self {
        self.toggle = toggle;
        self
    }

    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    // A count in its badge; none if None.
    pub fn with_badge(mut self, badge: Option<u32>) -> Self {
        self.set_badge(badge);
        self
    }

    pub fn with_tooltip(mut self, tooltip: UiTooltipText) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    // `event` (Hovered or Pressed) plays `role` instead, or nothing for None.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.sounds.set(event, role);
        self
    }

    pub fn on_pressed<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiIconTile, &mut dyn UiWidgetContext) + 'static,
    {
        self.on_pressed = Some(smallbox!(f));
        self
    }

    // ----------------------
    // Updated per frame:
    // ----------------------

    pub fn set_glyph(&mut self, glyph: UiIconGlyph, pressed: Option<UiIconGlyph>) {
        self.glyph = Some(glyph);
        self.pressed_glyph = pressed;
    }

    // Its tooltip, changed after it's built (the build palette's, as the stock changes).
    pub fn set_tooltip(&mut self, tooltip: Option<UiTooltipText>) {
        self.tooltip = tooltip;
    }

    // A count in its badge, or no badge.
    pub fn set_badge(&mut self, badge: Option<u32>) {
        self.badge = badge.map(UiIconTileBadgeValue::Count);
    }

    // Text in its badge (e.g. "0/3"), or no badge.
    pub fn set_badge_text(&mut self, badge: Option<&str>) {
        match (badge, &mut self.badge) {
            (Some(text), Some(UiIconTileBadgeValue::Text(current))) if current == text => {}
            (Some(text), _) => self.badge = Some(UiIconTileBadgeValue::Text(text.to_string())),
            (None, _) => self.badge = None,
        }
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    // A new look, e.g. after a skin switch.
    pub fn set_look(&mut self, look: UiIconTileLook) {
        self.look = look;
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn look(&self) -> &UiIconTileLook {
        &self.look
    }

    fn badge_label_look(&self) -> UiLabelLook {
        let badge = &self.look.badge;
        UiLabelLook { size: badge.font_size, heading_font: badge.heading_font, color: badge.text_color, ..Default::default() }
    }
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect { min: Vec2::new(x0, y0), max: Vec2::new(x1, y1) }
    }

    #[test]
    fn selected_draws_pressed_and_disabled_wins() {
        assert_eq!(tile_state(true, false, false, false) as usize, UiSpriteButtonState::Idle as usize);
        assert_eq!(tile_state(true, false, false, true) as usize, UiSpriteButtonState::Hovered as usize);
        assert_eq!(tile_state(true, true, false, true) as usize, UiSpriteButtonState::Pressed as usize);
        assert_eq!(tile_state(true, false, true, true) as usize, UiSpriteButtonState::Pressed as usize);
        assert_eq!(tile_state(false, true, true, true) as usize, UiSpriteButtonState::Disabled as usize);
    }

    #[test]
    fn frame_bleeds_past_the_tile() {
        let look = UiIconTileLook::new().with_frame_bleed([2.0, 14.0, 2.0, 3.0]);
        assert_eq!(look.frame_rect(rect(10.0, 20.0, 58.0, 68.0)), rect(8.0, 6.0, 60.0, 71.0));
    }

    #[test]
    fn glyph_fits_its_inset_and_drops_when_pressed() {
        let look = UiIconTileLook::new().with_glyph_inset([4.0, 6.0, 4.0, 2.0]).with_pressed_offset(2.0);
        let tile = rect(0.0, 0.0, 48.0, 48.0);
        // A 40 × 40 room; a wide glyph (2:1) is 40 × 20, centred down it.
        assert_eq!(look.glyph_rect(tile, Vec2::new(64.0, 32.0), UiSpriteButtonState::Idle), rect(4.0, 16.0, 44.0, 36.0));
        assert_eq!(look.glyph_rect(tile, Vec2::new(64.0, 32.0), UiSpriteButtonState::Hovered), rect(4.0, 16.0, 44.0, 36.0));
        assert_eq!(look.glyph_rect(tile, Vec2::new(64.0, 32.0), UiSpriteButtonState::Pressed), rect(4.0, 18.0, 44.0, 38.0));
        // A square one fills it.
        assert_eq!(look.glyph_rect(tile, Vec2::new(10.0, 10.0), UiSpriteButtonState::Idle), rect(4.0, 6.0, 44.0, 46.0));
    }

    #[test]
    fn atlas_glyph_uv_from_pixels() {
        let glyph = UiIconGlyph::from_atlas_pixels(TextureHandle::White, Vec2::new(256.0, 128.0), rect(64.0, 32.0, 128.0, 96.0));
        assert_eq!(glyph.uv, rect(0.25, 0.25, 0.5, 0.75));
        assert_eq!(UiIconGlyph::new(TextureHandle::White).uv, rect(0.0, 0.0, 1.0, 1.0));
    }

    #[test]
    fn badge_is_a_disc_or_a_pill_at_its_anchor() {
        let badge = UiIconTileBadge { anchor: UiDecorationAnchor::TopRight, offset: Vec2::new(-2.0, 3.0), radius: 8.0, ..Default::default() };
        let tile = rect(0.0, 0.0, 40.0, 40.0);
        // Short text: a 16 point disc centred at (38, 3).
        assert_eq!(badge.rect(tile, 6.0), rect(30.0, -5.0, 46.0, 11.0));
        // "0/3" 20 points wide: 28 wide, as high.
        assert_eq!(badge.rect(tile, 20.0), rect(24.0, -5.0, 52.0, 11.0));

        let bottom_left = UiIconTileBadge { anchor: UiDecorationAnchor::BottomLeft, offset: Vec2::zero(), radius: 5.0, ..badge };
        assert_eq!(bottom_left.rect(tile, 0.0), rect(-5.0, 35.0, 5.0, 45.0));
    }

    #[test]
    fn badge_values() {
        let look = UiIconTileLook::new();
        let mut tile = UiIconTile::new("Hud/goals", Vec2::new(32.0, 32.0), look);
        assert!(tile.badge.is_none());
        tile.set_badge(Some(2));
        assert_eq!(tile.badge.as_ref().unwrap().json(), serde_json::json!(2));
        assert_eq!(tile.badge.as_ref().unwrap().text(), "2");
        tile.set_badge_text(Some("0/3"));
        assert_eq!(tile.badge.as_ref().unwrap().json(), serde_json::json!("0/3"));
        tile.set_badge(None);
        assert!(tile.badge.is_none());
    }
}
