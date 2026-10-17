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
    automation::{
        Request,
        RequestQueue,
        RequestSender,
        Responder,
        input::{GestureId, InputStep, InputTimeline},
        recording::{Recorder, RecordingEnd, Replay},
    },
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

    // Simulated input (see automation/input.rs).
    input_captured: bool,
    input_timeline: InputTimeline,

    // --record and --replay (see automation/recording.rs).
    recorder: Option<Recorder>,
    replay: Option<Replay>,
    final_checksum: Option<String>, // From the run loop, for the recording's end.

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

        let frames = self.frame_clock.frame_number();
        let checksum = self.final_checksum.take();

        if let Some(recorder) = &mut self.recorder {
            if !recorder.has_started() {
                recorder.start(self.app.window_size(), self.app.content_scale(), self.input_captured);
            }
            recorder.finish(RecordingEnd { frames, checksum: checksum.clone() });
            log::info!(log::channel!("engine"), "Recorded {frames} frames to {}.", recorder.path().display());
        }

        if let Some(replay) = self.replay.take() {
            self.finish_replay(&replay, frames, checksum);
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

    // Lets automation drive the input: the InputSystem reports a virtual mouse and keyboard, real
    // input is ignored (except quitting and resizing), and cursor confinement stops moving the real
    // cursor. Turning it off drops unfinished gestures and gives the real devices back.
    pub fn set_input_capture(&mut self, capture: bool) {
        if capture == self.input_captured {
            return;
        }

        self.input_captured = capture;

        if capture {
            self.app.input_system_mut().set_virtual_input(Some(VirtualInput::new()));
        } else {
            self.input_timeline.cancel();
            self.app.input_system_mut().set_virtual_input(None);
        }

        self.app.pause_cursor_confinement(capture || self.replay.is_some());
        log::info!(log::channel!("engine"), "Input capture {}.", if capture { "on" } else { "off" });
    }

    #[inline]
    pub fn is_input_captured(&self) -> bool {
        self.input_captured
    }

    // Plays a gesture (see automation/input.rs) on the virtual input, starting next frame. Turns
    // input capture on.
    pub fn queue_input_gesture(&mut self, steps: Vec<InputStep>) -> GestureId {
        self.set_input_capture(true);
        self.input_timeline.push_gesture(steps)
    }

    #[inline]
    pub fn is_input_gesture_done(&self, id: GestureId) -> bool {
        self.input_timeline.is_gesture_done(id)
    }

    #[inline]
    pub fn app_events(&self) -> &ApplicationEventList {
        &self.frame_events
    }

    // ----------------------
    // Record and replay:
    // ----------------------

    #[inline]
    pub fn is_recording(&self) -> bool {
        self.recorder.is_some()
    }

    #[inline]
    pub fn is_replaying(&self) -> bool {
        self.replay.is_some()
    }

    // --record: adds run loop details to the recording's header, such as its configs. Call
    // before the first frame.
    pub fn set_recording_info(&mut self, key: &str, value: serde_json::Value) {
        if let Some(recorder) = &mut self.recorder {
            recorder.set_info(key, value);
        }
    }

    // --replay: details the run loop added to the recording (see set_recording_info).
    pub fn replay_info(&self, key: &str) -> Option<&serde_json::Value> {
        self.replay.as_ref()?.header().info.get(key)
    }

    // --record: notes an automation command as it starts. Only commands from outside the game's
    // input: one typed into the game's UI comes back when the replay types it again.
    pub fn record_command(&mut self, method: &str, params: &serde_json::Value) {
        if let Some(recorder) = &mut self.recorder {
            recorder.record_command(self.frame_clock.frame_number(), method, params);
        }
    }

    // True when the run loop should hand its final state checksum to set_final_checksum.
    #[inline]
    pub fn needs_final_checksum(&self) -> bool {
        self.recorder.is_some() || self.replay.is_some()
    }

    // The run loop's hash of its state once the loop has stopped (before finish_run). A
    // recording stores it; a replay must end with the same one.
    pub fn set_final_checksum(&mut self, checksum: Option<String>) {
        self.final_checksum = checksum;
    }

    // ----------------------
    // Begin/End Frame:
    // ----------------------

    pub fn begin_frame(&mut self) -> (Seconds, Vec2, Milliseconds) {
        let begin_frame_timer = PerfTimer::begin();

        self.frame_clock.begin_frame();
        let frame = self.frame_clock.frame_number();
        platform::set_crash_report_frame(frame);

        // A replay sets the frame's time step, devices and commands first.
        let replayed_events = self.begin_replay_frame(frame);

        // Simulated input next, so this frame's events and the UI see it.
        let simulated_events = match self.app.input_system_mut().virtual_input_mut() {
            Some(virtual_input) => self.input_timeline.apply_frame(virtual_input),
            None => ApplicationEventList::new(),
        };

        let device_events = match replayed_events {
            Some(events) => events,
            None => self.poll_device_events(frame),
        };

        self.frame_events = self.handle_app_events(device_events, simulated_events);

        let input_sys = self.app.input_system();

        self.render_system.begin_frame(self.app.window_size(), self.app.framebuffer_size());
        self.ui_system.begin_frame(&self.app, input_sys, self.frame_clock.delta_time());

        let begin_frame_time_ms = begin_frame_timer.end();

        (self.frame_clock.delta_time(), input_sys.cursor_pos(), begin_frame_time_ms)
    }

    pub fn end_frame(&mut self) -> (Milliseconds, Milliseconds) {
        let end_frame_timer = PerfTimer::begin();

        self.request_one_shot_screenshot();

        // Keep toasts and the badge out of screenshots.
        let draw_overlays = !self.render_system.has_pending_ui_capture();
        let badge = self.badge_text();

        let mut ui_frame_bundle = self.ui_system.end_frame(self.render_system.clone(), draw_overlays, badge.as_deref());
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

        if cli::get().has("capture-input") {
            engine.set_input_capture(true);
        }

        if let Some(path) = cli::path_value("record") {
            let recorder = Recorder::create(&path, cli::recording_args()).unwrap_or_else(|err| cli::exit_with_error(&err));
            log::info!(log::channel!("engine"), "Recording the run to {}.", path.display());
            engine.recorder = Some(recorder);
        }

        if let Some(replay) = cli::take_replay() {
            engine.start_replay(replay);
        }

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
            input_captured: false,
            input_timeline: InputTimeline::default(),
            recorder: None,
            replay: None,
            final_checksum: None,
            exit_code: 0,
        }
    }

    // ----------------------
    // Internal:
    // ----------------------

    // The real devices' events. While recording, also samples the devices' state for the frame,
    // so every read this frame sees the same state, and records both.
    fn poll_device_events(&mut self, frame: u64) -> ApplicationEventList {
        let events = self.app.poll_events();

        if let Some(recorder) = &mut self.recorder {
            if !recorder.has_started() {
                recorder.start(self.app.window_size(), self.app.content_scale(), self.input_captured);
            }

            let devices = self.app.input_system().sample_devices();
            recorder.record_frame(frame, self.frame_clock.delta_time(), &devices, &events);
            self.app.input_system_mut().set_device_state(Some(devices));
        }

        events
    }

    // Forwards this frame's events: from the real devices (or a replay's recording) and simulated.
    #[must_use]
    fn handle_app_events(
        &mut self,
        real_events: ApplicationEventList,
        simulated_events: ApplicationEventList,
    ) -> ApplicationEventList {
        let mut events_forwarded = ApplicationEventList::new();

        // While automation drives the input, real input events only quit or resize, except for
        // Shift+Esc, which lets a person take over.
        if self.input_captured {
            let take_over = real_events.iter().any(is_take_over_key);
            if take_over {
                log::info!(log::channel!("engine"), "Shift+Esc: taking input back from automation.");
                self.set_input_capture(false);
            }
        }

        let input_captured = self.input_captured;
        let events: ApplicationEventList = real_events
            .into_iter()
            .filter(|event| !input_captured || matches!(event, ApplicationEvent::Quit | ApplicationEvent::WindowResize { .. }))
            .chain(simulated_events)
            .collect();

        for event in events {
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

    fn start_replay(&mut self, replay: Replay) {
        let header = replay.header();
        let channel = log::channel!("engine");

        match replay.end() {
            Some(end) => log::info!(channel, "Replaying {} ({} frames).", replay.path().display(), end.frames),
            None => log::warning!(
                channel,
                "Replaying {}. It has no end line: the recorded run crashed or was killed. The input comes back to you after its last frame.",
                replay.path().display()
            ),
        }

        let window_size = self.app.window_size();
        if [window_size.width, window_size.height] != header.window_size {
            log::warning!(
                channel,
                "The window is {window_size}, not the recorded {}x{}. The window size is part of the game state, so the replay will differ.",
                header.window_size[0],
                header.window_size[1]
            );
        }

        let content_scale = self.app.content_scale();
        if [content_scale.x, content_scale.y] != header.content_scale {
            log::warning!(
                channel,
                "The display scale is {content_scale}, not the recorded {:?}: the UI is laid out differently, so recorded clicks may land elsewhere.",
                header.content_scale
            );
        }

        self.replay = Some(replay);
        self.app.input_system_mut().set_device_state(Some(VirtualInput::new()));
        self.app.pause_cursor_confinement(true);
    }

    // --replay: sets this frame's recorded time step and device state, queues its commands, and
    // returns its events. Real input is ignored, except quitting, resizing, and Shift+Esc, which
    // stops the replay. None when not replaying.
    fn begin_replay_frame(&mut self, frame: u64) -> Option<ApplicationEventList> {
        let mut replay = self.replay.take()?;
        let channel = log::channel!("engine");

        let real_events = self.app.poll_events();

        if real_events.iter().any(is_take_over_key) {
            log::info!(channel, "Shift+Esc: replay stopped at frame {frame}. The input is yours.");
            self.stop_replay();
            return Some(real_events);
        }

        // A recording without an end line: after its last frame, the input comes back.
        if replay.end().is_none() && replay.last_frame().is_none_or(|last_frame| frame > last_frame) {
            log::warning!(channel, "Replay done at frame {frame}, where the recording ends. The input is yours.");
            self.stop_replay();
            return Some(real_events);
        }

        if frame == 0 {
            self.set_input_capture(replay.header().input_captured);
        }

        let replay_frame = replay.next_frame(frame);
        self.frame_clock.set_replay_delta_time(replay_frame.dt);
        self.app.input_system_mut().set_device_state(Some(replay_frame.devices));

        for (method, params) in replay_frame.commands {
            let responder = {
                let method = method.clone();
                Responder::new(move |response| {
                    // Cancelled: the game stopped first.
                    if let Err(err) = response.result
                        && response.frame.is_some()
                    {
                        log::warning!(log::channel!("engine"), "Replayed command '{method}' failed: {}: {}", err.code, err.message);
                    }
                })
            };
            self.automation_requests.sender().send(Request { method, params, responder });
        }

        let mut events = ApplicationEventList::new();

        for event in real_events {
            match event {
                ApplicationEvent::Quit => events.push(event),
                ApplicationEvent::WindowResize { window_size, .. } => {
                    log::warning!(channel, "The window was resized to {window_size} during the replay, so it may differ from here on.");
                    events.push(event);
                }
                _ => {}
            }
        }

        for event in replay_frame.events {
            if let ApplicationEvent::WindowResize { window_size, .. } = event {
                log::warning!(
                    channel,
                    "The recorded run resized the window to {window_size} at frame {frame}, which a replay can't do. It may differ from here on."
                );
                continue;
            }
            events.push(event);
        }

        // Quit after the recorded run's last frame.
        if replay.end().is_some_and(|end| frame + 1 >= end.frames) {
            self.request_exit(0);
        }

        self.replay = Some(replay);
        Some(events)
    }

    // Gives the input back to the real devices.
    fn stop_replay(&mut self) {
        self.replay = None;
        self.frame_clock.set_replay_delta_time(None);
        self.app.input_system_mut().set_device_state(None);
        self.set_input_capture(false);
        self.app.pause_cursor_confinement(self.input_captured);
    }

    // Exit status 0 if the replay ended like the recorded run, 1 if not.
    fn finish_replay(&mut self, replay: &Replay, frames: u64, checksum: Option<String>) {
        let Some(end) = replay.end() else {
            return; // Nothing to compare with.
        };

        let channel = log::channel!("engine");
        let show = |checksum: &Option<String>| checksum.clone().unwrap_or_else(|| "none".into());

        if frames == end.frames && checksum == end.checksum {
            log::info!(channel, "Replay matched the recording: {frames} frames, final state checksum {}.", show(&checksum));
            self.exit_code = 0;
            return;
        }

        if frames != end.frames {
            log::error!(channel, "Replay ran {frames} frames, but the recorded run lasted {}.", end.frames);
        }
        if checksum != end.checksum {
            log::error!(
                channel,
                "Replay ended in a different state: checksum {} instead of the recorded {}.",
                show(&checksum),
                show(&end.checksum)
            );
        }
        self.exit_code = 1;
    }

    fn badge_text(&self) -> Option<String> {
        if let Some(replay) = &self.replay {
            let frame = self.frame_clock.frame_number();
            return Some(match replay.end() {
                Some(end) => format!("REPLAY: frame {frame} of {}. Shift+Esc to stop", end.frames),
                None => format!("REPLAY: frame {frame}. Shift+Esc to stop"),
            });
        }

        self.input_captured.then(|| "AUTOMATION: input captured. Shift+Esc to take over".to_string())
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

// Shift+Esc takes the input back from automation, and stops a replay.
fn is_take_over_key(event: &ApplicationEvent) -> bool {
    matches!(event, ApplicationEvent::KeyInput(InputKey::Escape, InputAction::Press, modifiers)
        if modifiers.intersects(InputModifiers::Shift))
}

// ----------------------------------------------
// Engine Global Singleton
// ----------------------------------------------

singleton_late_init! { ENGINE_SINGLETON, Engine }
