use strum::VariantArray;
use num_enum::TryFromPrimitive;

use super::*;
use crate::{
    implement_dialog_menu,
    render::TextureFilter,
    game::{GameLoop, config::GameConfigs},
};

// ----------------------------------------------
// GraphicsSettings
// ----------------------------------------------

pub struct GraphicsSettings {
    category: SettingsCategoryRcMut,
    menu: UiMenuRcMut,
}

implement_dialog_menu! { GraphicsSettings, "Graphics Settings" }

impl GraphicsSettings {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let mut category = SettingsCategory::new();

        let texture_filter_options: Vec<String> = TextureFilter::VARIANTS
            .iter()
            .map(|filter| filter.to_string())
            .collect();

        category
        .add_setting(SettingImpl::new(
            "Use Texture Mipmaps",
            SettingsWidgetKind::Checkbox,
            || {
                let texture_settings = GameLoop::get().engine().texture_cache().current_texture_settings();
                texture_settings.gen_mipmaps
            },
            |gen_mipmaps| {
                let tex_cache = GameLoop::get_mut().engine_mut().texture_cache();
                let mut texture_settings = tex_cache.current_texture_settings();
                texture_settings.gen_mipmaps = gen_mipmaps;
                tex_cache.change_texture_settings(texture_settings);
                GameConfigs::get_mut().engine.texture_settings = texture_settings;
            }
        ))
        .add_setting(SettingImpl::new(
            "Texture Filtering",
            SettingsWidgetKind::Dropdown(texture_filter_options),
            || {
                let texture_settings = GameLoop::get().engine().texture_cache().current_texture_settings();
                texture_settings.filter as usize
            },
            |selected_index: usize| {
                let tex_cache = GameLoop::get_mut().engine_mut().texture_cache();
                let mut texture_settings = tex_cache.current_texture_settings();
                texture_settings.filter = TextureFilter::try_from_primitive(selected_index as u32).unwrap();
                tex_cache.change_texture_settings(texture_settings);
                GameConfigs::get_mut().engine.texture_settings = texture_settings;
            }
        ));

        let menu = category.build_menu(category.downgrade(), context, Self::KIND, Self::TITLE);
        Self { menu, category }
    }
}
