use num_enum::TryFromPrimitive;
use strum::VariantNames;
use common::format_small;

use crate::{
    render::texture::{TextureCache, TextureFilter, TextureSettings, TextureWrapMode},
    ui::{self, DrawDebugUi, UiSystem},
};

// Lists all loaded textures and exposes the global filtering settings.
impl DrawDebugUi for TextureCache {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let Some(tab) = ui::tab_bar(ui_sys, "Texture Cache Tab Bar", &["Filtering", "Loaded Textures"]) else {
            return;
        };

        match tab.index {
            0 => draw_filtering(self, ui_sys),
            _ => draw_loaded_textures(self, ui_sys),
        }
    }
}

fn draw_filtering(cache: &mut TextureCache, ui_sys: &UiSystem) {
    let current_settings = cache.current_texture_settings();
    let mut settings_changed = false;

    let mut current_filter_index = current_settings.filter as usize;
    settings_changed |= ui::combo(ui_sys, "Filter", &mut current_filter_index, TextureFilter::VARIANTS);

    let mut current_wrap_mode_index = current_settings.wrap_mode as usize;
    settings_changed |= ui::combo(ui_sys, "Wrap Mode", &mut current_wrap_mode_index, TextureWrapMode::VARIANTS);

    let mut current_gen_mipmaps = current_settings.mipmaps;
    settings_changed |= ui::checkbox(ui_sys, "Mipmaps", &mut current_gen_mipmaps);

    if settings_changed {
        let new_settings = TextureSettings {
            filter: TextureFilter::try_from_primitive(current_filter_index as u32).unwrap(),
            wrap_mode: TextureWrapMode::try_from_primitive(current_wrap_mode_index as u32).unwrap(),
            mipmaps: current_gen_mipmaps,
        };
        cache.change_texture_settings(new_settings);
    }
}

fn draw_loaded_textures(cache: &TextureCache, ui_sys: &UiSystem) {
    let bool_str = |val: bool| {
        if val { "yes" } else { "no" }
    };

    ui::text(ui_sys, format_small!("Loaded Count: {}", cache.loaded_textures_count()));
    ui::separator(ui_sys);

    ui::grid(ui_sys, "texture_columns", 7, |grid| {
        // Header row:
        for header in ["Index", "Name", "Size", "Change Settings", "Mipmaps", "Filter", "Wrap Mode"] {
            grid.text(header);
        }

        grid.separator();

        for info in cache.loaded_textures_debug_info() {
            grid.text(format_small!("{}", info.index));
            grid.text(info.name);
            grid.text(format_small!("{}x{}", info.size.width, info.size.height));
            grid.text(bool_str(info.allow_settings_change));
            grid.text(bool_str(info.has_mipmaps));
            grid.text(format_small!("{}", info.filter));
            grid.text(format_small!("{}", info.wrap_mode));
        }
    });
}
