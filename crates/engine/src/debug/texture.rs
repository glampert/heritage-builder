use common::format_fixed_string;
use num_enum::TryFromPrimitive;
use strum::VariantArray;

use crate::{
    render::texture::{TextureCache, TextureFilter, TextureSettings, TextureWrapMode},
    ui::{DrawDebugUi, UiSystem},
};

// Lists all loaded textures and exposes the global filtering settings.
impl DrawDebugUi for TextureCache {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        if let Some(_tab_bar) = ui.tab_bar("Texture Cache Tab Bar") {
            if let Some(_tab) = ui.tab_item("Filtering") {
                draw_filtering(self, ui);
            }

            if let Some(_tab) = ui.tab_item("Loaded Textures") {
                draw_loaded_textures(self, ui);
            }
        }
    }
}

fn draw_filtering(cache: &mut TextureCache, ui: &imgui::Ui) {
    let current_settings = cache.current_texture_settings();
    let mut settings_changed = false;

    let mut current_filter_index = current_settings.filter as usize;
    settings_changed |=
        ui.combo("Filter", &mut current_filter_index, TextureFilter::VARIANTS, |filter| filter.to_string().into());

    let mut current_wrap_mode_index = current_settings.wrap_mode as usize;
    settings_changed |=
        ui.combo("Wrap Mode", &mut current_wrap_mode_index, TextureWrapMode::VARIANTS, |mode| mode.to_string().into());

    let mut current_gen_mipmaps = current_settings.mipmaps;
    settings_changed |= ui.checkbox("Mipmaps", &mut current_gen_mipmaps);

    if settings_changed {
        let new_settings = TextureSettings {
            filter: TextureFilter::try_from_primitive(current_filter_index as u32).unwrap(),
            wrap_mode: TextureWrapMode::try_from_primitive(current_wrap_mode_index as u32).unwrap(),
            mipmaps: current_gen_mipmaps,
        };
        cache.change_texture_settings(new_settings);
    }
}

fn draw_loaded_textures(cache: &TextureCache, ui: &imgui::Ui) {
    let table_col = |label: &str| {
        ui.text(label);
        ui.next_column();
    };

    let bool_str = |val: bool| {
        if val { "yes" } else { "no" }
    };

    ui.text(format_fixed_string!(64, "Loaded Count: {}", cache.loaded_textures_count()));
    ui.separator();

    // Set number of rows (emulated with columns):
    ui.columns(7, "texture_columns", true);

    // Header row:
    table_col("Index");
    table_col("Name");
    table_col("Size");
    table_col("Change Settings");
    table_col("Mipmaps");
    table_col("Filter");
    table_col("Wrap Mode");

    ui.separator();

    for info in cache.loaded_textures_debug_info() {
        table_col(&format_fixed_string!(64, "{}", info.index));
        table_col(info.name);
        table_col(&format_fixed_string!(64, "{}x{}", info.size.width, info.size.height));
        table_col(bool_str(info.allow_settings_change));
        table_col(bool_str(info.has_mipmaps));
        table_col(&format_fixed_string!(64, "{}", info.filter));
        table_col(&format_fixed_string!(64, "{}", info.wrap_mode));
    }

    // Return to single column.
    ui.columns(1, "", false);
}
