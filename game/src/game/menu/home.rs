use std::any::Any;
use arrayvec::ArrayVec;
use num_enum::TryFromPrimitive;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    GameMenusMode,
    GameMenusSystem,
    GameMenusContext,
    GameMenusInputArgs,
    TilePalette,
    TilePlacement,
    TileInspector,
    modal::*,
    widgets,
};
use crate::{
    game::GameLoop,
    engine::time::Seconds,
    save::{Save, Load},
    tile::rendering::TileMapRenderFlags,
    utils::{Size, Vec2, coords::CellRange},
    ui::{self, UiInputEvent, UiTextureHandle, UiTheme, widgets::UiWidgetContext},
};

// ----------------------------------------------
// HomeMenus
// ----------------------------------------------

type Background = StaticFullScreenBackground;

pub struct HomeMenus {
    main_menu: HomeMainMenu,
    background: Background,
}

impl HomeMenus {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        context.ui_sys.set_ui_theme(UiTheme::InGame);
        Self {
            main_menu: HomeMainMenu::new(context),
            background: Background::new(context),
        }
    }
}

impl GameMenusSystem for HomeMenus {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn mode(&self) -> GameMenusMode {
        GameMenusMode::Home
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
        let mut widget_context =
            UiWidgetContext::new(context.sim, context.world, context.engine);

        if self.background.anim_scene_completed() {
            self.main_menu.draw(&mut widget_context);
        }

        self.background.draw(&mut widget_context);
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
// HomeMainMenu
// ----------------------------------------------

const MAIN_MENU_BUTTON_COUNT: usize = HomeMainMenuButton::COUNT;

#[repr(usize)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, TryFromPrimitive, EnumCount, EnumProperty, EnumIter)]
enum HomeMainMenuButton {
    #[strum(props(Label = "New Game"))]
    NewGame,

    #[strum(props(Label = "Continue"))]
    Continue,

    #[strum(props(Label = "Load Game"))]
    LoadGame,

    #[strum(props(Label = "Custom Game"))]
    CustomGame,

    #[strum(props(Label = "Settings"))]
    Settings,

    #[strum(props(Label = "About"))]
    About,

    #[strum(props(Label = "Exit Game"))]
    Exit,
}

impl HomeMainMenuButton {
    fn label(self) -> &'static str {
        self.get_str("Label").unwrap()
    }

    fn labels() -> ArrayVec<&'static str, MAIN_MENU_BUTTON_COUNT> {
        let mut labels = ArrayVec::new();
        for button in HomeMainMenuButton::iter() {
            labels.push(button.label());
        }
        labels
    }
}

pub struct HomeMainMenu {
    menu: BasicModalMenu,
    separator_ui_texture: UiTextureHandle,
    child_menus: ArrayVec<Option<Box<dyn ModalMenu>>, MAIN_MENU_BUTTON_COUNT>,
}

impl HomeMainMenu {
    pub const FONT_SCALE_CHILD_MENU: f32 = 1.2; // Child menu default
    pub const FONT_SCALE_HOME_BTN: f32 = 1.5;   // Home buttons
    pub const FONT_SCALE_HEADING: f32 = 1.8;    // Headings

    pub const BG_SPRITE: &str = "misc/scroll_bg.png";
    pub const SEPARATOR_SPRITE: &str = "misc/brush_stroke_divider.png";

    pub const POSITION: Vec2 = Vec2::new(50.0, 50.0);
    pub fn calc_size(context: &UiWidgetContext) -> Size {
        Size::new(550, context.viewport_size.height - 100)
    }

