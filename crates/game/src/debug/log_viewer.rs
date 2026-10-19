// Log viewer and in-game console. The window lists recent log lines, and its input field runs
// automation commands (see crate::automation). Typed commands are queued here and run by the
// GameLoop at its command dispatch point, never while the window draws: the viewer's lock is
// held for the whole draw, and commands log.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Mutex, OnceLock},
};

use arrayvec::ArrayString;
use common::{Vec2, format_fixed_string};
use engine::{
    log,
    ui::{self, UiCondition, UiScrollArea, UiSystem, UiTextEdit, UiTextHistory, UiWindow},
};

// Console input and command results are shown under this channel. They aren't logged.
const CONSOLE_CHANNEL: log::Channel = log::channel!("console");

const MAX_CONSOLE_HISTORY: usize = 64;

// ----------------------------------------------
// LogViewerSingleton
// ----------------------------------------------

struct LogViewerSingleton {
    is_window_open: bool,
    is_channel_filter_window_open: bool,
    auto_scroll: bool,
    error_count: u32,
    warning_count: u32,
    max_lines: usize,
    lines: VecDeque<log::Record>,
    channel_filter: HashMap<log::Channel, bool>,

    // Console:
    console_input: String,
    console_history: Vec<String>, // Oldest first.
    console_history_pos: Option<usize>,
    console_queue: Vec<String>, // Submitted, waiting for the GameLoop to run them.
    scroll_to_bottom: bool,
    focus_console_input: bool,
}

impl LogViewerSingleton {
    fn new(start_open: bool, max_lines: usize) -> Self {
        Self {
            is_window_open: start_open,
            is_channel_filter_window_open: false,
            auto_scroll: false,
            error_count: 0,
            warning_count: 0,
            max_lines,
            lines: VecDeque::with_capacity(max_lines),
            channel_filter: HashMap::new(),
            console_input: String::new(),
            console_history: Vec::new(),
            console_history_pos: None,
            console_queue: Vec::new(),
            scroll_to_bottom: false,
            focus_console_input: start_open,
        }
    }

    // Called by the log listener callback to push new messages.
    fn push_line(&mut self, line: log::Record) {
        if let Some(channel) = &line.channel {
            if !self.channel_filter.contains_key(channel) {
                self.channel_filter.insert(*channel, true); // Defaults to enabled.
            }
        }

        match line.level {
            log::Level::Error => self.error_count += 1,
            log::Level::Warning => self.warning_count += 1,
            _ => {}
        }

        self.append_line(line);
        self.auto_scroll = true;
    }

    // Console lines don't count towards the error and warning totals.
    fn push_console_line(&mut self, level: log::Level, message: String) {
        self.channel_filter.entry(CONSOLE_CHANNEL).or_insert(true);

        self.append_line(log::Record {
            seq: 0,
            level,
            channel: Some(CONSOLE_CHANNEL),
            location: log::Location { file: file!(), line: line!(), module: module_path!() },
            message,
        });

        self.scroll_to_bottom = true;
    }

