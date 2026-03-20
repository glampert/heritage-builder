#![allow(clippy::mut_from_ref)]

use std::{marker::PhantomData, any::Any};

use crate::{
    log,
    ui::{self, UiSystem},
    sound::SoundSystem,
    app::{
        self, input::*,
        Application, ApplicationBuilder, ApplicationFactory,
        ApplicationEvent, ApplicationEventList,
    },
    render::{
        self,
        TextureCache, TextureHandle,
        RenderStats, RenderSystem, RenderSystemBuilder, RenderSystemFactory,
    },
    tile::{
        rendering::{TileMapRenderFlags, TileMapRenderStats, TileMapRenderer},
        selection::TileSelection,
        TileMap, camera::Camera,
    },
    utils::{
        coords::CellRange, mem::RcMut,
        Color, Rect, RectTexCoords, Vec2,
        time::{FrameClock, PerfTimer, Seconds, Milliseconds},
    },
};

pub mod config;
use config::EngineConfigs;

// ----------------------------------------------
// Engine Backends
// ----------------------------------------------

pub mod backend {
    use super::*;

    #[cfg(feature = "desktop")]
    pub type GlfwOpenGlEngine = EngineBackend<app::backend::GlfwApplication,
                                              app::backend::GlfwInputSystem,
                                              render::backend::OpenGlRenderSystem>;

    #[cfg(feature = "desktop")]
    pub type WinitOpenGlEngine = EngineBackend<app::backend::WinitOpenGlApplication,
                                               app::backend::WinitInputSystem,
                                               render::backend::OpenGlRenderSystem>;

    pub type WinitWgpuEngine = EngineBackend<app::backend::WinitWgpuApplication,
                                             app::backend::WinitInputSystem,
                                             render::backend::WgpuRenderSystem>;
}

// ----------------------------------------------
// EngineSystemsMutRefs
// ----------------------------------------------

pub struct EngineSystemsMutRefs<'game> {
    pub ui_sys: &'game UiSystem,
    pub input_sys: &'game dyn InputSystem,
    pub sound_sys: &'game mut SoundSystem,
    pub render_sys: &'game mut dyn RenderSystem,
}

// ----------------------------------------------
// Engine
// ----------------------------------------------

pub trait Engine: Any {
    fn as_any(&self) -> &dyn Any;
    fn systems_mut_refs(&mut self) -> EngineSystemsMutRefs<'_>;

    fn app(&self) -> &dyn Application;
    fn app_mut(&mut self) -> &mut dyn Application;

    fn render_system(&self) -> &dyn RenderSystem;
    fn render_system_mut(&mut self) -> &mut dyn RenderSystem;

    fn texture_cache(&self) -> &dyn TextureCache;
    fn texture_cache_mut(&mut self) -> &mut dyn TextureCache;

    fn debug_draw(&self) -> &dyn DebugDraw;
    fn debug_draw_mut(&mut self) -> &mut dyn DebugDraw;

    fn sound_system(&self) -> &SoundSystem;
    fn sound_system_mut(&mut self) -> &mut SoundSystem;

    fn input_system(&self) -> &dyn InputSystem;
    fn ui_system(&self) -> &UiSystem;

    fn frame_clock(&self) -> &FrameClock;
    fn viewport(&self) -> Rect;

    fn render_stats(&self) -> &RenderStats;
    fn tile_map_render_stats(&self) -> &TileMapRenderStats;

    fn set_grid_line_thickness(&mut self, thickness: f32);
    fn grid_line_thickness(&self) -> f32;

    fn is_running(&self) -> bool;
    fn app_events(&self) -> &ApplicationEventList;

    fn begin_frame(&mut self) -> (Seconds, Vec2, Milliseconds);
    fn end_frame(&mut self) -> (Milliseconds, Milliseconds);

    fn draw_tile_map(&mut self,
                     tile_map: &TileMap,
                     tile_selection: &TileSelection,
                     camera: &Camera,
                     visible_range: CellRange,
                     flags: TileMapRenderFlags);
}

// ----------------------------------------------
// EngineBackend
// ----------------------------------------------

pub struct EngineBackend<AppBackendImpl, InputSystemBackendImpl, RenderSystemBackendImpl> {
    app: RcMut<AppBackendImpl>,

    render_system: RcMut<RenderSystemBackendImpl>,
    render_stats: RenderStats,

    tile_map_renderer: TileMapRenderer,
    tile_map_render_stats: TileMapRenderStats,

    ui_system: UiSystem,
    sound_system: SoundSystem,
    debug_draw: DebugDrawBackend<RenderSystemBackendImpl>,

    frame_clock: FrameClock,
    frame_events: ApplicationEventList,