    fn new(context: &mut UiWidgetContext) -> Self {
        let separator_tex_handle = context.tex_cache.load_texture_with_settings(
            ui::assets_path().join(Self::SEPARATOR_SPRITE).to_str().unwrap(),
            Some(ui::texture_settings())
        );
        Self {
            menu: BasicModalMenu::new(
                context,
                ModalMenuParams {
                    start_open: true,
                    position: Some(Self::POSITION),
                    size: Some(Self::calc_size(context)),
                    background_sprite: Some(Self::BG_SPRITE),
                    btn_hover_sprite: Some(Self::SEPARATOR_SPRITE),
                    font_scale: Self::FONT_SCALE_CHILD_MENU,
                    btn_font_scale: Self::FONT_SCALE_HOME_BTN,
                    heading_font_scale: Self::FONT_SCALE_HEADING,
                    ..Default::default()
                }
            ),
            separator_ui_texture: context.ui_sys.to_ui_texture(context.tex_cache, separator_tex_handle),
            child_menus: Self::create_child_menus(context),
        }
    }

    fn child_menu_for_button(&mut self, button: HomeMainMenuButton) -> Option<&mut dyn ModalMenu> {
        self.child_menus[button as usize]
            .as_mut()
            .map(|menu| menu.as_mut())
    }

    fn close_child_menus(&mut self, context: &mut UiWidgetContext) {
        for menu in (&mut self.child_menus).into_iter().flatten() {
            menu.close(context);
        }
    }

    fn draw_child_menus(&mut self, context: &mut UiWidgetContext) -> bool {
        let mut any_child_menu_open = false;
        for menu in (&mut self.child_menus).into_iter().flatten() {
            if menu.is_open() {
                menu.draw(context);
                any_child_menu_open = true;
            }
        }
        any_child_menu_open
    }

    fn create_child_menus(context: &mut UiWidgetContext) -> ArrayVec<Option<Box<dyn ModalMenu>>, MAIN_MENU_BUTTON_COUNT> {
        let mut menus = ArrayVec::new();
        for button in HomeMainMenuButton::iter() {
            menus.push(Self::create_child_menu_for_button(context, button));
        }
        menus
    }

    fn create_child_menu_for_button(context: &mut UiWidgetContext, button: HomeMainMenuButton) -> Option<Box<dyn ModalMenu>> {
        let shared_params = ModalMenuParams {
            position: Some(Self::POSITION),
            size: Some(Self::calc_size(context)),
            background_sprite: Some(Self::BG_SPRITE),
            btn_hover_sprite: Some(Self::SEPARATOR_SPRITE),
            font_scale: Self::FONT_SCALE_CHILD_MENU,
            btn_font_scale: Self::FONT_SCALE_HOME_BTN,
            heading_font_scale: Self::FONT_SCALE_HEADING,
            ..Default::default()
        };

        let menu: Box<dyn ModalMenu> = match button {
            HomeMainMenuButton::NewGame => {
                let mut params = shared_params;
                params.title = Some("New Game".into());
                Box::new(NewGameModalMenu::new(context, params))
            }
            HomeMainMenuButton::Continue => {
                return None; // TODO
            }
            HomeMainMenuButton::LoadGame => {
                let mut params = shared_params;
                params.title = Some("Load Game".into());
                Box::new(SaveGameModalMenu::new(context, params, SaveGameActions::Load))
            }
            HomeMainMenuButton::CustomGame => {
                return None; // TODO
            }
            HomeMainMenuButton::Settings => {
                let mut params = shared_params;
                params.title = Some("Settings".into());
                Box::new(SettingsModalMenu::new(context, params))
            }
            HomeMainMenuButton::About => {
                let mut params = shared_params;
                params.title = Some("About".into());
                Box::new(AboutModalMenu::new(context, params))
            }
            HomeMainMenuButton::Exit => {
                return None; // Exit - no menu.
            }
        };

        Some(menu)
    }

    fn is_button_enabled(button: HomeMainMenuButton) -> bool {
        match button {
            HomeMainMenuButton::NewGame    => true,
            HomeMainMenuButton::Continue   => false, // TODO
            HomeMainMenuButton::LoadGame   => true,
            HomeMainMenuButton::CustomGame => false, // TODO
            HomeMainMenuButton::Settings   => true,
            HomeMainMenuButton::About      => true,
            HomeMainMenuButton::Exit       => true,
        }
    }

