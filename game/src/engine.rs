use std::marker::PhantomData;

use crate::{
    app::{
        self, Application, ApplicationBuilder, ApplicationEvent, ApplicationEventList,
        ApplicationFactory, input::*,
    },
    debug::log_viewer::LogViewerWindow,
    imgui_ui::{self, UiRenderer, UiRendererFactory, UiSystem},
    log,
    render::{
        self, RenderStats, RenderSystem, RenderSystemBuilder, RenderSystemFactory, TextureCache,
        TextureHandle,
    },
    tile::{
        TileMap,
        rendering::{self, TileMapRenderFlags, TileMapRenderStats, TileMapRenderer},
        selection::TileSelection,
    },
    utils::{
        Color, FrameClock, Rect, RectTexCoords, Seconds, Size, UnsafeWeakRef, Vec2,
        coords::{CellRange, WorldToScreenTransform},
    },
};

// ----------------------------------------------
// Engine Backends
// ----------------------------------------------

pub mod backend {
    use super::*;
    pub type GlfwOpenGlEngine = EngineBackend<
        app::backend::GlfwApplication,
        app::backend::GlfwInputSystem,
        render::backend::RenderSystemOpenGl,
        imgui_ui::backend::UiRendererOpenGl,
    >;
}

// ----------------------------------------------
// EngineConfigs
// ----------------------------------------------

// TODO: Deserialize with serde. Load from json config file.
pub struct EngineConfigs {
    // Window/Rendering:
    pub window_title: String,
    pub window_size: Size,
    pub window_background_color: Color,
    pub fullscreen: bool,
    pub confine_cursor_to_window: bool,

    // Debug Grid:
    pub grid_color: Color,
    pub grid_line_thickness: f32,

    // Debug Log:
    pub log_level: log::Level,
    pub log_viewer_start_open: bool,
    pub log_viewer_max_lines: usize,
}

impl Default for EngineConfigs {
    fn default() -> Self {
        Self {
            // Window/Rendering:
            window_title: "CitySim".into(),
            window_size: Size::new(1024, 768),
            window_background_color: rendering::MAP_BACKGROUND_COLOR,
            fullscreen: false,
            confine_cursor_to_window: true,

            // Debug Grid:
            grid_color: rendering::DEFAULT_GRID_COLOR,
            grid_line_thickness: 1.0,

            // Debug Log:
            log_level: log::Level::Verbose,
            log_viewer_start_open: false,
            log_viewer_max_lines: 32,
        }
    }
}

// ----------------------------------------------
// Engine
// ----------------------------------------------

pub trait Engine {
    fn app(&mut self) -> &mut dyn Application;
    fn input_system(&self) -> &dyn InputSystem;

    fn render_system(&mut self) -> &mut dyn RenderSystem;
    fn render_stats(&self) -> RenderStats;
    fn tile_map_render_stats(&self) -> TileMapRenderStats;

    fn texture_cache(&self) -> &dyn TextureCache;
    fn texture_cache_mut(&mut self) -> &mut dyn TextureCache;

    fn ui_system(&self) -> &UiSystem;
    fn debug_draw(&mut self) -> &mut dyn DebugDraw;

    fn frame_clock(&self) -> &FrameClock;
    fn log_viewer(&self) -> &LogViewerWindow;
    fn viewport(&self) -> Rect;

    fn is_running(&self) -> bool;
    fn poll_events(&mut self) -> ApplicationEventList;

    fn begin_frame(&mut self) -> (Seconds, Vec2);
    fn end_frame(&mut self);

    fn draw_tile_map(
        &mut self,
        tile_map: &TileMap,
        tile_selection: &TileSelection,
        transform: WorldToScreenTransform,
        visible_range: CellRange,
        flags: TileMapRenderFlags,
    );
}

// ----------------------------------------------
// EngineBackend
// ----------------------------------------------

pub struct EngineBackend<
    AppBackendImpl,
    InputSystemBackendImpl,
    RenderSystemBackendImpl,
    UiRendererBackendImpl,
