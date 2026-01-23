use arrayvec::ArrayVec;
use std::{any::Any, path::PathBuf};
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    modal::*,
    button::{SpriteButton, ButtonState, ButtonDef},
    widgets::{self, UiStyleTextLabelInvisibleButtons},
    home::HomeMainMenu,
    GameMenusInputArgs,
};
use crate::{
    utils::{self, Size, Rect, Vec2},
    app::input::{InputAction, InputKey},
    ui::{self, UiTextureHandle, UiInputEvent, widgets::UiWidgetContext},
};

// ----------------------------------------------
// MenuBarsWidget
// ----------------------------------------------

pub struct MenuBarsWidget {
    // NOTE: These need stable addresses for parent backreferences
    // in child Modal Windows, thus using Box here.
    top_bar: Box<TopBar>,
    left_bar: Box<LeftBar>,
    game_speed_controls_bar: Box<GameSpeedControlsBar>,
}

impl MenuBarsWidget {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        Self {
            top_bar: TopBar::new(context),
            left_bar: LeftBar::new(context),
            game_speed_controls_bar: GameSpeedControlsBar::new(context),
        }
    }

    pub fn handle_input(&mut self, context: &mut UiWidgetContext, args: GameMenusInputArgs) -> UiInputEvent {
        self.left_bar.handle_input(context, args)
    }

    pub fn draw(&mut self, context: &mut UiWidgetContext) {
        let ui = context.ui_sys.ui();

        const HORIZONTAL_SPACING: f32 = 2.0;
        const VERTICAL_SPACING: f32 = 4.0;

        let _item_spacing = widgets::push_item_spacing(ui, HORIZONTAL_SPACING, VERTICAL_SPACING);

        // Center top bar to the middle of the display:
        widgets::set_next_window_pos(
            Vec2::new(ui.io().display_size[0] * 0.5, 0.0),
            Vec2::new(0.5, 0.0),
            imgui::Condition::Always
        );
        Self::draw_bar_widget(context,
                              None, // Handled by set_next_window_pos instead.
                              "Menu Bar Widget Top",
                              |context| self.top_bar.draw(context));

        // Left-hand-side vertical bar:
        Self::draw_bar_widget(context,
                              Some([0.0, 60.0]),
                              "Menu Bar Widget Left",
                              |context| self.left_bar.draw(context));

        // Game speed controls horizontal top bar:
        Self::draw_bar_widget(context,
                              Some([0.0, 0.0]),
                              "Menu Bar Widget Speed Ctrls",
                              |context| self.game_speed_controls_bar.draw(context));
    }

    fn draw_bar_widget<DrawFn>(context: &mut UiWidgetContext,
                               position: Option<[f32; 2]>,
                               name: &str,
                               draw_fn: DrawFn)
        where DrawFn: FnOnce(&mut UiWidgetContext)
    {
        let pos_cond = if position.is_some() { imgui::Condition::Always } else { imgui::Condition::Never };
        let ui = context.ui_sys.ui();
        ui.window(name)
            .position(position.unwrap_or([0.0, 0.0]), pos_cond)
            .flags(widgets::window_flags() | imgui::WindowFlags::NO_BACKGROUND)
            .build(|| {
                ui.set_window_font_scale(0.8);
                draw_fn(context);
                widgets::draw_current_window_debug_rect(context.ui_sys.ui());
                ui.set_window_font_scale(1.0);
            });
    }
}

// ----------------------------------------------
// MenuBar
// ----------------------------------------------

pub trait MenuBar: Any {
    fn as_any(&self) -> &dyn Any;

    fn draw(&mut self, context: &mut UiWidgetContext);
    fn handle_input(&mut self, _context: &mut UiWidgetContext, _args: GameMenusInputArgs) -> UiInputEvent {
        UiInputEvent::NotHandled
    }

    fn open_modal_menu(&mut self, _context: &mut UiWidgetContext, _menu_id: ModalMenuId) -> Option<&mut dyn ModalMenu> { None }
    fn close_modal_menu(&mut self, _context: &mut UiWidgetContext, _menu_id: ModalMenuId) {}
    fn close_all_modal_menus(&mut self, _context: &mut UiWidgetContext) -> bool { false }
}

// ----------------------------------------------
// TopBar
// ----------------------------------------------

const TOP_BAR_ICON_COUNT: usize = TopBarIcon::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum TopBarIcon {
    #[strum(props(Sprite = "population_icon", Width = 35, Height = 20))]
    Population,

    #[strum(props(Sprite = "player_icon", Width = 45, Height = 45))]
    Player,

    #[strum(props(Sprite = "gold_icon", Width = 35, Height = 20))]
    Gold,
}

