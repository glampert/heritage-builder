use std::any::Any;

use common::coords::CellRange;
use engine::{
    app::input::{InputAction, InputKey},
    file_sys::paths::AssetPath,
    ui::{
        UiInputEvent,
        widgets::{UiSlideshow, UiSlideshowFlags, UiSlideshowLoopMode, UiWidget},
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
    skin,
};
use crate::{
    realm,
    save_context::{Load, PreLoadContext, Save},
    tile::rendering::TileMapRenderFlags,
    ui_context::GameUiContext,
};

// ----------------------------------------------
// HomeMenus
// ----------------------------------------------

pub struct HomeMenus {
    slideshow: UiSlideshow,
    skin_version: u32, // The skin the backdrop was built with.
    played_intro: bool, // The first backdrop played its intro: a rebuilt one replays it under the dialogs.
}

impl HomeMenus {
    pub fn new(context: &mut GameUiContext) -> Self {
        // The home menu's realm's skin, before anything is built with it.
        skin::follow(context, realm::ui_realm_for(true).realm);
        skin::apply_theme(context.ui_sys);

        // Back from a game, the dialogs exist: the home menu and the campaign picker are built
        // again, since the game may have changed the profile and the last save.
        dialog::set_home_menu(true);
        let returning = dialog::is_initialized();
        dialog::initialize(context);
        if returning {
            dialog::forget_last_save();
            dialog::rebuild_profile_dialogs(context);
        }
        dialog::set_backdrop_dim(context, true);

        Self { slideshow: Self::make_background_slideshow(context), skin_version: skin::version(), played_intro: false }
    }

    // The menu's full-screen background, from the skin: a still, or an animation's frames whose
    // last two loop.
    fn make_background_slideshow(context: &mut GameUiContext) -> UiSlideshow {
        let (frames, frame_secs) = skin::home_backdrop();
        let frames: Vec<AssetPath> = frames.iter().map(|frame| AssetPath::from_str(frame)).collect();
        let slideshow = UiSlideshow::new(context, frames.iter()).with_flags(UiSlideshowFlags::Fullscreen);
        if frames.len() > 1 {
            slideshow.with_loop_mode(UiSlideshowLoopMode::FramesFromEnd(2)).with_frame_duration_secs(frame_secs)
        } else {
            slideshow
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

    fn sync_skin(&mut self, context: &mut GameUiContext) {
        skin::follow(context, realm::ui_realm_for(true).realm);
        skin::apply_theme(context.ui_sys);
        dialog::set_backdrop_dim(context, true);
        dialog::sync(context);

        if self.skin_version != skin::version() {
            self.skin_version = skin::version();
            self.slideshow = Self::make_background_slideshow(context);
        }
    }

    fn begin_frame(&mut self, _context: &mut GameUiContext) {}

    fn handle_input(&mut self, context: &mut GameUiContext, args: GameMenusInputArgs) -> UiInputEvent {
        if let GameMenusInputArgs::Key { key, action, .. } = args {
            // The dialog on top's own keys (the comic viewer's).
            if action == InputAction::Press && dialog::handle_key(key, context) {
                return UiInputEvent::Handled;
            }
            // [ESCAPE]: Close the dialog on top (only that one: the dialogs don't close themselves on
            // Escape), unless it's the home menu's own.
            if key == InputKey::Escape && action == InputAction::Press {
                if dialog::close_current_on_escape(context) {
                    return UiInputEvent::Handled; // Key press is handled.
                }
            }
        }

        UiInputEvent::NotHandled // Let the event propagate.
    }

    fn end_frame(&mut self, context: &mut GameUiContext, _visible_range: CellRange) {
        self.slideshow.draw(context);

        // The dialogs wait for the intro once only: a skin change rebuilds the backdrop, which
        // starts over, but mustn't hide the dialogs that are open while it does.
        self.played_intro |= self.slideshow.has_flags(UiSlideshowFlags::PlayedOnce);
        if self.played_intro {
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
