// Command text, as the game's console reads it (a copy of game::automation::text, which this tool
// can't depend on without the whole engine):
//   name
//   name key=value key=value ...    Values are JSON (2, true, [3,4], "a b"), or else plain text.
//   name {"key": value, ...}        Parameters as a JSON object.
//   {"method": "name", "params": {...}}

use serde_json::{Map, Value};

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
        parse_key_values(&split_tokens(rest)?)?
    };

    Ok((name.to_string(), params))
}

// Parameters from command-line arguments: one JSON object, or key=value pairs.
pub fn parse_param_args(args: &[String]) -> Result<Value, String> {
    match args {
        [] => Ok(Value::Object(Map::new())),
        [single] if single.trim_start().starts_with('{') => {
            let params: Value = serde_json::from_str(single).map_err(|err| format!("Invalid JSON parameters: {err}"))?;
            if params.is_object() { Ok(params) } else { Err("Parameters must be a JSON object.".into()) }
        }
        _ => parse_key_values(&args.iter().map(String::as_str).collect::<Vec<_>>()),
    }
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

fn parse_key_values(tokens: &[&str]) -> Result<Value, String> {
    let mut params = Map::new();

    for token in tokens {
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_parse_command_text() {
        assert_eq!(parse_command_text("ping").unwrap(), ("ping".to_string(), json!({})));
        assert_eq!(
            parse_command_text(r#"world.place tile=house0 cell=[3, 4] note="a b""#).unwrap(),
            ("world.place".to_string(), json!({"tile": "house0", "cell": [3, 4], "note": "a b"}))
        );
        assert_eq!(
            parse_command_text(r#"{"method": "sim.speed", "params": {"speed": 3}}"#).unwrap(),
            ("sim.speed".to_string(), json!({"speed": 3}))
        );
        assert!(parse_command_text("cmd novalue").is_err());
        assert!(parse_command_text("cmd cell=[1, 2").is_err());
    }

    #[test]
    fn test_parse_param_args() {
        let args = |args: &[&str]| args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();

        assert_eq!(parse_param_args(&[]).unwrap(), json!({}));
        assert_eq!(parse_param_args(&args(&[r#"{"preset": 2}"#])).unwrap(), json!({"preset": 2}));
        assert_eq!(parse_param_args(&args(&["tile=well", "cell=[4,4]"])).unwrap(), json!({"tile": "well", "cell": [4, 4]}));
        assert!(parse_param_args(&args(&["[1]"])).is_err());
    }
}
