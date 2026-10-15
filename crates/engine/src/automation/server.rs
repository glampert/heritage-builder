// Automation control server (desktop only): newline-delimited JSON-RPC 2.0 over a localhost TCP
// connection. Started with --automation[=PORT].
//
// - Binds 127.0.0.1 only. The port and a random per-launch token are written to
//   logs/automation.json (the latest instance) and logs/automation-<pid>.json. A connection must
//   call `auth` with the token before anything else, so other local programs can't drive the game
//   by accident.
// - The server's threads only parse and serialize. Each request becomes an automation::Request on
//   the engine's queue, which the game runs on the main thread; its Responder hands the response
//   to the connection's writer thread. Responses carry the frame the command finished on.
// - The server answers auth, logs.subscribe and logs.unsubscribe itself. It also sends
//   notifications (requests without an id): "log" records to subscribers, game events such as
//   "session.changed" to every authorized connection, and "panic" if the game panics.

use std::{
    io::{self, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        Mutex,
        MutexGuard,
        PoisonError,
        Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde_json::{Map, Value, json};

use super::{Request, RequestSender, Responder, Response};
use crate::{log, platform};

pub const DEFAULT_PORT: u16 = 7878;

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const SHUTDOWN_FLUSH_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;

// JSON-RPC error codes. Errors also carry the game's own code (e.g. "NotInGame") in data.code.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const COMMAND_FAILED: i64 = -32000;
const NOT_AUTHORIZED: i64 = -32001;

fn rpc_error_code(code: &str) -> i64 {
    match code {
        "UnknownCommand" => METHOD_NOT_FOUND,
        "InvalidParams" => INVALID_PARAMS,
        "NotAuthorized" => NOT_AUTHORIZED,
        _ => COMMAND_FAILED,
    }
}

// ----------------------------------------------
// AutomationServer
// ----------------------------------------------

pub struct AutomationServer {
    port: u16,
    shared: Arc<Shared>,
    accept_thread: Option<JoinHandle<()>>,
    info_files: InfoFiles,
}

impl AutomationServer {
    // `port`: None for DEFAULT_PORT, falling back to a free port if it's taken; Some(0) for a free
    // port; any other port must be free, or this fails.
    pub fn start(port: Option<u16>, requests: RequestSender) -> Result<Self, String> {
        let listener = bind(port)?;
        let port = listener.local_addr().map_err(|err| format!("Automation server: {err}"))?.port();

        let token = new_token()?;
        let shared = Arc::new(Shared { token, stopping: AtomicBool::new(false), connections: Mutex::new(Vec::new()) });

        let info_files = InfoFiles::write(port, &shared.token)?;

        register_log_listener(Arc::downgrade(&shared));
        register_panic_listener(Arc::downgrade(&shared), info_files.clone());

        let accept_shared = shared.clone();
        let accept_thread = thread::Builder::new()
            .name("automation-accept".into())
            .spawn(move || run_accept(listener, accept_shared, requests))
            .map_err(|err| format!("Automation server: can't start its thread: {err}"))?;

        Ok(Self { port, shared, accept_thread: Some(accept_thread), info_files })
    }

    #[inline]
    pub fn port(&self) -> u16 {
        self.port
    }

    #[inline]
    pub fn info_file(&self) -> &Path {
        &self.info_files.latest
    }

    // Sends a notification to every authorized connection, e.g. "session.changed".
    pub fn send_event(&self, method: &str, params: Value) {
        let notification = json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string();
        for connection in self.shared.connections().iter() {
            if connection.authorized.load(Ordering::Relaxed) {
                connection.send(notification.clone());
            }
        }
    }
}

impl Drop for AutomationServer {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::Relaxed);

        if let Some(accept_thread) = self.accept_thread.take() {
            let _ = accept_thread.join();
        }

        // Give the writers a moment to send what's queued, such as the response to `quit`.
        let deadline = Instant::now() + SHUTDOWN_FLUSH_TIMEOUT;
        while Instant::now() < deadline && self.shared.connections().iter().any(|connection| connection.has_pending_writes()) {
            thread::sleep(Duration::from_millis(5));
        }

        self.info_files.remove();
    }
}

fn bind(port: Option<u16>) -> Result<TcpListener, String> {
    let bind_to = |port: u16| TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)));

    match port {
        Some(port) => bind_to(port).map_err(|err| {
            format!("Automation server: can't listen on 127.0.0.1:{port}: {err}. Use --automation=0 to pick a free port.")
        }),
        None => bind_to(DEFAULT_PORT).or_else(|err| {
            log::warning!(
                log::channel!("automation"),
                "Port {DEFAULT_PORT} is taken ({err}). Listening on a free port instead; see logs/automation.json."
            );
            bind_to(0).map_err(|err| format!("Automation server: can't listen on 127.0.0.1: {err}"))
        }),
    }
}

