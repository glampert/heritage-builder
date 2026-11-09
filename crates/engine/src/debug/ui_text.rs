use strum::IntoEnumIterator;

use crate::ui::{
    self, DrawDebugUi, UiSystem,
    text::{self, UiTextCategory, UiTextDebug},
};

// The localized strings: the current language (switchable), each language's categories with their
// keys and text, and the keys looked up and not found. Header labels stay fixed, since a header's
// label is its egui id and its automation path: counts go inside.
impl DrawDebugUi for UiTextDebug {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let store = self.store();
        let languages = text::languages();

        let mut current = languages.iter().position(|language| *language == text::current_language()).unwrap_or(0);
        if ui::combo(ui_sys, "Language", &mut current, &languages)
            && let Some(language) = languages.get(current)
        {
            let _ = text::set_language(language);
        }

        if let Some(_path) = ui::collapsing_header_scope(ui_sys, "Missing Keys", false) {
            ui::indent(ui_sys, 10.0, |ui_sys| {
                let missing = text::missing_keys();
                let missing_count: usize = missing.values().map(Vec::len).sum();
                ui::text(ui_sys, format_args!("Looked up and not in {}.json: {missing_count}", text::ENGLISH));
                for (category, keys) in &missing {
                    for key in keys {
                        ui::text(ui_sys, format_args!("{category}/{key}"));
                    }
                }
                if ui::button(ui_sys, "Reset") {
                    text::reset_misses();
                }
            });
        }

        for (language_index, language) in store.languages.iter().enumerate() {
            let Some(_path) = ui::collapsing_header_scope(ui_sys, &common::format_small!("Language '{}'", language.name), false)
            else {
                continue;
            };

            // Each language's category headers keep their own open state: sibling indents share
            // an egui id salt, so give each language its own scope.
            ui::id_scope(ui_sys, language_index, |ui_sys| ui::indent(ui_sys, 10.0, |ui_sys| {
                for category in UiTextCategory::iter() {
                    let strings = language.strings(category);
                    if let Some(_path) = ui::collapsing_header_scope(ui_sys, category.into(), false) {
                        ui::indent(ui_sys, 10.0, |ui_sys| {
                            ui::text(ui_sys, format_args!("Keys: {}", strings.entries.len()));
                            for (key, entry) in &strings.entries {
                                for (form, text) in entry.forms() {
                                    if form.is_empty() {
                                        ui::text(ui_sys, format_args!("{key}: '{text}'"));
                                    } else {
                                        ui::text(ui_sys, format_args!("{key} [{form}]: '{text}'"));
                                    }
                                }
                            }
                        });
                    }
                }
            }));
        }
    }
}
