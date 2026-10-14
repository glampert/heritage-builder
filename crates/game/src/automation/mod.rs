// Automation commands: named commands that take JSON parameters and return JSON results.
//
// Commands run on the main thread at one point in GameLoop::update, right after the session
// command queue, where the world is unlocked and no SimContext is live. The in-game console
// (an input field in the debug LogViewer) is the first way in; script mode and the control
// server will feed the same registry (see docs/plans/agent-automation.md).

use serde::de::DeserializeOwned;
use serde_json::Value;
use strum::Display;

use common::coords::Cell;
use engine::{Engine, log};

use crate::{
    config::GameConfigs,
    debug::log_viewer::LogViewer,
    menu::GameMenusMode,
    session::{GameSession, GameSessionCmdQueue},
};

mod commands;
mod text;

pub use text::{format_output, parse_command_text};

// ----------------------------------------------
// CommandError
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug, Display)]
pub enum ErrorCode {
    UnknownCommand,
    InvalidParams,
    NotInGame, // Needs a map: not available at the home menu.
    NotFound,
    Failed,
}

#[derive(Debug)]
pub struct CommandError {
    pub code: ErrorCode,
    pub message: String,
}

impl CommandError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidParams, message)
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Failed, message)
    }
}

// ----------------------------------------------
// CommandResult / PendingCommand
// ----------------------------------------------

pub type CommandOutput = Result<Value, CommandError>;

pub enum Outcome {
    Done(Value),
    Pending(Box<dyn PendingCommand>), // Finishes in a later frame.
}

pub type CommandResult = Result<Outcome, CommandError>;

// A command that spans frames, such as a session load or a screenshot. Polled once per frame at
// the dispatch point until it returns a result. Keep ids, never references: a session load
// replaces the world.
pub trait PendingCommand {
    fn poll(&mut self, context: &mut CommandContext) -> Option<CommandOutput>;
}

struct PendingFn<F>(F);

impl<F> PendingCommand for PendingFn<F>
where
    F: FnMut(&mut CommandContext) -> Option<CommandOutput>,
{
    fn poll(&mut self, context: &mut CommandContext) -> Option<CommandOutput> {
        (self.0)(context)
    }
}

#[inline]
pub fn done(value: Value) -> CommandResult {
    Ok(Outcome::Done(value))
}

#[inline]
pub fn pending<F>(poll_fn: F) -> CommandResult
where
    F: FnMut(&mut CommandContext) -> Option<CommandOutput> + 'static,
{
    Ok(Outcome::Pending(Box::new(PendingFn(poll_fn))))
}

// ----------------------------------------------
// CommandContext
// ----------------------------------------------

pub struct CommandContext<'game> {
    pub engine: &'game mut Engine,
    pub session: &'game mut GameSession,
    pub session_cmds: &'game mut GameSessionCmdQueue,
    pub configs: &'static GameConfigs,
}

impl CommandContext<'_> {
    #[inline]
    pub fn frame(&self) -> u64 {
        self.engine.frame_clock().frame_number()
    }

    // Commands that touch the map or the simulation need a game in progress.
    pub fn require_map(&self) -> Result<(), CommandError> {
        if self.session.current_menus_mode() == Some(GameMenusMode::Home) {
            return Err(CommandError::new(ErrorCode::NotInGame, "Not available at the home menu."));
        }
        if !self.session.tile_map().size_in_cells().is_valid() {
            return Err(CommandError::new(ErrorCode::NotInGame, "No map is loaded."));
        }
        Ok(())
    }

    pub fn require_cell_in_map(&self, cell: Cell) -> Result<(), CommandError> {
        if !self.session.tile_map().is_cell_within_bounds(cell) {
            let map_size = self.session.tile_map().size_in_cells();
            return Err(CommandError::invalid_params(format!(
                "Cell [{}, {}] is outside the {}x{} map.",
                cell.x, cell.y, map_size.width, map_size.height
            )));
        }
        Ok(())
    }
}

// ----------------------------------------------
// CommandDef
// ----------------------------------------------

pub type CommandFn = fn(&mut CommandContext, Value) -> CommandResult;

pub struct ParamDef {
    pub name: &'static str,
    pub kind: &'static str, // E.g. "int", "string", "cell", shown by `help`.
    pub required: bool,
    pub help: &'static str,
}