fn new_token() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|err| format!("Automation server: can't make an access token: {err}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

// ----------------------------------------------
// Info files
// ----------------------------------------------

// logs/automation.json names the latest instance; logs/automation-<pid>.json names this one, for
// tools that launched it.
#[derive(Clone)]
struct InfoFiles {
    latest: PathBuf,
    this_instance: PathBuf,
}

impl InfoFiles {
    fn write(port: u16, token: &str) -> Result<Self, String> {
        let logs_path = log::logs_path();
        let logs_dir = logs_path.as_path();
        std::fs::create_dir_all(logs_dir).map_err(|err| format!("Automation server: can't create {}: {err}", logs_dir.display()))?;

        let pid = std::process::id();
        let info = json!({ "pid": pid, "port": port, "token": token }).to_string();

        let files = Self { latest: logs_dir.join("automation.json"), this_instance: logs_dir.join(format!("automation-{pid}.json")) };

        // The instance file goes first: tools that launched this process wait for it.
        for path in [&files.this_instance, &files.latest] {
            write_private_file(path, &info).map_err(|err| format!("Automation server: can't write {}: {err}", path.display()))?;
        }

        Ok(files)
    }

    fn remove(&self) {
        let _ = std::fs::remove_file(&self.this_instance);

        // Another instance may have started since and taken over the latest file.
        let pid = std::process::id();
        let still_ours = std::fs::read_to_string(&self.latest)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .is_some_and(|info| info["pid"] == json!(pid));

        if still_ours {
            let _ = std::fs::remove_file(&self.latest);
        }
    }
}

// The token grants control of the game, so only the user can read it.
fn write_private_file(path: &Path, contents: &str) -> io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options.open(path)?;
    file.write_all(contents.as_bytes())
}

// ----------------------------------------------
// Shared state
// ----------------------------------------------

struct Shared {
    token: String,
    stopping: AtomicBool,
    connections: Mutex<Vec<Arc<Connection>>>,
}

impl Shared {
    // Never log while holding this: the log listener takes the same lock.
    fn connections(&self) -> MutexGuard<'_, Vec<Arc<Connection>>> {
        self.connections.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

struct Connection {
    sender: mpsc::Sender<String>, // Lines for the writer thread.
    pending_writes: Arc<AtomicUsize>,
    stream: Arc<Mutex<TcpStream>>, // Written by the writer thread, and by the panic listener.
    authorized: AtomicBool,
    log_level: Mutex<Option<log::Level>>, // Lowest level sent while subscribed to the log.
}

impl Connection {
    fn send(&self, line: String) {
        send_line(&self.sender, &self.pending_writes, line);
    }

    #[inline]
    fn has_pending_writes(&self) -> bool {
        self.pending_writes.load(Ordering::Relaxed) > 0
    }

    // Writes a line right away, without the writer thread. For the panic listener: the panic may
    // stop this process before the writer gets to run.
    fn write_now(&self, line: &str) {
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            match self.stream.try_lock() {
                Ok(mut stream) => {
                    let _ = stream.write_all(format!("{line}\n").as_bytes());
                    let _ = stream.flush();
                    return;
                }
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
                Err(_) => return,
            }
        }
    }
}

fn send_line(sender: &mpsc::Sender<String>, pending_writes: &AtomicUsize, line: String) {
    pending_writes.fetch_add(1, Ordering::Relaxed);
    if sender.send(line).is_err() {
        pending_writes.fetch_sub(1, Ordering::Relaxed);
    }
}

// ----------------------------------------------
// Threads
// ----------------------------------------------

fn run_accept(listener: TcpListener, shared: Arc<Shared>, requests: RequestSender) {
    // Non-blocking, so the loop sees `stopping` without another connection arriving.
    if let Err(err) = listener.set_nonblocking(true) {
        log::error!(log::channel!("automation"), "Automation server stopped: {err}");
        return;
    }

    let mut next_connection_id = 1u64;

    while !shared.stopping.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, address)) => {
                if !address.ip().is_loopback() {
                    continue; // Can't happen while bound to 127.0.0.1.
                }
                if let Err(err) = start_connection(stream, next_connection_id, &shared, &requests) {
                    log::warning!(log::channel!("automation"), "Dropped a connection: {err}");
                }
                next_connection_id += 1;
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => thread::sleep(POLL_INTERVAL),
            Err(err) => {
                log::warning!(log::channel!("automation"), "Accepting a connection failed: {err}");
                thread::sleep(POLL_INTERVAL);
            }
        }
    }
}

