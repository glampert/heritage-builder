use arrayvec::ArrayVec;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    widgets::{self, Button, ButtonState, ButtonDef, UiStyleOverrides},
};
use crate::{
    imgui_ui::UiSystem,
    render::TextureCache,
    utils::{self, Size},
};

// ----------------------------------------------
// MenuBarWidget
// ----------------------------------------------

pub struct MenuBarWidget {
    left_bar_buttons: LeftBarButtons,
    game_speed_control_buttons: GameSpeedControlButtons,
}

impl MenuBarWidget {
    pub fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Self {
        Self {
            left_bar_buttons: LeftBarButtons::new(tex_cache, ui_sys),
            game_speed_control_buttons: GameSpeedControlButtons::new(tex_cache, ui_sys),
        }
    }

    pub fn draw(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        const HORIZONTAL_SPACING: f32 = 2.0;
        const VERTICAL_SPACING: f32 = 4.0;

        let _style_overrides =
            UiStyleOverrides::in_game_hud_menus(ui_sys);

        let _item_spacing =
            UiStyleOverrides::set_item_spacing(ui_sys, HORIZONTAL_SPACING, VERTICAL_SPACING);

        // Screen-centered horizontal top bar:
        let top_bar_width = 500.0;
        let top_bar_position = [
            (ui.io().display_size[0] * 0.5) - (top_bar_width * 0.5),
            0.0
        ];
        Self::draw_bar_widget(ui_sys,
                              top_bar_position,
                              "Menu Bar Widget Top",
                              |_ui_sys| {
                                  // TODO
                              });

        // Left-hand-side vertical bar:
        Self::draw_bar_widget(ui_sys,
                              [0.0, 60.0],
                              "Menu Bar Widget Left",
                              |ui_sys| {
                                  self.left_bar_buttons.draw(ui_sys);
                              });

        // Game speed controls horizontal top bar:
        Self::draw_bar_widget(ui_sys,
                              [0.0, 0.0],
                              "Menu Bar Widget Speed Ctrls",
                              |ui_sys| {
                                  self.game_speed_control_buttons.draw(ui_sys);
                              });
    }

    fn draw_bar_widget<F>(ui_sys: &UiSystem,
                          position: [f32; 2],
                          name: &str,
                          builder: F)
        where F: FnOnce(&UiSystem)
    {
        ui_sys.ui().window(name)
            .position(position, imgui::Condition::Always)
            .flags(widgets::invisible_window_flags())
            .build(|| {
                builder(ui_sys);
                if widgets::is_debug_draw_enabled() {
                    widgets::draw_current_window_debug_rect(ui_sys.ui());
                }
            });
    }
}

// ----------------------------------------------
// LeftBarButtons
// ----------------------------------------------

const LEFT_BAR_BUTTON_COUNT: usize = LeftBarButtonKind::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum LeftBarButtonKind {
    #[strum(props(Sprite = "menu_bar/save_game"))]
    SaveGame,

    #[strum(props(Sprite = "menu_bar/settings"))]
    Settings,

    #[strum(props(Sprite = "menu_bar/mission_info"))]
    MissionInfo,
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
                    tooltip: Some(self.tooltip())
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

struct LeftBarButtons {
    buttons: ArrayVec<LeftBarButton, LEFT_BAR_BUTTON_COUNT>,
}

impl LeftBarButtons {
    fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Self {
        Self { buttons: LeftBarButtonKind::create_all(tex_cache, ui_sys) }
    }

    fn draw(&mut self, ui_sys: &UiSystem) {
        for button in &mut self.buttons {
            button.btn.draw(ui_sys);
        }
    }
}

// ----------------------------------------------
// GameSpeedControlButtons
// ----------------------------------------------

const GAME_SPEED_CONTROL_BUTTON_COUNT: usize = GameSpeedControlButtonKind::COUNT;

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
    const BUTTON_SIZE: Size = Size::new(18, 18);

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
                    tooltip: Some(self.tooltip())
                },
                ButtonState::Idle,
            ),
            kind: self,
        }
    }

    fn create_all(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem)
                  -> ArrayVec<GameSpeedControlButton, GAME_SPEED_CONTROL_BUTTON_COUNT>
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

struct GameSpeedControlButtons {
    buttons: ArrayVec<GameSpeedControlButton, GAME_SPEED_CONTROL_BUTTON_COUNT>,
}

impl GameSpeedControlButtons {
    fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Self {
        Self { buttons: GameSpeedControlButtonKind::create_all(tex_cache, ui_sys) }
    }

    fn draw(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        for button in &mut self.buttons {
            button.btn.draw(ui_sys);
            ui.same_line(); // Horizontal layout.
        }

        widgets::draw_vertical_separator(ui, 1.0, 6.0);
        ui.same_line();
        ui.text("Paused");
    }
}
