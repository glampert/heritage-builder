use std::any::Any;

use super::{
    GameMenusMode,
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
    save::{Save, Load},
    render::RenderSystem,
    app::input::{InputAction, MouseButton},
    ui::{UiInputEvent, UiTheme, widgets::UiWidgetContext},
    tile::{Tile, minimap::InGameUiMinimapRenderer},
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
    minimap_renderer: InGameUiMinimapRenderer,
}

impl InGameHudMenus {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        context.ui_sys.set_ui_theme(UiTheme::InGame);
        Self {
            tile_placement: TilePlacement::new(),
            tile_palette: TilePaletteMenu::new(context),
            tile_inspector: TileInspectorMenu::new(),
            menu_bars: MenuBarsWidget::new(context),
            minimap_renderer: InGameUiMinimapRenderer::new(context),
        }
    }
}

impl GameMenusSystem for InGameHudMenus {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn mode(&self) -> GameMenusMode {
        GameMenusMode::InGame
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
        let mut widget_context =
            UiWidgetContext::new(context.sim, context.world, context.engine);

        self.menu_bars.handle_input(&mut widget_context, args)
    }

    fn end_frame(&mut self, context: &mut GameMenusContext, _visible_range: CellRange) {
        let mut widget_context =
            UiWidgetContext::new(context.sim, context.world, context.engine);

        self.tile_palette.draw(&mut widget_context,
                               context.engine.render_system(),
                               context.cursor_screen_pos,
                               context.camera.transform(),
                               context.tile_selection.has_valid_placement());
    
        self.menu_bars.draw(&mut widget_context);

        let minimap = context.tile_map.minimap_mut();
        minimap.draw(&mut self.minimap_renderer, &mut widget_context, context.camera);
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
    fn new(context: &mut UiWidgetContext) -> Self {
        Self {
            widget: TilePaletteWidget::new(context),
            left_mouse_button_pressed: false,
        }
    }

    fn draw(&mut self,
            widget_context: &mut UiWidgetContext,
            render_sys: &mut dyn RenderSystem,
            cursor_screen_pos: Vec2,
            transform: WorldToScreenTransform,
            has_valid_placement: bool)
    {
        self.widget.draw(widget_context);
        self.widget.draw_selected_tile(render_sys, cursor_screen_pos, transform, has_valid_placement);
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

    fn clear_selection(&mut self, _context: &mut GameMenusContext) {
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
