use arrayvec::ArrayVec;
use std::{any::Any, path::PathBuf};
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    GameMenusContext,
    GameMenusInputArgs,
    modal::*,
    button::{Button, ButtonState, ButtonDef},
    widgets::{self, UiStyleOverrides, UiStyleTextLabelInvisibleButtons},
};
use crate::{
    engine::time::Seconds,
    utils::{self, Size, Rect, Vec2},
    game::{sim::Simulation, world::World},
    app::input::{InputAction, InputKey},
    imgui_ui::{UiSystem, UiTextureHandle, UiInputEvent},
    render::{TextureCache, TextureSettings, TextureFilter},
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
    pub fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Self {
        Self {
            top_bar: TopBar::new(tex_cache, ui_sys),
            left_bar: LeftBar::new(tex_cache, ui_sys),
            game_speed_controls_bar: GameSpeedControlsBar::new(tex_cache, ui_sys),
        }
    }

    pub fn handle_input(&mut self, context: &mut GameMenusContext, args: GameMenusInputArgs) -> UiInputEvent {
        self.left_bar.handle_input(context, args)
    }

    pub fn draw(&mut self, sim: &mut Simulation, world: &World, ui_sys: &UiSystem, delta_time_secs: Seconds) {
        let ui = ui_sys.ui();

        const HORIZONTAL_SPACING: f32 = 2.0;
        const VERTICAL_SPACING: f32 = 4.0;

        let _style_overrides =
            UiStyleOverrides::in_game_hud_menus(ui_sys);

        let _item_spacing =
            UiStyleOverrides::set_item_spacing(ui_sys, HORIZONTAL_SPACING, VERTICAL_SPACING);

        // Center top bar to the middle of the display:
        widgets::set_next_window_pos(
            Vec2::new(ui.io().display_size[0] * 0.5, 0.0),
            Vec2::new(0.5, 0.0),
            imgui::Condition::Always
        );
        Self::draw_bar_widget(ui_sys,
                              None, // Handled by set_next_window_pos instead.
                              "Menu Bar Widget Top",
                              || self.top_bar.draw(sim, world, ui_sys, delta_time_secs));

        // Left-hand-side vertical bar:
        Self::draw_bar_widget(ui_sys,
                              Some([0.0, 60.0]),
                              "Menu Bar Widget Left",
                              || self.left_bar.draw(sim, world, ui_sys, delta_time_secs));

        // Game speed controls horizontal top bar:
        Self::draw_bar_widget(ui_sys,
                              Some([0.0, 0.0]),
                              "Menu Bar Widget Speed Ctrls",
                              || self.game_speed_controls_bar.draw(sim, world, ui_sys, delta_time_secs));
    }

    fn draw_bar_widget<F>(ui_sys: &UiSystem,
                          position: Option<[f32; 2]>,
                          name: &str,
                          f: F)
        where F: FnOnce()
    {
        let pos_cond = if position.is_some() { imgui::Condition::Always } else { imgui::Condition::Never };
        ui_sys.ui().window(name)
            .position(position.unwrap_or([0.0, 0.0]), pos_cond)
            .flags(widgets::window_flags())
            .build(|| {
                f();
                widgets::draw_current_window_debug_rect(ui_sys.ui());
            });
    }
}

// ----------------------------------------------
// MenuBar
// ----------------------------------------------

pub trait MenuBar: Any {
    fn as_any(&self) -> &dyn Any;

    fn draw(&mut self, sim: &mut Simulation, world: &World, ui_sys: &UiSystem, delta_time_secs: Seconds);
    fn handle_input(&mut self, _context: &mut GameMenusContext, _args: GameMenusInputArgs) -> UiInputEvent {
        UiInputEvent::NotHandled
    }

    fn open_modal_menu(&mut self, _sim: &mut Simulation, _menu_id: ModalMenuId) -> Option<&mut dyn ModalMenu> { None }
    fn close_modal_menu(&mut self, _sim: &mut Simulation, _menu_id: ModalMenuId) {}
    fn close_all_modal_menus(&mut self, _sim: &mut Simulation) -> bool { false }
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
        super::ui_assets_path()
            .join("icons")
            .join(sprite_name)
            .with_extension("png")
    }

    fn load_texture(self, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> UiTextureHandle {
        let settings = TextureSettings {
            filter: TextureFilter::Linear,
            gen_mipmaps: false,
            ..Default::default()
        };

        let sprite_path = self.asset_path();
        let tex_handle = tex_cache.load_texture_with_settings(sprite_path.to_str().unwrap(), Some(settings));
        ui_sys.to_ui_texture(tex_cache, tex_handle)
    }
}

struct TopBar {
    icon_textures: ArrayVec<UiTextureHandle, TOP_BAR_ICON_COUNT>,
}

impl TopBar {
    fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Box<Self> {
        let mut icon_textures = ArrayVec::new();
        for icon in TopBarIcon::iter() {
            icon_textures.push(icon.load_texture(tex_cache, ui_sys));
        }
        Box::new(Self { icon_textures })
    }
}