> {
    app: Box<AppBackendImpl>,

    render_system: Box<RenderSystemBackendImpl>,
    render_stats: RenderStats,

    tile_map_renderer: TileMapRenderer,
    tile_map_render_stats: TileMapRenderStats,

    ui_system: UiSystem,
    debug_draw: DebugDrawBackend<RenderSystemBackendImpl>,

    frame_clock: FrameClock,
    log_viewer: LogViewerWindow,

    _ui_marker: PhantomData<UiRendererBackendImpl>,
    _input_marker: PhantomData<InputSystemBackendImpl>,
}

impl<AppBackendImpl, InputSystemBackendImpl, RenderSystemBackendImpl, UiRendererBackendImpl>
    EngineBackend<
        AppBackendImpl,
        InputSystemBackendImpl,
        RenderSystemBackendImpl,
        UiRendererBackendImpl,
    >
where
    AppBackendImpl: Application + ApplicationFactory + 'static,
    InputSystemBackendImpl: InputSystem + 'static,
    RenderSystemBackendImpl: RenderSystem + RenderSystemFactory + 'static,
    UiRendererBackendImpl: UiRenderer + UiRendererFactory + 'static,
{
    pub fn new(configs: &EngineConfigs) -> Self {
        log::set_level(configs.log_level);
        let log_viewer =
            LogViewerWindow::new(configs.log_viewer_start_open, configs.log_viewer_max_lines);

        log::info!(log::channel!("engine"), "--- Engine Initialization ---");
        log::info!(
            log::channel!("engine"),
            "The current directory is: {:?}",
            std::env::current_dir().unwrap()
        );

        let app: Box<AppBackendImpl> = ApplicationBuilder::new()
            .window_title(&configs.window_title)
            .window_size(configs.window_size)
            .fullscreen(configs.fullscreen)
            .confine_cursor_to_window(configs.confine_cursor_to_window)
            .build();

        log::info!(log::channel!("engine"), "Application created.");

        let render_system: Box<RenderSystemBackendImpl> = RenderSystemBuilder::new()
            .viewport_size(configs.window_size)
            .clear_color(configs.window_background_color)
            .build();

        log::info!(log::channel!("engine"), "RenderSystem created.");

        let ui_system = UiSystem::new::<UiRendererBackendImpl>(&*app);
        let debug_draw = DebugDrawBackend::new(&*render_system);

        log::info!(log::channel!("engine"), "Debug UI initialized.");

        Self {
            app,
            render_system,
            render_stats: RenderStats::default(),
            tile_map_renderer: TileMapRenderer::new(
                configs.grid_color,
                configs.grid_line_thickness,
            ),
            tile_map_render_stats: TileMapRenderStats::default(),
            ui_system,
            debug_draw,
            frame_clock: FrameClock::new(),
            log_viewer,
            _ui_marker: PhantomData,
            _input_marker: PhantomData,
        }
    }
}

impl<AppBackendImpl, InputSystemBackendImpl, RenderSystemBackendImpl, UiRendererBackendImpl> Engine
    for EngineBackend<
        AppBackendImpl,
        InputSystemBackendImpl,
        RenderSystemBackendImpl,
        UiRendererBackendImpl,
    >
