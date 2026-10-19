use common::time::UpdateTimer;

use crate::ui::{self, DrawDebugUi, UiNumberFormat, UiSystem};

// `UpdateTimer` lives in `common` (which has no UI), so its debug-UI impl lives
// here in the engine crate, which owns the `DrawDebugUi` trait.
impl DrawDebugUi for UpdateTimer {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        self.draw_debug_ui_with_header("Update Timer", ui_sys);
    }

    // Renders the timer fields under a plain label (no collapsing header), matching
    // the previous `UpdateTimerDebugUi` presentation.
    fn draw_debug_ui_with_header(&mut self, header: &str, ui_sys: &UiSystem) {
        // Scope all widget ids to this timer instance so multiple timers shown in the
        // same window never collide, regardless of their header text.
        let id = &*self as *const Self as usize;

        ui::id_scope(ui_sys, id, |ui_sys| {
            ui::text(ui_sys, format_args!("{}:", header));

            // Same for UI automation paths: "Spawn Patrol/Frequency (secs)".
            let _path = ui::path_scope(ui_sys, header);

            let decimals = UiNumberFormat::Decimals(2);
            ui::input_float(ui_sys, "Frequency (secs)", &mut self.update_frequency_secs, 0.5, decimals, false);
            ui::input_float(ui_sys, "Time since last", &mut self.time_since_last_secs(), 0.0, decimals, true);
        });
    }
}