impl TopBarIcon {
    fn size(self) -> Vec2 {
        let width  = self.get_int("Width").unwrap()  as f32;
        let height = self.get_int("Height").unwrap() as f32;
        Vec2::new(width, height)
    }

    fn asset_path(self) -> PathBuf {
        let sprite_name = self.get_str("Sprite").unwrap();
        ui::assets_path()
            .join("icons")
            .join(sprite_name)
            .with_extension("png")
    }

    fn load_texture(self, context: &mut UiWidgetContext) -> UiTextureHandle {
        let sprite_path = self.asset_path();
        let tex_handle = context.tex_cache.load_texture_with_settings(
            sprite_path.to_str().unwrap(),
            Some(ui::texture_settings())
        );
        context.ui_sys.to_ui_texture(context.tex_cache, tex_handle)
    }
}

struct TopBar {
    icon_textures: ArrayVec<UiTextureHandle, TOP_BAR_ICON_COUNT>,
    background_sprite: UiTextureHandle,
}

impl TopBar {
    fn new(context: &mut UiWidgetContext) -> Box<Self> {
        let background_sprite_path = ui::assets_path().join("misc/wide_page_bg.png");
        let background_sprite = context.tex_cache.load_texture_with_settings(
            background_sprite_path.to_str().unwrap(),
            Some(ui::texture_settings())
        );

        let mut icon_textures = ArrayVec::new();
        for icon in TopBarIcon::iter() {
            icon_textures.push(icon.load_texture(context));
        }

        Box::new(Self {
            icon_textures,
            background_sprite: context.ui_sys.to_ui_texture(context.tex_cache, background_sprite),
        })
    }
}

impl MenuBar for TopBar {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        let ui = context.ui_sys.ui();

        // Draw background:
        {
            let window_rect = Rect::from_pos_and_size(
                Vec2::from_array(ui.window_pos()),
                Vec2::from_array(ui.window_size())
            );
            ui.get_window_draw_list()
                .add_image(self.background_sprite, window_rect.min.to_array(), window_rect.max.to_array())
                .build();
        }

        // Spacing is handled manually with dummy items.
        let _item_spacing = widgets::push_item_spacing(ui, 0.0, 0.0);

        // We'll use buttons for text labels, for straightforward text centering and layout.
        let _btn_style_overrides =
            UiStyleTextLabelInvisibleButtons::apply_overrides(context.ui_sys);

        // Population:
        {
            let icon_size = TopBarIcon::Population.size();
            let icon_texture = self.icon_textures[TopBarIcon::Population as usize];

            widgets::draw_sprite(context.ui_sys, "Population", icon_size, icon_texture, self.background_sprite, Some("Population"));
            ui.same_line(); // Horizontal layout.

            let population = context.world.stats().population.total;
            widgets::draw_centered_text_label(ui, &population.to_string(), Vec2::new(50.0, icon_size.y));
            ui.same_line();

            widgets::spacing(ui, Vec2::new(30.0, 0.0));
            ui.same_line();
        }

        // Player Icon:
        {
            let icon_size = TopBarIcon::Player.size();
            let icon_texture = self.icon_textures[TopBarIcon::Player as usize];

            widgets::spacing(ui, Vec2::new(icon_size.x, 0.0));
            ui.same_line();

            let icon_rect = Rect::from_extents(
                Vec2::new(ui.item_rect_min()[0], 0.0),
                Vec2::new(ui.item_rect_max()[0], icon_size.y)
            );

            let draw_list = ui.get_window_draw_list();
            // NOTE: Draw with fullscreen clip rect so that the player icon is allowed to overflow the window bounds.
            draw_list.with_clip_rect([0.0, 0.0], ui.io().display_size, || {
                widgets::draw_window_style_rect(
                    ui,
                    &draw_list,
                    icon_rect.min,
                    icon_rect.max
                );
                draw_list
                    .add_image(icon_texture, icon_rect.min.to_array(), icon_rect.max.to_array())
                    .build();
            });

            widgets::spacing(ui, Vec2::new(30.0, 0.0));
            ui.same_line();
        }

