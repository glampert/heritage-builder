// Command-line options, parsed once at startup by the runner.
//
// Options take the forms `--name` (a flag), `--name=value` or `--name value`. Each crate declares
// the options it understands in a `CliOption` table: the engine's are below, and the run loop
// adds its own through `RunLoop::cli_options`. Desktop reads the process arguments; the web
// build has none.

use std::{
    fmt::Display,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Mutex, OnceLock},
};

use common::{Size, time::Seconds};

use crate::{app::ApplicationApi, automation::recording::Replay, log, render::RenderApi};

// ----------------------------------------------
// CliOption
// ----------------------------------------------

pub struct CliOption {
    pub name: &'static str, // Without the leading dashes.

    // Value placeholder shown in the usage text. None for flags. In brackets ("[PORT]"), the value
    // is optional and only given in the `--name=value` form.
    pub value_name: Option<&'static str>,

    pub help: &'static str,
}

impl CliOption {
    #[inline]
    fn has_optional_value(&self) -> bool {
        self.value_name.is_some_and(|value_name| value_name.starts_with('['))
    }
}

// Options handled by the engine.
pub const ENGINE_CLI_OPTIONS: &[CliOption] = &[
    CliOption { name: "help", value_name: None, help: "List the command-line options and exit." },
    CliOption {
        name: "fixed-dt",
        value_name: Some("SECONDS"),
        help: "Advance every frame by this time step instead of the measured frame time, so runs repeat exactly.",
    },
    CliOption { name: "window-size", value_name: Some("WxH"), help: "Initial window size, e.g. 1280x720." },
    CliOption {
        name: "app-api",
        value_name: Some("Glfw|Winit"),
        help: "Window and input backend, instead of engine.app_api from configs.json. Desktop only.",
    },
    CliOption {
        name: "render-api",
        value_name: Some("OpenGl|Wgpu"),
        help: "Renderer, instead of engine.render_api from configs.json. Wgpu also picks the Winit app backend \
               unless --app-api says otherwise. Desktop only.",
    },
    CliOption {
        name: "screenshot-after-frames",
        value_name: Some("N"),
        help: "Save a screenshot to --screenshot-out after N frames, then quit. Exits with status 1 if it fails.",
    },
    CliOption {
        name: "screenshot-out",
        value_name: Some("PATH"),
        help: "PNG file for --screenshot-after-frames. Relative to the directory the game was started from.",
    },
    CliOption { name: "screenshot-no-ui", value_name: None, help: "Leave the UI out of --screenshot-after-frames." },
    CliOption {
        name: "capture-input",
        value_name: None,
        help: "Start with input capture on: the game ignores the real mouse and keyboard and takes simulated \
               input from automation commands (input.*, ui.click).",
    },
    CliOption {
        name: "automation",
        value_name: Some("[PORT]"),
        help: "Accept automation commands over a localhost connection, on PORT (default 7878; 0 picks a free one). \
               The port and an access token go to logs/automation.json.",
    },
    CliOption {
        name: "record",
        value_name: Some("FILE"),
        help: "Record the run to FILE (JSON lines): each frame's time step, the real mouse and keyboard, and \
               automation commands from scripts and outside tools. --replay plays it back.",
    },
    CliOption {
        name: "replay",
        value_name: Some("FILE"),
        help: "Play back a run recorded with --record, with its options and window size, then exit: status 0 if \
               it ended in the same state, 1 if not. Options given here are added to the recorded ones.",
    },
];

// Engine options a recording leaves out (see recording_args). --screenshot-after-frames quits once
// its PNG is written, which takes a varying number of frames; the recording's end says when the
// run ended.
const ENGINE_OPTIONS_NOT_RECORDED: &[&str] =
    &["help", "record", "replay", "automation", "screenshot-after-frames", "screenshot-out", "screenshot-no-ui"];

// ----------------------------------------------
// CommandLine
// ----------------------------------------------