impl MenuBar for TopBar {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, _sim: &mut Simulation, world: &World, ui_sys: &UiSystem, _delta_time_secs: Seconds) {
        let ui = ui_sys.ui();
        let draw_list = ui.get_window_draw_list();

        // Spacing is handled manually with dummy items.
        let _item_spacing =
            UiStyleOverrides::set_item_spacing(ui_sys, 0.0, 0.0);

        // We'll use buttons for text labels, for straightforward text centering and layout.
        let _btn_style_overrides =
            UiStyleTextLabelInvisibleButtons::apply_overrides(ui_sys);

        // Population:
        {
            let icon_size = TopBarIcon::Population.size();
            let icon_texture = self.icon_textures[TopBarIcon::Population as usize];

            widgets::draw_sprite(ui, &draw_list, "Population", icon_size, icon_texture, Some("Population"));
            ui.same_line(); // Horizontal layout.

            let population = world.stats().population.total;
            widgets::draw_centered_text_label(ui, &draw_list, &population.to_string(), Vec2::new(50.0, icon_size.y));
            ui.same_line();

            widgets::spacing(ui, &draw_list, Vec2::new(30.0, 0.0));
            ui.same_line();
        }

        // Player Icon:
        {
            let icon_size = TopBarIcon::Player.size();
            let icon_texture = self.icon_textures[TopBarIcon::Player as usize];

            widgets::spacing(ui, &draw_list, Vec2::new(icon_size.x, 0.0));
            ui.same_line();

            let icon_rect = Rect::from_extents(
                Vec2::new(ui.item_rect_min()[0], 0.0),
                Vec2::new(ui.item_rect_max()[0], icon_size.y)
            );

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

            widgets::spacing(ui, &draw_list, Vec2::new(30.0, 0.0));
            ui.same_line();
        }