        // Gold:
        {
            let icon_size = TopBarIcon::Gold.size();
            let icon_texture = self.icon_textures[TopBarIcon::Gold as usize];

            widgets::draw_sprite(context.ui_sys, "Gold", icon_size, icon_texture, self.background_sprite, Some("Gold"));
            ui.same_line(); // Horizontal layout.

            let gold_units_total = context.world.stats().treasury.gold_units_total;
            widgets::draw_centered_text_label(ui, &gold_units_total.to_string(), Vec2::new(50.0, icon_size.y));
            ui.same_line();
        }
    }
}

// ----------------------------------------------
// LeftBar
// ----------------------------------------------

const LEFT_BAR_BUTTON_COUNT: usize = LeftBarButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum LeftBarButtonKind {
    #[strum(props(Sprite = "menu_bar/main_menu", Tooltip = "Game"))]
    MainMenu,

    #[strum(props(Sprite = "menu_bar/save_game", Tooltip = "Load | Save"))]
    SaveGame,

    #[strum(props(Sprite = "menu_bar/settings"))]
    Settings,

    #[strum(props(Sprite = "menu_bar/new_game", Hidden = true))]
    NewGame,
}

impl LeftBarButtonKind {
    const BUTTON_SIZE: Size = Size::new(24, 24);

    fn sprite_path(self) -> &'static str {
        self.get_str("Sprite").unwrap()
    }

    fn name(self) -> &'static str {
        let sprite_path = self.sprite_path();
        // Take the base sprite name following "menu_bar/":
        sprite_path.split_at(sprite_path.find("/").unwrap() + 1).1
    }

    fn tooltip(self) -> String {
        if let Some(tooltip) = self.get_str("Tooltip") {
            return tooltip.to_string();
        }
        utils::snake_case_to_title::<64>(self.name()).to_string()
    }

    fn is_hidden(self) -> bool {
        self.get_bool("Hidden").unwrap_or(false)
    }

    fn new_button(self, context: &mut UiWidgetContext) -> LeftBarButton {
        LeftBarButton {
            btn: SpriteButton::new(
                context,
                ButtonDef {
                    name: self.sprite_path(),
                    size: Self::BUTTON_SIZE,
                    tooltip: Some(self.tooltip()),
                    show_tooltip_when_pressed: true,
                    state_transition_secs: 0.5,
                    hidden: self.is_hidden(),
                },
                ButtonState::Idle,
            ),
            kind: self,
        }
    }

    fn create_all(context: &mut UiWidgetContext) -> ArrayVec<LeftBarButton, LEFT_BAR_BUTTON_COUNT> {
        let mut buttons = ArrayVec::new();
        for btn_kind in Self::iter() {
            buttons.push(btn_kind.new_button(context));
        }
        buttons
    }
}

struct LeftBarButton {
    btn: SpriteButton,
    kind: LeftBarButtonKind,
}

struct LeftBar {
    buttons: ArrayVec<LeftBarButton, LEFT_BAR_BUTTON_COUNT>,
    modal_menus: ArrayVec<Box<dyn ModalMenu>, LEFT_BAR_BUTTON_COUNT>,
    background_sprite: UiTextureHandle,
}

impl LeftBar {
    fn new(context: &mut UiWidgetContext) -> Box<Self> {
        let background_sprite_path = ui::assets_path().join("misc/tall_page_bg.png");
        let background_sprite = context.tex_cache.load_texture_with_settings(
            background_sprite_path.to_str().unwrap(),
            Some(ui::texture_settings())
        );

        let mut left_bar = Box::new(Self {
            buttons: LeftBarButtonKind::create_all(context),
            modal_menus: ArrayVec::new(),
            background_sprite: context.ui_sys.to_ui_texture(context.tex_cache, background_sprite),
        });

        // Same settings as the home menus.
        let modal_params = ModalMenuParams {
            size: Some(HomeMainMenu::calc_size(context)),
            background_sprite: Some(HomeMainMenu::BG_SPRITE),
            btn_hover_sprite: Some(HomeMainMenu::SEPARATOR_SPRITE),
            font_scale: HomeMainMenu::FONT_SCALE_CHILD_MENU,
            btn_font_scale: HomeMainMenu::FONT_SCALE_HOME_BTN,
            heading_font_scale: HomeMainMenu::FONT_SCALE_HEADING,
            ..Default::default()
        };

        for button_kind in LeftBarButtonKind::iter() {
            match button_kind {
                LeftBarButtonKind::MainMenu =>{
                    let mut params = modal_params.clone();
                    params.title = Some(LeftBarButtonKind::MainMenu.tooltip());
                    left_bar.modal_menus.push(
                        Box::new(
                            MainModalMenu::new(context, params, left_bar.as_ref())
                        )
                    )
                }
                LeftBarButtonKind::SaveGame => {
                    let mut params = modal_params.clone();
                    params.title = Some(LeftBarButtonKind::SaveGame.tooltip());
                    left_bar.modal_menus.push(
                        Box::new(
                            SaveGameModalMenu::new(context, params, SaveGameActions::Save | SaveGameActions::Load)
                        )
                    )
                }
                LeftBarButtonKind::Settings => {
                    let mut params = modal_params.clone();
                    params.title = Some(LeftBarButtonKind::Settings.tooltip());
                    left_bar.modal_menus.push(
                        Box::new(
                            SettingsModalMenu::new(context, params)
                        )
                    )
                }
                LeftBarButtonKind::NewGame => {
                    let mut params = modal_params.clone();
                    params.title = Some(LeftBarButtonKind::NewGame.tooltip());
                    left_bar.modal_menus.push(
                        Box::new(
                            NewGameModalMenu::new(context, params)
                        )
                    )
                }
            }
        }

        left_bar
    }
}

