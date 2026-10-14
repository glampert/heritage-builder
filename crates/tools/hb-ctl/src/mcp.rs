// hb-ctl mcp: serves the game's automation commands as MCP tools over stdio (JSON-RPC 2.0, one
// message per line). Claude Code starts it through the project's .mcp.json.
//
// - game_launch and game_connect start or find a game. Every game command is a tool too, with dots
//   turned into underscores (world.place is world_place), listed once a game is connected; the
//   server then tells the client its tool list changed.
// - game_call runs any command by name, for clients that don't refresh their tool list.
// - screenshot returns the image itself, at window-point resolution unless told otherwise.
// - A game started with game_launch is asked to quit when the client disconnects.
//
// stdout carries MCP messages only; diagnostics go to stderr.

use std::{
    io::{BufRead, Write},
    time::Duration,
};

use serde_json::{Map, Value, json};

use crate::{
    LaunchArgs,
    Options,
    client::{Client, RpcError},
    launch,
};

const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];
const DEFAULT_PROTOCOL_VERSION: &str = "2025-06-18";

const INSTRUCTIONS: &str = "Drives the Heritage Builder city builder. Start a game with game_launch \
(or game_connect to one already running), then use the game's commands as tools: help lists them. \
Map cells are [x, y]. Screenshots come back as images, one pixel per window point by default. \
Commands that need a map fail with NotInGame at the home menu.";

pub fn run(options: &Options) -> Result<u8, String> {
    let mut server = McpServer { options, client: None, commands: Vec::new(), launched_pid: None };

    // List a running game's tools right away.
    let _ = server.connect(None);

    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line.map_err(|err| format!("Can't read stdin: {err}"))?;
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<Value>(&line) {
            Ok(message) => server.handle(message),
            Err(err) => send(&error_response(&Value::Null, -32700, &format!("Invalid JSON: {err}"))),
        }
    }

    // The client went away: quit a game this session started, so it doesn't linger.
    if let (Some(pid), Some(client)) = (server.launched_pid, &mut server.client) {
        eprintln!("hb-ctl mcp: quitting the game this session started (pid {pid}).");
        let _ = client.call("quit", json!({}));
    }

    Ok(0)
}

struct McpServer<'a> {
    options: &'a Options,
    client: Option<Client>,
    commands: Vec<Value>, // The game's `help` list, once connected.
    launched_pid: Option<u32>,
}

