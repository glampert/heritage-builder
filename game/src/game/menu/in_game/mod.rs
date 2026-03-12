use std::any::Any;

use inspector::{TileInspectorMenu, TileInspectorMenuRcMut};
use palette::{TilePaletteMenu, TilePaletteMenuRcMut};
use bars::{InGameMenuBars, InGameMenuBarsRcMut};

use super::{
    GameMenusMode,
    GameMenusSystem,
    GameMenusInputArgs,
    TilePlacement,
    TileInspector,
    TilePalette,
    dialog,
};
use crate::{
    utils::coords::CellRange,
    save::{Save, Load, PreLoadContext},
    app::input::{InputAction, InputKey},
    tile::minimap::{MinimapRenderer, InGameUiMinimapRenderer},
    ui::{UiInputEvent, UiTheme, widgets::{UiWidgetContext, UiMenuFlags}},
};

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
        dialog::set_global_menu_flags(UiMenuFlags::AlignCenter);
        dialog::set_bg_dim_alpha(context, 0.15);

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

    fn handle_custom_input(&mut self, context: &mut UiWidgetContext, args: GameMenusInputArgs) -> UiInputEvent {
        if let GameMenusInputArgs::Key { key, action, .. } = args {
            // [ESCAPE]: Close all dialog menus and return to game.
            if key == InputKey::Escape && action == InputAction::Press {
                if dialog::close_current(context) {
                    return UiInputEvent::Handled; // Key press is handled.
                }
            }
        }

        UiInputEvent::NotHandled // Let the event propagate.
    }

    fn end_frame(&mut self, context: &mut UiWidgetContext, _visible_range: CellRange) {
        self.minimap_renderer.draw(context);
        self.tile_palette.draw(context);
        self.menu_bars.draw(context);
        self.tile_inspector.draw(context);
        dialog::draw_current(context);
    }
}

// ----------------------------------------------
// Drop for InGameMenus
// ----------------------------------------------

impl Drop for InGameMenus {
    fn drop(&mut self) {
        dialog::reset();
    }
}

// ----------------------------------------------
// Save/Load for InGameMenus
// ----------------------------------------------

impl Save for InGameMenus {}
impl Load for InGameMenus {
    fn pre_load(&mut self, _context: &PreLoadContext) {
        dialog::reset();
    }
}
