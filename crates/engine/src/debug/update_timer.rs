use common::time::UpdateTimer;

use crate::ui::{DrawDebugUi, UiSystem};

// `UpdateTimer` lives in `common` (which is ImGui-free), so its debug-UI impl lives
// here in the engine crate, which owns the `DrawDebugUi` trait.
impl DrawDebugUi for UpdateTimer {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        self.draw_debug_ui_with_header("Update Timer", ui_sys);
    }

    // Renders the timer fields under a plain label (no collapsing header), matching
    // the previous `UpdateTimerDebugUi` presentation.
    fn draw_debug_ui_with_header(&mut self, header: &str, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        // Scope all widget ids to this timer instance so multiple timers shown in the
        // same window never collide, regardless of their header text.
        let _id = ui.push_id_ptr(&*self);

        ui.text(common::format_fixed_string_trunc!(128, "{}:", header));

        ui.input_float("Frequency (secs)", &mut self.update_frequency_secs)
            .display_format("%.2f")
            .step(0.5)
            .build();

        ui.input_float("Time since last", &mut self.time_since_last_secs())
            .display_format("%.2f")
            .read_only(true)
            .build();
    }
}
