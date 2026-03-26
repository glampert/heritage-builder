use crate::{
    log,
    ui::{self, UiSystem},
    sound::SoundSystem,
    camera::Camera,
    app::{
        input::*,
        Application, ApplicationEvent, ApplicationEventList,
    },
    render::{
        RenderSystem, RenderStats,
        texture::TextureCache, debug::DebugDraw,
    },
    tile::{
        TileMap,
        selection::TileSelection,
        rendering::{TileMapRenderFlags, TileMapRenderStats, TileMapRenderer},
    },
    utils::{
        Rect, Vec2,
        coords::CellRange, mem::{RcMut, singleton_late_init},
        time::{FrameClock, PerfTimer, Seconds, Milliseconds},
    },
};

pub mod config;
use config::EngineConfigs;

// ----------------------------------------------
// EngineSystemsMutRefs
// ----------------------------------------------

pub struct EngineSystemsMutRefs<'game> {
    pub ui_sys: &'game UiSystem,
    pub input_sys: &'game InputSystem,
    pub sound_sys: &'game mut SoundSystem,
    pub render_sys: &'game mut RenderSystem,
}

// ----------------------------------------------
// Engine
// ----------------------------------------------

pub struct Engine {
    app: RcMut<Application>,

    render_system: RcMut<RenderSystem>,
    render_stats: RenderStats,

    tile_map_renderer: TileMapRenderer,
    tile_map_render_stats: TileMapRenderStats,

    ui_system: UiSystem,
    sound_system: SoundSystem,
    debug_draw: DebugDraw,

    frame_clock: FrameClock,
    frame_events: ApplicationEventList,
}

impl Engine {
    #[inline]
    pub fn app(&self) -> &Application {
        &self.app
    }

    #[inline]
    pub fn app_mut(&mut self) -> &mut Application {
        &mut self.app
    }

    #[inline]
    pub fn render_system(&self) -> &RenderSystem {
        &self.render_system
    }

    #[inline]
    pub fn render_system_mut(&mut self) -> &mut RenderSystem {
        &mut self.render_system
    }

    #[inline]
    pub fn texture_cache(&self) -> &TextureCache {
        self.render_system.texture_cache()
    }

    #[inline]
    pub fn texture_cache_mut(&mut self) -> &mut TextureCache {
        self.render_system.texture_cache_mut()
    }

    #[inline]
    pub fn debug_draw(&self) -> &DebugDraw {
        &self.debug_draw
    }

    #[inline]
    pub fn debug_draw_mut(&mut self) -> &mut DebugDraw {
        &mut self.debug_draw
    }

    #[inline]
    pub fn sound_system(&self) -> &SoundSystem {
        &self.sound_system
    }

    #[inline]
    pub fn sound_system_mut(&mut self) -> &mut SoundSystem {
        &mut self.sound_system
    }

    #[inline]
    pub fn input_system(&self) -> &InputSystem {
        self.app.input_system()
    }

    #[inline]
    pub fn ui_system(&self) -> &UiSystem {
        &self.ui_system
    }

    #[inline]
    pub fn frame_clock(&self) -> &FrameClock {
        &self.frame_clock
    }

    #[inline]
    pub fn viewport(&self) -> Rect {
        self.render_system.viewport()
    }

    #[inline]
    pub fn render_stats(&self) -> &RenderStats {
        &self.render_stats
    }

    #[inline]
    pub fn tile_map_render_stats(&self) -> &TileMapRenderStats {
        &self.tile_map_render_stats
    }

