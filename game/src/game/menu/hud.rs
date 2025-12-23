use std::any::Any;

use super::{
    GameMenuMode,
    GameMenusSystem,
    GameMenusContext,
    GameMenusInputArgs,
    TilePlacement,
    TileInspector,
    TilePalette,
    TilePaletteSelection,
    palette::TilePaletteWidget,
    bar::MenuBarsWidget,
};
use crate::{
    tile::Tile,
    save::{Save, Load},
    render::TextureCache,
    engine::{Engine, time::Seconds},
    imgui_ui::{UiSystem, UiInputEvent},
    app::input::{InputAction, MouseButton},
    utils::{Vec2, coords::{CellRange, WorldToScreenTransform}},
};

// ----------------------------------------------
// HUD -> Heads Up Display, AKA in-game menus
// ----------------------------------------------

pub struct InGameHudMenus {
    tile_placement: TilePlacement,
    tile_palette: TilePaletteMenu,
    tile_inspector: TileInspectorMenu,
    menu_bars: MenuBarsWidget,
}

impl InGameHudMenus {
    pub fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Self {
        Self {
            tile_placement: TilePlacement::new(),
            tile_palette: TilePaletteMenu::new(tex_cache, ui_sys),
            tile_inspector: TileInspectorMenu::new(),
            menu_bars: MenuBarsWidget::new(tex_cache, ui_sys),
        }
    }
}

impl GameMenusSystem for InGameHudMenus {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn mode(&self) -> GameMenuMode {
        GameMenuMode::InGameHud
    }

    fn tile_placement(&mut self) -> Option<&mut TilePlacement> {
        Some(&mut self.tile_placement)
    }

    fn tile_palette(&mut self) -> Option<&mut dyn TilePalette> {
        Some(&mut self.tile_palette)
    }

    fn tile_inspector(&mut self) -> Option<&mut dyn TileInspector> {
        Some(&mut self.tile_inspector)
    }

    fn handle_custom_input(&mut self, context: &mut GameMenusContext, args: GameMenusInputArgs) -> UiInputEvent {
        self.menu_bars.handle_input(context, args)
    }

    fn end_frame(&mut self, context: &mut GameMenusContext, _visible_range: CellRange) {
        self.tile_palette.draw(context.engine,
                               context.cursor_screen_pos,
                               context.camera.transform(),
                               context.tile_selection.has_valid_placement(),
                               context.delta_time_secs);

        self.menu_bars.draw(context.sim,
                            context.world,
                            context.engine.ui_system(),
                            context.delta_time_secs);
    }
}

// ----------------------------------------------
// Save/Load for InGameHudMenus
// ----------------------------------------------

impl Save for InGameHudMenus {}
impl Load for InGameHudMenus {}

// ----------------------------------------------
// TilePaletteMenu
// ----------------------------------------------

struct TilePaletteMenu {
    widget: TilePaletteWidget,
    left_mouse_button_pressed: bool,
}

impl TilePaletteMenu {
    fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Self {
        Self {
            widget: TilePaletteWidget::new(tex_cache, ui_sys),
            left_mouse_button_pressed: false,
        }
    }

    fn draw(&mut self,
            engine: &mut dyn Engine,
            cursor_screen_pos: Vec2,
            transform: WorldToScreenTransform,
            has_valid_placement: bool,
            delta_time_secs: Seconds) {
        self.widget.draw(engine.ui_system(), delta_time_secs);
        self.widget.draw_selected_tile(engine.render_system(), cursor_screen_pos, transform, has_valid_placement);
    }
}

impl TilePalette for TilePaletteMenu {
    fn on_mouse_button(&mut self, button: MouseButton, action: InputAction) -> UiInputEvent {
        if button == MouseButton::Left {
            if action == InputAction::Press {
                self.left_mouse_button_pressed = true;
            } else if action == InputAction::Release {
                self.left_mouse_button_pressed = false;
            }
            UiInputEvent::Handled
        } else {
            UiInputEvent::NotHandled
        }
    }

    fn wants_to_place_or_clear_tile(&self) -> bool {
        self.left_mouse_button_pressed && self.has_selection()
    }

    fn current_selection(&self) -> TilePaletteSelection {
        self.widget.current_selection
    }

    fn clear_selection(&mut self) {
        self.widget.clear_selection();
        self.left_mouse_button_pressed = false;
    }
}

// ----------------------------------------------
// TileInspectorMenu
// ----------------------------------------------

struct TileInspectorMenu {
    // TODO / WIP
}

impl TileInspectorMenu {
    fn new() -> Self {
        Self {}
    }
}

impl TileInspector for TileInspectorMenu {
    fn on_mouse_button(&mut self,
                       _button: MouseButton,
                       _action: InputAction,
                       _selected_tile: &Tile)
                       -> UiInputEvent {
        UiInputEvent::NotHandled
    }

    fn close(&mut self) {
    }
}
