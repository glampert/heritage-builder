// Text forms of commands, for the in-game console.
//
// A command line is one of:
//   name
//   name key=value key=value ...    Values are JSON (2, true, [3,4], "a b"), or else plain text.
//   name {"key": value, ...}        Parameters as a JSON object.
//   {"method": "name", "params": {...}}

use serde_json::{Map, Value};

use super::CommandOutput;

pub fn parse_command_text(text: &str) -> Result<(String, Value), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Empty command.".into());
    }

    if text.starts_with('{') {
        return parse_request_object(text);
    }

    let (name, rest) = match text.split_once(char::is_whitespace) {
        Some((name, rest)) => (name, rest.trim()),
        None => (text, ""),
    };

    let params = if rest.is_empty() {
        Value::Object(Map::new())
    } else if rest.starts_with('{') {
        let params: Value = serde_json::from_str(rest).map_err(|err| format!("Invalid JSON parameters: {err}"))?;
        if !params.is_object() {
            return Err("Parameters must be a JSON object.".into());
        }
        params
    } else {
        parse_key_values(rest)?
    };

    Ok((name.to_string(), params))
}

fn parse_request_object(text: &str) -> Result<(String, Value), String> {
    let mut request: Map<String, Value> = serde_json::from_str(text).map_err(|err| format!("Invalid JSON: {err}"))?;

    let Some(Value::String(name)) = request.remove("method") else {
        return Err("Expected a \"method\" string.".into());
    };

    let params = match request.remove("params") {
        None | Some(Value::Null) => Value::Object(Map::new()),
        Some(params @ Value::Object(_)) => params,
        Some(_) => return Err("\"params\" must be a JSON object.".into()),
    };

    Ok((name, params))
}

fn parse_key_values(text: &str) -> Result<Value, String> {
    let mut params = Map::new();

    for token in split_tokens(text)? {
        let Some((key, value)) = token.split_once('=') else {
            return Err(format!("Expected key=value, got '{token}'."));
        };

        if key.is_empty() {
            return Err(format!("Missing name before '=' in '{token}'."));
        }

        // JSON when it parses (numbers, booleans, arrays, quoted strings), plain text otherwise.
        let value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
        params.insert(key.to_string(), value);
    }

    Ok(Value::Object(params))
}

// Splits on whitespace, except inside quotes and brackets.
fn split_tokens(text: &str) -> Result<Vec<&str>, String> {
    let mut tokens = Vec::new();
    let mut token_start = None;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;

    for (index, c) in text.char_indices() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        if c.is_whitespace() && depth == 0 {
            if let Some(start) = token_start.take() {
                tokens.push(&text[start..index]);
            }
            continue;
        }

        token_start.get_or_insert(index);

        match c {
            '"' => in_string = true,
            '[' | '{' => depth += 1,
            ']' | '}' => depth -= 1,
            _ => {}
        }
    }

    if in_string || depth != 0 {
        return Err("Unbalanced quotes or brackets.".into());
    }

    if let Some(start) = token_start {
        tokens.push(&text[start..]);
    }

    Ok(tokens)
}

// ----------------------------------------------
// Output
// ----------------------------------------------

const MAX_OUTPUT_LINES: usize = 40;

// One-line JSON when it's short, indented JSON otherwise, cut at MAX_OUTPUT_LINES.
pub fn format_output(name: &str, output: &CommandOutput) -> String {
    if name == "help" {
        if let Ok(value) = output {
            if let Some(text) = format_help(value) {
                return text;
            }
        }
    }

    match output {
        Ok(Value::Null) => format!("{name}: ok"),
        Ok(value) => {
            let compact = value.to_string();
            if compact.len() <= 100 {
                return format!("{name}: {compact}");
            }

            let pretty = serde_json::to_string_pretty(value).unwrap_or(compact);
            let line_count = pretty.lines().count();

            if line_count <= MAX_OUTPUT_LINES {
                format!("{name}:\n{pretty}")
            } else {
                let kept: Vec<&str> = pretty.lines().take(MAX_OUTPUT_LINES).collect();
                format!("{name}:\n{}\n... ({} more lines)", kept.join("\n"), line_count - MAX_OUTPUT_LINES)
            }
        }
        Err(err) if name.is_empty() => format!("{}: {}", err.code, err.message),
        Err(err) => format!("{name}: {}: {}", err.code, err.message),
    }
}

