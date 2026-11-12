// The Graphics tab: the window mode, resolution, renderer, vertical sync and frame rate limit,
// which the engine reads only when it starts ("after a restart"), then texture filtering and
// mipmaps, applied at once, and animated water.
//
// TODO(realm-ui): on the web, the window mode, resolution and renderer don't apply (the page
// sizes the canvas, and it always runs Wgpu): show those rows disabled there.

use super::*;

const WINDOW_MODES: [ApplicationWindowMode; 3] =
    [ApplicationWindowMode::Windowed, ApplicationWindowMode::FullScreen, ApplicationWindowMode::ExclusiveFullScreen];

// The window sizes offered, besides the configs' own and their default (resolutions()).
const RESOLUTIONS: [(i32, i32); 7] = [(1280, 720), (1366, 768), (1440, 800), (1600, 900), (1920, 1080), (2560, 1440), (3840, 2160)];

// The frame rate limits offered (0: none).
const FRAME_RATE_LIMITS: [u32; 5] = [0, 30, 60, 120, 144];

fn window_mode_items() -> Vec<String> {
    WINDOW_MODES
        .iter()
        .map(|mode| match mode {
            ApplicationWindowMode::Windowed => ui::text::get(UiTextCategory::Menu, "settings.window_mode.windowed"),
            ApplicationWindowMode::FullScreen => ui::text::get(UiTextCategory::Menu, "settings.window_mode.fullscreen"),
            ApplicationWindowMode::ExclusiveFullScreen => ui::text::get(UiTextCategory::Menu, "settings.window_mode.exclusive"),
        })
        .map(str::to_string)
        .collect()
}

// The window sizes offered: the common ones, the size the configs have and their default, smallest
// first.
fn resolutions() -> Vec<Size> {
    let mut sizes: Vec<Size> = RESOLUTIONS.iter().map(|&(width, height)| Size::new(width, height)).collect();
    for size in [GameConfigs::get().engine.window_size, GameConfigs::defaults().engine.window_size] {
        if size.is_valid() && !sizes.contains(&size) {
            sizes.push(size);
        }
    }
    sizes.sort_by_key(|size| (size.width, size.height));
    sizes
}

fn resolution_items(sizes: &[Size]) -> Vec<String> {
    sizes
        .iter()
        .map(|size| {
            ui::text::format(UiTextCategory::Menu, "settings.resolution.size", &[("width", &size.width.to_string()), ("height", &size.height.to_string())])
        })
        .collect()
}

// The renderers this build has.
fn renderers() -> Vec<RenderApi> {
    vec![
        RenderApi::Wgpu,
        #[cfg(feature = "desktop")]
        RenderApi::OpenGl,
    ]
}

// The renderer the configs will start with: Wgpu runs on Winit only, and the runner falls back to
// OpenGl on Glfw (runner/desktop.rs).
fn runs_on(configs: &GameConfigs) -> RenderApi {
    #[cfg(feature = "desktop")]
    if configs.engine.app_api == ApplicationApi::Glfw {
        return RenderApi::OpenGl;
    }
    configs.engine.render_api
}

// Sets the renderer and the app backend it runs on: Winit for Wgpu, the default backend for OpenGl
// (Glfw, unless the default configs say otherwise).
fn set_renderer(configs: &mut GameConfigs, api: RenderApi) {
    configs.engine.render_api = api;
    configs.engine.app_api = match api {
        RenderApi::Wgpu => ApplicationApi::Winit,
        #[cfg(feature = "desktop")]
        RenderApi::OpenGl => GameConfigs::defaults().engine.app_api,
    };
}

fn renderer_items() -> Vec<String> {
    renderers()
        .into_iter()
        .map(|api| match api {
            RenderApi::Wgpu => ui::text::get(UiTextCategory::Menu, "settings.renderer.wgpu").to_string(),
            #[cfg(feature = "desktop")]
            RenderApi::OpenGl => ui::text::get(UiTextCategory::Menu, "settings.renderer.opengl").to_string(),
        })
        .collect()
}

fn frame_rate_limit_items() -> Vec<String> {
    FRAME_RATE_LIMITS
        .iter()
        .map(|&limit| match limit {
            0 => ui::text::get(UiTextCategory::Menu, "settings.frame_rate.unlimited").to_string(),
            _ => ui::text::format(UiTextCategory::Menu, "settings.frame_rate.fps", &[("fps", &limit.to_string())]),
        })
        .collect()
}

