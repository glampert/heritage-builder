// Toasts: short messages shown at the bottom of the screen for a few seconds, such as
// "Screenshot saved". They draw above every window and ignore the mouse. Show one with
// UiSystem::show_toast.

use common::time::Seconds;

const TOAST_LIFETIME_SECS: Seconds = 4.0;
const TOAST_FADE_SECS: Seconds = 0.5; // Fades out over the end of its lifetime.
const MAX_TOASTS: usize = 4; // The oldest goes first when there are more.

const BOTTOM_MARGIN: f32 = 80.0;
const PADDING: [f32; 2] = [12.0, 6.0];
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
    pub fn update(&mut self, ui: &imgui::Ui, delta_time_secs: Seconds, visible: bool) {
        self.toasts.retain_mut(|toast| {
            toast.time_left -= delta_time_secs;
            toast.time_left > 0.0
        });

        if !visible || self.toasts.is_empty() {
            return;
        }

        let [screen_width, screen_height] = ui.io().display_size;
        let draw_list = ui.get_foreground_draw_list();
        let mut bottom = screen_height - BOTTOM_MARGIN;

        for toast in self.toasts.iter().rev() {
            let alpha = (toast.time_left / TOAST_FADE_SECS).min(1.0);
            let [text_width, text_height] = ui.calc_text_size(&toast.text);

            let min = [((screen_width - text_width) * 0.5 - PADDING[0]).max(0.0), bottom - text_height - PADDING[1] * 2.0];
            let max = [min[0] + text_width + PADDING[0] * 2.0, bottom];

            draw_list
                .add_rect(min, max, imgui::ImColor32::from_rgba_f32s(0.08, 0.08, 0.1, 0.8 * alpha))
                .filled(true)
                .rounding(6.0)
                .build();

            draw_list.add_text(
                [min[0] + PADDING[0], min[1] + PADDING[1]],
                imgui::ImColor32::from_rgba_f32s(1.0, 1.0, 1.0, alpha),
                &toast.text,
            );

            bottom = min[1] - SPACING;
        }
    }
}
