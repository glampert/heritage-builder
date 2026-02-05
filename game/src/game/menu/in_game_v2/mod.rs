use std::any::Any;

use inspector::{TileInspectorMenu, TileInspectorMenuRcMut};
use palette::{TilePaletteMenu, TilePaletteMenuRcMut};
use bars::{InGameMenuBars, InGameMenuBarsRcMut};

use super::{
    GameMenusMode,
    GameMenusSystem,
    GameMenusContext,
    GameMenusInputArgs,
    TilePlacement,
    TileInspector,
    TilePalette,
};
use crate::{
    save::{Save, Load},
    utils::coords::CellRange,
    app::input::{InputAction, InputKey},
    tile::minimap::InGameUiMinimapRenderer,
    ui::{UiInputEvent, UiTheme, widgets::UiWidgetContext},
};

mod dialog;
mod bars;
mod inspector;
mod palette;

// ----------------------------------------------
// InGameMenus
// ----------------------------------------------

pub struct InGameMenus {
    tile_placement: TilePlacement,
    tile_palette: TilePaletteMenuRcMut,
    tile_inspector: TileInspectorMenuRcMut,
    menu_bars: InGameMenuBarsRcMut,
    minimap_renderer: InGameUiMinimapRenderer,
}

impl InGameMenus {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        context.ui_sys.set_ui_theme(UiTheme::InGame);
        dialog::initialize(context);

        Self {
            tile_placement: TilePlacement::new(),
            tile_palette: TilePaletteMenu::new(context),
            tile_inspector: TileInspectorMenu::new(context),
            menu_bars: InGameMenuBars::new(context),
            minimap_renderer: InGameUiMinimapRenderer::new(context),
        }
    }
}

impl GameMenusSystem for InGameMenus {
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
        Some(self.tile_palette.as_mut())
    }

    fn tile_inspector(&mut self) -> Option<&mut dyn TileInspector> {
        Some(self.tile_inspector.as_mut())
    }

    fn handle_custom_input(&mut self, context: &mut GameMenusContext, args: GameMenusInputArgs) -> UiInputEvent {
        if let GameMenusInputArgs::Key { key, action, .. } = args {
            // [ESCAPE]: Close all dialog menus and return to game.
            if key == InputKey::Escape && action == InputAction::Press {
                let mut ui_context = UiWidgetContext::new(
                    context.sim,
                    context.world,
                    context.engine
                );

                if dialog::close_current(&mut ui_context) {
                    return UiInputEvent::Handled; // Key press is handled.
                }
            }
        }

        UiInputEvent::NotHandled // Let the event propagate.
    }

    fn end_frame(&mut self, context: &mut GameMenusContext, _visible_range: CellRange) {
        let mut ui_context = UiWidgetContext::new(
            context.sim,
            context.world,
            context.engine
        );

        self.tile_inspector.draw(&mut ui_context);

        self.tile_palette.draw(&mut ui_context,
                               context.camera.transform(),
                               context.tile_selection.has_valid_placement());

        self.menu_bars.draw(&mut ui_context);

        let minimap = context.tile_map.minimap_mut();
        minimap.draw(&mut self.minimap_renderer, &mut ui_context, context.camera);

        dialog::draw_current(&mut ui_context);
    }
}

// ----------------------------------------------
// Save/Load for InGameMenus
// ----------------------------------------------

impl Save for InGameMenus {}
impl Load for InGameMenus {}
