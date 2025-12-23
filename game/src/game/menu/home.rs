use std::any::Any;

use super::{
    widgets,
    GameMenuMode,
    GameMenusSystem,
    GameMenusContext,
    GameMenusInputArgs,
    TilePalette,
    TilePlacement,
    TileInspector,
    modal::{BasicModalMenu, ModalMenuParams},
};
use crate::{
    save::{Save, Load},
    utils::{Size, Vec2, coords::CellRange},
    tile::rendering::TileMapRenderFlags,
    imgui_ui::{UiSystem, UiInputEvent},
    render::{TextureHandle, TextureCache, TextureSettings, TextureFilter},
};

// ----------------------------------------------
// HomeMenus
// ----------------------------------------------

pub struct HomeMenus {
    background: FullScreenBackground,
    main_menu: BasicModalMenu,
}

impl HomeMenus {
    pub fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem, viewport_size: Size) -> Self {
        Self {
            background: FullScreenBackground::load(tex_cache),
            main_menu: BasicModalMenu::new(
                tex_cache,
                ui_sys,
                ModalMenuParams {
                    start_open: true,
                    position: Some(Vec2::new(50.0, 50.0)),
                    size: Some(Size::new(550, viewport_size.height - 100)),
                    background_sprite: Some("misc/scroll_bg.png"),
                    ..Default::default()
                }
            ),
        }
    }
}

impl GameMenusSystem for HomeMenus {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn mode(&self) -> GameMenuMode {
        GameMenuMode::Home
    }

    fn tile_placement(&mut self) -> Option<&mut TilePlacement> {
        None
    }

    fn tile_palette(&mut self) -> Option<&mut dyn TilePalette> {
        None
    }

    fn tile_inspector(&mut self) -> Option<&mut dyn TileInspector> {
        None
    }

    fn selected_render_flags(&self) -> TileMapRenderFlags {
        TileMapRenderFlags::empty()
    }

    fn begin_frame(&mut self, _context: &mut GameMenusContext) {
    }

    fn end_frame(&mut self, context: &mut GameMenusContext, _visible_range: CellRange) {
        let tex_cache = context.engine.texture_cache();
        let ui_sys = context.engine.ui_system();

        self.main_menu.draw(context.sim, ui_sys, |_sim| {
            let ui = ui_sys.ui();
            ui.set_window_font_scale(1.5);

            let _pressed_button_index = widgets::draw_centered_button_group(
                ui,
                &ui.get_window_draw_list(),
                &["New Game", "Continue", "Load Game", "Custom Game", "Settings", "About", "Exit Game"],
                Some(Size::new(200, 60))
            );
        });

        self.background.draw(tex_cache, ui_sys);
    }

    fn handle_input(&mut self, _context: &mut GameMenusContext, _args: GameMenusInputArgs) -> UiInputEvent {
        UiInputEvent::NotHandled
    }
}

// ----------------------------------------------
// Save/Load for HomeMenus
// ----------------------------------------------

impl Save for HomeMenus {}
impl Load for HomeMenus {}

// ----------------------------------------------
// FullScreenBackground
// ----------------------------------------------

struct FullScreenBackground {
    tex_handle: TextureHandle,
}

impl FullScreenBackground {
    fn load(tex_cache: &mut dyn TextureCache) -> Self {
        let settings = TextureSettings {
            filter: TextureFilter::Linear,
            gen_mipmaps: false,
            ..Default::default()
        };

        let bg_file_path =
            super::ui_assets_path()
            .join("misc/home_menu_bg.png");

        Self {
            tex_handle: tex_cache.load_texture_with_settings(bg_file_path.to_str().unwrap(), Some(settings))
        }
    }

    fn draw(&self, tex_cache: &dyn TextureCache, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        let draw_list = ui.get_background_draw_list();

        // Draw full-screen rectangle with the background image:
        let ui_texture = ui_sys.to_ui_texture(tex_cache, self.tex_handle);
        draw_list.add_image(ui_texture, [0.0, 0.0], ui.io().display_size).build();
    }
}
