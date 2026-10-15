// Records a run to a file (--record FILE) and plays one back (--replay FILE), so a session can be
// reproduced exactly, e.g. from a bug report.
//
// A recording holds what the game can't recompute: each frame's time step, the real mouse and
// keyboard, and the automation commands that came from outside the game (a script, outside tools).
// Input is recorded below input capture, so simulated input is recomputed from the replayed
// commands, and console commands come back through the replayed typing. The file is JSON lines:
//
//   {"recording": 1, "build": "release", "args": ["--map=preset:2"], "window_size": [1280, 720], ...}
//   {"frame": 0, "dt": 0.016667000204324722, "cursor": [512.0, 384.0]}
//   {"frame": 3, "keys": ["F12"], "events": [{"type": "key", "key": "F12", "action": "press", "mods": ""}]}
//   {"frame": 7, "command": "world.place", "params": {"tile": "small_well", "cell": [5, 5]}}
//   {"frames": 120, "checksum": "cccb92c47902be3f"}
//
// Frame lines appear only for frames with something new: a different time step, cursor position,
// set of held buttons or keys, input events. Command lines are written as the commands start.
// Lines are written as they happen, so a recording survives a crash. The last line closes the
// recording: how many frames the run lasted and the run loop's hash of its final state. Without
// it, the recorded run crashed or was killed.
//
// A replay starts the game with the recorded options and window size, feeds the frames back in,
// runs the commands on their frames, and at the end compares the frame count and the checksum.
// See Engine::begin_frame and Engine::finish_run.

use std::{
    collections::VecDeque,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};

use common::{Size, Vec2, time::Seconds};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    app::{
        ApplicationEvent,
        ApplicationEventList,
        input::{InputKey, InputModifiers, MouseButton, VirtualInput},
    },
    log,
};

pub const RECORDING_FORMAT_VERSION: u32 = 1;

// ----------------------------------------------
// File lines
// ----------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordingHeader {
    pub recording: u32, // Format version.
    pub build: String,  // "Debug" or "Release", for reference.

    // Command-line options the replay runs with. Run modes the recording covers (--script,
    // --automation) are left out.
    pub args: Vec<String>,

    pub window_size: [i32; 2],
    pub content_scale: [f32; 2],
    pub input_captured: bool, // Input capture at the start of the first frame.

    // Added by the run loop, e.g. the game's version and configs.
    #[serde(default)]
    pub info: Map<String, Value>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputLine {
    frame: u64,

    // Floats are stored as f64 so that they read back as the exact f32 values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dt: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cursor: Option<[f64; 2]>,

    // The full set held down, whenever it changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    buttons: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    keys: Option<Vec<String>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    events: Vec<RecordedEvent>,
}

