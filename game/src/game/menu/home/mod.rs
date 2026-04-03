use std::any::Any;

use super::{
    GameMenusMode,
    GameMenusSystem,
    GameMenusInputArgs,
    TilePlacement,
    TileInspector,
    TilePalette,
    dialog::{self, DialogMenuKind},
};
use crate::{
    tile::rendering::TileMapRenderFlags,
    save::{Save, Load, PreLoadContext},
    app::input::{InputAction, InputKey},
    file_sys::paths::AssetPath,
    game::ui_context::GameUiContext,
    utils::{
        time::Seconds,
        coords::CellRange,
        fixed_string::format_fixed_string
    },
    ui::{
        UiInputEvent, UiTheme,
        widgets::{
            UiWidget,
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
    pub fn new(context: &mut GameUiContext) -> Self {
        context.ui_sys.set_ui_theme(UiTheme::InGame);

        dialog::initialize(context);
        dialog::set_global_menu_flags(UiMenuFlags::AlignCenter | UiMenuFlags::AlignLeft);
        dialog::set_bg_dim_alpha(context, 0.0); // No bg dimming.

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
            let path = format_fixed_string!(64, "misc/home_menu_anim/frame{i}.jpg");
            frames.push(AssetPath::from_str(&path));
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
            frames: vec![AssetPath::from_str("misc/home_menu_static_bg.png")],
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

    fn begin_frame(&mut self, _context: &mut GameUiContext) {
    }

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