pub struct CommandDef {
    pub name: &'static str,
    pub help: &'static str,
    pub params: &'static [ParamDef],
    pub run: CommandFn,
}

#[inline]
pub fn all_commands() -> impl Iterator<Item = &'static CommandDef> {
    commands::COMMAND_GROUPS.iter().flat_map(|group| group.iter())
}

#[inline]
pub fn find_command(name: &str) -> Option<&'static CommandDef> {
    all_commands().find(|command| command.name == name)
}

pub fn run_command(context: &mut CommandContext, name: &str, params: Value) -> CommandResult {
    match find_command(name) {
        Some(command) => (command.run)(context, params),
        None => Err(CommandError::new(ErrorCode::UnknownCommand, format!("Unknown command '{name}'. Try 'help'."))),
    }
}

// Parses a command's parameters into T. Missing parameters are the same as `{}`.
pub fn parse_params<T: DeserializeOwned>(params: Value) -> Result<T, CommandError> {
    let params = if params.is_null() { Value::Object(Default::default()) } else { params };
    serde_json::from_value(params).map_err(|err| CommandError::invalid_params(err.to_string()))
}

// ----------------------------------------------
// Automation
// ----------------------------------------------

// Where a command's result goes.
#[derive(Copy, Clone)]
enum ReplyTo {
    Console,
}

struct InFlightCommand {
    name: &'static str,
    reply_to: ReplyTo,
    command: Box<dyn PendingCommand>,
}

// Owned by the GameLoop. Takes new requests and finishes pending commands once per frame.
#[derive(Default)]
pub struct Automation {
    in_flight: Vec<InFlightCommand>,
}

impl Automation {
    pub fn update(&mut self, context: &mut CommandContext) {
        self.in_flight.retain_mut(|in_flight| match in_flight.command.poll(context) {
            Some(output) => {
                reply(in_flight.reply_to, in_flight.name, output);
                false
            }
            None => true,
        });

        for line in LogViewer::take_console_input() {
            self.run_text(context, &line, ReplyTo::Console);
        }
    }

    fn run_text(&mut self, context: &mut CommandContext, text: &str, reply_to: ReplyTo) {
        let (name, params) = match parse_command_text(text) {
            Ok(parsed) => parsed,
            Err(err) => {
                reply(reply_to, "", Err(CommandError::invalid_params(err)));
                return;
            }
        };

        let Some(command) = find_command(&name) else {
            let err = CommandError::new(ErrorCode::UnknownCommand, format!("Unknown command '{name}'. Try 'help'."));
            reply(reply_to, "", Err(err));
            return;
        };

        match (command.run)(context, params) {
            Ok(Outcome::Done(value)) => reply(reply_to, command.name, Ok(value)),
            Err(err) => reply(reply_to, command.name, Err(err)),
            Ok(Outcome::Pending(mut pending)) => {
                // Some finish right away, e.g. waiting zero frames.
                match pending.poll(context) {
                    Some(output) => reply(reply_to, command.name, output),
                    None => self.in_flight.push(InFlightCommand { name: command.name, reply_to, command: pending }),
                }
            }
        }
    }
}

fn reply(reply_to: ReplyTo, name: &str, output: CommandOutput) {
    match reply_to {
        ReplyTo::Console => {
            let level = if output.is_ok() { log::Level::Info } else { log::Level::Error };
            LogViewer::print_console_output(level, &format_output(name, &output));
        }
    }
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_names_are_unique_and_documented() {
        let mut names: Vec<&str> = all_commands().map(|command| command.name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "Duplicate command names.");

        for command in all_commands() {
            assert!(!command.help.is_empty(), "'{}' has no help text.", command.name);
            for param in command.params {
                assert!(!param.help.is_empty(), "'{}' param '{}' has no help text.", command.name, param.name);
            }
        }

        assert!(find_command("help").is_some());
        assert!(find_command("no.such.command").is_none());
    }

    #[test]
    fn test_parse_params() {
        #[derive(Debug, serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Params {
            #[serde(default)]
            count: u32,
        }

        assert_eq!(parse_params::<Params>(Value::Null).unwrap().count, 0);
        assert_eq!(parse_params::<Params>(serde_json::json!({"count": 3})).unwrap().count, 3);

        let err = parse_params::<Params>(serde_json::json!({"cuont": 3})).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidParams);
        assert!(err.message.contains("cuont"));
    }
}