fn start_connection(stream: TcpStream, id: u64, shared: &Arc<Shared>, requests: &RequestSender) -> io::Result<()> {
    // Accepted sockets inherit non-blocking mode from the listener on some systems.
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(POLL_INTERVAL))?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;

    let write_stream = Arc::new(Mutex::new(stream.try_clone()?));
    let (sender, receiver) = mpsc::channel::<String>();
    let pending_writes = Arc::new(AtomicUsize::new(0));

    let connection = Arc::new(Connection {
        sender,
        pending_writes: pending_writes.clone(),
        stream: write_stream.clone(),
        authorized: AtomicBool::new(false),
        log_level: Mutex::new(None),
    });

    thread::Builder::new()
        .name(format!("automation-writer-{id}"))
        .spawn(move || run_writer(receiver, write_stream, pending_writes))?;

    shared.connections().push(connection.clone());

    let reader_connection = connection.clone();
    let reader_shared = shared.clone();
    let reader_requests = requests.clone();
    let spawned = thread::Builder::new()
        .name(format!("automation-reader-{id}"))
        .spawn(move || run_reader(stream, reader_connection, reader_shared, reader_requests));

    if let Err(err) = spawned {
        shared.connections().retain(|other| !Arc::ptr_eq(other, &connection));
        return Err(err);
    }

    Ok(())
}

// Ends once every sender is gone: the connection's own and those held by pending responses.
fn run_writer(receiver: mpsc::Receiver<String>, stream: Arc<Mutex<TcpStream>>, pending_writes: Arc<AtomicUsize>) {
    for line in receiver {
        let written = {
            let mut stream = stream.lock().unwrap_or_else(PoisonError::into_inner);
            stream.write_all(format!("{line}\n").as_bytes()).and_then(|_| stream.flush())
        };
        pending_writes.fetch_sub(1, Ordering::Relaxed);

        if written.is_err() {
            break; // The client is gone or stopped reading.
        }
    }
    pending_writes.store(0, Ordering::Relaxed);
}

fn run_reader(mut stream: TcpStream, connection: Arc<Connection>, shared: Arc<Shared>, requests: RequestSender) {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];

    while !shared.stopping.load(Ordering::Relaxed) {
        match stream.read(&mut chunk) {
            Ok(0) => break, // Closed by the client.
            Ok(count) => {
                buffer.extend_from_slice(&chunk[..count]);

                while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
                    let line: Vec<u8> = buffer.drain(..=newline).collect();
                    let line = String::from_utf8_lossy(&line);
                    if !line.trim().is_empty() {
                        handle_message(line.trim(), &connection, &shared, &requests);
                    }
                }

                if buffer.len() > MAX_REQUEST_BYTES {
                    connection.send(error_response(&Value::Null, INVALID_REQUEST, "InvalidRequest", "Request too long.").to_string());
                    break;
                }
            }
            Err(err) if matches!(err.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted) => {}
            Err(_) => break,
        }
    }

    // Drops the connection's sender: its writer ends once pending responses have gone out.
    shared.connections().retain(|other| !Arc::ptr_eq(other, &connection));
}

// ----------------------------------------------
// Messages
// ----------------------------------------------