impl McpServer<'_> {
    fn handle(&mut self, message: Value) {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return; // A response to something we sent; there's nothing to do with it.
        };

        let id = message.get("id").cloned();
        let params = message.get("params").cloned().unwrap_or(Value::Null);

        let result = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": self.tools() })),
            "tools/call" => Ok(self.call_tool(&params)),
            _ if id.is_none() => return, // Notifications, e.g. notifications/initialized.
            _ => Err((-32601, format!("Method not found: {method}"))),
        };

        let Some(id) = id else {
            return;
        };

        send(&match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => error_response(&id, code, &message),
        });
    }

    fn initialize(&self, params: &Value) -> Value {
        let requested = params["protocolVersion"].as_str().unwrap_or_default();
        let version =
            if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) { requested } else { DEFAULT_PROTOCOL_VERSION };

        json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": true } },
            "serverInfo": { "name": "heritage-builder", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        })
    }

    // ----------------------
    // Connection:
    // ----------------------

    // Connects to a running game (the latest, or `pid`) and loads its command list.
    fn connect(&mut self, pid: Option<u32>) -> Result<Value, String> {
        let options = Options { pid: pid.or(self.options.pid), ..self.options.clone() };
        let instance = options.instance()?;
        let client = Client::connect(&instance, self.options.timeout)?;
        self.set_client(client)?;
        Ok(json!({ "pid": instance.pid, "port": instance.port }))
    }

    fn set_client(&mut self, mut client: Client) -> Result<(), String> {
        let help = client.call("help", json!({})).map_err(|err| format!("The game didn't list its commands: {err}"))?;
        let had_tools = !self.commands.is_empty();

        self.commands = help.result["commands"].as_array().cloned().unwrap_or_default();
        self.client = Some(client);

        if !had_tools {
            send(&json!({ "jsonrpc": "2.0", "method": "notifications/tools/list_changed" }));
        }
        Ok(())
    }

    // ----------------------
    // Tools:
    // ----------------------

    fn tools(&self) -> Vec<Value> {
        let mut tools = vec![
            json!({
                "name": "game_launch",
                "description": "Build and start the game with its automation server, and connect to it. Takes a while on the \
                                first build. Game arguments, e.g. [\"--map\", \"preset:2\", \"--fixed-dt\", \"0.016667\"], \
                                start it on a map with a fixed time step.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "args": { "type": "array", "items": { "type": "string" }, "description": "Game command-line arguments." },
                        "release": { "type": "boolean", "description": "Release build. Default false." },
                    },
                },
            }),
            json!({
                "name": "game_connect",
                "description": "Connect to a game that's already running with --automation (the latest one, or by pid).",
                "inputSchema": {
                    "type": "object",
                    "properties": { "pid": { "type": "integer", "description": "Process id of the game to use." } },
                },
            }),
            json!({
                "name": "game_call",
                "description": "Run any game command by name, e.g. method \"world.place\" with params {\"tile\": \"house0\", \
                                \"cell\": [4, 4]}. help lists the commands.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "method": { "type": "string", "description": "Command name, e.g. session.state." },
                        "params": { "type": "object", "description": "The command's parameters." },
                    },
                    "required": ["method"],
                },
            }),
        ];

        tools.extend(self.commands.iter().filter_map(command_tool));
        tools
    }

    fn call_tool(&mut self, params: &Value) -> Value {
        let name = params["name"].as_str().unwrap_or_default();
        let arguments = match &params["arguments"] {
            Value::Object(arguments) => Value::Object(arguments.clone()),
            _ => Value::Object(Map::new()),
        };

        match name {
            "game_launch" => self.tool_launch(&arguments),
            "game_connect" => match self.connect(arguments["pid"].as_u64().map(|pid| pid as u32)) {
                Ok(instance) => tool_text(&instance, false),
                Err(err) => tool_error(&err),
            },
            "game_call" => {
                let method = arguments["method"].as_str().unwrap_or_default().to_string();
                let params = match &arguments["params"] {
                    Value::Object(params) => Value::Object(params.clone()),
                    _ => Value::Object(Map::new()),
                };
                self.run_command(&method, params)
            }
            _ => match self.commands.iter().find_map(|command| {
                let command_name = command["name"].as_str()?;
                (tool_name(command_name) == name).then(|| command_name.to_string())
            }) {
                Some(method) => self.run_command(&method, arguments),
                None => tool_error(&format!("Unknown tool '{name}'.")),
            },
        }
    }

    fn tool_launch(&mut self, arguments: &Value) -> Value {
        let game_args: Vec<String> = arguments["args"]
            .as_array()
            .map(|args| args.iter().filter_map(|arg| arg.as_str().map(str::to_string)).collect())
            .unwrap_or_default();

        let launch_args = LaunchArgs {
            release: arguments["release"].as_bool().unwrap_or(false),
            build: true,
            capture_input: true,
            wait: Duration::from_secs(600),
            game_args,
        };

        let launched = match launch(self.options, &launch_args) {
            Ok(launched) => launched,
            Err(err) => return tool_error(&err),
        };

        let pid = launched.instance.pid;
        if let Err(err) = self.set_client(launched.client) {
            return tool_error(&err);
        }
        self.launched_pid = Some(pid);

        tool_text(&json!({ "pid": pid, "port": launched.instance.port, "log": launched.log_path.display().to_string() }), false)
    }

    fn run_command(&mut self, method: &str, params: Value) -> Value {
        let Some(client) = &mut self.client else {
            return tool_error("No game is connected. Use game_launch to start one, or game_connect for one already running.");
        };

        if method == "screenshot" {
            return screenshot(client, params);
        }

        match client.call(method, params) {
            Ok(reply) => tool_text(&reply.result, false),
            Err(err) => {
                let text = describe_error(client, &err);
                if err.code == "Disconnected" {
                    self.client = None;
                    self.launched_pid = None;
                }
                tool_error(&text)
            }
        }
    }
}

// ----------------------------------------------
// Screenshots
// ----------------------------------------------