impl InputLine {
    fn is_empty(&self) -> bool {
        self.dt.is_none() && self.cursor.is_none() && self.buttons.is_none() && self.keys.is_none() && self.events.is_empty()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandLine {
    frame: u64,
    command: String,
    params: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordingEnd {
    pub frames: u64,              // Frames the run lasted.
    pub checksum: Option<String>, // The run loop's hash of its final state, if it has one.
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RecordedEvent {
    Key { key: String, action: String, mods: InputModifiers },
    Char { char: char },
    Scroll { x: f64, y: f64 },
    Mouse { button: String, action: String, mods: InputModifiers },
    Quit,
    Resize { window: [i32; 2], framebuffer: [i32; 2] },
}

impl RecordedEvent {
    fn from_event(event: &ApplicationEvent) -> Self {
        match *event {
            ApplicationEvent::Quit => Self::Quit,
            ApplicationEvent::WindowResize { window_size, framebuffer_size } => {
                Self::Resize { window: size_to_array(window_size), framebuffer: size_to_array(framebuffer_size) }
            }
            ApplicationEvent::KeyInput(key, action, mods) => Self::Key { key: name_of(key), action: name_of(action), mods },
            ApplicationEvent::CharInput(char) => Self::Char { char },
            ApplicationEvent::Scroll(amount) => Self::Scroll { x: amount.x as f64, y: amount.y as f64 },
            ApplicationEvent::MouseButton(button, action, mods) => {
                Self::Mouse { button: name_of(button), action: name_of(action), mods }
            }
        }
    }

    fn to_event(&self) -> Result<ApplicationEvent, String> {
        Ok(match self {
            Self::Quit => ApplicationEvent::Quit,
            Self::Resize { window, framebuffer } => ApplicationEvent::WindowResize {
                window_size: Size::new(window[0], window[1]),
                framebuffer_size: Size::new(framebuffer[0], framebuffer[1]),
            },
            Self::Key { key, action, mods } => ApplicationEvent::KeyInput(parse_name(key)?, parse_name(action)?, *mods),
            Self::Char { char } => ApplicationEvent::CharInput(*char),
            Self::Scroll { x, y } => ApplicationEvent::Scroll(Vec2::new(*x as f32, *y as f32)),
            Self::Mouse { button, action, mods } => {
                ApplicationEvent::MouseButton(parse_name(button)?, parse_name(action)?, *mods)
            }
        })
    }
}

fn name_of<T: Into<&'static str>>(value: T) -> String {
    value.into().to_string()
}

fn parse_name<T: FromStr>(name: &str) -> Result<T, String> {
    name.parse::<T>().map_err(|_| format!("Unknown {} '{name}'.", short_type_name::<T>()))
}

fn short_type_name<T>() -> &'static str {
    let name = std::any::type_name::<T>();
    name.rsplit("::").next().unwrap_or(name)
}

fn size_to_array(size: Size) -> [i32; 2] {
    [size.width, size.height]
}

// ----------------------------------------------
// Recorder
// ----------------------------------------------

// Writes the recording as the run goes (--record).
pub(crate) struct Recorder {
    path: PathBuf,
    file: Option<File>, // None after a write error: the recording stops there.
    args: Vec<String>,
    info: Map<String, Value>,
    started: bool, // The header is written at the start of the first frame.
    last_dt: Option<Seconds>,
    last_devices: VirtualInput,
}

impl Recorder {
    pub fn create(path: &Path, args: Vec<String>) -> Result<Self, String> {
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|err| format!("--record: can't create {}: {err}", parent.display()))?;
        }

        let file = File::create(path).map_err(|err| format!("--record: can't create {}: {err}", path.display()))?;

        Ok(Self {
            path: path.to_path_buf(),
            file: Some(file),
            args,
            info: Map::new(),
            started: false,
            last_dt: None,
            last_devices: VirtualInput::new(),
        })
    }

    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[inline]
    pub fn has_started(&self) -> bool {
        self.started
    }

    // Adds to the header. Only before the first frame.
    pub fn set_info(&mut self, key: &str, value: Value) {
        debug_assert!(!self.started, "Recording info must be set before the first frame.");
        self.info.insert(key.to_string(), value);
    }

    pub fn start(&mut self, window_size: Size, content_scale: Vec2, input_captured: bool) {
        debug_assert!(!self.started);
        self.started = true;

        let header = RecordingHeader {
            recording: RECORDING_FORMAT_VERSION,
            build: crate::platform::build_profile().to_string(),
            args: std::mem::take(&mut self.args),
            window_size: size_to_array(window_size),
            content_scale: [content_scale.x, content_scale.y],
            input_captured,
            info: std::mem::take(&mut self.info),
        };
        self.write_line(&header);
    }

    // The frame's time step and the real devices: their state, sampled once this frame, and the
    // events polled from them.
    pub fn record_frame(&mut self, frame: u64, dt: Seconds, devices: &VirtualInput, events: &[ApplicationEvent]) {
        let mut line = InputLine { frame, ..Default::default() };

        if self.last_dt != Some(dt) {
            line.dt = Some(dt as f64);
            self.last_dt = Some(dt);
        }

        if devices.cursor_pos != self.last_devices.cursor_pos {
            line.cursor = Some([devices.cursor_pos.x as f64, devices.cursor_pos.y as f64]);
        }

        if devices.pressed_mouse_buttons().ne(self.last_devices.pressed_mouse_buttons()) {
            line.buttons = Some(devices.pressed_mouse_buttons().map(name_of).collect());
        }

        if devices.pressed_keys().ne(self.last_devices.pressed_keys()) {
            line.keys = Some(devices.pressed_keys().map(name_of).collect());
        }

        line.events = events.iter().map(RecordedEvent::from_event).collect();
        self.last_devices = devices.clone();

        if !line.is_empty() {
            self.write_line(&line);
        }
    }

    pub fn record_command(&mut self, frame: u64, method: &str, params: &Value) {
        self.write_line(&CommandLine { frame, command: method.to_string(), params: params.clone() });
    }

