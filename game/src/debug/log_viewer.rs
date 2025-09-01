use std::collections::{VecDeque, HashMap};
use crate::{log, imgui_ui::UiSystem, utils::SingleThreadStatic};

// ----------------------------------------------
// LogViewerSingleton
// ----------------------------------------------

struct LogViewerSingleton {
    is_window_open: bool,
    is_channel_filter_window_open: bool,
    auto_scroll: bool,
    max_lines: usize,
    lines: VecDeque<log::Record>,
    channel_filter: HashMap<log::Channel, bool>,
}

impl LogViewerSingleton {
    fn new(start_open: bool, max_lines: usize) -> Self {
        Self {
            is_window_open: start_open,
            is_channel_filter_window_open: false,
            auto_scroll: false,
            max_lines,
            lines: VecDeque::with_capacity(max_lines),
            channel_filter: HashMap::new(),
        }
    }

    // Called by the log listener callback to push new messages.
    fn push_line(&mut self, line: log::Record) {
        if let Some(channel) = &line.channel {
            if !self.channel_filter.contains_key(channel) {
                self.channel_filter.insert(*channel, true); // Defaults to enabled.
            }
        }

        if self.lines.len() == self.max_lines {
            self.lines.pop_front();
        }

        self.lines.push_back(line);
        self.auto_scroll = true;
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

    fn draw(&mut self, ui_sys: &UiSystem) {
        let mut is_window_open = self.is_window_open;
        if !is_window_open {
            return;
        }

        let ui = ui_sys.builder();

        ui.window("Log Viewer")
            .opened(&mut is_window_open)
            .position([250.0, 5.0], imgui::Condition::FirstUseEver)
            .size([550.0, 350.0], imgui::Condition::FirstUseEver)
            .horizontal_scrollbar(true)
            .menu_bar(true)
            .build(|| {
                // Draw menu bar:
                if let Some(_menu_bar) = ui.begin_menu_bar() {
                    if let Some(_menu) = ui.begin_menu("Filter") {
                        if ui.menu_item("Channels") {
                            self.is_channel_filter_window_open = true;
                        }
                    }
                }

                // Draw log lines:
                for line in &self.lines {
                    if !self.is_channel_enabled(&line.channel) {
                        continue;
                    }

                    let color = line.level.color();
                    ui.text_colored(color.to_array(), Self::line_prefix(line));
                    ui.same_line();
                    ui.text(&line.message);
                }

                // Auto-scroll to bottom if we just added something.
                if ui.is_window_focused() && self.auto_scroll {
                    ui.set_scroll_here_y_with_ratio(1.0);
                    self.auto_scroll = false;
                }

                if self.is_channel_filter_window_open {
                    self.draw_channel_filter_child_window(ui);
                }
            });
    
        if !is_window_open {
            self.is_channel_filter_window_open = false;
        }

        self.is_window_open = is_window_open;
    }

    fn line_prefix(line: &log::Record) -> String {
        let chan_str = line.channel
            .as_ref()
            .map(|chan| chan.name)
            .unwrap_or_default();

        format!("[{:?}]{}", line.level, chan_str)
    }

    fn draw_channel_filter_child_window(&mut self, ui: &imgui::Ui) {
        ui.window("Log Channel Filter")
            .opened(&mut self.is_channel_filter_window_open)
            .size([250.0, 300.0], imgui::Condition::FirstUseEver)
            .build(|| {
                for (channel, is_enabled) in self.channel_filter.iter_mut() {
                    ui.checkbox(channel.name, is_enabled);
                }
            });
    }
}

// Global instance:
static LOG_VIEWER_SINGLETON: SingleThreadStatic<Option<LogViewerSingleton>> = SingleThreadStatic::new(None);

// ----------------------------------------------
// LogViewerWindow
// ----------------------------------------------

pub struct LogViewerWindow;

impl LogViewerWindow {
    pub fn new(start_open: bool, max_lines: usize) -> Self {
        if LOG_VIEWER_SINGLETON.is_some() {
            panic!("Log Viewer singleton already initialized!");
        }

        LOG_VIEWER_SINGLETON.set(Some(
            LogViewerSingleton::new(start_open, max_lines)
        ));

        log::set_listener(|line| {
            if let Some(viewer) = LOG_VIEWER_SINGLETON.as_mut() {
                viewer.push_line(line);
            }
        });

        Self
    }

    pub fn show(&self, show: bool) {
        if let Some(viewer) = LOG_VIEWER_SINGLETON.as_mut() {
            viewer.is_window_open = show;
        }
    }

    pub fn set_enabled_channels(&self, channels: &[(log::Channel, bool)]) {
        if let Some(viewer) = LOG_VIEWER_SINGLETON.as_mut() {
            viewer.set_enabled_channels(channels);
        }
    }

    pub fn draw(&self, ui_sys: &UiSystem) -> bool {
        if let Some(viewer) = LOG_VIEWER_SINGLETON.as_mut() {
            viewer.draw(ui_sys);
            return viewer.is_window_open;
        }
        false
    }
}