#[derive(Default)]
pub struct CommandLine {
    options: Vec<(&'static str, Option<String>)>, // In command-line order.
    known_options: Vec<&'static CliOption>,
    not_recorded: Vec<&'static str>,
}

impl CommandLine {
    // Parses `args` (without the program name) against the known options.
    pub fn parse<I>(args: I, known_options: &[&'static [CliOption]]) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let known_options: Vec<&'static CliOption> = known_options.iter().flat_map(|options| options.iter()).collect();

        let mut options = Vec::new();
        let mut args = args.into_iter().peekable();

        while let Some(arg) = args.next() {
            let Some(name_and_value) = arg.strip_prefix("--") else {
                // Some launchers pass extra arguments (e.g. MacOS's -psn_...), so this isn't fatal.
                log::warning!(log::channel!("cli"), "Ignoring unexpected command-line argument '{arg}'.");
                continue;
            };

            let (name, inline_value) = match name_and_value.split_once('=') {
                Some((name, value)) => (name, Some(value.to_string())),
                None => (name_and_value, None),
            };

            let option = known_options
                .iter()
                .find(|option| option.name == name)
                .ok_or_else(|| format!("Unknown option '--{name}'."))?;

            let value = match (option.value_name, inline_value) {
                (Some(_), Some(value)) => Some(value),
                (Some(_), None) if option.has_optional_value() => None,
                (Some(value_name), None) => {
                    // `--name value` form. The value can't look like another option.
                    let value = args.next_if(|next| !next.starts_with("--"));
                    Some(value.ok_or_else(|| format!("Option '--{name}' needs a value: --{name} {value_name}"))?)
                }
                (None, Some(_)) => return Err(format!("Option '--{name}' doesn't take a value.")),
                (None, None) => None,
            };

            options.push((option.name, value));
        }

        Ok(Self { options, known_options, not_recorded: Vec::new() })
    }

    // The options, as arguments, except for `excluded` ones.
    pub fn to_args(&self, excluded: &[&str]) -> Vec<String> {
        self.options
            .iter()
            .filter(|(name, _)| !excluded.contains(name))
            .map(|(name, value)| match value {
                Some(value) => format!("--{name}={value}"),
                None => format!("--{name}"),
            })
            .collect()
    }

    #[inline]
    pub fn has(&self, name: &str) -> bool {
        self.options.iter().any(|(option_name, _)| *option_name == name)
    }

    // Value of an option. The last occurrence wins if it's given more than once.
    #[inline]
    pub fn value(&self, name: &str) -> Option<&str> {
        self.options.iter().rev().find(|(option_name, _)| *option_name == name).and_then(|(_, value)| value.as_deref())
    }

    // Value of an option converted to T. A value that doesn't convert is an error.
    pub fn parse_value<T>(&self, name: &str) -> Result<Option<T>, String>
    where
        T: FromStr,
        T::Err: Display,
    {
        match self.value(name) {
            Some(value) => {
                value.parse::<T>().map(Some).map_err(|err| format!("Invalid value '{value}' for '--{name}': {err}"))
            }
            None => Ok(None),
        }
    }

    // Logs each known option, one per line.
    #[inline]
    pub fn log_usage(&self) {
        log_usage(self.known_options.iter().copied());
    }
}

fn log_usage<'a>(options: impl Iterator<Item = &'a CliOption>) {
    log::info!(log::channel!("cli"), "Command-line options:");

    for option in options {
        let usage = match option.value_name {
            Some(value_name) if option.has_optional_value() => {
                format!("--{}[={}]", option.name, value_name.trim_start_matches('[').trim_end_matches(']'))
            }
            Some(value_name) => format!("--{} {}", option.name, value_name),
            None => format!("--{}", option.name),
        };
        log::info!(log::channel!("cli"), "  {usage:<34} {}", option.help);
    }
}

// ----------------------------------------------
// Value parsing helpers
// ----------------------------------------------

// Parses a size written as WIDTHxHEIGHT, e.g. "1280x720". Both dimensions must be positive.
pub fn parse_size(text: &str) -> Result<Size, String> {
    let error = || format!("Expected WIDTHxHEIGHT with positive numbers (e.g. 1280x720), got '{text}'.");

    let (width, height) = text.split_once(['x', 'X']).ok_or_else(error)?;
    let width = width.trim().parse::<i32>().map_err(|_| error())?;
    let height = height.trim().parse::<i32>().map_err(|_| error())?;

    if width <= 0 || height <= 0 {
        return Err(error());
    }

    Ok(Size::new(width, height))
}

// ----------------------------------------------
// Global instance
// ----------------------------------------------

static COMMAND_LINE: OnceLock<CommandLine> = OnceLock::new();
static LAUNCH_DIR: OnceLock<PathBuf> = OnceLock::new();
static REPLAY: Mutex<Option<Replay>> = Mutex::new(None);

// Parses the process arguments against the engine's options plus `run_loop_options`.
// Called once by the runner. `--help` logs the options and exits; bad input logs the
// problem with the options and exits with status 2.
//
// With `--replay FILE`, the recording's options come first, then its window size, then the
// process arguments, so options given on the command line win. `run_loop_not_recorded` names
// run loop options that --record leaves out, such as run modes whose commands the recording holds.
pub fn initialize(run_loop_options: &'static [CliOption], run_loop_not_recorded: &'static [&'static str]) {
    #[cfg(feature = "desktop")]
    let args: Vec<String> = std::env::args().skip(1).collect();

    // The runner changes the working directory next, so remember where we were started from.
    #[cfg(feature = "desktop")]
    if let Ok(current_dir) = std::env::current_dir() {
        let _ = LAUNCH_DIR.set(current_dir);
    }

    #[cfg(not(feature = "desktop"))]
    let args: Vec<String> = Vec::new();

    let known_options = [ENGINE_CLI_OPTIONS, run_loop_options];

    let mut command_line = match CommandLine::parse(args.clone(), &known_options) {
        Ok(command_line) => command_line,
        Err(err) => {
            log::error!(log::channel!("cli"), "{err}");
            log_usage(known_options.iter().flat_map(|options| options.iter()));
            exit_process(2);
        }
    };

    if command_line.has("help") {
        command_line.log_usage();
        exit_process(0);
    }

    if let Some(path) = command_line.value("replay") {
        if command_line.has("record") {
            exit_with_error("Use either --record or --replay, not both.");
        }

        let path = resolve_launch_path(Path::new(path));
        let replay = Replay::load(&path).unwrap_or_else(|err| exit_with_error(&err));

        let [width, height] = replay.header().window_size;
        let mut replay_args = replay.header().args.clone();
        replay_args.push(format!("--window-size={width}x{height}"));
        replay_args.extend(args);

        command_line = CommandLine::parse(replay_args, &known_options).unwrap_or_else(|err| {
            exit_with_error(&format!("--replay: {}: the recorded options don't work with this build: {err}", path.display()))
        });

        *REPLAY.lock().unwrap() = Some(replay);
    }

    command_line.not_recorded = ENGINE_OPTIONS_NOT_RECORDED.iter().chain(run_loop_not_recorded).copied().collect();

    if COMMAND_LINE.set(command_line).is_err() {
        panic!("Command line already initialized!");
    }
}

// The parsed command line. Empty if initialize() hasn't run (in tests, for instance).
pub fn get() -> &'static CommandLine {
    static EMPTY: CommandLine = CommandLine { options: Vec::new(), known_options: Vec::new(), not_recorded: Vec::new() };
    COMMAND_LINE.get().unwrap_or(&EMPTY)
}

// The options a recording (--record) stores for its replay, as arguments.
pub fn recording_args() -> Vec<String> {
    let command_line = get();
    command_line.to_args(&command_line.not_recorded)
}

// The recording given with --replay, loaded by initialize(). Taken once, by the engine.
pub(crate) fn take_replay() -> Option<Replay> {
    REPLAY.lock().unwrap().take()
}

// A path option's value. Relative paths are resolved against the directory the game was started
// from, not the working directory the runner switches to.
pub fn path_value(name: &str) -> Option<PathBuf> {
    get().value(name).map(|value| resolve_launch_path(Path::new(value)))
}

// Resolves a relative path against the directory the game was started from.
pub fn resolve_launch_path(path: &Path) -> PathBuf {
    match LAUNCH_DIR.get() {
        Some(launch_dir) if path.is_relative() => launch_dir.join(path),
        _ => path.to_path_buf(),
    }
}

// Ends the process for a command-line value that the game can't use, e.g. a map preset that
// doesn't exist. Logs `message` and the option list, then exits with status 2.
pub fn fail(message: &str) -> ! {
    log::error!(log::channel!("cli"), "{message}");
    get().log_usage();
    exit_process(2);
}

// Like fail(), without the option list, for problems inside a file an option names.
pub fn exit_with_error(message: &str) -> ! {
    log::error!(log::channel!("cli"), "{message}");
    exit_process(2);
}

fn exit_process(code: i32) -> ! {
    #[cfg(feature = "desktop")]
    std::process::exit(code);

    #[cfg(not(feature = "desktop"))]
    panic!("Command-line exit with status {code}.");
}

// ----------------------------------------------
// Engine options
// ----------------------------------------------

// `--window-size`, if given.
pub fn window_size() -> Option<Size> {
    get().value("window-size").map(|value| parse_size(value).unwrap_or_else(|err| fail(&format!("--window-size: {err}"))))
}

// `--app-api`, if given.
pub fn app_api() -> Option<ApplicationApi> {
    let value = get().value("app-api")?;
    match value.to_ascii_lowercase().as_str() {
        "winit" => Some(ApplicationApi::Winit),
        #[cfg(feature = "desktop")]
        "glfw" => Some(ApplicationApi::Glfw),
        _ => fail(&format!("--app-api: expected Glfw or Winit, got '{value}'.")),
    }
}

// `--render-api`, if given.
pub fn render_api() -> Option<RenderApi> {
    let value = get().value("render-api")?;
    match value.to_ascii_lowercase().as_str() {
        "wgpu" => Some(RenderApi::Wgpu),
        #[cfg(feature = "desktop")]
        "opengl" => Some(RenderApi::OpenGl),
        _ => fail(&format!("--render-api: expected OpenGl or Wgpu, got '{value}'.")),
    }
}

pub struct OneShotScreenshotOptions {
    pub after_frames: u64,
    pub path: PathBuf,
    pub include_ui: bool,
}

// `--screenshot-after-frames N` with `--screenshot-out PATH` and `--screenshot-no-ui`, if given.
pub fn one_shot_screenshot() -> Option<OneShotScreenshotOptions> {
    let command_line = get();
    let after_frames = command_line.parse_value::<u64>("screenshot-after-frames").unwrap_or_else(|err| fail(&err));

    let Some(after_frames) = after_frames else {
        if command_line.has("screenshot-out") || command_line.has("screenshot-no-ui") {
            fail("--screenshot-out and --screenshot-no-ui need --screenshot-after-frames.");
        }
        return None;
    };

    let path = path_value("screenshot-out").unwrap_or_else(|| fail("--screenshot-after-frames needs --screenshot-out PATH."));

    Some(OneShotScreenshotOptions { after_frames, path, include_ui: !command_line.has("screenshot-no-ui") })
}

// `--automation[=PORT]`: None without the option, Some(None) for the default port.
pub fn automation_port() -> Option<Option<u16>> {
    let command_line = get();
    if !command_line.has("automation") {
        return None;
    }
    Some(command_line.parse_value::<u16>("automation").unwrap_or_else(|err| fail(&err)))
}

// `--fixed-dt`, if given.
pub fn fixed_delta_time() -> Option<Seconds> {
    let fixed_delta_time = get().parse_value::<Seconds>("fixed-dt").unwrap_or_else(|err| fail(&err))?;

    if !(fixed_delta_time > 0.0 && fixed_delta_time.is_finite()) {
        fail(&format!("--fixed-dt must be a positive number of seconds, got {fixed_delta_time}."));
    }

    Some(fixed_delta_time)
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_OPTIONS: &[CliOption] = &[
        CliOption { name: "flag", value_name: None, help: "A flag." },
        CliOption { name: "count", value_name: Some("N"), help: "A number." },
        CliOption { name: "name", value_name: Some("TEXT"), help: "Some text." },
        CliOption { name: "port", value_name: Some("[PORT]"), help: "An optional number." },
    ];

    fn parse(args: &[&str]) -> Result<CommandLine, String> {
        CommandLine::parse(args.iter().map(|arg| arg.to_string()), &[TEST_OPTIONS])
    }

    fn parse_error(args: &[&str]) -> String {
        match parse(args) {
            Ok(_) => panic!("Expected {args:?} to fail parsing."),
            Err(err) => err,
        }
    }

    #[test]
    fn test_parse_forms() {
        let cli = parse(&["--flag", "--count=3", "--name", "hello"]).unwrap();

        assert!(cli.has("flag"));
        assert_eq!(cli.value("flag"), None);
        assert_eq!(cli.value("count"), Some("3"));
        assert_eq!(cli.value("name"), Some("hello"));
        assert_eq!(cli.parse_value::<u32>("count"), Ok(Some(3)));
        assert_eq!(cli.parse_value::<u32>("missing"), Ok(None));
        assert!(!cli.has("missing"));
    }

    #[test]
    fn test_optional_value() {
        let cli = parse(&["--port", "--flag"]).unwrap();
        assert!(cli.has("port") && cli.has("flag"));
        assert_eq!(cli.value("port"), None);

        let cli = parse(&["--port=0"]).unwrap();
        assert_eq!(cli.parse_value::<u16>("port"), Ok(Some(0)));

        // The value only comes from the `=` form: a following argument isn't taken (it's ignored
        // like any argument that isn't an option).
        let cli = parse(&["--port", "7878"]).unwrap();
        assert!(cli.has("port"));
        assert_eq!(cli.value("port"), None);
    }

    #[test]
    fn test_last_value_wins() {
        let cli = parse(&["--count", "1", "--count=2"]).unwrap();
        assert_eq!(cli.value("count"), Some("2"));
    }

    #[test]
    fn test_empty() {
        let cli = parse(&[]).unwrap();
        assert!(!cli.has("flag"));
        assert_eq!(cli.value("count"), None);
    }

    #[test]
    fn test_errors() {
        assert!(parse_error(&["--unknown"]).contains("--unknown"));
        assert!(parse_error(&["--count"]).contains("needs a value"));
        assert!(parse_error(&["--count", "--flag"]).contains("needs a value"));
        assert!(parse_error(&["--flag=yes"]).contains("doesn't take a value"));

        let cli = parse(&["--count", "abc"]).unwrap();
        assert!(cli.parse_value::<u32>("count").unwrap_err().contains("abc"));
    }

    #[test]
    fn test_to_args() {
        let cli = parse(&["--flag", "--count", "3", "--port", "--name=a b"]).unwrap();
        assert_eq!(cli.to_args(&[]), ["--flag", "--count=3", "--port", "--name=a b"]);
        assert_eq!(cli.to_args(&["count", "port"]), ["--flag", "--name=a b"]);

        // The arguments parse back to the same options.
        let reparsed = CommandLine::parse(cli.to_args(&[]), &[TEST_OPTIONS]).unwrap();
        assert_eq!(reparsed.to_args(&[]), cli.to_args(&[]));
    }

    #[test]
    fn test_non_option_arguments_are_ignored() {
        let cli = parse(&["-psn_0_12345", "--flag"]).unwrap();
        assert!(cli.has("flag"));
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("1280x720"), Ok(Size::new(1280, 720)));
        assert_eq!(parse_size("64X32"), Ok(Size::new(64, 32)));
        assert!(parse_size("1280").is_err());
        assert!(parse_size("0x720").is_err());
        assert!(parse_size("-5x5").is_err());
        assert!(parse_size("axb").is_err());
    }
}