fn handle_message(text: &str, connection: &Arc<Connection>, shared: &Shared, requests: &RequestSender) {
    let message: Value = match serde_json::from_str(text) {
        Ok(message) => message,
        Err(err) => {
            let response = error_response(&Value::Null, PARSE_ERROR, "ParseError", &format!("Invalid JSON: {err}"));
            return connection.send(response.to_string());
        }
    };

    let Value::Object(mut message) = message else {
        let message = "Send one JSON-RPC request object per line. Batches aren't supported.";
        return connection.send(error_response(&Value::Null, INVALID_REQUEST, "InvalidRequest", message).to_string());
    };

    // A request without an id is a notification: it runs, but gets no response.
    let id = message.remove("id");
    let reply = |result: Result<Value, (i64, &str, String)>| {
        let Some(id) = &id else {
            return;
        };
        let response = match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, name, message)) => error_response(id, code, name, &message),
        };
        connection.send(response.to_string());
    };

    let method = match message.remove("method") {
        Some(Value::String(method)) => method,
        _ => return reply(Err((INVALID_REQUEST, "InvalidRequest", "Expected a \"method\" string.".into()))),
    };

    let params = match message.remove("params") {
        None | Some(Value::Null) => Value::Object(Map::new()),
        Some(params @ Value::Object(_)) => params,
        Some(_) => return reply(Err((INVALID_PARAMS, "InvalidParams", "params must be an object.".into()))),
    };

    match method.as_str() {
        "auth" => {
            let token = params.get("token").and_then(Value::as_str);
            if token == Some(shared.token.as_str()) {
                connection.authorized.store(true, Ordering::Relaxed);
                reply(Ok(json!({ "authorized": true, "pid": std::process::id() })));
            } else {
                reply(Err((NOT_AUTHORIZED, "NotAuthorized", "Wrong token. It's in logs/automation.json.".into())));
            }
        }
        _ if !connection.authorized.load(Ordering::Relaxed) => {
            let message = "Call auth first, with the token from logs/automation.json.".to_string();
            reply(Err((NOT_AUTHORIZED, "NotAuthorized", message)));
        }
        "logs.subscribe" => match params.get("min_level").map(parse_level).transpose() {
            Ok(min_level) => {
                let min_level = min_level.unwrap_or(log::Level::Info);
                *connection.log_level.lock().unwrap_or_else(PoisonError::into_inner) = Some(min_level);
                reply(Ok(json!({ "subscribed": true, "min_level": min_level.to_string() })));
            }
            Err(err) => reply(Err((INVALID_PARAMS, "InvalidParams", err))),
        },
        "logs.unsubscribe" => {
            *connection.log_level.lock().unwrap_or_else(PoisonError::into_inner) = None;
            reply(Ok(json!({ "subscribed": false })));
        }
        _ => {
            let sender = connection.sender.clone();
            let pending_writes = connection.pending_writes.clone();
            let responder = Responder::new(move |response| {
                if let Some(id) = id {
                    send_line(&sender, &pending_writes, response_json(&id, response).to_string());
                }
            });
            requests.send(Request { method, params, responder });
        }
    }
}

fn parse_level(value: &Value) -> Result<log::Level, String> {
    let name = value.as_str().unwrap_or_default();
    [log::Level::Verbose, log::Level::Info, log::Level::Warning, log::Level::Error]
        .into_iter()
        .find(|level| level.to_string().eq_ignore_ascii_case(name))
        .ok_or_else(|| format!("min_level must be Verbose, Info, Warning or Error, got {value}."))
}

fn response_json(id: &Value, response: Response) -> Value {
    let mut message = match response.result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(err) => error_response(id, rpc_error_code(&err.code), &err.code, &err.message),
    };

    if let Some(frame) = response.frame {
        message["frame"] = json!(frame);
    }

    message
}

fn error_response(id: &Value, code: i64, name: &str, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message, "data": { "code": name } },
    })
}

// ----------------------------------------------
// Notifications
// ----------------------------------------------

// Sends log records to subscribed connections. Holds a weak reference: logging outlives the server.
fn register_log_listener(shared: Weak<Shared>) {
    log::add_listener(move |record| {
        let Some(shared) = shared.upgrade() else {
            return;
        };

        let mut notification: Option<String> = None;

        for connection in shared.connections().iter() {
            let min_level = *connection.log_level.lock().unwrap_or_else(PoisonError::into_inner);
            if min_level.is_some_and(|min_level| record.level >= min_level) {
                let notification = notification.get_or_insert_with(|| {
                    let channel = record.channel.map(|channel| channel.name.trim().trim_start_matches('[').trim_end_matches(']'));
                    json!({
                        "jsonrpc": "2.0",
                        "method": "log",
                        "params": {
                            "seq": record.seq,
                            "level": record.level.to_string(),
                            "channel": channel,
                            "message": record.message,
                        },
                    })
                    .to_string()
                });
                connection.send(notification.clone());
            }
        }
    });
}

fn register_panic_listener(shared: Weak<Shared>, info_files: InfoFiles) {
    platform::add_panic_listener(move |report| {
        let Some(shared) = shared.upgrade() else {
            return;
        };

        // A panic on the main thread ends the game without dropping the server, so its files would
        // point clients at a dead port. Panics on other threads leave the game running.
        if platform::is_main_thread() {
            info_files.remove();
        }

        let notification = json!({
            "jsonrpc": "2.0",
            "method": "panic",
            "params": {
                "message": report.message,
                "location": report.location,
                "frame": report.frame,
                "crash_file": report.crash_file.map(|path| path.display().to_string()),
            },
        })
        .to_string();

        // The panicking thread may hold the lock; skip the notification rather than wait forever.
        let Ok(connections) = shared.connections.try_lock() else {
            return;
        };

        for connection in connections.iter() {
            if connection.authorized.load(Ordering::Relaxed) {
                connection.write_now(&notification);
            }
        }
    });
}
