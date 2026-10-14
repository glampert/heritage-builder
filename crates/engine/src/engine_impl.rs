use std::path::PathBuf;

use common::{
    Rect,
    Vec2,
    mem::RcMut,
    singleton_late_init,
    time::{FrameClock, Milliseconds, PerfTimer, Seconds},
};

use crate::{
    app::{Application, ApplicationEvent, ApplicationEventList, input::*},
    automation::{Request, RequestQueue, RequestSender},
    cli::{self, OneShotScreenshotOptions},
    config::EngineConfigs,
    log,
    platform,
    render::{RenderStats, RenderSystem, capture::CaptureRequest, debug::DebugDraw, texture::TextureCache},
    screenshot::{self, ScreenshotId, ScreenshotOutput, ScreenshotRequest, ScreenshotResult, ScreenshotScale, ScreenshotSystem},
    sound::SoundSystem,
    ui::UiSystem,
};

// ----------------------------------------------
// EngineSystemsMutRefs
// ----------------------------------------------

pub struct EngineSystemsMutRefs<'engine> {
    pub ui_sys: &'engine UiSystem,
    pub input_sys: &'engine InputSystem,
    pub sound_sys: &'engine mut SoundSystem,
    pub render_sys: &'engine mut RenderSystem,
    pub debug_draw: &'engine mut DebugDraw,
}

// ----------------------------------------------
// Engine
// ----------------------------------------------

pub struct Engine {
    render_system: RcMut<RenderSystem>,
    render_stats: RenderStats,

    ui_system: UiSystem,
    sound_system: SoundSystem,
    debug_draw: DebugDraw,

    frame_clock: FrameClock,
    frame_events: ApplicationEventList,

    screenshots: ScreenshotSystem,
    screenshot_name: String, // File name prefix, from the window title.
    one_shot_screenshot: Option<OneShotScreenshot>,

    automation_requests: RequestQueue,

    #[cfg(feature = "desktop")]
    automation_server: Option<crate::automation::server::AutomationServer>,

    exit_code: i32,

    // Declared last so it's dropped last: it owns the window and graphics context,
    // which the render resources above still need while they're being destroyed.
    app: RcMut<Application>,
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

    // Makes every frame advance by a fixed time step instead of the measured frame time,
    // so frame-driven updates repeat exactly from run to run. None goes back to real time.
    #[inline]
    pub fn set_fixed_delta_time(&mut self, fixed_delta_time: Option<Seconds>) {
        self.frame_clock.set_fixed_delta_time(fixed_delta_time);
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
    pub fn systems_mut_refs(&mut self) -> EngineSystemsMutRefs<'_> {
        EngineSystemsMutRefs {
            ui_sys: &self.ui_system,
            input_sys: self.app.input_system(),
            sound_sys: &mut self.sound_system,
            render_sys: &mut self.render_system,
            debug_draw: &mut self.debug_draw,
        }
    }

    #[inline]
    pub fn is_running(&self) -> bool {
        !self.app.should_quit()
    }

    // Quits after this frame. On desktop, the process exits with `exit_code` once the engine has
    // shut down. A non-zero code isn't overwritten by a later request.
    pub fn request_exit(&mut self, exit_code: i32) {
        if self.exit_code == 0 {
            self.exit_code = exit_code;
        }
        self.app.request_quit();
    }