// Returns the image: inline from the game, or read back from the file when a path was given.
fn screenshot(client: &mut Client, mut params: Value) -> Value {
    let path = params.get("path").and_then(Value::as_str).map(str::to_string);

    if path.is_none() {
        params["inline"] = json!(true);
    }
    if params.get("scale").is_none() {
        params["scale"] = json!("points");
    }

    let reply = match client.call("screenshot", params) {
        Ok(reply) => reply,
        Err(err) => return tool_error(&describe_error(client, &err)),
    };

    let mut info = reply.result;
    let data = match info.as_object_mut().and_then(|info| info.remove("png_base64")) {
        Some(Value::String(data)) => data,
        _ => {
            let path = info["path"].as_str().unwrap_or_default().to_string();
            match std::fs::read(&path) {
                Ok(png) => base64(&png),
                Err(err) => return tool_error(&format!("The game saved {path}, but it can't be read: {err}")),
            }
        }
    };

    json!({
        "content": [
            { "type": "image", "data": data, "mimeType": "image/png" },
            { "type": "text", "text": info.to_string() },
        ],
        "isError": false,
    })
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

fn send(message: &Value) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{message}");
    let _ = stdout.flush();
}

fn error_response(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tool_text(value: &Value, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": value.to_string() }], "isError": is_error })
}

fn tool_error(message: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": message }], "isError": true })
}

// The error, plus what the game said about a panic if it crashed.
fn describe_error(client: &mut Client, err: &RpcError) -> String {
    let mut text = json!({ "error": err.to_json() }).to_string();

    for notification in client.take_notifications() {
        if notification["method"] == "panic" {
            let params = &notification["params"];
            text += &format!(
                "\nThe game panicked: {} at {}. Crash report: {}",
                params["message"].as_str().unwrap_or_default(),
                params["location"].as_str().unwrap_or_default(),
                params["crash_file"].as_str().unwrap_or("none")
            );
        }
    }

    text
}

// MCP tool names allow letters, digits, '_' and '-'.
fn tool_name(command_name: &str) -> String {
    command_name.replace('.', "_")
}

fn command_tool(command: &Value) -> Option<Value> {
    let name = command["name"].as_str()?;

    let mut properties = Map::new();
    let mut required = Vec::new();

    for param in command["params"].as_array()? {
        let param_name = param["name"].as_str()?;
        let mut schema = param_schema(param["type"].as_str().unwrap_or("any"));
        if let Some(help) = param["help"].as_str() {
            schema["description"] = json!(help);
        }
        properties.insert(param_name.to_string(), schema);

        if param["required"].as_bool() == Some(true) {
            required.push(json!(param_name));
        }
    }

    Some(json!({
        "name": tool_name(name),
        "description": format!("{} (game command {name})", command["help"].as_str().unwrap_or_default()),
        "inputSchema": { "type": "object", "properties": properties, "required": required },
    }))
}

// JSON Schema for the parameter types `help` reports.
fn param_schema(kind: &str) -> Value {
    let pair = |item_type: &str| json!({ "type": "array", "items": { "type": item_type }, "minItems": 2, "maxItems": 2 });

    match kind {
        "int" => json!({ "type": "integer" }),
        "number" => json!({ "type": "number" }),
        "string" => json!({ "type": "string" }),
        "bool" => json!({ "type": "boolean" }),
        "object" => json!({ "type": "object" }),
        "string[]" => json!({ "type": "array", "items": { "type": "string" } }),
        "cell" | "size" => pair("integer"),
        "point" => pair("number"),
        _ => json!({}), // "any", or a union such as "number|string".
    }
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let triple = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let bits = (u32::from(triple[0]) << 16) | (u32::from(triple[1]) << 8) | u32::from(triple[2]);

        out.push(TABLE[(bits >> 18) as usize & 63] as char);
        out.push(TABLE[(bits >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(bits >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[bits as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }

    #[test]
    fn test_command_tool() {
        let command = json!({
            "name": "world.place",
            "help": "Place a tile.",
            "params": [
                {"name": "tile", "type": "string", "required": true, "help": "Tile name."},
                {"name": "cell", "type": "cell", "required": true, "help": "[x, y]."},
                {"name": "scale", "type": "number|string", "required": false, "help": "Scale."},
            ],
        });

        let tool = command_tool(&command).unwrap();
        assert_eq!(tool["name"], "world_place");
        assert_eq!(tool["inputSchema"]["required"], json!(["tile", "cell"]));
        assert_eq!(tool["inputSchema"]["properties"]["cell"]["type"], "array");
        assert_eq!(tool["inputSchema"]["properties"]["scale"]["description"], "Scale.");
        assert!(tool["inputSchema"]["properties"]["scale"].get("type").is_none());
    }
}
