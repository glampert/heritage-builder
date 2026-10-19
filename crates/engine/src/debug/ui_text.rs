use crate::ui::{self, DrawDebugUi, UiSystem, text::UiTextStoreSingleton};

// Lists every localized UI-text category and its keyed strings.
impl DrawDebugUi for UiTextStoreSingleton {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let Some(_path) = ui::collapsing_header_scope(ui_sys, "Categories", false) else {
            return;
        };

        ui::indent(ui_sys, 10.0, |ui_sys| {
            for entry in self.categories() {
                let category = common::format_small!("{:?}", entry.category);
                if let Some(_path) = ui::collapsing_header_scope(ui_sys, &category, false) {
                    ui::indent(ui_sys, 10.0, |ui_sys| {
                        for (index, string) in entry.strings.iter().enumerate() {
                            ui::text(ui_sys, format_args!("[{}]: key:'{}', text:'{}'", index, string.key, string.text));
                        }
                    });
                }
            }
        });
    }
}