where
    AppBackendImpl: Application,
    InputSystemBackendImpl: InputSystem,
    RenderSystemBackendImpl: RenderSystem,
    UiRendererBackendImpl: UiRenderer,
{
    #[inline]
    fn app(&mut self) -> &mut dyn Application {
        &mut *self.app
    }

    #[inline]
    fn input_system(&self) -> &dyn InputSystem {
        self.app.input_system()
    }

    #[inline]
    fn render_system(&mut self) -> &mut dyn RenderSystem {
        &mut *self.render_system
    }

    #[inline]
    fn render_stats(&self) -> RenderStats {
        self.render_stats
    }

    #[inline]
    fn tile_map_render_stats(&self) -> TileMapRenderStats {
        self.tile_map_render_stats
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
    fn ui_system(&self) -> &UiSystem {
        &self.ui_system
    }

    #[inline]
    fn debug_draw(&mut self) -> &mut dyn DebugDraw {
        &mut self.debug_draw
    }

    #[inline]
    fn frame_clock(&self) -> &FrameClock {
        &self.frame_clock
    }

    #[inline]
    fn log_viewer(&self) -> &LogViewerWindow {
        &self.log_viewer
    }

    #[inline]
    fn viewport(&self) -> Rect {
        self.render_system.viewport()
    }

    #[inline]
    fn is_running(&self) -> bool {
        !self.app.should_quit()
    }

    fn poll_events(&mut self) -> ApplicationEventList {
        // Forwarded input events not handled here by the UI system.
        let mut events_forwarded = ApplicationEventList::new();

        for event in self.app.poll_events() {
            match event {
                ApplicationEvent::Quit => {
                    self.app.request_quit();
                    events_forwarded.push(event);
                }
                ApplicationEvent::WindowResize(window_size) => {
                    self.render_system.set_viewport_size(window_size);
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
                    if self.ui_system.on_mouse_click(button, action, modifiers).not_handled() {
                        events_forwarded.push(event);
                    }
                }
            }
        }

        events_forwarded
    }

    fn begin_frame(&mut self) -> (Seconds, Vec2) {
        // Pass in the concrete InputSystem implementation to UiSystem.
        let input_sys =
            self.app.input_system().as_any().downcast_ref::<InputSystemBackendImpl>().unwrap();

        self.frame_clock.begin_frame();
        self.ui_system.begin_frame(&*self.app, input_sys, self.frame_clock.delta_time());
        self.render_system.begin_frame();

        (self.frame_clock.delta_time(), input_sys.cursor_pos())
    }

    fn end_frame(&mut self) {
        self.render_stats = self.render_system.end_frame();
        self.ui_system.end_frame();
        self.app.present();
        self.frame_clock.end_frame();
    }

    fn draw_tile_map(
        &mut self,
        tile_map: &TileMap,
        tile_selection: &TileSelection,
        transform: WorldToScreenTransform,
        visible_range: CellRange,
        flags: TileMapRenderFlags,
    ) {
        if !tile_map.size_in_cells().is_valid() {
            return;
        }

        let render_sys = &mut *self.render_system;
        let ui_sys = &self.ui_system;

        self.tile_map_render_stats = self.tile_map_renderer.draw_map(
            render_sys,
            ui_sys,
            tile_map,
            transform,
            visible_range,
            flags,
        );

        tile_selection.draw(render_sys);
    }
}

// ----------------------------------------------
// DebugDraw
// ----------------------------------------------

pub trait DebugDraw {
    fn point(&mut self, pt: Vec2, color: Color, size: f32);
    fn line(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color);
    fn wireframe_rect(&mut self, rect: Rect, color: Color);
    fn wireframe_rect_with_thickness(&mut self, rect: Rect, color: Color, thickness: f32);
    fn colored_rect(&mut self, rect: Rect, color: Color);
    fn textured_colored_rect(
        &mut self,
        rect: Rect,
        tex_coords: &RectTexCoords,
        texture: TextureHandle,
        color: Color,
    );
}

// ----------------------------------------------
// DebugDrawBackend
// ----------------------------------------------

struct DebugDrawBackend<RenderSystemBackendImpl> {
    render_system: UnsafeWeakRef<RenderSystemBackendImpl>,
}

impl<RenderSystemBackendImpl> DebugDrawBackend<RenderSystemBackendImpl>
where
    RenderSystemBackendImpl: RenderSystem,
{
    fn new(render_system: &RenderSystemBackendImpl) -> Self {
        Self { render_system: UnsafeWeakRef::new(render_system) }
    }
}

impl<RenderSystemBackendImpl> DebugDraw for DebugDrawBackend<RenderSystemBackendImpl>
where
    RenderSystemBackendImpl: RenderSystem,
{
    #[inline]
    fn point(&mut self, pt: Vec2, color: Color, size: f32) {
        self.render_system.draw_point_fast(pt, color, size);
    }

    #[inline]
    fn line(&mut self, from_pos: Vec2, to_pos: Vec2, from_color: Color, to_color: Color) {
        self.render_system.draw_line_fast(from_pos, to_pos, from_color, to_color);
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
    fn textured_colored_rect(
        &mut self,
        rect: Rect,
        tex_coords: &RectTexCoords,
        texture: TextureHandle,
        color: Color,
    ) {
        self.render_system.draw_textured_colored_rect(rect, tex_coords, texture, color);
    }
}