impl MenuBar for LeftBar {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        // Each button is associated with a modal menu.
        debug_assert!(self.buttons.len() == self.modal_menus.len());

        // Draw background:
        {
            let ui = context.ui_sys.ui();

            let window_rect = Rect::from_pos_and_size(
                Vec2::from_array(ui.window_pos()),
                Vec2::from_array(ui.window_size())
            );
            ui.get_window_draw_list()
                .add_image(self.background_sprite, window_rect.min.to_array(), window_rect.max.to_array())
                .build();
        }

        let mut pressed_button_index: Option<usize> = None;

        for (index, button) in self.buttons.iter_mut().enumerate() {
            if button.kind.is_hidden() {
                continue;
            }

            let pressed = button.btn.draw(context, Some(self.background_sprite));

            if pressed && pressed_button_index.is_none() {
                pressed_button_index = Some(index);
            }

            // Pressed state doesn't persist.
            button.btn.unpress();
        }

        if let Some(pressed_index) = pressed_button_index {
            // Ensure any other menus are closed first.
            self.close_all_modal_menus(context);
            // Open new menu.
            self.modal_menus[pressed_index].open(context);
        }

        // Draw open modal menus.
        let mut any_modal_menu_open = false;
        for modal in &mut self.modal_menus {
            modal.draw(context);
            any_modal_menu_open |= modal.is_open();
        }

        if any_modal_menu_open {
            context.sim.pause();
        }
    }

    fn handle_input(&mut self, context: &mut UiWidgetContext, args: GameMenusInputArgs) -> UiInputEvent {
        if let GameMenusInputArgs::Key { key, action, .. } = args {
            if action == InputAction::Press && key == InputKey::Escape {
                // Close all modal menus and return to game.
                if self.close_all_modal_menus(context) {
                    // Handled the key press.
                    return UiInputEvent::Handled;
                }
            }
        }

        // Let the event propagate.
        UiInputEvent::NotHandled
    }

    fn open_modal_menu(&mut self, context: &mut UiWidgetContext, menu_id: ModalMenuId) -> Option<&mut dyn ModalMenu> {
        // Only one modal menu open at a time.
        self.close_all_modal_menus(context);

        for modal in &mut self.modal_menus {
            if modal.id() == menu_id {
                modal.open(context);
                return Some(modal.as_mut());
            }
        }

        panic!("Modal Menu not found!");
    }

    fn close_modal_menu(&mut self, context: &mut UiWidgetContext, menu_id: ModalMenuId) {
        for modal in &mut self.modal_menus {
            if modal.id() == menu_id {
                modal.close(context);
                return;
            }
        }

        panic!("Modal Menu not found!");
    }

    fn close_all_modal_menus(&mut self, context: &mut UiWidgetContext) -> bool {
        let mut any_modal_menu_open = false;
        for modal in &mut self.modal_menus {
            if modal.is_open() {
                modal.close(context);
                any_modal_menu_open = true;
            }
        }
        any_modal_menu_open
    }
}

// ----------------------------------------------
// GameSpeedControlsBar
// ----------------------------------------------

const GAME_SPEED_CONTROLS_BUTTON_COUNT: usize = GameSpeedControlButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum GameSpeedControlButtonKind {
    #[strum(props(Sprite = "menu_bar/play"))]
    Play,

    #[strum(props(Sprite = "menu_bar/pause"))]
    Pause,

    #[strum(props(Sprite = "menu_bar/slowdown"))]
    Slowdown,

    #[strum(props(Sprite = "menu_bar/speedup"))]
    Speedup,
}

