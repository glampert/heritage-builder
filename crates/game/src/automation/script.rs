// Script mode (`--script FILE`): runs a file of automation commands and prints each result to
// stdout as a line of JSON. Lines run in order, each one once the line before it has finished.
// The run stops at the first failure unless --script-continue-on-error is given. The process then
// exits with status 0 if every command succeeded, or 1 if not.
//
// Each line holds one command, in any of the console's forms (see text.rs). JSON objects are
// the usual form in files:
//   {"method": "session.load_preset", "params": {"preset": 1}}
// Blank lines and lines starting with # are skipped.
//
// Results go straight to stdout, not through engine::log: they are the run's machine-readable
// output, and logging moves to stderr in this mode (see GameLoop::on_early_init).

use std::{collections::VecDeque, io::Write, path::Path};

use serde_json::{Value, json};

use engine::Engine;

use super::{CommandOutput, ErrorCode, find_command, parse_command_text};

pub struct ScriptLine {
    pub number: usize, // In the file, starting at 1.
    pub name: String,
    pub params: Value,
}

pub struct ScriptRunner {
    script_name: String,
    print_fn: fn(&Value), // Where output lines go: stdout, except in tests.
    lines: VecDeque<ScriptLine>, // Not started yet.
    continue_on_error: bool,
    waiting: bool,  // A line is running.
    stopping: bool, // A line failed, or quit: run no more lines.
    finished: bool, // The summary is out.
    succeeded: usize,
    failed: usize,
}

impl ScriptRunner {
    // Reads and checks a script. A script that can't be read, or has lines that don't parse or
    // name unknown commands, never starts: each bad line is reported on stdout, and the error
    // returned names how many there were.
    pub fn load(path: &Path, continue_on_error: bool) -> Result<Self, String> {
        let source =
            std::fs::read_to_string(path).map_err(|err| format!("Can't read script '{}': {err}", path.display()))?;

        let script_name = path.display().to_string();

        let lines = parse_script(&source).map_err(|errors| {
            for (number, name, err) in &errors {
                print_json_line(&json!({
                    "line": number,
                    "command": name,
                    "ok": false,
                    "error": { "code": err.0.to_string(), "message": err.1 },
                }));
            }
            print_json_line(&json!({ "script": script_name, "ok": false, "invalid_lines": errors.len() }));

            format!("Script '{script_name}' has {} invalid line(s). Nothing ran.", errors.len())
        })?;

        Ok(Self {
            script_name,
            print_fn: print_json_line,
            lines: lines.into(),
            continue_on_error,
            waiting: false,
            stopping: false,
            finished: false,
            succeeded: 0,
            failed: 0,
        })
    }

    #[inline]
    pub fn is_waiting(&self) -> bool {
        self.waiting
    }

    #[inline]
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    // The next line to run, or None once the script is over: at its end, after a failure (unless
    // continuing on errors), or after a quit command.
    pub fn start_next_line(&mut self) -> Option<ScriptLine> {
        debug_assert!(!self.waiting && !self.finished);
        if self.stopping {
            return None;
        }
        let line = self.lines.pop_front()?;
        self.waiting = true;
        Some(line)
    }

    pub fn on_line_finished(&mut self, line: usize, name: &str, output: CommandOutput, frame: u64) {
        debug_assert!(self.waiting);
        self.waiting = false;

        let succeeded = output.is_ok();

        (self.print_fn)(&match output {
            Ok(result) => json!({ "line": line, "command": name, "ok": true, "result": result, "frame": frame }),
            Err(err) => json!({
                "line": line,
                "command": name,
                "ok": false,
                "error": { "code": err.code.to_string(), "message": err.message },
                "frame": frame,
            }),
        });

        if succeeded {
            self.succeeded += 1;
        } else {
            self.failed += 1;
        }

        // A quit command ends the script; the lines after it don't run.
        if (!succeeded && !self.continue_on_error) || (succeeded && name == "quit") {
            self.stopping = true;
        }
    }

    // Once start_next_line() returns None: prints the summary and quits with the script's exit
    // status (a quit command's own nonzero status stays).
    pub fn finish(&mut self, engine: &mut Engine, frame: u64) {
        debug_assert!(!self.waiting && !self.finished);
        self.finished = true;
        self.print_summary(frame, false);
        engine.request_exit(self.exit_code());
    }

    // The game stopped before the script finished, e.g. because its window was closed.
    pub fn finish_interrupted(&mut self, engine: &mut Engine) {
        if !self.finished {
            self.finished = true;
            self.print_summary(engine.frame_clock().frame_number(), true);
            engine.request_exit(1);
        }
    }

    #[inline]
    fn exit_code(&self) -> i32 {
        if self.failed == 0 { 0 } else { 1 }
    }

