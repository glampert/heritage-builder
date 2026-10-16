use common::time::UpdateTimer;

use crate::ui::{DrawDebugUi, UiSystem, dev};

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

        // Same for UI automation paths: "Spawn Patrol/Frequency (secs)".
        let _path = dev::path_scope(ui_sys, header);

        dev::input_float(ui_sys, "Frequency (secs)", &mut self.update_frequency_secs, 0.5, Some("%.2f"), false);
        dev::input_float(ui_sys, "Time since last", &mut self.time_since_last_secs(), 0.0, Some("%.2f"), true);
    }
}