    #[inline]
    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }

    // Called by the runner once the run loop has stopped, before it reads exit_code().
    pub fn finish_run(&mut self) {
        // A run that quit before `--screenshot-after-frames` saved its PNG failed, even if
        // something else (the window closing, the smoke test ending) asked to quit.
        if self.one_shot_screenshot.is_some() {
            self.screenshots.wait_for_writes();
            self.finish_one_shot_screenshot();

            if self.one_shot_screenshot.take().is_some() {
                log::error!(log::channel!("engine"), "--screenshot-after-frames: quit before the screenshot was saved.");
                self.request_exit(1);
            }
        }
    }

    // ----------------------
    // Screenshots:
    // ----------------------

    // Captures the frame being drawn and saves it as a PNG (see screenshot.rs). Poll
    // take_screenshot_result: the result is ready once the PNG is written, usually after a frame.
    pub fn request_screenshot(&mut self, request: ScreenshotRequest) -> ScreenshotId {
        let id = self.render_system.request_capture(CaptureRequest { include_ui: request.include_ui });
        self.screenshots.add_request(id, request.scale, request.output);
        id
    }

    // Each result is returned once.
    #[inline]
    pub fn take_screenshot_result(&mut self, id: ScreenshotId) -> Option<ScreenshotResult> {
        self.screenshots.take_result(id)
    }

    // A path for a new screenshot in screenshots/, named after the game and the local time.
    #[inline]
    pub fn new_screenshot_path(&self) -> PathBuf {
        self.screenshots.new_screenshot_path(&self.screenshot_name)
    }

    // ----------------------
    // Automation:
    // ----------------------

    // For transports that send automation requests to the game (see automation/mod.rs).
    #[inline]
    pub fn automation_request_sender(&self) -> RequestSender {
        self.automation_requests.sender()
    }

    // Automation requests queued since the last call, oldest first.
    #[inline]
    pub fn take_automation_requests(&mut self) -> Vec<Request> {
        self.automation_requests.take_requests()
    }

    // Notifies the clients of the automation server (--automation) of a game event, such as
    // "session.changed". Does nothing without a server.
    pub fn send_automation_event(&self, method: &str, params: serde_json::Value) {
        #[cfg(feature = "desktop")]
        if let Some(server) = &self.automation_server {
            server.send_event(method, params);
        }

        #[cfg(not(feature = "desktop"))]
        let _ = (method, params);
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
        platform::set_crash_report_frame(self.frame_clock.frame_number());

        self.frame_events = self.poll_app_events();

        let input_sys = self.app.input_system();

        self.render_system.begin_frame(self.app.window_size(), self.app.framebuffer_size());
        self.ui_system.begin_frame(&self.app, input_sys, self.frame_clock.delta_time());

        let begin_frame_time_ms = begin_frame_timer.end();

        (self.frame_clock.delta_time(), input_sys.cursor_pos(), begin_frame_time_ms)
    }

    pub fn end_frame(&mut self) -> (Milliseconds, Milliseconds) {
        let end_frame_timer = PerfTimer::begin();

        self.request_one_shot_screenshot();

        // Keep toasts out of screenshots.
        let draw_toasts = !self.render_system.has_pending_ui_capture();

        let mut ui_frame_bundle = self.ui_system.end_frame(self.render_system.clone(), draw_toasts);
        self.render_stats = self.render_system.end_frame(&mut ui_frame_bundle);

        let present_timer = PerfTimer::begin();
        self.app.present();
        let present_frame_time_ms = present_timer.end();

        for (id, result) in self.render_system.take_finished_captures() {
            self.screenshots.on_capture_finished(id, result);
        }
        self.screenshots.update();
        self.finish_one_shot_screenshot();

        self.frame_events.clear();
        self.frame_clock.end_frame();

        let end_frame_time_ms = end_frame_timer.end();

        (end_frame_time_ms, present_frame_time_ms)
    }

    // ----------------------
    // Initialization:
    // ----------------------

    pub fn start(
        configs: &EngineConfigs,
        app: RcMut<Application>,
        render_system: RcMut<RenderSystem>,
    ) -> &'static mut Engine {
        let mut engine = Self::new(configs, app, render_system);

        if let Some(fixed_delta_time) = cli::fixed_delta_time() {
            log::info!(log::channel!("engine"), "Using a fixed frame time step of {fixed_delta_time}s.");
            engine.set_fixed_delta_time(Some(fixed_delta_time));
        }

        engine.one_shot_screenshot =
            cli::one_shot_screenshot().map(|options| OneShotScreenshot { options, id: None });

        #[cfg(feature = "desktop")]
        if let Some(port) = cli::automation_port() {
            let server = crate::automation::server::AutomationServer::start(port, engine.automation_requests.sender())
                .unwrap_or_else(|err| cli::exit_with_error(&err));

            log::info!(
                log::channel!("engine"),
                "Automation server listening on 127.0.0.1:{} (details in {}).",
                server.port(),
                server.info_file().display()
            );
            engine.automation_server = Some(server);
        }

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
        crate::ui::sound::initialize(&mut sound_system);
        log::info!(log::channel!("engine"), "SoundSystem initialized.");

        let debug_draw = DebugDraw::new(render_system.clone());
        log::info!(log::channel!("engine"), "DebugDraw initialized.");

        Self {
            app,
            render_system,
            render_stats: RenderStats::default(),
            ui_system,
            sound_system,
            debug_draw,
            frame_clock: FrameClock::new(),
            frame_events: ApplicationEventList::new(),
            screenshots: ScreenshotSystem::new(),
            screenshot_name: configs.window_title.clone(),
            one_shot_screenshot: None,
            automation_requests: RequestQueue::new(),
            #[cfg(feature = "desktop")]
            automation_server: None,
            exit_code: 0,
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
                    let ui_input = self.ui_system.on_key_input(key, action, modifiers);

                    // Screenshot keys reach the run loop even while the UI has keyboard focus.
                    if ui_input.not_handled() || screenshot::is_hotkey(key) {
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

    // `--screenshot-after-frames`: requests the capture on the chosen frame...
    fn request_one_shot_screenshot(&mut self) {
        let frame_number = self.frame_clock.frame_number();

        let request = match &self.one_shot_screenshot {
            Some(shot) if shot.id.is_none() && frame_number >= shot.options.after_frames => ScreenshotRequest {
                include_ui: shot.options.include_ui,
                scale: ScreenshotScale::Pixels,
                output: ScreenshotOutput::File(shot.options.path.clone()),
            },
            _ => return,
        };

        let id = self.request_screenshot(request);
        if let Some(shot) = &mut self.one_shot_screenshot {
            shot.id = Some(id);
        }
    }

    // ...and quits once the PNG is written.
    fn finish_one_shot_screenshot(&mut self) {
        let Some(id) = self.one_shot_screenshot.as_ref().and_then(|shot| shot.id) else {
            return;
        };

        let Some(result) = self.screenshots.take_result(id) else {
            return;
        };

        self.one_shot_screenshot = None;

        match result {
            Ok(screenshot) => {
                let path = screenshot.path.unwrap_or_default();
                log::info!(log::channel!("engine"), "Saved screenshot {} ({}).", path.display(), screenshot.size);
                self.request_exit(0);
            }
            Err(err) => {
                log::error!(log::channel!("engine"), "--screenshot-after-frames failed: {err}");
                self.request_exit(1);
            }
        }
    }
}

struct OneShotScreenshot {
    options: OneShotScreenshotOptions,
    id: Option<ScreenshotId>, // Set once requested.
}

// ----------------------------------------------
// Engine Global Singleton
// ----------------------------------------------

singleton_late_init! { ENGINE_SINGLETON, Engine }
