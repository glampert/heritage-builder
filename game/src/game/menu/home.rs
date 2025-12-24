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
    modal::{BasicModalMenu, ModalMenuParams},
    widgets::{self, UiStyleOverrides, UiStyleTextLabelInvisibleButtons},
};
use crate::{
    save::{Save, Load},
    game::sim::Simulation,
    render::TextureCache,
    tile::rendering::TileMapRenderFlags,
    utils::{Size, Rect, Vec2, coords::CellRange},
    imgui_ui::{UiSystem, UiInputEvent, UiTextureHandle},
};

// ----------------------------------------------
// HomeMenus
// ----------------------------------------------

pub struct HomeMenus {
    main_menu: MainMenu,
    background: FullScreenBackground,
}

impl HomeMenus {
    pub fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem, viewport_size: Size) -> Self {
        Self {
            main_menu: MainMenu::new(tex_cache, ui_sys, viewport_size),
            background: FullScreenBackground::new(tex_cache, ui_sys),
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

        self.main_menu.draw(context.sim, ui_sys);
        self.background.draw(ui_sys);
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
// MainMenu
// ----------------------------------------------

const MAIN_MENU_BUTTON_COUNT: usize = MainMenuButton::COUNT;

#[repr(usize)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, TryFromPrimitive, EnumCount, EnumProperty, EnumIter)]
enum MainMenuButton {
    #[strum(props(Label = "NEW GAME"))]
    NewGame,

    #[strum(props(Label = "CONTINUE"))]
    ContinueGame,

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

impl MainMenuButton {
    fn label(self) -> &'static str {
        self.get_str("Label").unwrap()
    }
}

struct MainMenu {
    menu: BasicModalMenu,
    separator_ui_texture: UiTextureHandle,
}

impl MainMenu {
    fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem, viewport_size: Size) -> Self {
        let separator_tex_handle = tex_cache.load_texture_with_settings(
            super::ui_assets_path().join("misc/brush_stroke_divider.png").to_str().unwrap(),
            Some(super::ui_texture_settings())
        );
        Self {
            menu: BasicModalMenu::new(
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
            separator_ui_texture: ui_sys.to_ui_texture(tex_cache, separator_tex_handle),
        }
    }

    fn draw(&mut self, sim: &mut Simulation, ui_sys: &UiSystem) {
        // Make button background transparent and borderless.
        let _button_style_overrides =
            UiStyleTextLabelInvisibleButtons::apply_overrides(ui_sys);

        const BUTTON_SPACING: Vec2 = Vec2::new(6.0, 6.0);
        let _item_spacing =
            UiStyleOverrides::set_item_spacing(ui_sys, BUTTON_SPACING.x, BUTTON_SPACING.y);

        self.menu.draw(sim, ui_sys, |_sim| {
            let ui = ui_sys.ui();
            let window_draw_list = ui.get_window_draw_list();

            // Bigger font for the heading.
            ui.set_window_font_scale(1.8);
            // Draw heading as buttons, so everything is properly centered.
            widgets::draw_centered_button_group_ex::<fn(&imgui::Ui, &imgui::DrawListMut<'_>, usize)>(
                ui,
                &window_draw_list,
                &["Heritage Builder", "The Dragon Legacy"],
                None,
                Some(Vec2::new(0.0, -200.0)),
                None,
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

            let mut button_labels = ArrayVec::<&str, MAIN_MENU_BUTTON_COUNT>::new();
            for button in MainMenuButton::iter() {
                button_labels.push(button.label());
            }

            // Bigger font for the buttons.
            ui.set_window_font_scale(1.5);
            // Draw actual menu buttons:
            widgets::draw_centered_button_group_ex(
                ui,
                &window_draw_list,
                &button_labels,
                Some(Size::new(180, 40)),
                Some(Vec2::new(0.0, 150.0)),
                Some(|ui: &imgui::Ui, draw_list: &imgui::DrawListMut<'_>, _button_index: usize| {
                    // Draw underline effect when hovered / active:
                    let button_rect = Rect::from_extents(
                        Vec2::from_array(ui.item_rect_min()),
                        Vec2::from_array(ui.item_rect_max())
                    ).translated(Vec2::new(0.0, 20.0));

                    let underline_tint_color = if ui.is_item_active() {
                        imgui::ImColor32::from_rgba_f32s(1.0, 1.0, 1.0, 0.5)
                    } else {
                        imgui::ImColor32::WHITE
                    };

                    draw_list.add_image(self.separator_ui_texture,
                                        button_rect.min.to_array(),
                                        button_rect.max.to_array())
                                        .col(underline_tint_color)
                                        .build();
                })
            );

            // TODO: Handle button clicks.
        });
    }
}

// ----------------------------------------------
// FullScreenBackground
// ----------------------------------------------

struct FullScreenBackground {
    ui_texture: UiTextureHandle,
}

impl FullScreenBackground {
    fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Self {
        let bg_tex_handle = tex_cache.load_texture_with_settings(
            super::ui_assets_path().join("misc/home_menu_bg.png").to_str().unwrap(),
            Some(super::ui_texture_settings())
        );
        Self {
            ui_texture: ui_sys.to_ui_texture(tex_cache, bg_tex_handle),
        }
    }

    fn draw(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        let draw_list = ui.get_background_draw_list();

        // Draw full-screen rectangle with the background image:
        draw_list.add_image(self.ui_texture, [0.0, 0.0], ui.io().display_size).build();
    }
}
