use arrayvec::ArrayVec;
use common::hash::{self, NULL_HASH, PreHashedKeyMap, StringHash};
use serde::{Deserialize, Serialize};
use strum::EnumCount;

use crate::{config::Configs, configurations, log, ui::UiSystem};

// ----------------------------------------------
// Public API
// ----------------------------------------------

pub type UiTextKey = StringHash;

#[repr(usize)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, EnumCount)]
pub enum UiTextCategory {
    TileDescription,
    UnitDialog,
}

pub fn initialize() {
    UiTextStoreSingleton::load();
}

pub fn find_str(category: UiTextCategory, key: UiTextKey) -> Option<&'static str> {
    UiTextStoreSingleton::get().find_str(category, key)
}

pub fn draw_debug_ui(ui_sys: &UiSystem) {
    UiTextStoreSingleton::get().draw_debug_ui(ui_sys);
}

// ----------------------------------------------
// UiKeyedString
// ----------------------------------------------

#[derive(Default, Serialize, Deserialize)]
struct UiKeyedString {
    #[serde(skip)]
    key_hash: UiTextKey,
    key: String,
    text: String,
}

// ----------------------------------------------
// UiTextCategoryEntry
// ----------------------------------------------

const UI_TEXT_CATEGORY_COUNT: usize = UiTextCategory::COUNT;

#[derive(Serialize, Deserialize)]
struct UiTextCategoryEntry {
    category: UiTextCategory,
    strings: Vec<UiKeyedString>,

    // Runtime lookup, patched on post_load.
    #[serde(skip)]
    mapping: PreHashedKeyMap<UiTextKey, usize>, // UiTextKey => strings[index]
}

impl UiTextCategoryEntry {
    fn post_load(&mut self, category_index: usize) {
        if category_index != self.category as usize {
            panic!("Category index mismatch! Make sure file order of declaration matches the UiTextCategory enum.");
        }

        debug_assert!(self.mapping.is_empty());

        for (index, string) in self.strings.iter_mut().enumerate() {
            debug_assert!(string.key_hash == NULL_HASH);

            if string.key.is_empty() {
                log::error!(
                    log::channel!("ui"),
                    "{:?}: Empty text entry key at [{}]! A key string must be provided.",
                    self.category,
                    index
                );
                continue;
            }

            string.key_hash = hash::fnv1a_from_str(&string.key);
            debug_assert!(string.key_hash != NULL_HASH);

            if self.mapping.insert(string.key_hash, index).is_some() {
                log::error!(
                    log::channel!("ui"),
                    "{:?}: Duplicate text entry key at [{}]: '{}'",
                    self.category,
                    index,
                    string.key
                );
            }
        }
    }
}

// ----------------------------------------------
// UiTextStoreSingleton
// ----------------------------------------------

// Localized UI strings global store.
#[derive(Default, Serialize, Deserialize)]
struct UiTextStoreSingleton {
    categories: ArrayVec<UiTextCategoryEntry, UI_TEXT_CATEGORY_COUNT>,
}

impl UiTextStoreSingleton {
    fn find_str(&'static self, category: UiTextCategory, key: UiTextKey) -> Option<&'static str> {
        if key == NULL_HASH {
            return None;
        }

        let category_entry = &self.categories[category as usize];
        debug_assert!(
            category_entry.category == category,
            "Category index mismatch! Make sure file order of declaration matches the UiTextCategory enum."
        );

        let string_index = category_entry.mapping.get(&key)?;
        let string = &category_entry.strings[*string_index];

        if string.text.is_empty() {
            return None;
        }

        debug_assert!(string.key_hash == key, "Text entry hash mismatch!");
        Some(&string.text)
    }

    fn post_load(&'static mut self) {
        for (index, entry) in self.categories.iter_mut().enumerate() {
            entry.post_load(index);
        }
    }

    fn draw_debug_ui_with_header(&'static self, _header: &str, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        if ui.collapsing_header("Categories", imgui::TreeNodeFlags::empty()) {
            ui.indent_by(10.0);
            for entry in &self.categories {
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

// Global instance:
configurations! { UI_TEXT_STORE_SINGLETON, UiTextStoreSingleton, "ui/text" }