    _input_system: PhantomData<InputSystemBackendImpl>,
}

impl<AppBackendImpl, InputSystemBackendImpl, RenderSystemBackendImpl>
    EngineBackend<AppBackendImpl, InputSystemBackendImpl, RenderSystemBackendImpl>
    where AppBackendImpl: Application + 'static,
          InputSystemBackendImpl: InputSystem + 'static,
          RenderSystemBackendImpl: RenderSystem + 'static
{
    // Create an engine from pre-constructed app and render system.
    // Used on WASM where async initialization prevents using the
    // ApplicationFactory / RenderSystemFactory traits.
    pub fn from_parts(
        app: AppBackendImpl,
        render_system: RenderSystemBackendImpl,
        configs: &EngineConfigs,
    ) -> Self {
        log::set_level(configs.log_level);
        log::info!(log::channel!("engine"), "--- Engine Initialization (from_parts) ---");

        let app: RcMut<AppBackendImpl> = RcMut::new(app);
        let mut render_system: RcMut<RenderSystemBackendImpl> = RcMut::new(render_system);

        let ui_system  = UiSystem::new(&mut *render_system);
        let debug_draw = DebugDrawBackend::new(render_system.clone());

        log::info!(log::channel!("engine"), "Window Size: {}", app.window_size());
        log::info!(log::channel!("engine"), "Framebuffer Size: {}", app.framebuffer_size());
        log::info!(log::channel!("engine"), "Content Scale: {}", app.content_scale());

        let mut sound_system = SoundSystem::new(configs.sound_settings);
        log::info!(log::channel!("engine"), "SoundSystem initialized.");
        ui::sound::initialize(&mut sound_system);

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
            _input_system: PhantomData,
        }
    }

    #[must_use]
    fn poll_app_events(app: &mut RcMut<AppBackendImpl>,
                       render_system: &mut RcMut<RenderSystemBackendImpl>,
                       ui_system: &mut UiSystem) -> ApplicationEventList
    {
        let mut events_forwarded = ApplicationEventList::new();

        for event in app.poll_events() {
            match event {
                ApplicationEvent::Quit => {
                    app.request_quit();
                    events_forwarded.push(event);
                }
                ApplicationEvent::WindowResize { window_size, framebuffer_size } => {
                    render_system.set_viewport_size(window_size);
                    render_system.set_framebuffer_size(framebuffer_size);
                    events_forwarded.push(event);
                }
                ApplicationEvent::KeyInput(key, action, modifiers) => {
                    if ui_system.on_key_input(key, action, modifiers).not_handled() {
                        events_forwarded.push(event);
                    }
                }
                ApplicationEvent::CharInput(c) => {
                    if ui_system.on_char_input(c).not_handled() {
                        events_forwarded.push(event);
                    }
                }
                ApplicationEvent::Scroll(amount) => {
                    if ui_system.on_scroll(amount).not_handled() {
                        events_forwarded.push(event);
                    }
                }
                ApplicationEvent::MouseButton(button, action, modifiers) => {
                    if ui_system.on_mouse_button(button, action, modifiers).not_handled() {
                        events_forwarded.push(event);
                    }
                }
            }
        }

        events_forwarded
    }
}

impl<AppBackendImpl, InputSystemBackendImpl, RenderSystemBackendImpl>
    EngineBackend<AppBackendImpl, InputSystemBackendImpl, RenderSystemBackendImpl>
    where AppBackendImpl: Application + ApplicationFactory + 'static,
          InputSystemBackendImpl: InputSystem + 'static,
          RenderSystemBackendImpl: RenderSystem + RenderSystemFactory + 'static
{
    pub fn new(configs: &EngineConfigs) -> Self {
        log::set_level(configs.log_level);
        log::info!(log::channel!("engine"), "--- Engine Initialization ---");

        let app: RcMut<AppBackendImpl> = RcMut::new(
            ApplicationBuilder::new()
                .window_title(&configs.window_title)
                .window_size(configs.window_size)
                .window_mode(configs.window_mode)
                .resizable_window(configs.resizable_window)
                .confine_cursor_to_window(configs.confine_cursor_to_window)
                .content_scale(configs.content_scale)
                .build()
        );

        log::info!(log::channel!("engine"), "App instance initialized.");

        let mut render_system: RcMut<RenderSystemBackendImpl> = RcMut::new({
            let mut builder = RenderSystemBuilder::new();
            builder
                .viewport_size(app.window_size())
                .framebuffer_size(app.framebuffer_size())
                .clear_color(configs.window_background_color)
                .texture_settings(configs.texture_settings);
            if let Some(ctx) = app.app_context() {
                builder.app_context(ctx);
            }
            builder.build()
        });

        log::info!(log::channel!("engine"), "RenderSystem initialized.");

        let ui_system  = UiSystem::new(&mut *render_system);
        let debug_draw = DebugDrawBackend::new(render_system.clone());

        log::info!(log::channel!("engine"), "Debug UI initialized.");
        log::info!(log::channel!("engine"), "Window Size: {}", app.window_size());
        log::info!(log::channel!("engine"), "Framebuffer Size: {}", app.framebuffer_size());
        log::info!(log::channel!("engine"), "Content Scale: {}", app.content_scale());

        let mut sound_system = SoundSystem::new(configs.sound_settings);
        log::info!(log::channel!("engine"), "SoundSystem initialized.");
        ui::sound::initialize(&mut sound_system);

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
            _input_system: PhantomData,
        }
    }

}