// `help` reads better as text: one line per command, or one command with its parameters.
fn format_help(value: &Value) -> Option<String> {
    // "name param [optional_param] - help"
    fn command_line(command: &Value) -> Option<String> {
        let name = command.get("name")?.as_str()?;
        let help = command.get("help")?.as_str()?;

        let mut line = name.to_string();
        for param in command.get("params")?.as_array()? {
            let param_name = param.get("name")?.as_str()?;
            if param.get("required")?.as_bool()? {
                line += &format!(" {param_name}");
            } else {
                line += &format!(" [{param_name}]");
            }
        }

        Some(format!("{line} - {help}"))
    }

    if let Some(commands) = value.get("commands").and_then(Value::as_array) {
        let lines: Option<Vec<String>> = commands.iter().map(command_line).collect();
        return Some(format!("help: commands, then help command=NAME for details\n  {}", lines?.join("\n  ")));
    }

    let mut text = command_line(value)?;
    for param in value.get("params")?.as_array()? {
        let required = if param.get("required")?.as_bool()? { "" } else { ", optional" };
        text += &format!(
            "\n  {}: {}{required} - {}",
            param.get("name")?.as_str()?,
            param.get("type")?.as_str()?,
            param.get("help")?.as_str()?
        );
    }

    Some(text)
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::automation::{CommandError, ErrorCode};

    fn parse(text: &str) -> (String, Value) {
        parse_command_text(text).unwrap()
    }

    #[test]
    fn test_parse_name_only() {
        assert_eq!(parse("help"), ("help".to_string(), json!({})));
        assert_eq!(parse("  sim.pause  "), ("sim.pause".to_string(), json!({})));
    }

    #[test]
    fn test_parse_key_values() {
        let (name, params) = parse(r#"camera.teleport cell=[3, 4] note="a b" fast=true count=2 label=plain"#);
        assert_eq!(name, "camera.teleport");
        assert_eq!(params, json!({"cell": [3, 4], "note": "a b", "fast": true, "count": 2, "label": "plain"}));
    }

    #[test]
    fn test_parse_json_forms() {
        assert_eq!(parse(r#"session.load_preset {"preset": 2}"#), ("session.load_preset".to_string(), json!({"preset": 2})));
        assert_eq!(parse(r#"{"method": "ping"}"#), ("ping".to_string(), json!({})));
        assert_eq!(parse(r#"{"method": "sim.speed", "params": {"speed": 3}}"#), ("sim.speed".to_string(), json!({"speed": 3})));
    }

    #[test]
    fn test_parse_errors() {
        assert!(parse_command_text("").is_err());
        assert!(parse_command_text("cmd novalue").is_err());
        assert!(parse_command_text("cmd =1").is_err());
        assert!(parse_command_text("cmd cell=[1, 2").is_err());
        assert!(parse_command_text(r#"cmd note="open"#).is_err());
        assert!(parse_command_text("cmd {not json}").is_err());
        assert!(parse_command_text("cmd [1, 2]").is_err());
        assert!(parse_command_text(r#"{"params": {}}"#).is_err());
        assert!(parse_command_text(r#"{"method": "x", "params": 3}"#).is_err());
    }

    #[test]
    fn test_format_output() {
        assert_eq!(format_output("sim.pause", &Ok(Value::Null)), "sim.pause: ok");
        assert_eq!(format_output("ping", &Ok(json!({"pong": true}))), r#"ping: {"pong":true}"#);

        let err = CommandError::new(ErrorCode::NotFound, "No such save.");
        assert_eq!(format_output("session.load_save", &Err(err)), "session.load_save: NotFound: No such save.");

        let long: Vec<u32> = (0..100).collect();
        let text = format_output("big", &Ok(json!(long)));
        assert!(text.starts_with("big:\n["));
        assert!(text.ends_with("(62 more lines)"));
    }

    #[test]
    fn test_format_help() {
        let command = json!({
            "name": "world.place",
            "help": "Place a tile.",
            "params": [
                {"name": "tile", "type": "string", "required": true, "help": "Tile name."},
                {"name": "undo", "type": "bool", "required": false, "help": "Record undo."},
            ],
        });

        let text = format_output("help", &Ok(command.clone()));
        assert_eq!(text, "world.place tile [undo] - Place a tile.\n  tile: string - Tile name.\n  undo: bool, optional - Record undo.");

        let text = format_output("help", &Ok(json!({ "commands": [command] })));
        assert!(text.ends_with("\n  world.place tile [undo] - Place a tile."));
    }
}
