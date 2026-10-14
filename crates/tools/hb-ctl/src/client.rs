// Finding a running game and talking to its automation server: newline-delimited JSON-RPC 2.0
// over TCP (see crates/engine/src/automation/server.rs).

use std::{
    collections::VecDeque,
    fmt,
    io::{BufRead, BufReader, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    time::Duration,
};

use serde_json::{Value, json};

// ----------------------------------------------
// Instance discovery
// ----------------------------------------------

// A running game: what its logs/automation*.json file says.
#[derive(Clone, Debug)]
pub struct Instance {
    pub pid: u32,
    pub port: u16,
    pub token: String,
}

impl Instance {
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|err| format!("Can't read {}: {err}", path.display()))?;
        let info: Value = serde_json::from_str(&text).map_err(|err| format!("Can't parse {}: {err}", path.display()))?;

        let pid = info["pid"].as_u64().and_then(|pid| u32::try_from(pid).ok());
        let port = info["port"].as_u64().and_then(|port| u16::try_from(port).ok());
        let token = info["token"].as_str();

        match (pid, port, token) {
            (Some(pid), Some(port), Some(token)) => Ok(Self { pid, port, token: token.to_string() }),
            _ => Err(format!("{} is missing pid, port or token.", path.display())),
        }
    }
}

// The game's base directory: where its assets/, logs/ and saves/ are. In a checkout that's the
// repository root, found by walking up from the current directory.
pub fn find_game_dir() -> PathBuf {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    current_dir
        .ancestors()
        .find(|dir| dir.join("assets").is_dir() && dir.join("Cargo.toml").is_file())
        .map(Path::to_path_buf)
        .unwrap_or(current_dir)
}

pub fn latest_instance_file(game_dir: &Path) -> PathBuf {
    game_dir.join("logs").join("automation.json")
}

pub fn instance_file(game_dir: &Path, pid: u32) -> PathBuf {
    game_dir.join("logs").join(format!("automation-{pid}.json"))
}

// ----------------------------------------------
// RpcError
// ----------------------------------------------

#[derive(Clone, Debug)]
pub struct RpcError {
    pub code: String, // The game's error code, e.g. "InvalidParams", or a transport problem.
    pub message: String,
}

impl RpcError {
    pub fn transport(message: impl Into<String>) -> Self {
        Self { code: "Disconnected".into(), message: message.into() }
    }

    pub fn to_json(&self) -> Value {
        json!({ "code": self.code, "message": self.message })
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

// ----------------------------------------------
// Client
// ----------------------------------------------

pub struct Client {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    next_id: u64,
    notifications: VecDeque<Value>, // Received while waiting for a response.
}

pub struct Reply {
    pub result: Value,
    pub frame: Option<u64>,
}

impl Client {
    // Connects and authorizes. `timeout`: how long to wait for any response; None waits forever.
    pub fn connect(instance: &Instance, timeout: Option<Duration>) -> Result<Self, String> {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, instance.port));
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))
            .map_err(|err| format!("Can't connect to the game on port {}: {err}. Is it running?", instance.port))?;

        stream.set_nodelay(true).map_err(|err| err.to_string())?;
        stream.set_read_timeout(timeout).map_err(|err| err.to_string())?;

        let writer = stream.try_clone().map_err(|err| err.to_string())?;
        let mut client = Self { reader: BufReader::new(stream), writer, next_id: 1, notifications: VecDeque::new() };

        client
            .call("auth", json!({ "token": instance.token }))
            .map_err(|err| format!("The game refused the connection: {err}"))?;

        Ok(client)
    }

    pub fn set_timeout(&mut self, timeout: Option<Duration>) {
        let _ = self.reader.get_ref().set_read_timeout(timeout);
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Reply, RpcError> {
        let id = self.next_id;
        self.next_id += 1;

        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.writer
            .write_all(format!("{request}\n").as_bytes())
            .map_err(|err| RpcError::transport(format!("Can't send to the game: {err}")))?;

        loop {
            let message = self.read_message()?;

            if message.get("id").and_then(Value::as_u64) != Some(id) {
                if message.get("method").is_some() {
                    self.notifications.push_back(message);
                }
                continue;
            }

            let frame = message.get("frame").and_then(Value::as_u64);

            if let Some(error) = message.get("error") {
                let code = error["data"]["code"].as_str().map_or_else(|| error["code"].to_string(), str::to_string);
                let message = error["message"].as_str().unwrap_or_default().to_string();
                return Err(RpcError { code, message });
            }

            return Ok(Reply { result: message.get("result").cloned().unwrap_or(Value::Null), frame });
        }
    }

    // Notifications received so far while waiting for responses, oldest first.
    pub fn take_notifications(&mut self) -> Vec<Value> {
        self.notifications.drain(..).collect()
    }

    // The next notification (a message with a method and no id), waiting for one if needed.
    pub fn next_notification(&mut self) -> Result<Value, RpcError> {
        loop {
            if let Some(notification) = self.notifications.pop_front() {
                return Ok(notification);
            }

            let message = self.read_message()?;
            if message.get("method").is_some() && message.get("id").is_none() {
                return Ok(message);
            }
        }
    }

    fn read_message(&mut self) -> Result<Value, RpcError> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => Err(RpcError::transport("The game closed the connection.")),
            Ok(_) => serde_json::from_str(&line).map_err(|err| RpcError::transport(format!("Bad message from the game: {err}"))),
            Err(err) if matches!(err.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                Err(RpcError { code: "Timeout".into(), message: "The game didn't answer in time.".into() })
            }
            Err(err) => Err(RpcError::transport(format!("Can't read from the game: {err}"))),
        }
    }
}