    fn handle_button_click(&mut self, context: &mut UiWidgetContext, button: HomeMainMenuButton) {
        match button {
            HomeMainMenuButton::NewGame    => self.on_new_game_button(context),
            HomeMainMenuButton::Continue   => self.on_continue_button(context),
            HomeMainMenuButton::LoadGame   => self.on_load_game_button(context),
            HomeMainMenuButton::CustomGame => self.on_custom_game_button(context),
            HomeMainMenuButton::Settings   => self.on_settings_button(context),
            HomeMainMenuButton::About      => self.on_about_button(context),
            HomeMainMenuButton::Exit       => self.on_exit_button(context),
        }
    }

    fn on_new_game_button(&mut self, context: &mut UiWidgetContext) {
        if let Some(menu) = self.child_menu_for_button(HomeMainMenuButton::NewGame) {
            menu.open(context);
        }
    }

    fn on_continue_button(&mut self, context: &mut UiWidgetContext) {
        if let Some(menu) = self.child_menu_for_button(HomeMainMenuButton::Continue) {
            menu.open(context);
        }
    }

    fn on_load_game_button(&mut self, context: &mut UiWidgetContext) {
        if let Some(menu) = self.child_menu_for_button(HomeMainMenuButton::LoadGame) {
            menu.open(context);
            menu.as_any_mut()
                .downcast_mut::<SaveGameModalMenu>()
                .unwrap()
                .set_actions(SaveGameActions::Load);
        }
    }

    fn on_custom_game_button(&mut self, context: &mut UiWidgetContext) {
        if let Some(menu) = self.child_menu_for_button(HomeMainMenuButton::CustomGame) {
            menu.open(context);
        }
    }

    fn on_settings_button(&mut self, context: &mut UiWidgetContext) {
        if let Some(menu) = self.child_menu_for_button(HomeMainMenuButton::Settings) {
            menu.open(context);
        }
    }

    fn on_about_button(&mut self, context: &mut UiWidgetContext) {
        if let Some(menu) = self.child_menu_for_button(HomeMainMenuButton::About) {
            menu.open(context);
        }
    }

    fn on_exit_button(&mut self, _context: &mut UiWidgetContext) {
        GameLoop::get_mut().request_quit();
    }
}

impl ModalMenu for HomeMainMenu {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn is_open(&self) -> bool {
        self.menu.is_open()
    }

    fn open(&mut self, context: &mut UiWidgetContext) {
        self.menu.open(context);
    }

    fn close(&mut self, context: &mut UiWidgetContext) {
        self.menu.close(context);
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        let any_child_menu_open = self.draw_child_menus(context);
        if any_child_menu_open {
            return;
        }

        let mut pressed_button: Option<HomeMainMenuButton> = None;

        self.menu.draw(context, |context, this| {
            let ui = context.ui_sys.ui();

            const BUTTON_SPACING: Vec2 = Vec2::new(6.0, 6.0);
            let _item_spacing = widgets::push_item_spacing(ui, BUTTON_SPACING.x, BUTTON_SPACING.y);

            // Draw heading and separator:
            this.draw_menu_heading(context, &["Heritage Builder", "The Dragon Legacy"]);

            // Set font for the buttons:
            ui.set_window_font_scale(Self::FONT_SCALE_HOME_BTN);
            // Draw actual menu buttons:
            let pressed_button_index = widgets::draw_centered_button_group_custom_hover(
                context.ui_sys,
                &HomeMainMenuButton::labels(),
                Some(MODAL_BUTTON_LARGE_SIZE),
                Some(Vec2::new(0.0, 150.0)),
                self.separator_ui_texture,
                Some(|button_index: usize| -> bool {
                    Self::is_button_enabled(HomeMainMenuButton::try_from_primitive(button_index).unwrap())
                })
            );

            if let Some(pressed_index) = pressed_button_index {
                pressed_button = HomeMainMenuButton::try_from_primitive(pressed_index).ok();
            }
        });

        if let Some(button) = pressed_button {
            self.close_child_menus(context);
            self.handle_button_click(context, button);
        }
    }
}

