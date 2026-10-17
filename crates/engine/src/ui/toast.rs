// Toasts: short messages shown at the bottom of the screen for a few seconds, such as
// "Screenshot saved". They draw above every window and ignore the mouse. Show one with
// UiSystem::show_toast.

use common::{Color, Rect, Vec2, time::Seconds};

use super::{self as ui, UiLayer, UiSystem};

const TOAST_LIFETIME_SECS: Seconds = 4.0;
const TOAST_FADE_SECS: Seconds = 0.5; // Fades out over the end of its lifetime.
const MAX_TOASTS: usize = 4; // The oldest goes first when there are more.

const BOTTOM_MARGIN: f32 = 80.0;
const PADDING: Vec2 = Vec2::new(12.0, 6.0);
const SPACING: f32 = 6.0;

struct UiToast {
    text: String,
    time_left: Seconds,
}

#[derive(Default)]
pub(super) struct UiToasts {
    toasts: Vec<UiToast>, // Oldest first.
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

        let screen_size = ui::screen_size(ui_sys);
        let painter = ui::painter(ui_sys, UiLayer::Foreground);
        let mut bottom = screen_size.y - BOTTOM_MARGIN;

        for toast in self.toasts.iter().rev() {
            let alpha = (toast.time_left / TOAST_FADE_SECS).min(1.0);
            let text_size = ui::calc_text_size(ui_sys, &toast.text);

            let min = Vec2::new(((screen_size.x - text_size.x) * 0.5 - PADDING.x).max(0.0), bottom - text_size.y - PADDING.y * 2.0);
            let max = Vec2::new(min.x + text_size.x + PADDING.x * 2.0, bottom);

            painter.rect_filled(Rect::from_extents(min, max), Color::new(0.08, 0.08, 0.1, 0.8 * alpha), 6.0);
            painter.text(min + PADDING, Color::new(1.0, 1.0, 1.0, alpha), &toast.text);

            bottom = min.y - SPACING;
        }
    }
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
