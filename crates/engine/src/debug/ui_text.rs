use crate::ui::{DrawDebugUi, UiSystem, dev, text::UiTextStoreSingleton};

// Lists every localized UI-text category and its keyed strings.
impl DrawDebugUi for UiTextStoreSingleton {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        if let Some(_path) = dev::collapsing_header_scope(ui_sys, "Categories", imgui::TreeNodeFlags::empty()) {
            ui.indent_by(10.0);
            for entry in self.categories() {
                let category = common::format_small!("{:?}", entry.category);
                if let Some(_path) = dev::collapsing_header_scope(ui_sys, &category, imgui::TreeNodeFlags::empty()) {
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