    pub fn finish(&mut self, end: RecordingEnd) {
        self.write_line(&end);
        self.file = None;
    }

    fn write_line<T: Serialize>(&mut self, line: &T) {
        let Some(file) = &mut self.file else {
            return;
        };

        let mut text = serde_json::to_string(line).unwrap_or_default();
        text.push('\n');

        // Unbuffered: each line reaches the file at once, in case the game crashes.
        if let Err(err) = file.write_all(text.as_bytes()) {
            log::error!(
                log::channel!("engine"),
                "--record: can't write to {}: {err}. The recording stops here.",
                self.path.display()
            );
            self.file = None;
        }
    }
}

// ----------------------------------------------
// Replay
// ----------------------------------------------

struct ReplayInput {
    frame: u64,
    dt: Option<Seconds>,
    cursor: Option<Vec2>,
    buttons: Option<Vec<MouseButton>>,
    keys: Option<Vec<InputKey>>,
    events: Vec<ApplicationEvent>,
}

enum ReplayEntry {
    Input(ReplayInput),
    Command { frame: u64, method: String, params: Value },
}

impl ReplayEntry {
    fn frame(&self) -> u64 {
        match self {
            Self::Input(input) => input.frame,
            Self::Command { frame, .. } => *frame,
        }
    }
}

// One frame of a replay.
pub(crate) struct ReplayFrame {
    pub dt: Option<Seconds>, // None until the recording gives one.
    pub devices: VirtualInput,
    pub events: ApplicationEventList,
    pub commands: Vec<(String, Value)>,
}

// A recording loaded for --replay. Hands out its frames in order.
pub(crate) struct Replay {
    path: PathBuf,
    header: RecordingHeader,
    entries: VecDeque<ReplayEntry>,
    last_frame: Option<u64>, // Of the last frame line or command.
    end: Option<RecordingEnd>,

    // Carried from frame to frame: lines only hold what changed.
    dt: Option<Seconds>,
    devices: VirtualInput,
}

impl Replay {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|err| format!("--replay: can't read {}: {err}", path.display()))?;
        Self::parse(&text)
            .map(|mut replay| {
                replay.path = path.to_path_buf();
                replay
            })
            .map_err(|err| format!("--replay: {}: {err}", path.display()))
    }

    fn parse(text: &str) -> Result<Self, String> {
        let mut header = None;
        let mut entries = VecDeque::new();
        let mut end: Option<RecordingEnd> = None;
        let mut last_frame: Option<u64> = None;

        for (index, line) in text.lines().enumerate() {
            let line_number = index + 1;
            let fail = |message: String| format!("line {line_number}: {message}");

            if line.trim().is_empty() {
                continue;
            }
            if end.is_some() {
                return Err(fail("Nothing can follow the end line.".into()));
            }

            let value: Value = serde_json::from_str(line).map_err(|err| fail(err.to_string()))?;
            let Some(object) = value.as_object() else {
                return Err(fail("Expected a JSON object.".into()));
            };

            if header.is_none() {
                if !object.contains_key("recording") {
                    return Err(fail("The first line must be the recording's header.".into()));
                }
                let parsed: RecordingHeader = serde_json::from_value(value).map_err(|err| fail(err.to_string()))?;
                if parsed.recording != RECORDING_FORMAT_VERSION {
                    return Err(fail(format!(
                        "Recording format {} isn't supported; this build reads format {RECORDING_FORMAT_VERSION}.",
                        parsed.recording
                    )));
                }
                header = Some(parsed);
                continue;
            }

            let entry = if object.contains_key("frames") {
                end = Some(serde_json::from_value(value).map_err(|err| fail(err.to_string()))?);
                continue;
            } else if object.contains_key("command") {
                let command: CommandLine = serde_json::from_value(value).map_err(|err| fail(err.to_string()))?;
                ReplayEntry::Command { frame: command.frame, method: command.command, params: command.params }
            } else if object.contains_key("frame") {
                let input: InputLine = serde_json::from_value(value).map_err(|err| fail(err.to_string()))?;
                ReplayEntry::Input(parse_input(input).map_err(fail)?)
            } else {
                return Err(fail("Expected a frame, a command or the end line.".into()));
            };

            if last_frame.is_some_and(|last_frame| entry.frame() < last_frame) {
                return Err(fail(format!("Frame {} comes after frame {}.", entry.frame(), last_frame.unwrap_or(0))));
            }
            last_frame = Some(entry.frame());
            entries.push_back(entry);
        }

        let header = header.ok_or("The recording is empty.")?;

        if let (Some(end), Some(last_frame)) = (&end, last_frame)
            && last_frame >= end.frames
        {
            return Err(format!("Frame {last_frame} is past the end of the run ({} frames).", end.frames));
        }

        Ok(Self { path: PathBuf::new(), header, entries, last_frame, end, dt: None, devices: VirtualInput::new() })
    }

    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[inline]
    pub fn header(&self) -> &RecordingHeader {
        &self.header
    }

    #[inline]
    pub fn end(&self) -> Option<&RecordingEnd> {
        self.end.as_ref()
    }

    // The last frame the recording has anything for.
    #[inline]
    pub fn last_frame(&self) -> Option<u64> {
        self.last_frame
    }

    // Everything recorded for `frame`. Frames must be asked for in order.
    pub fn next_frame(&mut self, frame: u64) -> ReplayFrame {
        let mut events = ApplicationEventList::new();
        let mut commands = Vec::new();

        while let Some(entry) = self.entries.front() {
            if entry.frame() > frame {
                break;
            }

            match self.entries.pop_front() {
                Some(ReplayEntry::Input(input)) => {
                    debug_assert_eq!(input.frame, frame, "Replay frames skipped.");
                    if let Some(dt) = input.dt {
                        self.dt = Some(dt);
                    }
                    if let Some(cursor) = input.cursor {
                        self.devices.cursor_pos = cursor;
                    }
                    if let Some(buttons) = input.buttons {
                        for button in self.devices.pressed_mouse_buttons().collect::<Vec<_>>() {
                            self.devices.set_mouse_button(button, false);
                        }
                        for button in buttons {
                            self.devices.set_mouse_button(button, true);
                        }
                    }
                    if let Some(keys) = input.keys {
                        for key in self.devices.pressed_keys().collect::<Vec<_>>() {
                            self.devices.set_key(key, false);
                        }
                        for key in keys {
                            self.devices.set_key(key, true);
                        }
                    }
                    events.extend(input.events);
                }
                Some(ReplayEntry::Command { method, params, .. }) => commands.push((method, params)),
                None => break,
            }
        }

        ReplayFrame { dt: self.dt, devices: self.devices.clone(), events, commands }
    }
}