impl<AppBackendImpl, InputSystemBackendImpl, RenderSystemBackendImpl> Engine
    for EngineBackend<AppBackendImpl, InputSystemBackendImpl, RenderSystemBackendImpl>
    where AppBackendImpl: Application + 'static,
          InputSystemBackendImpl: InputSystem + 'static,
          RenderSystemBackendImpl: RenderSystem + 'static
{
    #[inline]
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[inline]
    fn systems_mut_refs(&mut self) -> EngineSystemsMutRefs<'_> {
        EngineSystemsMutRefs {
            ui_sys: &self.ui_system,
            input_sys: self.app.input_system(),
            sound_sys: &mut self.sound_system,
            render_sys: &mut *self.render_system,
        }
    }

    #[inline]
    fn app(&self) -> &dyn Application {
        &*self.app
    }

    #[inline]
    fn app_mut(&mut self) -> &mut dyn Application {
        &mut *self.app
    }

    #[inline]
    fn render_system(&self) -> &dyn RenderSystem {
        &*self.render_system
    }

    #[inline]
    fn render_system_mut(&mut self) -> &mut dyn RenderSystem {
        &mut *self.render_system
    }

    #[inline]
    fn texture_cache(&self) -> &dyn TextureCache {
        self.render_system.texture_cache()
    }

    #[inline]
    fn texture_cache_mut(&mut self) -> &mut dyn TextureCache {
        self.render_system.texture_cache_mut()
    }

    #[inline]
    fn debug_draw(&self) -> &dyn DebugDraw {
        &self.debug_draw
    }

    #[inline]
    fn debug_draw_mut(&mut self) -> &mut dyn DebugDraw {
        &mut self.debug_draw
    }

    #[inline]
    fn sound_system(&self) -> &SoundSystem {
        &self.sound_system
    }

    #[inline]
    fn sound_system_mut(&mut self) -> &mut SoundSystem {
        &mut self.sound_system
    }

    #[inline]
    fn input_system(&self) -> &dyn InputSystem {
        self.app.input_system()
    }

    #[inline]
    fn ui_system(&self) -> &UiSystem {
        &self.ui_system
    }

    #[inline]
    fn frame_clock(&self) -> &FrameClock {
        &self.frame_clock
    }

    #[inline]
    fn viewport(&self) -> Rect {
        self.render_system.viewport()
    }

    #[inline]
    fn render_stats(&self) -> &RenderStats {
        &self.render_stats
    }

    #[inline]
    fn tile_map_render_stats(&self) -> &TileMapRenderStats {
        &self.tile_map_render_stats
    }

    #[inline]
    fn set_grid_line_thickness(&mut self, thickness: f32) {
        self.tile_map_renderer.set_grid_line_thickness(thickness);
    }

    #[inline]
    fn grid_line_thickness(&self) -> f32 {
        self.tile_map_renderer.grid_line_thickness()
    }

    #[inline]
    fn is_running(&self) -> bool {
        !self.app.should_quit()
    }

    #[inline]
    fn app_events(&self) -> &ApplicationEventList {
        &self.frame_events
    }

    fn begin_frame(&mut self) -> (Seconds, Vec2, Milliseconds) {
        let begin_frame_timer = PerfTimer::begin();

        self.frame_clock.begin_frame();
        self.frame_events = Self::poll_app_events(
            &mut self.app,
            &mut self.render_system,
            &mut self.ui_system);

        // Pass in the concrete InputSystem implementation to UiSystem.
        let input_sys = self.app.input_system()
            .as_any()
            .downcast_ref::<InputSystemBackendImpl>()
            .unwrap();

        self.render_system.begin_frame(self.app.window_size(), self.app.framebuffer_size());
        self.ui_system.begin_frame(&*self.app, input_sys, self.frame_clock.delta_time());

        let begin_frame_time_ms = begin_frame_timer.end();

        (self.frame_clock.delta_time(), input_sys.cursor_pos(), begin_frame_time_ms)
    }

    fn end_frame(&mut self) -> (Milliseconds, Milliseconds) {
        let end_frame_timer = PerfTimer::begin();

        let mut ui_frame_bundle = self.ui_system.end_frame();
        self.render_stats = self.render_system.end_frame(&mut ui_frame_bundle);

        let present_timer = PerfTimer::begin();
        self.app.present();
        let present_frame_time_ms = present_timer.end();

        self.frame_events.clear();
        self.frame_clock.end_frame();

        let end_frame_time_ms = end_frame_timer.end();

        (end_frame_time_ms, present_frame_time_ms)
    }

    fn draw_tile_map(&mut self,
                     tile_map: &TileMap,
                     tile_selection: &TileSelection,
                     camera: &Camera,
                     visible_range: CellRange,
                     flags: TileMapRenderFlags) {
        if !tile_map.size_in_cells().is_valid() {
            return;
        }

        let render_sys = &mut *self.render_system;
        let ui_sys = &self.ui_system;

        self.tile_map_render_stats = self.tile_map_renderer.draw_map(render_sys,
                                                                     ui_sys,
                                                                     tile_map,
                                                                     camera.transform(),
                                                                     visible_range,
                                                                     flags);

        tile_selection.draw(render_sys);
    }
}