// ----------------------------------------------
// StaticFullScreenBackground
// ----------------------------------------------

// Static, single image/frame background.
struct StaticFullScreenBackground {
    ui_texture: UiTextureHandle,
}

impl StaticFullScreenBackground {
    fn new(context: &mut UiWidgetContext) -> Self {
        let bg_tex_handle = context.tex_cache.load_texture_with_settings(
            ui::assets_path().join("misc/home_menu_static_bg.png").to_str().unwrap(),
            Some(ui::texture_settings())
        );
        Self {
            ui_texture: context.ui_sys.to_ui_texture(context.tex_cache, bg_tex_handle),
        }
    }

    fn draw(&self, context: &mut UiWidgetContext) {
        let ui = context.ui_sys.ui();
        let draw_list = ui.get_background_draw_list();

        // Draw full-screen rectangle with the background image:
        draw_list.add_image(self.ui_texture, [0.0, 0.0], ui.io().display_size).build();
    }

    #[inline]
    fn anim_scene_completed(&self) -> bool {
        true
    }
}

// ----------------------------------------------
// AnimatedFullScreenBackground
// ----------------------------------------------

const BACKGROUND_ANIM_FRAME_COUNT: usize = 30;
const BACKGROUND_ANIM_FRAME_DURATION: Seconds = 0.3;
const BACKGROUND_ANIM_LOOPING: bool = true;

struct AnimatedFullScreenBackground {
    frames: ArrayVec<UiTextureHandle, BACKGROUND_ANIM_FRAME_COUNT>,
    frame_index: usize,
    frame_play_time_secs: Seconds,
    played_once: bool,
}

impl AnimatedFullScreenBackground {
    fn new(context: &mut UiWidgetContext) -> Self {
        let mut frames = ArrayVec::new();

        for i in 0..BACKGROUND_ANIM_FRAME_COUNT {
            let tex_handle = context.tex_cache.load_texture_with_settings(
                ui::assets_path().join(format!("misc/home_menu_anim/frame{i}.jpg")).to_str().unwrap(),
                Some(ui::texture_settings())
            );
            frames.push(context.ui_sys.to_ui_texture(context.tex_cache, tex_handle));
        }

        // TODO: Temporary, should get the sound system from UiWidgetContext instead.
        let sound_sys = GameLoop::get_mut().engine_mut().sound_system();
        let sound_key = sound_sys.load_music("dynastys_legacy_1.mp3");
        sound_sys.play_music(sound_key, true);

        Self {
            frames,
            frame_index: 0,
            frame_play_time_secs: 0.0,
            played_once: false,
        }
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        // Advance animation:
        self.frame_play_time_secs += context.delta_time_secs;

        if self.frame_play_time_secs >= BACKGROUND_ANIM_FRAME_DURATION {
            if self.frame_index < self.frames.len() - 1 {
                // Move to next frame.
                self.frame_index += 1;
            } else {
                // Played the whole anim.
                self.played_once = true;
                if BACKGROUND_ANIM_LOOPING {
                    self.frame_index = BACKGROUND_ANIM_FRAME_COUNT - 2; // Loop the last 2 frames.
                }
            }
            // Reset the clock.
            self.frame_play_time_secs = 0.0;
        }

        let frame_ui_texture = self.frames[self.frame_index];

        // Draw full-screen rectangle with the background image:
        let ui = context.ui_sys.ui();
        let draw_list = ui.get_background_draw_list();
        draw_list.add_image(frame_ui_texture, [0.0, 0.0], ui.io().display_size).build();
    }

    #[inline]
    fn anim_scene_completed(&self) -> bool {
        self.played_once
    }
}