    #[inline]
    pub fn systems_mut_refs(&mut self) -> EngineSystemsMutRefs<'_> {
        EngineSystemsMutRefs {
            ui_sys: &self.ui_system,
            input_sys: self.app.input_system(),
            sound_sys: &mut self.sound_system,
            render_sys: &mut self.render_system,
        }
    }

    #[inline]
    pub fn set_grid_line_thickness(&mut self, thickness: f32) {
        self.tile_map_renderer.set_grid_line_thickness(thickness);
    }

    #[inline]
    pub fn grid_line_thickness(&self) -> f32 {
        self.tile_map_renderer.grid_line_thickness()
    }

    #[inline]
    pub fn is_running(&self) -> bool {
        !self.app.should_quit()
    }

    #[inline]
    pub fn app_events(&self) -> &ApplicationEventList {
        &self.frame_events
    }

    // ----------------------
    // Begin/End Frame:
    // ----------------------

    pub fn begin_frame(&mut self) -> (Seconds, Vec2, Milliseconds) {
        let begin_frame_timer = PerfTimer::begin();

        self.frame_clock.begin_frame();
        self.frame_events = self.poll_app_events();

        let input_sys = self.app.input_system();

        self.render_system.begin_frame(self.app.window_size(), self.app.framebuffer_size());
        self.ui_system.begin_frame(&self.app, input_sys, self.frame_clock.delta_time());

        let begin_frame_time_ms = begin_frame_timer.end();

        (self.frame_clock.delta_time(), input_sys.cursor_pos(), begin_frame_time_ms)
    }

    pub fn end_frame(&mut self) -> (Milliseconds, Milliseconds) {
        let end_frame_timer = PerfTimer::begin();

        let mut ui_frame_bundle = self.ui_system.end_frame(self.render_system.clone());
        self.render_stats = self.render_system.end_frame(&mut ui_frame_bundle);

        let present_timer = PerfTimer::begin();
        self.app.present();
        let present_frame_time_ms = present_timer.end();

        self.frame_events.clear();
        self.frame_clock.end_frame();

        let end_frame_time_ms = end_frame_timer.end();

        (end_frame_time_ms, present_frame_time_ms)
    }

    pub fn draw_tile_map(&mut self,
                         tile_map: &TileMap,
                         tile_selection: &TileSelection,
                         camera: &Camera,
                         visible_range: CellRange,
                         flags: TileMapRenderFlags)
    {
        if !tile_map.size_in_cells().is_valid() {
            return;
        }

        let render_sys = &mut self.render_system;
        let debug_draw = &mut self.debug_draw;
        let ui_sys = &self.ui_system;

        self.tile_map_render_stats = self.tile_map_renderer.draw_map(render_sys,
                                                                     debug_draw,
                                                                     ui_sys,
                                                                     tile_map,
                                                                     camera.transform(),
                                                                     visible_range,
                                                                     flags);

        tile_selection.draw(render_sys);
    }

    // ----------------------
    // Initialization:
    // ----------------------

    pub fn start(configs: &EngineConfigs,
                 app: RcMut<Application>,
                 render_system: RcMut<RenderSystem>) -> &'static mut Engine
    {
        let engine = Self::new(configs, app, render_system);

        // Set global instance:
        Self::initialize(engine);
        Self::get_mut()
    }

    pub fn shutdown() {
        Self::terminate();
    }

    // NOTE: Application and RenderSystem are initialized outside
    // because we need bespoke initialization for Web/WASM.
    fn new(configs: &EngineConfigs, app: RcMut<Application>, mut render_system: RcMut<RenderSystem>) -> Self {
        log::info!(log::channel!("engine"), "Window Size: {}", app.window_size());
        log::info!(log::channel!("engine"), "Framebuffer Size: {}", app.framebuffer_size());
        log::info!(log::channel!("engine"), "Content Scale: {}", app.content_scale());

        let ui_system = UiSystem::new(&mut render_system);
        log::info!(log::channel!("engine"), "UiSystem initialized.");

        let mut sound_system = SoundSystem::new(configs.sound_settings);
        ui::sound::initialize(&mut sound_system);
        log::info!(log::channel!("engine"), "SoundSystem initialized.");

        let debug_draw = DebugDraw::new(render_system.clone());
        log::info!(log::channel!("engine"), "DebugDraw initialized.");

        Self {
            app,
            render_system,
            render_stats: RenderStats::default(),
            tile_map_renderer: TileMapRenderer::new(configs.grid_color, configs.grid_line_thickness),
            tile_map_render_stats: TileMapRenderStats::default(),
            ui_system,
            sound_system,
            debug_draw,
            frame_clock: FrameClock::new(),
            frame_events: ApplicationEventList::new(),
        }
    }

    // ----------------------
    // Internal:
    // ----------------------

    #[must_use]
    fn poll_app_events(&mut self) -> ApplicationEventList {
        let mut events_forwarded = ApplicationEventList::new();

        for event in self.app.poll_events() {
            match event {
                ApplicationEvent::Quit => {
                    self.app.request_quit();
                    events_forwarded.push(event);
                }
                ApplicationEvent::WindowResize { window_size, framebuffer_size } => {
                    self.render_system.set_viewport_size(window_size);
                    self.render_system.set_framebuffer_size(framebuffer_size);
                    events_forwarded.push(event);
                }
                ApplicationEvent::KeyInput(key, action, modifiers) => {
                    if self.ui_system.on_key_input(key, action, modifiers).not_handled() {
                        events_forwarded.push(event);
                    }
                }
                ApplicationEvent::CharInput(c) => {
                    if self.ui_system.on_char_input(c).not_handled() {
                        events_forwarded.push(event);
                    }
                }
                ApplicationEvent::Scroll(amount) => {
                    if self.ui_system.on_scroll(amount).not_handled() {
                        events_forwarded.push(event);
                    }
                }
                ApplicationEvent::MouseButton(button, action, modifiers) => {
                    if self.ui_system.on_mouse_button(button, action, modifiers).not_handled() {
                        events_forwarded.push(event);
                    }
                }
            }
        }

        events_forwarded
    }
}

// ----------------------------------------------
// Engine Global Singleton
// ----------------------------------------------

singleton_late_init! { ENGINE_SINGLETON, Engine }
