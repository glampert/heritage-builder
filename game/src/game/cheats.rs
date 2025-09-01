use proc_macros::DrawDebugUi;

use crate::{
    imgui_ui::UiSystem,
    utils::SingleThreadStatic
};

// ----------------------------------------------
// Game Cheats
// ----------------------------------------------

#[derive(DrawDebugUi)]
pub struct Cheats {
    #[debug_ui(edit)]
    pub ignore_worker_requirements: bool,
}

impl Cheats {
    const fn new() -> Self {
        Self {
            ignore_worker_requirements: false,
        }
    }
}

// ----------------------------------------------
// Global Instance
// ----------------------------------------------

static CHEATS: SingleThreadStatic<Cheats> = SingleThreadStatic::new(Cheats::new());

pub fn get() -> &'static Cheats {
    CHEATS.as_ref()
}

pub fn get_mut() -> &'static mut Cheats {
    CHEATS.as_mut()
}

pub fn draw_debug_ui(ui_sys: &UiSystem) {
    let ui = ui_sys.builder();

    if !ui.collapsing_header("Cheats", imgui::TreeNodeFlags::empty()) {
        return; // collapsed.
    }

    self::get_mut().draw_debug_ui(ui_sys);
}
