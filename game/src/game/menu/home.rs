use std::any::Any;
use arrayvec::ArrayVec;
use num_enum::TryFromPrimitive;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    GameMenuMode,
    GameMenusSystem,
    GameMenusContext,
    GameMenusInputArgs,
    TilePalette,
    TilePlacement,
    TileInspector,
    modal::*,
    widgets::{self, UiStyleOverrides, UiStyleTextLabelInvisibleButtons, UiWidgetContext},
};
use crate::{
    game::GameLoop,
    save::{Save, Load},
    tile::rendering::TileMapRenderFlags,
    utils::{Size, Rect, Vec2, coords::CellRange},
    imgui_ui::{UiInputEvent, UiTextureHandle},
};

// ----------------------------------------------
// HomeMenus
// ----------------------------------------------

pub struct HomeMenus {
    main_menu: HomeMainMenu,
    background: FullScreenBackground,
}

impl HomeMenus {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        Self {
            main_menu: HomeMainMenu::new(context),
            background: FullScreenBackground::new(context),
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
        let ui_sys = context.engine.ui_system();

        let _style_overrides =
            UiStyleOverrides::in_game_hud_menus(ui_sys);

        let mut widget_context =
            UiWidgetContext::new(context.sim, context.world, context.engine);

        self.main_menu.draw(&mut widget_context);
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
    #[strum(props(Label = "NEW GAME"))]
    NewGame,

    #[strum(props(Label = "CONTINUE"))]
    Continue,

    #[strum(props(Label = "LOAD GAME"))]
    LoadGame,

    #[strum(props(Label = "CUSTOM GAME"))]
    CustomGame,

    #[strum(props(Label = "SETTINGS"))]
    Settings,

    #[strum(props(Label = "ABOUT"))]
    About,

