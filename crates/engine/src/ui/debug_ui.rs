use crate::ui::UiSystem;

// ----------------------------------------------
// DrawDebugUi
// ----------------------------------------------

// Unified extension trait for ImGui debug-UI drawing.
//
// Implemented either by hand (engine-side impls live in `engine::debug`, game-side
// impls in `game::debug`) or automatically via `#[derive(DrawDebugUi)]`
// (see `proc_macros::DrawDebugUi`).
pub trait DrawDebugUi {
    // Draw the debug widgets for this value.
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem);

    // Draw the debug widgets nested under a collapsing header. Types that want a
    // different presentation (e.g. no collapsing header) can override this.
    fn draw_debug_ui_with_header(&mut self, header: &str, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if ui.collapsing_header(header, imgui::TreeNodeFlags::empty()) {
            self.draw_debug_ui(ui_sys);
        }
    }
}
