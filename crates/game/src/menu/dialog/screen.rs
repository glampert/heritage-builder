// What the full-screen dialogs laid out after the mockup generator's 1920 × 1080 compositions share
// (the world map, the debrief): their scale on the window, their text sizes in the generator's
// points, and the rows and lines of text they're built from.
//
// The composition is scaled to fit the window (the smaller of the width's and the height's
// ratios) and centred on a window of another shape. Its layout follows the window only: the
// player's UI scale grows the realm themes' text, so text here takes it back out (Scales::font),
// and looks the same at 75% and 200% (TODO(realm-ui) at skin::ui_scale: a full UI scale through
// egui's zoom).

use common::{Color, Vec2};
use engine::ui::{
    UiFontScale,
    widgets::*,
};

use super::skin;
use crate::ui_context::GameUiContext;

// The generator's screen, in points.
pub const REFERENCE: Vec2 = Vec2::new(1920.0, 1080.0);

// The tallest row of text the screens keep, in ems (tight).
const TIGHT_LINE: f32 = 1.3;

// How much a screen's sizes scale on a window of `window` points: as the generator's 1920 × 1080
// composition fits it (the smaller of the width's and the height's ratios).
pub fn screen_scale(window: Vec2) -> f32 {
    (window.x.max(1.0) / REFERENCE.x).min(window.y.max(1.0) / REFERENCE.y)
}

// A screen's scales on the current window: its sizes', and its text's over the realm theme's body
// size (which the UI scale grows; the screen's layout follows the window only, so its text takes
// the UI scale back out).
#[derive(Copy, Clone)]
pub struct Scales {
    pub size: f32,
    pub body: f32,    // The theme's body text size at 100%.
    pub origin: Vec2, // The composition's top left in the window.
}

impl Scales {
    pub fn new(window: Vec2) -> Self {
        let size = screen_scale(window);
        let origin = Vec2::new(((window.x - REFERENCE.x * size) * 0.5).max(0.0), ((window.y - REFERENCE.y * size) * 0.5).max(0.0));
        Self { size, body: body_text_size(), origin }
    }

    // Scales of `size` screen points per generator point, from the top left: a dialog laid out
    // from the generator's coordinates in a rect of its own (the briefing).
    pub fn with_size(size: f32) -> Self {
        Self { size, body: body_text_size(), origin: Vec2::zero() }
    }

    #[inline]
    pub fn pt(&self, points: f32) -> f32 {
        points * self.size
    }

    // The font scale of body text `points` large (the generator's font size: an em, where the
    // theme's sizes are line heights, skin::font_em).
    pub fn font(&self, points: f32) -> UiFontScale {
        UiFontScale((points / skin::font_em(false) * self.size / (self.body * skin::ui_scale())).max(0.05))
    }

    // The font scale of text `points` large in the heading font (UiRichText::with_heading_font).
    pub fn heading_font(&self, points: f32) -> UiFontScale {
        UiFontScale((points / skin::font_em(true) * self.size / (self.body * skin::ui_scale())).max(0.05))
    }

    // A button's text at the generator's button size `points` (in the title font), over the button
    // text style's own size at the UI scale.
    pub fn button_text(&self, button: UiTextButton, points: f32) -> UiTextButton {
        let points = points / skin::font_em(true) * self.size;
        let font = points / (style_number("/text_styles/button/size", 20.0) * skin::ui_scale());
        button.with_font_scale(UiFontScale(font.max(0.05)))
    }
}

// The realm theme's body text size at 100% (its style's `text_styles.body.size`).
fn body_text_size() -> f32 {
    style_number("/text_styles/body/size", 20.0)
}

fn style_number(pointer: &str, default: f32) -> f32 {
    skin::current().style.pointer(pointer).and_then(serde_json::Value::as_f64).map_or(default, |value| value as f32).max(0.01)
}

// A blank row `height` points tall.
pub fn spacer(height: f32) -> UiSeparator {
    UiSeparator::new().with_size(Vec2::new(1.0, height.max(0.0)))
}

// A group whose widgets keep to its left.
pub fn left_group() -> UiWidgetGroup {
    UiWidgetGroup::new().with_center_horizontally(false)
}

// A row holding `widget` `x` points (on the reference window) in from the composition's left.
pub fn row(scales: Scales, x: f32, widget: impl Into<UiWidgetImpl>) -> UiColumns {
    let mut columns = UiColumns::new().with_spacing(0.0);
    columns.add_column(UiColumnWidth::Fixed(scales.origin.x + scales.pt(x)), UiWidgetGroup::new());
    columns.add_column(UiColumnWidth::Fraction(1.0), left_group().with_widget(widget));
    columns
}

// A line of text, `width` points wide (0: its own), labeled for automation.
pub fn text_line(text: &str, label: &str, font: UiFontScale, color: Color, width: f32, align: UiTextAlign) -> UiRichText {
    let mut line = UiRichText::from_spans([UiTextSpan::plain(text)]).with_label(label).with_font_scale(font).with_align(align).with_text_color(color);
    if width > 0.0 {
        line = line.with_width(width);
    }
    line
}

// `line` with its row cut to TIGHT_LINE ems of its text, `points` large, where the realm font's own
// line is taller (Tampadesa's Amita draws its rows twice as tall as its letters): the mockup's
// rows. Its letters stay centred in the row.
pub fn tight(context: &GameUiContext, line: UiRichText, font: UiFontScale, points: f32, scales: Scales) -> UiRichText {
    let row = context.calc_text_size(font, "Ag").y;
    let spacing = (tight_row(points, scales) - row).min(0.0);
    if spacing < 0.0 { line.with_line_spacing(spacing) } else { line }
}

// The tallest row `tight` keeps for text `points` large, in window points.
pub fn tight_row(points: f32, scales: Scales) -> f32 {
    points * scales.size * TIGHT_LINE
}

// How wide `text` is at `font` in the theme's body font.
pub fn text_width(context: &GameUiContext, text: &str, font: UiFontScale) -> f32 {
    context.calc_text_size(font, text).x.ceil() + 2.0
}