    #[strum(props(Label = "EXIT GAME"))]
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

struct HomeMainMenu {
    menu: BasicModalMenu,
    separator_ui_texture: UiTextureHandle,
    child_menus: ArrayVec<Option<Box<dyn ModalMenu>>, MAIN_MENU_BUTTON_COUNT>,
}

impl HomeMainMenu {
    fn new(context: &mut UiWidgetContext) -> Self {
        let separator_tex_handle = context.tex_cache.load_texture_with_settings(
            super::ui_assets_path().join("misc/brush_stroke_divider.png").to_str().unwrap(),
            Some(super::ui_texture_settings())
        );
        Self {
            menu: BasicModalMenu::new(
                context,
                ModalMenuParams {
                    start_open: true,
                    position: Some(Vec2::new(50.0, 50.0)),
                    size: Some(Size::new(550, context.viewport_size.height - 100)),
                    background_sprite: Some("misc/scroll_bg.png"),
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

    fn draw_child_menus(&mut self, context: &mut UiWidgetContext) {
        for menu in (&mut self.child_menus).into_iter().flatten() {
            menu.draw(context);
        }
    }

    fn create_child_menus(context: &mut UiWidgetContext) -> ArrayVec<Option<Box<dyn ModalMenu>>, MAIN_MENU_BUTTON_COUNT> {
        let mut menus = ArrayVec::new();
        for button in HomeMainMenuButton::iter() {
            menus.push(Self::create_child_menu_for_button(context, button));
        }
        menus
    }

    fn create_child_menu_for_button(context: &mut UiWidgetContext,
                                    button: HomeMainMenuButton)
                                    -> Option<Box<dyn ModalMenu>> {
        let menu: Box<dyn ModalMenu> = match button {
            HomeMainMenuButton::NewGame    => Box::new(NewGameModalMenu::new(context, "New Game".into())),
            HomeMainMenuButton::Continue   => return None, // TODO
            HomeMainMenuButton::LoadGame   => Box::new(SaveGameModalMenu::new(context, SaveGameActions::Load)),
            HomeMainMenuButton::CustomGame => return None, // TODO
            HomeMainMenuButton::Settings   => Box::new(SettingsModalMenu::new(context, "Settings".into())),
            HomeMainMenuButton::About      => Box::new(AboutModalMenu::new(context, "About".into())),
            HomeMainMenuButton::Exit       => return None, // Exit - no menu.
        };
        Some(menu)
    }

    fn is_button_enabled(button: HomeMainMenuButton) -> bool {
        match button {
            HomeMainMenuButton::NewGame    => true,
            HomeMainMenuButton::Continue   => false,
            HomeMainMenuButton::LoadGame   => true,
            HomeMainMenuButton::CustomGame => false,
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
        let mut pressed_button: Option<HomeMainMenuButton> = None;

        self.menu.draw(context, |context| {
            let ui = context.ui_sys.ui();
            let window_draw_list = ui.get_window_draw_list();

            // Make button background transparent and borderless.
            let _button_style_overrides =
                UiStyleTextLabelInvisibleButtons::apply_overrides(context.ui_sys);

            const BUTTON_SPACING: Vec2 = Vec2::new(6.0, 6.0);
            let _item_spacing =
                UiStyleOverrides::set_item_spacing(context.ui_sys, BUTTON_SPACING.x, BUTTON_SPACING.y);

            // Bigger font for the heading.
            ui.set_window_font_scale(1.8);
            // Draw heading as buttons, so everything is properly centered.
            widgets::draw_centered_button_group_with_offsets(
                ui,
                &window_draw_list,
                &["Heritage Builder", "The Dragon Legacy"],
                None,
                Some(Vec2::new(0.0, -200.0))
            );

            // Draw separator:
            let heading_separator_width = ui.calc_text_size("The Dragon Legacy")[0];
            let heading_separator_rect = Rect::from_extents(
                Vec2::from_array([ui.item_rect_min()[0] - 40.0, ui.item_rect_min()[1] - 10.0]),
                Vec2::from_array([ui.item_rect_min()[0] + heading_separator_width + 40.0, ui.item_rect_max()[1] + 30.0])
            ).translated(Vec2::new(0.0, 40.0));
            window_draw_list.add_image(self.separator_ui_texture,
                                       heading_separator_rect.min.to_array(),
                                       heading_separator_rect.max.to_array())
                                       .build();

            // Bigger font for the buttons.
            ui.set_window_font_scale(1.5);
            // Draw actual menu buttons:
            let pressed_button_index = widgets::draw_centered_button_group_ex(
                ui,
                &window_draw_list,
                &HomeMainMenuButton::labels(),
                Some(Size::new(180, 40)),
                Some(Vec2::new(0.0, 150.0)),
                Some(|ui: &imgui::Ui, draw_list: &imgui::DrawListMut<'_>, button_index: usize| {
                    // Draw underline effect when hovered / active:
                    let button_rect = Rect::from_extents(
                        Vec2::from_array(ui.item_rect_min()),
                        Vec2::from_array(ui.item_rect_max())
                    ).translated(Vec2::new(0.0, 20.0));

                    let enabled = Self::is_button_enabled(
                        HomeMainMenuButton::try_from_primitive(button_index).unwrap()
                    );

                    let underline_tint_color = if ui.is_item_active() || !enabled {
                        imgui::ImColor32::from_rgba_f32s(1.0, 1.0, 1.0, 0.5)
                    } else {
                        imgui::ImColor32::WHITE
                    };

                    draw_list.add_image(self.separator_ui_texture,
                                        button_rect.min.to_array(),
                                        button_rect.max.to_array())
                                        .col(underline_tint_color)
                                        .build();
                }),
                Some(|button_index: usize| -> bool {
                    Self::is_button_enabled(HomeMainMenuButton::try_from_primitive(button_index).unwrap())
                })
            );

            // Restore default.
            ui.set_window_font_scale(1.0);

            if let Some(pressed_index) = pressed_button_index {
                pressed_button = HomeMainMenuButton::try_from_primitive(pressed_index).ok();
            }
        });

        if let Some(button) = pressed_button {
            self.close_child_menus(context);
            self.handle_button_click(context, button);
        }

        self.draw_child_menus(context);
    }
}

// ----------------------------------------------
// FullScreenBackground
// ----------------------------------------------

struct FullScreenBackground {
    ui_texture: UiTextureHandle,
}

impl FullScreenBackground {
    fn new(context: &mut UiWidgetContext) -> Self {
        let bg_tex_handle = context.tex_cache.load_texture_with_settings(
            super::ui_assets_path().join("misc/home_menu_bg.png").to_str().unwrap(),
            Some(super::ui_texture_settings())
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
}
