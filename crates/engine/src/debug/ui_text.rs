use crate::ui::{DrawDebugUi, UiSystem, text::UiTextStoreSingleton};

// Lists every localized UI-text category and its keyed strings.
impl DrawDebugUi for UiTextStoreSingleton {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        if ui.collapsing_header("Categories", imgui::TreeNodeFlags::empty()) {
            ui.indent_by(10.0);
            for entry in self.categories() {
                if ui.collapsing_header(format!("{:?}", entry.category), imgui::TreeNodeFlags::empty()) {
                    ui.indent_by(10.0);
                    for (index, string) in entry.strings.iter().enumerate() {
                        ui.text(format!("[{}]: key:'{}', text:'{}'", index, string.key, string.text));
                    }
                    ui.unindent_by(10.0);
                }
            }
            ui.unindent_by(10.0);
        }
    }
}
