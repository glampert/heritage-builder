use std::any::Any;

use inspector::TileInspectorMenu;
use palette::TilePaletteMenu;
use bars::InGameMenuBars;

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
    tile::minimap::InGameUiMinimapRenderer,
    ui::{UiInputEvent, UiTheme, widgets::UiWidgetContext},
};

mod inspector;
mod palette;
mod bars;

// ----------------------------------------------
// InGameMenus
// ----------------------------------------------

pub struct InGameMenus {
    tile_placement: TilePlacement,
    tile_palette: TilePaletteMenu,
    tile_inspector: TileInspectorMenu,
    menu_bars: InGameMenuBars,
    minimap_renderer: InGameUiMinimapRenderer,
}

impl InGameMenus {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        context.ui_sys.set_ui_theme(UiTheme::InGame);
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
        Some(&mut self.tile_palette)
    }

    fn tile_inspector(&mut self) -> Option<&mut dyn TileInspector> {
        Some(&mut self.tile_inspector)
    }

    fn handle_custom_input(&mut self, context: &mut GameMenusContext, args: GameMenusInputArgs) -> UiInputEvent {
        let mut ui_context = UiWidgetContext::new(
            context.sim,
            context.world,
            context.engine
        );
        self.menu_bars.handle_input(&mut ui_context, args)
    }

    fn end_frame(&mut self, context: &mut GameMenusContext, _visible_range: CellRange) {
        let mut ui_context = UiWidgetContext::new(
            context.sim,
            context.world,
            context.engine
        );

        self.tile_palette.draw(&mut ui_context,
                               context.camera.transform(),
                               context.tile_selection.has_valid_placement());

        self.menu_bars.draw(&mut ui_context);

        let minimap = context.tile_map.minimap_mut();
        minimap.draw(&mut self.minimap_renderer, &mut ui_context, context.camera);
    }
}

// ----------------------------------------------
// Save/Load for InGameMenus
// ----------------------------------------------

impl Save for InGameMenus {}
impl Load for InGameMenus {}
