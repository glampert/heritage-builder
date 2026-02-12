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
    engine::time::Seconds,
    utils::coords::CellRange,
    tile::rendering::TileMapRenderFlags,
    save::{Save, Load, PreLoadContext},
    app::input::{InputAction, InputKey},
    ui::{
        UiInputEvent, UiTheme,
        widgets::{
            UiWidget,
            UiWidgetContext,
            UiMenuFlags,
            UiSlideshow,
            UiSlideshowFlags,
            UiSlideshowLoopMode,
            UiSlideshowParams,
        }
    },
};

const ANIMATED_BACKGROUND: bool = false;

// ----------------------------------------------
// HomeMenus
// ----------------------------------------------

pub struct HomeMenus {
    slideshow: UiSlideshow,
}

impl HomeMenus {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        context.ui_sys.set_ui_theme(UiTheme::InGame);

        dialog::initialize(context);
        dialog::set_global_menu_flags(UiMenuFlags::PauseSimIfOpen | UiMenuFlags::AlignCenter | UiMenuFlags::AlignLeft);

        // Main Home Menu always open.
        dialog::open(DialogMenuKind::Home, true, context);

        let slideshow = if ANIMATED_BACKGROUND {
            // Animated menu background.
            UiSlideshow::new(context, Self::animated_background_slideshow())
        } else {
            // Static, single-frame menu background.
            UiSlideshow::new(context, Self::static_background_slideshow())
        };

        Self { slideshow }
    }

    fn animated_background_slideshow() -> UiSlideshowParams {
        const SLIDESHOW_FRAME_COUNT: usize = 30;
        const SLIDESHOW_FRAME_DURATION: Seconds = 0.3;

        let mut frames = Vec::with_capacity(SLIDESHOW_FRAME_COUNT);
        for i in 0..SLIDESHOW_FRAME_COUNT {
            frames.push(format!("misc/home_menu_anim/frame{i}.jpg"));
        }

        UiSlideshowParams {
            flags: UiSlideshowFlags::Fullscreen,
            loop_mode: UiSlideshowLoopMode::FramesFromEnd(2), // Loop last two frames from end.
            frame_duration_secs: SLIDESHOW_FRAME_DURATION,
            frames,
            ..Default::default()
        }
    }

    fn static_background_slideshow() -> UiSlideshowParams {
        UiSlideshowParams {
            flags: UiSlideshowFlags::Fullscreen,
            loop_mode: UiSlideshowLoopMode::None,
            frames: vec!["misc/home_menu_static_bg.png".into()],
            ..Default::default()
        }
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
                    if dialog::close_current(&mut context.as_ui_widget_context()) {
                        return UiInputEvent::Handled; // Key press is handled.
                    }
                }
            }
        }

        UiInputEvent::NotHandled // Let the event propagate.
    }

    fn end_frame(&mut self, context: &mut GameMenusContext, _visible_range: CellRange) {
        let mut ui_context = context.as_ui_widget_context();

        self.slideshow.draw(&mut ui_context);

        if self.slideshow.has_flags(UiSlideshowFlags::PlayedOnce) {
            dialog::draw_current(&mut ui_context);
        }
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