// ----------------------------------------------
// DebugDraw
// ----------------------------------------------

pub trait DebugDraw {
    fn texture_cache(&self) -> &dyn TextureCache;
    fn texture_cache_mut(&mut self) -> &mut dyn TextureCache;

    fn point(&mut self, pt: Vec2, color: Color, size: f32);

    fn line(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color);
    fn line_with_thickness(&mut self, from_pos: Vec2, to_pos: Vec2, color: Color, thickness: f32);

    fn wireframe_rect(&mut self, rect: Rect, color: Color);
    fn wireframe_rect_with_thickness(&mut self, rect: Rect, color: Color, thickness: f32);

    fn colored_rect(&mut self, rect: Rect, color: Color);
    fn textured_colored_rect(&mut self,
                             rect: Rect,
                             tex_coords: &RectTexCoords,
                             texture: TextureHandle,
                             color: Color);
}

// ----------------------------------------------
// DebugDrawBackend
// ----------------------------------------------

struct DebugDrawBackend<RenderSystemBackendImpl> {
    render_system: RcMut<RenderSystemBackendImpl>,
}

impl<RenderSystemBackendImpl> DebugDrawBackend<RenderSystemBackendImpl>
    where RenderSystemBackendImpl: RenderSystem
{
    fn new(render_system: RcMut<RenderSystemBackendImpl>) -> Self {
        Self { render_system }
    }
}

impl<RenderSystemBackendImpl> DebugDraw for DebugDrawBackend<RenderSystemBackendImpl>
    where RenderSystemBackendImpl: RenderSystem
{
    #[inline]
    fn texture_cache(&self) -> &dyn TextureCache {
        self.render_system.texture_cache()
    }

    #[inline]
    fn texture_cache_mut(&mut self) -> &mut dyn TextureCache {
        self.render_system.texture_cache_mut()
    }

    #[inline]
    fn point(&mut self, pt: Vec2, color: Color, size: f32) {
        self.render_system.draw_point_fast(pt, color, size);
    }

    #[inline]
    fn line(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color) {
        self.render_system.draw_line_fast(from_pos, to_pos, from_color, to_color);
    }

    #[inline]
    fn line_with_thickness(&mut self, from_pos: Vec2, to_pos: Vec2, color: Color, thickness: f32) {
        self.render_system.draw_line_with_thickness(from_pos, to_pos, color, thickness);
    }

    #[inline]
    fn wireframe_rect(&mut self, rect: Rect, color: Color) {
        self.render_system.draw_wireframe_rect_fast(rect, color);
    }

    #[inline]
    fn wireframe_rect_with_thickness(&mut self, rect: Rect, color: Color, thickness: f32) {
        self.render_system.draw_wireframe_rect_with_thickness(rect, color, thickness);
    }

    #[inline]
    fn colored_rect(&mut self, rect: Rect, color: Color) {
        self.render_system.draw_colored_rect(rect, color);
    }

    #[inline]
    fn textured_colored_rect(&mut self,
                             rect: Rect,
                             tex_coords: &RectTexCoords,
                             texture: TextureHandle,
                             color: Color) {
        self.render_system.draw_textured_colored_rect(rect, tex_coords, texture, color);
    }
}
