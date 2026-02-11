use std::any::Any;

use super::{
    GameMenusMode,
    GameMenusSystem,
    GameMenusContext,
    GameMenusInputArgs,
    TilePlacement,
    TileInspector,
    TilePalette,
    dialog::{self, DialogMenuKind},
};
use crate::{
    utils::coords::CellRange,
    save::{Save, Load, PreLoadContext},
    app::input::{InputAction, InputKey},
    tile::rendering::TileMapRenderFlags,
    ui::{
        UiInputEvent, UiTheme,
        widgets::{
            UiWidgetContext,
            UiMenuFlags,
            UiSlideshow,
            UiSlideshowFlags,
            UiSlideshowLoopMode,
            UiSlideshowParams,
        }
    },
};

// ----------------------------------------------
// HomeMenus
// ----------------------------------------------

pub struct HomeMenus;

impl HomeMenus {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        context.ui_sys.set_ui_theme(UiTheme::InGame);

        dialog::initialize(context);
        dialog::set_global_menu_flags(UiMenuFlags::PauseSimIfOpen | UiMenuFlags::AlignCenter | UiMenuFlags::AlignLeft);

        // Main Home Menu always open.
        dialog::open(DialogMenuKind::Home, true, context);

        Self
    }
}

impl GameMenusSystem for HomeMenus {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn mode(&self) -> GameMenusMode {
        GameMenusMode::Home
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

    fn handle_input(&mut self, context: &mut GameMenusContext, args: GameMenusInputArgs) -> UiInputEvent {
        if let GameMenusInputArgs::Key { key, action, .. } = args {
            // [ESCAPE]: Close child dialog menu.
            if key == InputKey::Escape && action == InputAction::Press {
                // Close if we're not already at the Main Home Menu.
                if dialog::current().is_some_and(|dialog| dialog != DialogMenuKind::Home) {
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
        }

        UiInputEvent::NotHandled // Let the event propagate.
    }

    fn end_frame(&mut self, context: &mut GameMenusContext, _visible_range: CellRange) {
        let mut ui_context = UiWidgetContext::new(
            context.sim,
            context.world,
            context.engine
        );

        dialog::draw_current(&mut ui_context);
    }
}

// ----------------------------------------------
// Drop for HomeMenus
// ----------------------------------------------

impl Drop for HomeMenus {
    fn drop(&mut self) {
        dialog::reset();
    }
}

// ----------------------------------------------
// Save/Load for HomeMenus
// ----------------------------------------------

impl Save for HomeMenus {}
impl Load for HomeMenus {
    fn pre_load(&mut self, _context: &PreLoadContext) {
        dialog::reset();
    }
}