impl GameSpeedControlButtonKind {
    const BUTTON_SIZE: Size = Size::new(20, 20);

    fn sprite_path(self) -> &'static str {
        self.get_str("Sprite").unwrap()
    }

    fn name(self) -> &'static str {
        let sprite_path = self.sprite_path();
        // Take the base sprite name following "menu_bar/":
        sprite_path.split_at(sprite_path.find("/").unwrap() + 1).1
    }

    fn tooltip(self) -> String {
        if let Some(tooltip) = self.get_str("Tooltip") {
            return tooltip.to_string();
        }
        utils::snake_case_to_title::<64>(self.name()).to_string()
    }

    fn new_button(self, context: &mut UiWidgetContext) -> GameSpeedControlButton {
        GameSpeedControlButton {
            btn: SpriteButton::new(
                context,
                ButtonDef {
                    name: self.sprite_path(),
                    size: Self::BUTTON_SIZE,
                    tooltip: Some(self.tooltip()),
                    show_tooltip_when_pressed: true,
                    state_transition_secs: 0.5,
                    hidden: false,
                },
                ButtonState::Idle,
            ),
            kind: self,
        }
    }

    fn create_all(context: &mut UiWidgetContext) -> ArrayVec<GameSpeedControlButton, GAME_SPEED_CONTROLS_BUTTON_COUNT> {
        let mut buttons = ArrayVec::new();
        for btn_kind in Self::iter() {
            buttons.push(btn_kind.new_button(context));
        }
        buttons
    }
}

struct GameSpeedControlButton {
    btn: SpriteButton,
    kind: GameSpeedControlButtonKind,
}

struct GameSpeedControlsBar {
    buttons: ArrayVec<GameSpeedControlButton, GAME_SPEED_CONTROLS_BUTTON_COUNT>,
    background_sprite: UiTextureHandle,
}

impl GameSpeedControlsBar {
    fn new(context: &mut UiWidgetContext) -> Box<Self> {
        let background_sprite_path = ui::assets_path().join("misc/wide_page_bg.png");
        let background_sprite = context.tex_cache.load_texture_with_settings(
            background_sprite_path.to_str().unwrap(),
            Some(ui::texture_settings())
        );
        
        Box::new(Self {
            buttons: GameSpeedControlButtonKind::create_all(context),
            background_sprite: context.ui_sys.to_ui_texture(context.tex_cache, background_sprite),
        })
    }
}

impl MenuBar for GameSpeedControlsBar {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        // We'll use buttons for text labels, for straightforward text centering and layout.
        let _btn_style_overrides =
            UiStyleTextLabelInvisibleButtons::apply_overrides(context.ui_sys);

        let ui = context.ui_sys.ui();

        // Draw background:
        {
            let window_rect = Rect::from_pos_and_size(
                Vec2::from_array(ui.window_pos()),
                Vec2::from_array(ui.window_size())
            );
            ui.get_window_draw_list()
                .add_image(self.background_sprite, window_rect.min.to_array(), window_rect.max.to_array())
                .build();
        }

        let mut pressed_button_index: Option<usize> = None;

        for (index, button) in self.buttons.iter_mut().enumerate() {
            let pressed = button.btn.draw(context, Some(self.background_sprite));
            ui.same_line(); // Horizontal layout.

            if pressed && pressed_button_index.is_none() {
                pressed_button_index = Some(index);
            }

            // Pressed state doesn't persist.
            button.btn.unpress();
        }

        if let Some(pressed_index) = pressed_button_index {
            match self.buttons[pressed_index].kind {
                GameSpeedControlButtonKind::Play => {
                    context.sim.resume();
                }
                GameSpeedControlButtonKind::Pause => {
                    context.sim.pause();
                }
                GameSpeedControlButtonKind::Slowdown => {
                    context.sim.resume();
                    context.sim.slowdown();
                }
                GameSpeedControlButtonKind::Speedup => {
                    context.sim.resume();
                    context.sim.speedup();
                }
            }
        }

        widgets::draw_vertical_separator(ui, 1.0, 6.0, 0.0);
        ui.same_line();

        let label = if context.sim.is_paused() {
            "Paused"
        } else {
            &format!("{:1}x", context.sim.speed())
        };

        let width = ui.calc_text_size(label)[0];
        let size = [width + 5.0, GameSpeedControlButtonKind::BUTTON_SIZE.height as f32];
        ui.button_with_size(label, size);
    }
}
