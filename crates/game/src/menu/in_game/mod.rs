use std::any::Any;

use bars::{InGameMenuBars, InGameMenuBarsRcMut};
use common::coords::CellRange;
use engine::{
    app::input::{InputAction, InputKey},
    ui::UiInputEvent,
};
use inspector::{TileInspectorMenu, TileInspectorMenuRcMut};
use palette::{TilePaletteMenu, TilePaletteMenuRcMut};

use super::{GameMenusInputArgs, GameMenusMode, GameMenusSystem, TileInspector, TilePalette, TilePlacement, dialog, skin};
use crate::{
    realm,
    campaign::{self, CampaignPrompt},
    save_context::{Load, PreLoadContext, Save},
    tile::minimap::{InGameUiMinimapRenderer, MinimapRenderer},
    ui_context::GameUiContext,
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
    built: (u32, u32), // The skin's and the strings' versions the HUD was built with.
}

impl InGameMenus {
    pub fn new(context: &mut GameUiContext) -> Self {
        // The game's realm's skin, before anything is built with it.
        skin::follow(context, realm::ui_realm_for(false).realm);
        skin::apply_theme(context.ui_sys);

        dialog::set_home_menu(false);
        dialog::initialize(context);
        dialog::set_backdrop_dim(context, false);

        Self {
            tile_placement: TilePlacement::new(),
            tile_palette: TilePaletteMenu::new(context),
            tile_inspector: TileInspectorMenu::new(context),
            menu_bars: InGameMenuBars::new(context),
            minimap_renderer: InGameUiMinimapRenderer::new(context),
            built: Self::versions(),
        }
    }

    fn versions() -> (u32, u32) {
        (skin::version(), engine::ui::text::version())
    }

    // Builds the HUD again from the current skin and strings: the palette (its selection
    // dropped), the bars and the inspector (closed). The minimap follows the skin's realm itself.
    // Closing the inspector resumes the simulation (`PauseSimIfOpen`), as closing it by hand does,
    // so the game is paused again only if a dialog still needs it, as DialogMenusSingleton::keep_paused
    // does: not for the inspector's own pause, which nothing open would lift.
    fn rebuild_hud(&mut self, context: &mut GameUiContext) {
        let was_paused = context.sim.is_paused();
        self.tile_palette.clear_current_selection(context);
        self.tile_inspector.close(context);
        if was_paused && dialog::current().is_some() {
            context.sim.pause();
        }
        self.tile_palette = TilePaletteMenu::new(context);
        self.tile_inspector = TileInspectorMenu::new(context);
        self.menu_bars = InGameMenuBars::new(context);
        self.built = Self::versions();
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

    // Every in-game dialog is modal (the Game menu, Settings, the world map, ...).
    fn covers_hud(&self) -> bool {
        dialog::current().is_some()
    }

    // The world map, full screen and opaque.
    fn covers_window(&self) -> bool {
        dialog::covers_window()
    }

    fn sync_skin(&mut self, context: &mut GameUiContext) {
        skin::follow(context, realm::ui_realm_for(false).realm);
        skin::apply_theme(context.ui_sys);
        dialog::set_backdrop_dim(context, false);
        dialog::sync(context);

        if self.built != Self::versions() {
            self.rebuild_hud(context);
        }
    }

    fn handle_custom_input(&mut self, context: &mut GameUiContext, args: GameMenusInputArgs) -> UiInputEvent {
        if let GameMenusInputArgs::Key { key, action, .. } = args {
            // The dialog on top's own keys (the comic viewer's).
            if action == InputAction::Press && dialog::handle_key(key, context) {
                dialog::preview::return_if_closed(context); // A preview's last screen closed: paused again before the step.
                return UiInputEvent::Handled;
            }
            // [ESCAPE]: Close the dialog on top, or its popup (only that: the dialogs don't close
            // themselves on Escape).
            if key == InputKey::Escape && action == InputAction::Press {
                if dialog::close_current_on_escape(context) {
                    dialog::preview::return_if_closed(context);
                    return UiInputEvent::Handled; // Key press is handled.
                }
            }
        }

        UiInputEvent::NotHandled // Let the event propagate.
    }

    fn end_frame(&mut self, context: &mut GameUiContext, _visible_range: CellRange) {
        self.minimap_renderer.draw(context);
        self.tile_palette.draw(context);
        self.menu_bars.draw(context);
        self.tile_inspector.draw(context);

        // Open a pending campaign prompt, but only when no other dialog is open
        // (don't interrupt the pause menu, etc.). The prompt stays pending until
        // then, so it is taken only when it is actually shown.
        // A mission's briefing opens first, as its map loads, after the mission's story sequence
        // if the profile hasn't seen it (dialog::open_mission_start): a prompt raised meanwhile (a
        // mission finished at once) waits for its Begin.
        // A preview from the developer editor's Screens menu (dialog::preview) opens its screen,
        // and goes back to the editor once it has closed (return_if_closed); no prompt opens meanwhile.
        let previewing = dialog::preview::update(context);
        if !previewing && dialog::current().is_none() && campaign::is_briefing_pending() {
            dialog::open_mission_start(context);
        }
        if !previewing && dialog::current().is_none() {
            match campaign::take_pending_prompt() {
                Some(CampaignPrompt::MissionComplete) => {
                    dialog::open(dialog::DialogMenuKind::MissionComplete, false, context);
                }
                Some(CampaignPrompt::CampaignComplete) => {
                    dialog::open(dialog::DialogMenuKind::CampaignComplete, false, context);
                }
                None => {}
            }
        }

        dialog::draw_current(context);
        dialog::preview::return_if_closed(context);
    }
}

// ----------------------------------------------
// Drop for InGameMenus
// ----------------------------------------------

impl Drop for InGameMenus {
    fn drop(&mut self) {
        dialog::reset();
        dialog::preview::reset();
    }
}

// ----------------------------------------------
// Save/Load for InGameMenus
// ----------------------------------------------

impl Save for InGameMenus {}
impl Load for InGameMenus {
    fn pre_load(&mut self, _context: &mut PreLoadContext) {
        dialog::reset();
        dialog::preview::reset();
    }
}
