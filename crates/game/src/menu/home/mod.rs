use std::any::Any;

use common::{coords::CellRange, format_fixed_string, time::Seconds};
use engine::{
    app::input::{InputAction, InputKey},
    file_sys::paths::AssetPath,
    ui::{
        UiInputEvent,
        UiTheme,
        widgets::{UiMenuFlags, UiSlideshow, UiSlideshowFlags, UiSlideshowLoopMode, UiWidget},
    },
};

use super::{
    GameMenusInputArgs,
    GameMenusMode,
    GameMenusSystem,
    TileInspector,
    TilePalette,
    TilePlacement,
    dialog::{self, DialogMenuKind},
};
use crate::{
    save_context::{Load, PreLoadContext, Save},
    tile::rendering::TileMapRenderFlags,
    ui_context::GameUiContext,
};

const ANIMATED_BACKGROUND: bool = false;

// ----------------------------------------------
// HomeMenus
// ----------------------------------------------

pub struct HomeMenus {
    slideshow: UiSlideshow,
}

impl HomeMenus {
    pub fn new(context: &mut GameUiContext) -> Self {
        context.ui_sys.set_ui_theme(UiTheme::InGame);

        dialog::initialize(context);
        dialog::set_global_menu_flags(UiMenuFlags::AlignCenter | UiMenuFlags::AlignLeft);
        dialog::set_bg_dim_alpha(context, 0.0); // No bg dimming.

        Self { slideshow: Self::make_background_slideshow(context) }
    }

    // The menu's full-screen background: animated, or a static single frame.
    fn make_background_slideshow(context: &mut GameUiContext) -> UiSlideshow {
        if ANIMATED_BACKGROUND {
            const SLIDESHOW_FRAME_COUNT: usize = 30;
            const SLIDESHOW_FRAME_DURATION: Seconds = 0.3;

            let frames: Vec<AssetPath> = (0..SLIDESHOW_FRAME_COUNT)
                .map(|i| AssetPath::from_str(&format_fixed_string!(64, "misc/home_menu_anim/frame{i}.jpg")))
                .collect();

            UiSlideshow::new(context, frames.iter())
                .with_flags(UiSlideshowFlags::Fullscreen)
                .with_loop_mode(UiSlideshowLoopMode::FramesFromEnd(2)) // Loop last two frames from end.
                .with_frame_duration_secs(SLIDESHOW_FRAME_DURATION)
        } else {
            UiSlideshow::new(context, ["misc/home_menu_static_bg.png"]).with_flags(UiSlideshowFlags::Fullscreen)
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

    fn begin_frame(&mut self, _context: &mut GameUiContext) {}

    fn handle_input(&mut self, context: &mut GameUiContext, args: GameMenusInputArgs) -> UiInputEvent {
        if let GameMenusInputArgs::Key { key, action, .. } = args {
            // [ESCAPE]: Close child dialog menu.
            if key == InputKey::Escape && action == InputAction::Press {
                // Close if we're not already at the Main Home Menu.
                if dialog::current().is_some_and(|dialog| dialog != DialogMenuKind::Home) {
                    if dialog::close_current(context) {
                        return UiInputEvent::Handled; // Key press is handled.
                    }
                }
            }
        }

        UiInputEvent::NotHandled // Let the event propagate.
    }

    fn end_frame(&mut self, context: &mut GameUiContext, _visible_range: CellRange) {
        self.slideshow.draw(context);

        if self.slideshow.has_flags(UiSlideshowFlags::PlayedOnce) {
            // Main Home Menu always open.
            if !dialog::is_open(DialogMenuKind::Home) {
                dialog::open(DialogMenuKind::Home, false, context);
            }
            dialog::draw_current(context);
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
    fn pre_load(&mut self, _context: &mut PreLoadContext) {
        dialog::reset();
    }
}