        // Gold:
        {
            let icon_size = TopBarIcon::Gold.size();
            let icon_texture = self.icon_textures[TopBarIcon::Gold as usize];

            widgets::draw_sprite(ui, &draw_list, "Gold", icon_size, icon_texture, Some("Gold"));
            ui.same_line(); // Horizontal layout.

            let gold_units_total = world.stats().treasury.gold_units_total;
            widgets::draw_centered_text_label(ui, &draw_list, &gold_units_total.to_string(), Vec2::new(50.0, icon_size.y));
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

    fn new_button(self, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> LeftBarButton {
        LeftBarButton {
            btn: Button::new(
                tex_cache,
                ui_sys,
                ButtonDef {
                    name: self.sprite_path(),
                    size: Self::BUTTON_SIZE,
                    tooltip: Some(self.tooltip()),
                    show_tooltip_when_pressed: true,
                    state_transition_secs: 0.5,
                },
                ButtonState::Idle,
            ),
            kind: self,
        }
    }

    fn create_all(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem)
                  -> ArrayVec<LeftBarButton, LEFT_BAR_BUTTON_COUNT>
    {
        let mut buttons = ArrayVec::new();
        for btn_kind in Self::iter() {
            buttons.push(btn_kind.new_button(tex_cache, ui_sys));
        }
        buttons
    }
}

struct LeftBarButton {
    btn: Button,
    kind: LeftBarButtonKind,
}

struct LeftBar {
    buttons: ArrayVec<LeftBarButton, LEFT_BAR_BUTTON_COUNT>,
    modal_menus: ArrayVec<Box<dyn ModalMenu>, LEFT_BAR_BUTTON_COUNT>,
}

impl LeftBar {
    fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Box<Self> {
        let mut left_bar = Box::new(Self {
            buttons: LeftBarButtonKind::create_all(tex_cache, ui_sys),
            modal_menus: ArrayVec::new(),
        });

        for button_kind in LeftBarButtonKind::iter() {
            match button_kind {
                LeftBarButtonKind::MainMenu =>{
                    left_bar.modal_menus.push(
                        Box::new(
                            MainModalMenu::new(LeftBarButtonKind::MainMenu.tooltip(), left_bar.as_ref())
                        )
                    )
                }
                LeftBarButtonKind::SaveGame => {
                    left_bar.modal_menus.push(
                        Box::new(
                            SaveGameModalMenu::new(LeftBarButtonKind::SaveGame.tooltip(), left_bar.as_ref())
                        )
                    )
                }
                LeftBarButtonKind::Settings => {
                    left_bar.modal_menus.push(
                        Box::new(
                            SettingsModalMenu::new(LeftBarButtonKind::Settings.tooltip(), left_bar.as_ref())
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

    fn draw(&mut self, sim: &mut Simulation, _world: &World, ui_sys: &UiSystem, delta_time_secs: Seconds) {
        // Each button is associated with a modal menu.
        debug_assert!(self.buttons.len() == self.modal_menus.len());

        let mut pressed_button_index: Option<usize> = None;

        for (index, button) in self.buttons.iter_mut().enumerate() {
            let pressed = button.btn.draw(ui_sys, delta_time_secs);

            if pressed && pressed_button_index.is_none() {
                pressed_button_index = Some(index);
            }

            // Pressed state doesn't persist.
            button.btn.unpress();
        }

        if let Some(pressed_index) = pressed_button_index {
            // Ensure any other menus are closed first.
            self.close_all_modal_menus(sim);
            // Open new menu.
            self.modal_menus[pressed_index].open(sim);
        }

        // Draw open modal menus.
        let mut any_modal_menu_open = false;
        for modal in &mut self.modal_menus {
            modal.draw(sim, ui_sys);
            any_modal_menu_open |= modal.is_open();
        }

        if any_modal_menu_open {
            sim.pause();
        }
    }

    fn handle_input(&mut self, context: &mut GameMenusContext, args: GameMenusInputArgs) -> UiInputEvent {
        if let GameMenusInputArgs::Key { key, action, .. } = args {
            if action == InputAction::Press && key == InputKey::Escape {
                // Close all modal menus and return to game.
                if self.close_all_modal_menus(context.sim) {
                    // Handled the key press.
                    return UiInputEvent::Handled;
                }
            }
        }

        // Let the event propagate.
        UiInputEvent::NotHandled
    }

    fn open_modal_menu(&mut self, sim: &mut Simulation, menu_id: ModalMenuId) -> Option<&mut dyn ModalMenu> {
        // Only one modal menu open at a time.
        self.close_all_modal_menus(sim);

        for modal in &mut self.modal_menus {
            if modal.id() == menu_id {
                modal.open(sim);
                return Some(modal.as_mut());
            }
        }

        panic!("Modal Menu not found!");
    }

    fn close_modal_menu(&mut self, sim: &mut Simulation, menu_id: ModalMenuId) {
        for modal in &mut self.modal_menus {
            if modal.id() == menu_id {
                modal.close(sim);
                return;
            }
        }

        panic!("Modal Menu not found!");
    }

    fn close_all_modal_menus(&mut self, sim: &mut Simulation) -> bool {
        let mut any_modal_menu_open = false;
        for modal in &mut self.modal_menus {
            if modal.is_open() {
                modal.close(sim);
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

    fn new_button(self, tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> GameSpeedControlButton {
        GameSpeedControlButton {
            btn: Button::new(
                tex_cache,
                ui_sys,
                ButtonDef {
                    name: self.sprite_path(),
                    size: Self::BUTTON_SIZE,
                    tooltip: Some(self.tooltip()),
                    show_tooltip_when_pressed: true,
                    state_transition_secs: 0.5,
                },
                ButtonState::Idle,
            ),
            kind: self,
        }
    }

    fn create_all(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem)
                  -> ArrayVec<GameSpeedControlButton, GAME_SPEED_CONTROLS_BUTTON_COUNT>
    {
        let mut buttons = ArrayVec::new();
        for btn_kind in Self::iter() {
            buttons.push(btn_kind.new_button(tex_cache, ui_sys));
        }
        buttons
    }
}

struct GameSpeedControlButton {
    btn: Button,
    kind: GameSpeedControlButtonKind,
}

struct GameSpeedControlsBar {
    buttons: ArrayVec<GameSpeedControlButton, GAME_SPEED_CONTROLS_BUTTON_COUNT>,
}

impl GameSpeedControlsBar {
    fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Box<Self> {
        Box::new(Self { buttons: GameSpeedControlButtonKind::create_all(tex_cache, ui_sys) })
    }
}

impl MenuBar for GameSpeedControlsBar {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, sim: &mut Simulation, _world: &World, ui_sys: &UiSystem, delta_time_secs: Seconds) {
        // We'll use buttons for text labels, for straightforward text centering and layout.
        let _btn_style_overrides =
            UiStyleTextLabelInvisibleButtons::apply_overrides(ui_sys);

        let ui = ui_sys.ui();
        let mut pressed_button_index: Option<usize> = None;

        for (index, button) in self.buttons.iter_mut().enumerate() {
            let pressed = button.btn.draw(ui_sys, delta_time_secs);
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
                    sim.resume();
                }
                GameSpeedControlButtonKind::Pause => {
                    sim.pause();
                }
                GameSpeedControlButtonKind::Slowdown => {
                    sim.resume();
                    sim.slowdown();
                }
                GameSpeedControlButtonKind::Speedup => {
                    sim.resume();
                    sim.speedup();
                }
            }
        }

        widgets::draw_vertical_separator(ui, &ui.get_window_draw_list(), 1.0, 6.0, 0.0);
        ui.same_line();

        let label = if sim.is_paused() {
            "Paused"
        } else {
            &format!("{:1}x", sim.speed())
        };

        let width = ui.calc_text_size(label)[0];
        let size = [width + 5.0, GameSpeedControlButtonKind::BUTTON_SIZE.height as f32];
        ui.button_with_size(label, size);
    }
}
