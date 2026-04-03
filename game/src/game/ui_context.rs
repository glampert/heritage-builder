use std::cell::{RefCell, RefMut};

use super::{
    world::World,
    sim::{Simulation, SimContext},
};
use crate::{
    camera::Camera,
    engine::Engine,
    sound::SoundSystem,
    render::RenderSystem,
    tile::{Tile, TileMap, selection::TileSelection},
    utils::{mem, hash::{self, FNV1aHash}, time::Seconds, Size, Vec2},
    ui::{UiSystem, UiFontScale, widgets::{UiWidgetContext, UiWidgetContextStaticTypeId}},
};

// ----------------------------------------------
// GameUiContext
// ----------------------------------------------

// Implements UiWidgetContext with game-session fields.
pub struct GameUiContext<'game> {
    // Engine:
    render_sys: RefCell<&'game mut RenderSystem>,
    sound_sys: RefCell<&'game mut SoundSystem>,
    pub ui_sys: &'game UiSystem,
    pub viewport_size: Size,
    pub delta_time_secs: Seconds,
    pub cursor_screen_pos: Vec2,

    // Game Session:
    pub sim: &'game mut Simulation,
    pub world: &'game mut World,
    pub tile_map: &'game mut TileMap,
    pub tile_selection: &'game mut TileSelection,
    pub camera: &'game mut Camera,

    // Internal:
    in_window_count: u32,           // Nonzero if we're inside a widget window.
    side_by_side_layout_count: u32, // Nonzero if we're inside a horizontal layout (side-by-side) group.
}

impl<'game> GameUiContext<'game> {
    #[inline]
    pub fn new(sim: &'game mut Simulation,
               world: &'game mut World,
               tile_map: &'game mut TileMap,
               tile_selection: &'game mut TileSelection,
               camera: &'game mut Camera,
               engine: &'game mut Engine) -> Self
    {
        let viewport_size = engine.viewport().integer_size();
        let delta_time_secs = engine.frame_clock().delta_time();
        let cursor_screen_pos = engine.input_system().cursor_pos();
        let systems = engine.systems_mut_refs();

        Self {
            render_sys: RefCell::new(systems.render_sys),
            sound_sys: RefCell::new(systems.sound_sys),
            ui_sys: systems.ui_sys,
            viewport_size,
            delta_time_secs,
            cursor_screen_pos,
            sim,
            world,
            tile_map,
            tile_selection,
            camera,
            in_window_count: 0,
            side_by_side_layout_count: 0,
        }
    }

    #[inline]
    pub fn topmost_selected_tile(&self) -> Option<&Tile> {
        self.tile_map.topmost_selected_tile(self.tile_selection)
    }

    #[inline]
    pub fn new_sim_context(&self) -> SimContext {
        // TODO: Clean this up once we switch to SimCmds queue and make SimContext truly immutable.
        // Shouldn't require the mut_ref_cast hacks after that is done.
        mem::mut_ref_cast(self.sim).new_sim_context(
            mem::mut_ref_cast(self.world),
            mem::mut_ref_cast(self.tile_map),
            self.delta_time_secs)
    }
}

// ----------------------------------------------
// UiWidgetContextStaticTypeId impl
// ----------------------------------------------

impl UiWidgetContextStaticTypeId for GameUiContext<'_> {
    const STATIC_TYPE_ID: FNV1aHash = hash::fnv1a_from_str("GameUiContext");
}

// ----------------------------------------------
// UiWidgetContext methods
// ----------------------------------------------

impl<'game> UiWidgetContext<'game> for GameUiContext<'game> {
    #[inline]
    fn runtime_type_id(&self) -> FNV1aHash {
        Self::STATIC_TYPE_ID
    }

    #[inline]
    fn ui_sys(&self) -> &'game UiSystem {
        self.ui_sys
    }

    #[inline]
    fn render_sys(&mut self) -> RefMut<'_, &'game mut RenderSystem> {
        self.render_sys.borrow_mut()
    }

    #[inline]
    fn sound_sys(&mut self) -> RefMut<'_, &'game mut SoundSystem> {
        self.sound_sys.borrow_mut()
    }

    #[inline]
    fn viewport_size(&self) -> Size {
        self.viewport_size
    }

    #[inline]
    fn delta_time_secs(&self) -> Seconds {
        self.delta_time_secs
    }

    #[inline]
    fn cursor_screen_pos(&self) -> Vec2 {
        self.cursor_screen_pos
    }

    #[inline]
    fn begin_widget_window(&mut self) {
        self.in_window_count += 1;
    }

    #[inline]
    fn end_widget_window(&mut self) {
        debug_assert!(!self.is_side_by_side_layout());
        debug_assert!(self.is_inside_widget_window());
        self.in_window_count -= 1;

        // Restore default font scale when ending a window.
        self.ui_sys.set_window_font_scale(UiFontScale::default());
    }

    #[inline]
    fn is_inside_widget_window(&self) -> bool {
        self.in_window_count != 0
    }

    #[inline]
    fn begin_side_by_side_layout(&mut self) {
        self.side_by_side_layout_count += 1;
    }

    #[inline]
    fn end_side_by_side_layout(&mut self) {
        debug_assert!(self.is_side_by_side_layout());
        self.side_by_side_layout_count -= 1;
    }

    #[inline]
    fn is_side_by_side_layout(&self) -> bool {
        self.side_by_side_layout_count != 0
    }

    #[inline]
    fn pause_simulation(&mut self) {
        self.sim.pause();
    }

    #[inline]
    fn resume_simulation(&mut self) {
        self.sim.resume();
    }
}