fn texture_filter_items() -> Vec<String> {
    TextureFilter::VARIANTS
        .iter()
        .map(|filter| match filter {
            TextureFilter::Nearest => ui::text::get(UiTextCategory::Menu, "settings.filter.nearest"),
            TextureFilter::Linear => ui::text::get(UiTextCategory::Menu, "settings.filter.linear"),
            TextureFilter::NearestMipmapNearest => ui::text::get(UiTextCategory::Menu, "settings.filter.nearest_mipmap_nearest"),
            TextureFilter::LinearMipmapNearest => ui::text::get(UiTextCategory::Menu, "settings.filter.linear_mipmap_nearest"),
            TextureFilter::NearestMipmapLinear => ui::text::get(UiTextCategory::Menu, "settings.filter.nearest_mipmap_linear"),
            TextureFilter::LinearMipmapLinear => ui::text::get(UiTextCategory::Menu, "settings.filter.linear_mipmap_linear"),
        })
        .map(str::to_string)
        .collect()
}

pub(super) fn settings() -> Vec<SettingDef> {
    let tab = SettingsTab::Graphics;
    let def = |label: UiString, control: SettingControl, restart: bool, read: fn(&GameConfigs) -> SettingValue, write: fn(&mut GameConfigs, SettingValue)| {
        SettingDef { tab, label, control, restart, read, write }
    };
    let toggle = || SettingControl::Toggle;

    vec![
        def(
            UiString::key(UiTextCategory::Menu, "settings.window_mode"),
            SettingControl::Dropdown { items: window_mode_items() },
            true,
            |configs| SettingValue::Choice(WINDOW_MODES.iter().position(|mode| *mode == configs.engine.window_mode).unwrap_or(0)),
            |configs, value| configs.engine.window_mode = WINDOW_MODES.get(value.as_choice()).copied().unwrap_or(ApplicationWindowMode::Windowed),
        ),
        def(
            UiString::key(UiTextCategory::Menu, "settings.resolution"),
            SettingControl::Dropdown { items: resolution_items(&resolutions()) },
            true,
            |configs| SettingValue::Choice(resolutions().iter().position(|size| *size == configs.engine.window_size).unwrap_or(0)),
            |configs, value| {
                if let Some(size) = resolutions().get(value.as_choice()) {
                    configs.engine.window_size = *size;
                }
            },
        ),
        def(
            UiString::key(UiTextCategory::Menu, "settings.renderer"),
            SettingControl::Dropdown { items: renderer_items() },
            true,
            |configs| SettingValue::Choice(renderers().iter().position(|api| *api == runs_on(configs)).unwrap_or(0)),
            |configs, value| {
                if let Some(api) = renderers().get(value.as_choice()) {
                    set_renderer(configs, *api);
                }
            },
        ),
        // Stored only, not read even at a restart (EngineConfigs::vsync's TODO(realm-ui)), so
        // their labels don't say "(after a restart)": that waits for the backends to read them.
        def(
            UiString::key(UiTextCategory::Menu, "settings.vsync"),
            toggle(),
            false,
            |configs| SettingValue::Bool(configs.engine.vsync),
            |configs, value| configs.engine.vsync = value.as_bool(),
        ),
        def(
            UiString::key(UiTextCategory::Menu, "settings.frame_rate_limit"),
            SettingControl::Dropdown { items: frame_rate_limit_items() },
            false,
            |configs| SettingValue::Choice(nearest_choice(&FRAME_RATE_LIMITS, configs.engine.frame_rate_limit)),
            |configs, value| configs.engine.frame_rate_limit = FRAME_RATE_LIMITS.get(value.as_choice()).copied().unwrap_or(0),
        ),
        def(
            UiString::key(UiTextCategory::Menu, "settings.texture_filtering"),
            SettingControl::Dropdown { items: texture_filter_items() },
            false,
            |configs| SettingValue::Choice(configs.engine.texture_settings.filter as usize),
            |configs, value| {
                if let Some(filter) = TextureFilter::VARIANTS.get(value.as_choice()) {
                    configs.engine.texture_settings.filter = *filter;
                }
            },
        ),
        def(
            UiString::key(UiTextCategory::Menu, "settings.mipmaps"),
            toggle(),
            false,
            |configs| SettingValue::Bool(configs.engine.texture_settings.mipmaps),
            |configs, value| configs.engine.texture_settings.mipmaps = value.as_bool(),
        ),
        // Stored only: see DisplayConfigs::animated_water's TODO(realm-ui).
        def(
            UiString::key(UiTextCategory::Menu, "settings.animated_water"),
            toggle(),
            false,
            |configs| SettingValue::Bool(configs.display.animated_water),
            |configs, value| configs.display.animated_water = value.as_bool(),
        ),
    ]
}
