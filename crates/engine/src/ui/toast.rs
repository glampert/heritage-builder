// Toasts: short messages shown at the bottom of the screen for a few seconds, such as
// "Screenshot saved". They draw above every window and ignore the mouse. Show one with
// UiSystem::show_toast; UiSystem::set_toast_look gives them a panel and colours (the game's skin's
// caption panel), else they're a dark rounded box with white text.

use common::{Color, Rect, Vec2, time::Seconds};

use super::{self as ui, UiLayer, UiNineSlice, UiSystem, UiThemeId};

const TOAST_LIFETIME_SECS: Seconds = 4.0;
const TOAST_FADE_SECS: Seconds = 0.5; // Fades out over the end of its lifetime.
const MAX_TOASTS: usize = 4; // The oldest goes first when there are more.

const BOTTOM_MARGIN: f32 = 80.0;
const PADDING: Vec2 = Vec2::new(12.0, 6.0);
const SPACING: f32 = 6.0;

// How toasts look: a 9-slice panel behind the text (or a filled box of `background`), the text's
// insets on it in points (left, top, right, bottom), its colour, font scale and the theme whose
// font it's drawn in (the one current when they draw if None).
#[derive(Copy, Clone, Debug)]
pub struct UiToastLook {
    pub theme: Option<UiThemeId>,
    pub panel: Option<UiNineSlice>,
    pub background: Color,
    pub insets: [f32; 4],
    pub text_color: Color,
    pub font_scale: f32,
}

impl Default for UiToastLook {
    fn default() -> Self {
        Self {
            theme: None,
            panel: None,
            background: Color::new(0.08, 0.08, 0.1, 0.8),
            insets: [PADDING.x, PADDING.y, PADDING.x, PADDING.y],
            text_color: Color::white(),
            font_scale: 1.0,
        }
    }
}

struct UiToast {
    text: String,
    time_left: Seconds,
}

#[derive(Default)]
pub(super) struct UiToasts {
    toasts: Vec<UiToast>, // Oldest first.
    look: UiToastLook,
}

impl UiToasts {
    pub fn push(&mut self, text: String) {
        if self.toasts.len() == MAX_TOASTS {
            self.toasts.remove(0);
        }
        self.toasts.push(UiToast { text, time_left: TOAST_LIFETIME_SECS });
    }

    // Counts down every toast, and draws the live ones if `visible`. Newest at the bottom.
    pub fn update(&mut self, ui_sys: &UiSystem, delta_time_secs: Seconds, visible: bool) {
        self.toasts.retain_mut(|toast| {
            toast.time_left -= delta_time_secs;
            toast.time_left > 0.0
        });

        if !visible || self.toasts.is_empty() {
            return;
        }

        let previous_theme = ui_sys.swap_ui_theme(self.look.theme.unwrap_or(ui_sys.current_ui_theme()));

        let screen_size = ui::screen_size(ui_sys);
        let painter = ui::painter(ui_sys, UiLayer::Foreground);
        let mut bottom = screen_size.y - BOTTOM_MARGIN;
        let look = &self.look;
        let [left, top, right, bottom_inset] = look.insets;

        for toast in self.toasts.iter().rev() {
            let alpha = (toast.time_left / TOAST_FADE_SECS).min(1.0);
            let text_size = ui::with_font_scale(ui_sys, look.font_scale, |ui_sys| ui::calc_text_size(ui_sys, &toast.text));
            let size = Vec2::new(text_size.x + left + right, text_size.y + top + bottom_inset);

            let min = Vec2::new(((screen_size.x - size.x) * 0.5).max(0.0), bottom - size.y);
            let rect = Rect::from_extents(min, min + size);

            match &look.panel {
                Some(panel) => painter.nine_slice(rect, panel, Color::new(1.0, 1.0, 1.0, alpha)),
                None => painter.rect_filled(rect, faded(look.background, alpha), 6.0),
            }
            painter.text_scaled(min + Vec2::new(left, top), faded(look.text_color, alpha), look.font_scale, &toast.text);

            bottom = min.y - SPACING;
        }

        ui_sys.swap_ui_theme(previous_theme);
    }

    pub fn set_look(&mut self, look: UiToastLook) {
        self.look = look;
    }
}

fn faded(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha)
}

// A status line at the top center of the screen, over everything else, e.g. while automation
// drives the input.
pub(super) fn draw_badge(ui_sys: &UiSystem, text: &str) {
    let screen_size = ui::screen_size(ui_sys);
    let text_size = ui::calc_text_size(ui_sys, text);

    let min = Vec2::new(((screen_size.x - text_size.x) * 0.5 - PADDING.x).max(0.0), BADGE_TOP_MARGIN);
    let max = min + text_size + PADDING * 2.0;

    let painter = ui::painter(ui_sys, UiLayer::Foreground);
    painter.rect_filled(Rect::from_extents(min, max), Color::new(0.6, 0.1, 0.1, 0.85), 6.0);
    painter.text(min + PADDING, Color::white(), text);
}

const BADGE_TOP_MARGIN: f32 = 60.0; // Below the HUD's top bar.
