use std::path::PathBuf;
use arrayvec::ArrayVec;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    widgets::{self, Button, ButtonState, ButtonDef, UiStyleOverrides},
};
use crate::{
    utils::{self, Size, Color, Rect, Vec2},
    imgui_ui::{UiSystem, UiTextureHandle},
    render::{TextureCache, TextureSettings, TextureFilter},
};

// ----------------------------------------------
// MenuBarWidget
// ----------------------------------------------

pub struct MenuBarWidget {
    top_bar: TopBar,
    left_bar: LeftBar,
    game_speed_controls: GameSpeedControls,
}

impl MenuBarWidget {
    pub fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Self {
        Self {
            top_bar: TopBar::new(tex_cache, ui_sys),
            left_bar: LeftBar::new(tex_cache, ui_sys),
            game_speed_controls: GameSpeedControls::new(tex_cache, ui_sys),
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

        // We use buttons for text items so that the text label is centered automatically.
        // Make all button backgrounds and frames transparent/invisible.
        let _border = ui.push_style_var(imgui::StyleVar::FrameBorderSize(0.0));
        let _btn_color = ui.push_style_color(imgui::StyleColor::Button, [0.0, 0.0, 0.0, 0.0]);
        let _btn_active = ui.push_style_color(imgui::StyleColor::ButtonActive, [0.0, 0.0, 0.0, 0.0]);
        let _btn_hovered = ui.push_style_color(imgui::StyleColor::ButtonHovered, [0.0, 0.0, 0.0, 0.0]);

        // Center top bar to the middle of the display:
        widgets::set_next_window_pos(
            Vec2::new(ui.io().display_size[0] * 0.5, 0.0),
            Vec2::new(0.5, 0.0),
            imgui::Condition::Always
        );
        Self::draw_bar_widget(ui_sys,
                              None, // Handled by set_next_window_pos instead.
                              "Menu Bar Widget Top",
                              |ui_sys| self.top_bar.draw(ui_sys));

        // Left-hand-side vertical bar:
        Self::draw_bar_widget(ui_sys,
                              Some([0.0, 60.0]),
                              "Menu Bar Widget Left",
                              |ui_sys| self.left_bar.draw(ui_sys));

        // Game speed controls horizontal top bar:
        Self::draw_bar_widget(ui_sys,
                              Some([0.0, 0.0]),
                              "Menu Bar Widget Speed Ctrls",
                              |ui_sys| self.game_speed_controls.draw(ui_sys));
    }

    fn draw_bar_widget<F>(ui_sys: &UiSystem,
                          position: Option<[f32; 2]>,
                          name: &str,
                          builder: F)
        where F: FnOnce(&UiSystem)
    {
        let pos_cond = if position.is_some() { imgui::Condition::Always } else { imgui::Condition::Never };
        ui_sys.ui().window(name)
            .position(position.unwrap_or([0.0, 0.0]), pos_cond)
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
// TopBar
// ----------------------------------------------

const TOP_BAR_ICON_COUNT: usize = TopBarIcon::COUNT;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
enum TopBarIcon {
    #[strum(props(Sprite = "population_icon", Width = 35, Height = 20))]
    Population,

    #[strum(props(Sprite = "player_icon"))]
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
    fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Self {
        let mut icon_textures = ArrayVec::new();
        for icon in TopBarIcon::iter() {
            icon_textures.push(icon.load_texture(tex_cache, ui_sys));
        }
        Self { icon_textures }
    }

    fn draw(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        let draw_list = ui.get_window_draw_list();

        // Spacing is handled manually with dummy items.
        let _item_spacing = UiStyleOverrides::set_item_spacing(ui_sys, 0.0, 0.0);

        // POPULATION:
        {
            let icon_size = TopBarIcon::Population.size().to_array();

            ui.invisible_button_flags("Population", icon_size, imgui::ButtonFlags::empty());
            widgets::draw_last_item_debug_rect(ui, &draw_list, Color::blue());
            ui.same_line(); // Horizontal layout.

            let icon_rect_min = ui.item_rect_min();
            let icon_rect_max = ui.item_rect_max();
            let icon_texture = self.icon_textures[TopBarIcon::Population as usize];

            draw_list
                .add_image(icon_texture, icon_rect_min, icon_rect_max)
                .build();

            ui.button_with_size("1000", [50.0, icon_size[1]]);
            widgets::draw_last_item_debug_rect(ui, &draw_list, Color::green());

            ui.same_line();
            ui.dummy([30.0, 0.0]);
            widgets::draw_last_item_debug_rect(ui, &draw_list, Color::yellow());
            ui.same_line();
        }

        // PLAYER ICON / NAME:
        {
            let icon_size = [45.0, 40.0];

            ui.dummy([icon_size[0], 0.0]);
            widgets::draw_last_item_debug_rect(ui, &draw_list, Color::green());
            ui.same_line();

            let icon_texture = self.icon_textures[TopBarIcon::Player as usize];
            let rect = Rect::from_extents(
                Vec2::new(ui.item_rect_min()[0], 0.0),
                Vec2::new(ui.item_rect_max()[0], icon_size[1])
            );

            let foreground_draw_list = ui.get_foreground_draw_list();
            widgets::draw_window_style_rect(
                ui,
                &foreground_draw_list,
                rect.min,
                rect.max
            );

            foreground_draw_list
                .add_image(icon_texture, rect.min.to_array(), rect.max.to_array())
                .build();

            ui.dummy([30.0, 0.0]);
            widgets::draw_last_item_debug_rect(ui, &draw_list, Color::yellow());
            ui.same_line();
        }

        // GOLD:
        {
            let icon_size = TopBarIcon::Gold.size().to_array();

            ui.invisible_button_flags("Gold", icon_size, imgui::ButtonFlags::empty());
            widgets::draw_last_item_debug_rect(ui, &draw_list, Color::blue());
            ui.same_line(); // Horizontal layout.

            let icon_rect_min = ui.item_rect_min();
            let icon_rect_max = ui.item_rect_max();
            let icon_texture = self.icon_textures[TopBarIcon::Gold as usize];

            draw_list
                .add_image(icon_texture, icon_rect_min, icon_rect_max)
                .build();

            ui.button_with_size("8888", [50.0, icon_size[1]]);
            widgets::draw_last_item_debug_rect(ui, &draw_list, Color::green());
        }
    }
}

// ----------------------------------------------
// LeftBar
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

struct LeftBar {
    buttons: ArrayVec<LeftBarButton, LEFT_BAR_BUTTON_COUNT>,
}

impl LeftBar {
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
// GameSpeedControls
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
                    tooltip: Some(self.tooltip())
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

struct GameSpeedControls {
    buttons: ArrayVec<GameSpeedControlButton, GAME_SPEED_CONTROLS_BUTTON_COUNT>,
}

impl GameSpeedControls {
    fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Self {
        Self { buttons: GameSpeedControlButtonKind::create_all(tex_cache, ui_sys) }
    }

    fn draw(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        for button in &mut self.buttons {
            button.btn.draw(ui_sys);
            ui.same_line(); // Horizontal layout.
        }

        widgets::draw_vertical_separator(ui, 1.0, 6.0, 0.0);
        ui.same_line();

        let width = ui.calc_text_size("Paused")[0];
        let size = [width + 5.0, GameSpeedControlButtonKind::BUTTON_SIZE.height as f32];
        ui.button_with_size("Paused", size);
    }
}