    fn print_summary(&self, frame: u64, interrupted: bool) {
        // A line that was running when the game stopped counts as skipped.
        let skipped = self.lines.len() + usize::from(self.waiting);
        debug_assert!(interrupted || !self.waiting);

        let mut summary = json!({
            "script": self.script_name,
            "ok": self.failed == 0 && !interrupted,
            "succeeded": self.succeeded,
            "failed": self.failed,
            "skipped": skipped,
            "frame": frame,
        });

        if interrupted {
            summary["interrupted"] = Value::Bool(true);
        }

        (self.print_fn)(&summary);
    }
}

type LineError = (usize, String, (ErrorCode, String)); // Line number, command name, error.

// Lines to run, or every line that's invalid.
fn parse_script(source: &str) -> Result<Vec<ScriptLine>, Vec<LineError>> {
    let mut lines = Vec::new();
    let mut errors = Vec::new();

    for (index, text) in source.lines().enumerate() {
        let number = index + 1;
        let text = text.trim();

        if text.is_empty() || text.starts_with('#') {
            continue;
        }

        match parse_command_text(text) {
            Ok((name, params)) => {
                if find_command(&name).is_some() {
                    lines.push(ScriptLine { number, name, params });
                } else {
                    let message = format!("Unknown command '{name}'. Try 'help'.");
                    errors.push((number, name, (ErrorCode::UnknownCommand, message)));
                }
            }
            Err(err) => errors.push((number, String::new(), (ErrorCode::InvalidParams, err))),
        }
    }

    if errors.is_empty() { Ok(lines) } else { Err(errors) }
}

fn print_json_line(value: &Value) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{value}");
    let _ = stdout.flush();
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_script() {
        let source = r#"
# Comment
{"method": "session.load_preset", "params": {"preset": 1}}

   wait.frames frames=2
ping
"#;

        let lines = parse_script(source).unwrap_or_else(|_| panic!("Expected the script to parse."));
        let summary: Vec<(usize, &str)> = lines.iter().map(|line| (line.number, line.name.as_str())).collect();
        assert_eq!(summary, [(3, "session.load_preset"), (5, "wait.frames"), (6, "ping")]);
        assert_eq!(lines[0].params, json!({"preset": 1}));
        assert_eq!(lines[1].params, json!({"frames": 2}));
    }

    #[test]
    fn test_parse_script_reports_every_bad_line() {
        let source = "ping\nno.such.command\n{\"method\": \n# fine\nhelp";

        let Err(errors) = parse_script(source) else {
            panic!("Expected errors.");
        };

        let codes: Vec<(usize, ErrorCode)> = errors.iter().map(|(number, _, (code, _))| (*number, *code)).collect();
        assert_eq!(codes, [(2, ErrorCode::UnknownCommand), (3, ErrorCode::InvalidParams)]);
    }

    fn runner(continue_on_error: bool, line_count: usize) -> ScriptRunner {
        let source = "ping\n".repeat(line_count);
        ScriptRunner {
            script_name: "test".into(),
            print_fn: |_| {},
            lines: parse_script(&source).unwrap_or_default().into(),
            continue_on_error,
            waiting: false,
            stopping: false,
            finished: false,
            succeeded: 0,
            failed: 0,
        }
    }

    fn fail() -> CommandOutput {
        Err(super::super::CommandError::failed("broken"))
    }

    #[test]
    fn test_stops_at_first_failure() {
        let mut script = runner(false, 3);

        let line = script.start_next_line().unwrap();
        script.on_line_finished(line.number, &line.name, Ok(Value::Null), 0);

        let line = script.start_next_line().unwrap();
        script.on_line_finished(line.number, &line.name, fail(), 0);
        assert!(script.start_next_line().is_none());
        assert_eq!((script.succeeded, script.failed, script.lines.len()), (1, 1, 1));
        assert_eq!(script.exit_code(), 1);
    }

    #[test]
    fn test_continue_on_error() {
        let mut script = runner(true, 2);

        let line = script.start_next_line().unwrap();
        script.on_line_finished(line.number, &line.name, fail(), 0);

        let line = script.start_next_line().unwrap();
        script.on_line_finished(line.number, &line.name, Ok(Value::Null), 0);
        assert!(script.start_next_line().is_none());
        assert_eq!((script.succeeded, script.failed), (1, 1));
        assert_eq!(script.exit_code(), 1);
    }

    #[test]
    fn test_quit_ends_the_script() {
        let mut script = runner(false, 2);
        let line = script.start_next_line().unwrap();
        script.on_line_finished(line.number, "quit", Ok(Value::Null), 0);
        assert!(script.start_next_line().is_none());
        assert_eq!(script.exit_code(), 0);
    }
}
