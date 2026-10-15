// Widget registry for UI automation: each frame, the retained widgets (widgets.rs) record what they
// drew, so automation can list them (ui.tree) and click or set them by name (ui.click, ui.set).
//
// A widget's path joins the labels of the menus and labeled groups around it with its own label,
// e.g. "New Game/New Map Width". Paths that repeat in a frame get "#2", "#3", ... Rects are in
// window points, the units input gestures use. Recording is off unless automation turns it on,
// so normal play pays nothing.

use serde_json::Value;

use common::Rect;

#[derive(Clone, Debug)]
pub struct UiWidgetInfo {
    pub path: String,
    pub kind: &'static str, // "text_button", "sprite_button", "checkbox", "slider", ...
    pub label: String,
    pub tooltip: Option<String>,
    pub rect: Rect,
    pub enabled: bool,
    pub hovered: bool,
    pub value: Option<Value>, // Current value, for checkboxes, sliders, inputs and dropdowns.
}

#[derive(Default)]
pub(super) struct UiWidgetRegistry {
    enabled: bool,
    path: Vec<String>,
    recording: Vec<UiWidgetInfo>,     // This frame's, so far.
    last_frame: Vec<UiWidgetInfo>,    // The previous frame's, complete.
    pending_values: Vec<PendingValue>, // From ui.set, applied by the widget on its next draw.
}

struct PendingValue {
    path: String,
    value: Value,
    frames_left: u32, // Dropped if the widget isn't drawn by then.
}

const PENDING_VALUE_FRAMES: u32 = 2;

impl UiWidgetRegistry {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.recording.clear();
            self.last_frame.clear();
            self.pending_values.clear();
        }
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn begin_frame(&mut self) {
        if self.enabled {
            self.last_frame = std::mem::take(&mut self.recording);
        }
        self.path.clear();

        self.pending_values.retain_mut(|pending| {
            pending.frames_left = pending.frames_left.saturating_sub(1);
            pending.frames_left > 0
        });
    }

    #[inline]
    pub fn widgets(&self) -> &[UiWidgetInfo] {
        &self.last_frame
    }

    pub fn push_path(&mut self, segment: &str) {
        if self.enabled {
            self.path.push(clean_segment(segment));
        }
    }

    pub fn pop_path(&mut self) {
        if self.enabled {
            self.path.pop();
        }
    }

    // Records a widget and returns its path, or None while recording is off.
    pub fn record(&mut self, mut info: UiWidgetInfo) -> Option<String> {
        if !self.enabled {
            return None;
        }

        let mut path: Vec<String> = self.path.iter().filter(|segment| !segment.is_empty()).cloned().collect();
        let label = clean_segment(&info.label);
        if !label.is_empty() {
            path.push(label);
        } else if path.is_empty() {
            path.push(info.kind.to_string());
        }

        let base_path = path.join("/");
        let mut unique_path = base_path.clone();
        let mut count = 1;
        while self.recording.iter().any(|recorded| recorded.path == unique_path) {
            count += 1;
            unique_path = format!("{base_path}#{count}");
        }

        info.path = unique_path.clone();
        self.recording.push(info);
        Some(unique_path)
    }

    pub fn queue_value(&mut self, path: String, value: Value) {
        self.pending_values.retain(|pending| pending.path != path);
        self.pending_values.push(PendingValue { path, value, frames_left: PENDING_VALUE_FRAMES });
    }

    pub fn take_value(&mut self, path: &str) -> Option<Value> {
        let index = self.pending_values.iter().position(|pending| pending.path == path)?;
        Some(self.pending_values.swap_remove(index).value)
    }
}

// Labels can hold ImGui id suffixes ("Save##3") or slashes (sprite paths); neither belongs in a path.
fn clean_segment(label: &str) -> String {
    let label = label.split("##").next().unwrap_or_default().trim();
    label.replace('/', "\u{2215}") // Division slash: keeps the text, can't split the path.
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use common::Vec2;

    use super::*;

    fn info(kind: &'static str, label: &str) -> UiWidgetInfo {
        UiWidgetInfo {
            path: String::new(),
            kind,
            label: label.into(),
            tooltip: None,
            rect: Rect::from_pos_and_size(Vec2::zero(), Vec2::new(10.0, 10.0)),
            enabled: true,
            hovered: false,
            value: None,
        }
    }

    #[test]
    fn test_paths() {
        let mut registry = UiWidgetRegistry::default();
        registry.set_enabled(true);
        registry.begin_frame();

        registry.push_path("Main Menu");
        assert_eq!(registry.record(info("text_button", "New Game")).as_deref(), Some("Main Menu/New Game"));
        assert_eq!(registry.record(info("text_button", "New Game")).as_deref(), Some("Main Menu/New Game#2"));
        registry.push_path("Map Width");
        assert_eq!(registry.record(info("int_input", "")).as_deref(), Some("Main Menu/Map Width"));
        registry.pop_path();
        registry.pop_path();
        assert_eq!(registry.record(info("sprite_button", "ui/palette/roads")).as_deref(), Some("ui\u{2215}palette\u{2215}roads"));
        assert_eq!(registry.record(info("checkbox", "Mute##7")).as_deref(), Some("Mute"));

        // Nothing is listed until the frame is complete.
        assert!(registry.widgets().is_empty());
        registry.begin_frame();
        assert_eq!(registry.widgets().len(), 5);
    }

    #[test]
    fn test_off_records_nothing() {
        let mut registry = UiWidgetRegistry::default();
        registry.begin_frame();
        assert_eq!(registry.record(info("text_button", "New Game")), None);
        registry.begin_frame();
        assert!(registry.widgets().is_empty());
    }

    #[test]
    fn test_values() {
        let mut registry = UiWidgetRegistry::default();
        registry.set_enabled(true);
        registry.queue_value("Settings/Volume".into(), Value::from(0.5));
        registry.queue_value("Settings/Volume".into(), Value::from(0.75));
        assert_eq!(registry.take_value("Settings/Volume"), Some(Value::from(0.75)));
        assert_eq!(registry.take_value("Settings/Volume"), None);

        // A value for a widget that isn't drawn expires.
        registry.queue_value("Gone".into(), Value::from(1));
        registry.begin_frame();
        assert!(registry.take_value("Gone").is_some());
        registry.queue_value("Gone".into(), Value::from(1));
        registry.begin_frame();
        registry.begin_frame();
        assert_eq!(registry.take_value("Gone"), None);
    }
}