fn parse_input(line: InputLine) -> Result<ReplayInput, String> {
    fn parse_all<T: FromStr>(names: Option<Vec<String>>) -> Result<Option<Vec<T>>, String> {
        names.map(|names| names.iter().map(|name| parse_name(name)).collect()).transpose()
    }

    Ok(ReplayInput {
        frame: line.frame,
        dt: line.dt.map(|dt| dt as f32),
        cursor: line.cursor.map(|[x, y]| Vec2::new(x as f32, y as f32)),
        buttons: parse_all(line.buttons)?,
        keys: parse_all(line.keys)?,
        events: line.events.iter().map(RecordedEvent::to_event).collect::<Result<_, _>>()?,
    })
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::app::input::InputAction;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("heritage-recording-test-{}-{name}.jsonl", std::process::id()))
    }

    #[test]
    fn test_events_round_trip() {
        let events = [
            ApplicationEvent::Quit,
            ApplicationEvent::WindowResize { window_size: Size::new(800, 600), framebuffer_size: Size::new(1600, 1200) },
            ApplicationEvent::KeyInput(
                InputKey::Escape,
                InputAction::Press,
                InputModifiers::Shift | InputModifiers::Control,
            ),
            ApplicationEvent::CharInput('é'),
            ApplicationEvent::Scroll(Vec2::new(0.0, -1.25)),
            ApplicationEvent::MouseButton(MouseButton::Right, InputAction::Release, InputModifiers::empty()),
        ];

        for event in events {
            let recorded = RecordedEvent::from_event(&event);
            let text = serde_json::to_string(&recorded).unwrap();
            let parsed: RecordedEvent = serde_json::from_str(&text).unwrap();
            assert_eq!(format!("{:?}", parsed.to_event().unwrap()), format!("{event:?}"), "{text}");
        }

        let key = serde_json::to_value(RecordedEvent::from_event(&events[2])).unwrap();
        assert_eq!(key, json!({"type": "key", "key": "Escape", "action": "press", "mods": "Shift | Control"}));
    }

    #[test]
    fn test_record_then_replay() {
        let path = temp_path("round-trip");
        let mut recorder = Recorder::create(&path, vec!["--map=preset:2".into()]).unwrap();
        recorder.set_info("game", json!({"version": "1.0"}));
        recorder.start(Size::new(1280, 720), Vec2::new(2.0, 2.0), true);

        let dt: Seconds = 1.0 / 60.0;
        let mut devices = VirtualInput::new();
        devices.cursor_pos = Vec2::new(10.5, 20.25);
        recorder.record_frame(0, dt, &devices, &[]);
        recorder.record_frame(1, dt, &devices, &[]); // Nothing new: no line.

        devices.set_key(InputKey::LeftShift, true);
        devices.set_mouse_button(MouseButton::Left, true);
        let click = ApplicationEvent::MouseButton(MouseButton::Left, InputAction::Press, InputModifiers::Shift);
        recorder.record_frame(2, dt, &devices, &[click]);
        recorder.record_command(2, "world.place", &json!({"tile": "small_well"}));
        recorder.record_command(4, "ping", &json!({}));

        devices.release_all();
        recorder.record_frame(5, 0.02, &devices, &[]);
        recorder.finish(RecordingEnd { frames: 8, checksum: Some("abc".into()) });

        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(text.lines().count(), 7, "{text}"); // Header, frames 0, 2 and 5, two commands, end.

        let mut replay = Replay::parse(&text).unwrap();
        assert_eq!(replay.header().args, ["--map=preset:2"]);
        assert_eq!(replay.header().window_size, [1280, 720]);
        assert!(replay.header().input_captured);
        assert_eq!(replay.header().info["game"]["version"], "1.0");
        assert_eq!(replay.last_frame(), Some(5));
        assert_eq!(replay.end(), Some(&RecordingEnd { frames: 8, checksum: Some("abc".into()) }));

        let frame = replay.next_frame(0);
        assert_eq!(frame.dt, Some(dt)); // Exactly the recorded f32.
        assert_eq!(frame.devices.cursor_pos, Vec2::new(10.5, 20.25));
        assert!(frame.events.is_empty() && frame.commands.is_empty());

        let frame = replay.next_frame(1); // Unchanged.
        assert_eq!(frame.dt, Some(dt));
        assert_eq!(frame.devices.cursor_pos, Vec2::new(10.5, 20.25));

        let frame = replay.next_frame(2);
        assert!(frame.devices.is_key_pressed(InputKey::LeftShift));
        assert!(frame.devices.is_mouse_button_pressed(MouseButton::Left));
        assert_eq!(format!("{:?}", frame.events.as_slice()), format!("{:?}", [click]));
        assert_eq!(frame.commands, [("world.place".to_string(), json!({"tile": "small_well"}))]);

        assert!(replay.next_frame(3).commands.is_empty());
        assert_eq!(replay.next_frame(4).commands.len(), 1);

        let frame = replay.next_frame(5);
        assert_eq!(frame.dt, Some(0.02));
        assert_eq!(frame.devices.pressed_keys().count(), 0);
        assert_eq!(frame.devices.pressed_mouse_buttons().count(), 0);
    }

    #[test]
    fn test_parse_errors() {
        fn parse_error(text: &str) -> String {
            match Replay::parse(text) {
                Ok(_) => panic!("Expected an error for {text}"),
                Err(err) => err,
            }
        }

        let header = r#"{"recording": 1, "build": "Debug", "args": [], "window_size": [8, 8], "content_scale": [1, 1], "input_captured": false}"#;
        let error = |body: &str| parse_error(&format!("{header}\n{body}"));

        assert!(parse_error(r#"{"frame": 0}"#).contains("header"));
        assert!(parse_error("").contains("empty"));
        assert!(parse_error(&header.replace(r#""recording": 1"#, r#""recording": 99"#)).contains("99"));

        assert!(error(r#"{"frame": 0, "keys": ["NoSuchKey"]}"#).contains("NoSuchKey"));
        assert!(error(r#"{"frame": 0, "colour": 1}"#).contains("colour"));
        assert!(error("{\"frame\": 2}\n{\"frame\": 1}").contains("line 3"));
        assert!(error("{\"frames\": 3, \"checksum\": null}\n{\"frame\": 4}").contains("follow"));
        assert!(error("{\"frame\": 5}\n{\"frames\": 3, \"checksum\": null}").contains("past the end"));
        assert!(error("[1, 2]").contains("object"));
    }
}