    fn append_line(&mut self, line: log::Record) {
        if self.lines.len() == self.max_lines {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    fn submit_console_input(&mut self) {
        let text = self.console_input.trim().to_string();
        self.console_input.clear();
        self.console_history_pos = None;

        if text.is_empty() {
            return;
        }

        if self.console_history.last() != Some(&text) {
            if self.console_history.len() == MAX_CONSOLE_HISTORY {
                self.console_history.remove(0);
            }
            self.console_history.push(text.clone());
        }

        self.push_console_line(log::Level::Info, format!("> {text}"));
        self.console_queue.push(text);
    }

    fn is_channel_enabled(&self, channel: &Option<log::Channel>) -> bool {
        if let Some(channel) = channel {
            if let Some(is_enabled) = self.channel_filter.get(channel) {
                return *is_enabled;
            }
        }
        // Messages without channel or unknown channels are always shown.
        true
    }

    fn set_enabled_channels(&mut self, channels: &[(log::Channel, bool)]) {
        for (channel, is_enabled) in channels {
            self.channel_filter.insert(*channel, *is_enabled);
        }
    }

    fn show(&mut self, show: bool) {
        if show && !self.is_window_open {
            self.focus_console_input = true;
        }
        self.is_window_open = show;
    }

    fn draw(&mut self, ui_sys: &UiSystem) -> bool {
        let mut is_window_open = self.is_window_open;
        if !is_window_open {
            return false;
        }

        // The viewer draws in every menus mode; keep the dev font so lines stay compact.
        let window = UiWindow::new("Log Viewer")
            .open(&mut is_window_open)
            .position(Vec2::new(10.0, 20.0), UiCondition::FirstUse)
            .size(Vec2::new(550.0, 350.0), UiCondition::FirstUse)
            .menu_bar()
            .no_scrollbar() // Its log lines scroll, above the console input.
            .monospace();

        // Recording the viewer's widgets doesn't log, which would deadlock on the viewer's lock.
        ui::window(ui_sys, window, |ui_sys| {
            ui::window_menu_bar(ui_sys, |ui_sys| {
                ui::menu(ui_sys, "Filter", |ui_sys| {
                    if ui::menu_item(ui_sys, "Channels") {
                        self.is_channel_filter_window_open = true;
                    }
                });
            });

            // Checked before the scroll area, which takes focus from this window.
            let is_window_focused = ui::is_window_focused(ui_sys);

            // Log lines, in a scroll area above the console input.
            let footer_height = ui::item_spacing(ui_sys).y + ui::frame_height_with_spacing(ui_sys);
            let log_lines = UiScrollArea::new("##log_lines").leave_at_bottom(footer_height).horizontal_scrollbar();

            ui::scroll_area(ui_sys, log_lines, |ui_sys| {
                for line in &self.lines {
                    if !self.is_channel_enabled(&line.channel) {
                        continue;
                    }

                    ui::horizontal(ui_sys, |ui_sys| {
                        ui::text_colored(ui_sys, line.level.color(), Self::line_prefix(line));
                        ui::text(ui_sys, &line.message);
                    });
                }

                // Auto-scroll to bottom if we just added something.
                if self.scroll_to_bottom || (is_window_focused && self.auto_scroll) {
                    ui::scroll_to_bottom(ui_sys);
                    self.auto_scroll = false;
                    self.scroll_to_bottom = false;
                }
            });

            self.draw_console_input(ui_sys);
        });

        // The channel filter is a window of its own.
        if !is_window_open {
            self.is_channel_filter_window_open = false;
        } else if self.is_channel_filter_window_open {
            self.draw_channel_filter_window(ui_sys);
        }

        self.is_window_open = is_window_open;
        is_window_open
    }

    // ui.set fills it in (path "Log Viewer/Console"); Enter runs it. Up and down step through
    // earlier commands.
    fn draw_console_input(&mut self, ui_sys: &UiSystem) {
        let input = UiTextEdit::new("Console")
            .hide_label()
            .hint("Command, e.g. help, or help command=sim.speed")
            .submit_on_enter()
            .full_width()
            .focus(self.focus_console_input)
            .history(UiTextHistory { entries: &self.console_history, position: &mut self.console_history_pos });

        self.focus_console_input = false;

        if ui::text_edit(ui_sys, input, &mut self.console_input).submitted {
            self.submit_console_input();
            self.focus_console_input = true; // Enter takes focus away from the field.
        }
    }

    fn line_prefix(line: &log::Record) -> ArrayString<128> {
        let chan_str = line.channel.as_ref().map(|chan| chan.name).unwrap_or_default();
        format_fixed_string!(128, "[{:?}]{}", line.level, chan_str)
    }

    fn draw_channel_filter_window(&mut self, ui_sys: &UiSystem) {
        let window = UiWindow::new("Log Channel Filter")
            .open(&mut self.is_channel_filter_window_open)
            .size(Vec2::new(250.0, 300.0), UiCondition::FirstUse);

        ui::window(ui_sys, window, |ui_sys| {
            for (channel, is_enabled) in self.channel_filter.iter_mut() {
                ui::checkbox(ui_sys, channel.name, is_enabled);
            }
        });
    }
}

// ----------------------------------------------
// Global instance:
// ----------------------------------------------

// Thread-safe log viewer global instance, to support logging from workers threads.
static LOG_VIEWER_SINGLETON: OnceLock<Mutex<LogViewerSingleton>> = OnceLock::new();

fn init_log_viewer_singleton_once() {
    if LOG_VIEWER_SINGLETON.get().is_some() {
        return; // Already initialized, early out.
    }

    const START_OPEN: bool = false;
    const MAX_LINES: usize = 128;
    let viewer = LogViewerSingleton::new(START_OPEN, MAX_LINES);

    LOG_VIEWER_SINGLETON.set(Mutex::new(viewer)).unwrap_or_else(|_| panic!("LogViewerSingleton already initialized!"));

    log::add_listener(|record| {
        let mut viewer = log_viewer_singleton().lock().unwrap();

        viewer.push_line(record.clone());
    });
}

fn log_viewer_singleton() -> &'static Mutex<LogViewerSingleton> {
    LOG_VIEWER_SINGLETON.get().expect("LogViewerSingleton is not initialized!")
}

// ----------------------------------------------
// LogViewer
// ----------------------------------------------

pub struct LogViewer;

impl LogViewer {
    pub fn initialize() {
        init_log_viewer_singleton_once();
    }

    pub fn new() -> Self {
        init_log_viewer_singleton_once();
        Self
    }

    pub fn error_and_warning_count(&self) -> (u32, u32) {
        let viewer = log_viewer_singleton().lock().unwrap();

        (viewer.error_count, viewer.warning_count)
    }

    pub fn show(&mut self, show: bool) {
        let mut viewer = log_viewer_singleton().lock().unwrap();

        viewer.show(show);
    }

    pub fn is_open(&self) -> bool {
        let viewer = log_viewer_singleton().lock().unwrap();

        viewer.is_window_open
    }

    // Commands typed into the console since the last call, oldest first.
    // Empty if the viewer was never initialized (in tests, for instance).
    pub fn take_console_input() -> Vec<String> {
        LOG_VIEWER_SINGLETON
            .get()
            .map(|viewer| std::mem::take(&mut viewer.lock().unwrap().console_queue))
            .unwrap_or_default()
    }

    // Shows a command result in the console. Must not be called while the viewer draws.
    pub fn print_console_output(level: log::Level, text: &str) {
        if let Some(viewer) = LOG_VIEWER_SINGLETON.get() {
            viewer.lock().unwrap().push_console_line(level, text.to_string());
        }
    }

    pub fn clear() {
        if let Some(viewer) = LOG_VIEWER_SINGLETON.get() {
            viewer.lock().unwrap().lines.clear();
        }
    }

    pub fn set_enabled_channels(&mut self, channels: &[(log::Channel, bool)]) {
        let mut viewer = log_viewer_singleton().lock().unwrap();

        viewer.set_enabled_channels(channels);
    }

    pub fn draw(&mut self, ui_sys: &UiSystem) -> bool {
        let mut viewer = log_viewer_singleton().lock().unwrap();

        viewer.draw(ui_sys)
    }
}
