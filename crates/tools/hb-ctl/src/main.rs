// hb-ctl: drives a running Heritage Builder through its automation server, so agents and scripts
// never write socket code. See AGENTS.md ("Live control").
//
// Prints results as JSON on stdout. Exit status: 0 on success, 1 if the game reported an error,
// 2 for bad usage or when the game can't be reached.

use std::{
    fs::File,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

mod client;
mod text;

use client::{Client, Instance};

const USAGE: &str = "\
hb-ctl: drive a running Heritage Builder through its automation server.

Usage: hb-ctl [OPTIONS] COMMAND [ARGS]

Commands:
  launch [--release] [--no-build] [--wait SECS] [GAME_ARGS...]
                            Build the game, start it with --automation=0 and wait until it
                            answers. Prints its pid, port and log file. GAME_ARGS go to the game,
                            e.g. --map preset:2 --fixed-dt 0.016667.
  call METHOD [PARAMS]      Run a game command and print its result. PARAMS is a JSON object or
                            key=value pairs (values are JSON or plain text).
  commands [NAME]           List the game's commands with their parameters, or describe one.
  screenshot FILE [--no-ui] Save a screenshot of the game as a PNG.
  logs [--follow] [--level LEVEL] [--channel NAME] [--json]
                            Print recent log records, or keep printing new ones.
  script FILE [--continue-on-error]
                            Run a script file (one command per line, as --script takes) against
                            the running game, printing a JSON result line per command.
  quit [EXIT_CODE]          Quit the game and wait for it to exit.

Options:
  --dir DIR        The game's base directory, with logs/ and assets/. Default: the repository
                   root above the current directory.
  --pid PID        Use the instance with this process id. Default: the latest one to start.
  --port PORT      Connect to this port (with --token) instead of reading logs/automation.json.
  --token TOKEN    Access token for --port.
  --timeout SECS   Give up waiting for a response after SECS seconds. Default 600.
  --pretty         Indent JSON output.
";

// ----------------------------------------------
// Options
// ----------------------------------------------

pub struct Options {
    pub game_dir: PathBuf,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub token: Option<String>,
    pub timeout: Option<Duration>,
    pub pretty: bool,
}

impl Options {
    pub fn instance(&self) -> Result<Instance, String> {
        if let Some(port) = self.port {
            let token = self.token.clone().ok_or("--port needs --token.")?;
            return Ok(Instance { pid: 0, port, token });
        }

        let path = match self.pid {
            Some(pid) => client::instance_file(&self.game_dir, pid),
            None => client::latest_instance_file(&self.game_dir),
        };

        if !path.exists() {
            return Err(format!("No running game found: {} doesn't exist. Start one with hb-ctl launch.", path.display()));
        }

        Instance::read(&path)
    }

    pub fn connect(&self) -> Result<Client, String> {
        Client::connect(&self.instance()?, self.timeout)
    }

    pub fn print(&self, value: &Value) {
        let text = if self.pretty { serde_json::to_string_pretty(value) } else { serde_json::to_string(value) };
        println!("{}", text.unwrap_or_default());
    }
}

fn parse_options(args: &mut Vec<String>) -> Result<Options, String> {
    let mut options = Options {
        game_dir: PathBuf::new(),
        pid: None,
        port: None,
        token: None,
        timeout: Some(Duration::from_secs(600)),
        pretty: false,
    };
    let mut game_dir = None;

    while let Some(arg) = args.first().cloned() {
        if !arg.starts_with("--") {
            break;
        }
        args.remove(0);

        let mut value = |name: &str| {
            if args.is_empty() { Err(format!("{name} needs a value.")) } else { Ok(args.remove(0)) }
        };

        match arg.as_str() {
            "--dir" => game_dir = Some(PathBuf::from(value("--dir")?)),
            "--pid" => options.pid = Some(value("--pid")?.parse().map_err(|_| "--pid takes a number.")?),
            "--port" => options.port = Some(value("--port")?.parse().map_err(|_| "--port takes a number.")?),
            "--token" => options.token = Some(value("--token")?),
            "--timeout" => {
                let secs: f64 = value("--timeout")?.parse().map_err(|_| "--timeout takes a number of seconds.")?;
                options.timeout = (secs > 0.0).then(|| Duration::from_secs_f64(secs));
            }
            "--pretty" => options.pretty = true,
            "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            _ => return Err(format!("Unknown option '{arg}'.")),
        }
    }

    options.game_dir = game_dir.unwrap_or_else(client::find_game_dir);
    Ok(options)
}

// ----------------------------------------------
// main
// ----------------------------------------------

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();

    // --pretty works anywhere; the other options go before the command.
    let pretty = args.iter().any(|arg| arg == "--pretty");
    args.retain(|arg| arg != "--pretty");

    let result = parse_options(&mut args).and_then(|mut options| {
        options.pretty |= pretty;

        let Some(command) = (!args.is_empty()).then(|| args.remove(0)) else {
            return Err(format!("Missing a command.\n\n{USAGE}"));
        };

        match command.as_str() {
            "launch" => cmd_launch(&options, &args),
            "call" => cmd_call(&options, &args),
            "commands" => cmd_commands(&options, &args),
            "screenshot" => cmd_screenshot(&options, &args),
            "logs" => cmd_logs(&options, &args),
            "script" => cmd_script(&options, &args),
            "quit" => cmd_quit(&options, &args),
            "help" => {
                print!("{USAGE}");
                Ok(0)
            }
            _ => Err(format!("Unknown command '{command}'. Try hb-ctl --help.")),
        }
    });

    match result {
        Ok(status) => ExitCode::from(status),
        Err(err) => {
            eprintln!("hb-ctl: {err}");
            ExitCode::from(2)
        }
    }
}

type Flag = (String, Option<String>); // Name and value.

// Splits `--flag` arguments (and `--name value` for the names in `with_values`) from the rest.
fn split_flags(args: &[String], with_values: &[&str]) -> Result<(Vec<Flag>, Vec<String>), String> {
    let mut flags = Vec::new();
    let mut rest = Vec::new();
    let mut args = args.iter();

    while let Some(arg) = args.next() {
        if arg == "--" {
            rest.extend(args.by_ref().cloned());
            break;
        }
        if arg.starts_with("--") {
            let value = if with_values.contains(&arg.as_str()) {
                Some(args.next().cloned().ok_or_else(|| format!("{arg} needs a value."))?)
            } else {
                None
            };
            flags.push((arg.clone(), value));
        } else {
            rest.push(arg.clone());
        }
    }

    Ok((flags, rest))
}

// ----------------------------------------------
// launch
// ----------------------------------------------

fn cmd_launch(options: &Options, args: &[String]) -> Result<u8, String> {
    let mut release = false;
    let mut build = true;
    let mut wait = Duration::from_secs(300);
    let mut game_args = Vec::new();

    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--release" => release = true,
            "--no-build" => build = false,
            "--wait" => {
                let secs: u64 = args.next().and_then(|secs| secs.parse().ok()).ok_or("--wait takes a number of seconds.")?;
                wait = Duration::from_secs(secs);
            }
            "--" => {
                game_args.extend(args.by_ref().cloned());
                break;
            }
            _ => game_args.push(arg.clone()), // Everything else goes to the game.
        }
    }

    let game_dir = &options.game_dir;
    let profile = if release { "release" } else { "debug" };

    if build {
        let mut cargo = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        cargo.args(["build", "-q", "-p", "HeritageBuilder"]).current_dir(game_dir).stdout(Stdio::null());
        if release {
            cargo.arg("--release");
        }

        let status = cargo.status().map_err(|err| format!("Can't run cargo: {err}"))?;
        if !status.success() {
            return Err(format!("cargo build failed ({status})."));
        }
    }

    let target_dir = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| game_dir.join("target"), |dir| game_dir.join(dir));
    let binary = target_dir.join(profile).join(format!("HeritageBuilder{}", std::env::consts::EXE_SUFFIX));
    if !binary.is_file() {
        return Err(format!("No game binary at {}. Build it, or drop --no-build.", binary.display()));
    }

    let logs_dir = game_dir.join("logs");
    std::fs::create_dir_all(&logs_dir).map_err(|err| format!("Can't create {}: {err}", logs_dir.display()))?;

    let millis = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis());
    let mut log_path = logs_dir.join(format!("launch-{millis}.log"));
    let log_file = File::create(&log_path).map_err(|err| format!("Can't create {}: {err}", log_path.display()))?;
    let log_file_for_stderr = log_file.try_clone().map_err(|err| err.to_string())?;

    let mut child = Command::new(&binary)
        .arg("--automation=0")
        .args(&game_args)
        .current_dir(game_dir)
        .stdin(Stdio::null())
        .stdout(log_file)
        .stderr(log_file_for_stderr)
        .spawn()
        .map_err(|err| format!("Can't start {}: {err}", binary.display()))?;

    let pid = child.id();

    // Name the log after the process. Renaming an open file works on Unix; elsewhere keep the name.
    let pid_log_path = logs_dir.join(format!("game-{pid}.log"));
    if std::fs::rename(&log_path, &pid_log_path).is_ok() {
        log_path = pid_log_path;
    }

    // The game writes its instance file once the engine is up, then answers once it's loaded.
    let instance_path = client::instance_file(game_dir, pid);
    let deadline = Instant::now() + wait;

    let instance = loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("The game exited ({status}) before it was ready. See {}.", log_path.display()));
        }
        if instance_path.exists() {
            if let Ok(instance) = Instance::read(&instance_path) {
                break instance;
            }
        }
        if Instant::now() > deadline {
            return Err(format!("The game didn't start within {}s. See {}.", wait.as_secs(), log_path.display()));
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    let remaining = deadline.saturating_duration_since(Instant::now()).max(Duration::from_secs(1));
    let mut client = Client::connect(&instance, Some(remaining))?;
    let reply = client.call("ping", json!({})).map_err(|err| format!("The game didn't answer: {err}"))?;

    options.print(&json!({
        "pid": pid,
        "port": instance.port,
        "log": log_path.display().to_string(),
        "frame": reply.frame,
    }));
    Ok(0)
}

// ----------------------------------------------
// call / commands / screenshot
// ----------------------------------------------

fn print_reply(options: &Options, reply: Result<client::Reply, client::RpcError>) -> u8 {
    match reply {
        Ok(reply) => {
            options.print(&reply.result);
            0
        }
        Err(err) => {
            options.print(&json!({ "error": err.to_json() }));
            1
        }
    }
}

fn cmd_call(options: &Options, args: &[String]) -> Result<u8, String> {
    let (method, params) = args.split_first().ok_or("call needs a METHOD. hb-ctl commands lists them.")?;
    let params = text::parse_param_args(params)?;

    let mut client = options.connect()?;
    Ok(print_reply(options, client.call(method, params)))
}

fn cmd_commands(options: &Options, args: &[String]) -> Result<u8, String> {
    let params = match args {
        [] => json!({}),
        [name] => json!({ "command": name }),
        _ => return Err("commands takes at most one command name.".into()),
    };

    let mut client = options.connect()?;
    Ok(print_reply(options, client.call("help", params)))
}

fn absolute_path(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir().map(|dir| dir.join(path)).map_err(|err| err.to_string())
}

fn cmd_screenshot(options: &Options, args: &[String]) -> Result<u8, String> {
    let (flags, rest) = split_flags(args, &[])?;
    let [path] = rest.as_slice() else {
        return Err("screenshot needs a FILE.".into());
    };

    let mut include_ui = true;
    for (flag, _) in &flags {
        match flag.as_str() {
            "--no-ui" => include_ui = false,
            _ => return Err(format!("Unknown screenshot option '{flag}'.")),
        }
    }

    // The game resolves relative paths against its own start directory.
    let path = absolute_path(path)?;

    let mut client = options.connect()?;
    let params = json!({ "path": path.display().to_string(), "include_ui": include_ui });
    Ok(print_reply(options, client.call("screenshot", params)))
}

// ----------------------------------------------
// logs
// ----------------------------------------------

fn cmd_logs(options: &Options, args: &[String]) -> Result<u8, String> {
    let (flags, rest) = split_flags(args, &["--level", "--channel"])?;
    if !rest.is_empty() {
        return Err(format!("Unexpected argument '{}'.", rest[0]));
    }

    let mut follow = false;
    let mut json_output = false;
    let mut level = "Info".to_string();
    let mut channel = None;

    for (flag, value) in flags {
        match flag.as_str() {
            "--follow" => follow = true,
            "--json" => json_output = true,
            "--level" => level = value.unwrap_or_default(),
            "--channel" => channel = value,
            _ => return Err(format!("Unknown logs option '{flag}'.")),
        }
    }

    let print_record = |record: &Value| {
        if json_output {
            println!("{record}");
        } else {
            let channel = record["channel"].as_str().map(|channel| format!(" [{channel}]")).unwrap_or_default();
            println!(
                "#{} [{}]{} {}",
                record["seq"],
                record["level"].as_str().unwrap_or_default(),
                channel,
                record["message"].as_str().unwrap_or_default()
            );
        }
    };

    let mut client = options.connect()?;

    if !follow {
        let mut params = json!({ "min_level": level, "limit": 1000 });
        if let Some(channel) = &channel {
            params["channel"] = json!(channel);
        }

        return match client.call("logs.tail", params) {
            Ok(reply) => {
                for record in reply.result["records"].as_array().into_iter().flatten() {
                    print_record(record);
                }
                Ok(0)
            }
            Err(err) => {
                options.print(&json!({ "error": err.to_json() }));
                Ok(1)
            }
        };
    }

    if let Err(err) = client.call("logs.subscribe", json!({ "min_level": level })) {
        options.print(&json!({ "error": err.to_json() }));
        return Ok(1);
    }

    client.set_timeout(None);
    loop {
        let notification = match client.next_notification() {
            Ok(notification) => notification,
            Err(_) => return Ok(0), // The game quit.
        };

        let params = &notification["params"];
        match notification["method"].as_str().unwrap_or_default() {
            "log" => {
                if channel.as_deref().is_none_or(|channel| params["channel"] == channel) {
                    print_record(params);
                }
            }
            _ if json_output => println!("{notification}"),
            "panic" => println!(
                "[Panic] {} at {}. Crash report: {}",
                params["message"].as_str().unwrap_or_default(),
                params["location"].as_str().unwrap_or_default(),
                params["crash_file"].as_str().unwrap_or("none")
            ),
            method => println!("[{method}] {params}"),
        }
    }
}

// ----------------------------------------------
// script
// ----------------------------------------------

// Same output as the game's --script mode: a JSON line per command, then a summary line.
fn cmd_script(options: &Options, args: &[String]) -> Result<u8, String> {
    let (flags, rest) = split_flags(args, &[])?;
    let [path] = rest.as_slice() else {
        return Err("script needs a FILE.".into());
    };

    let mut continue_on_error = false;
    for (flag, _) in &flags {
        match flag.as_str() {
            "--continue-on-error" => continue_on_error = true,
            _ => return Err(format!("Unknown script option '{flag}'.")),
        }
    }

    let source = std::fs::read_to_string(path).map_err(|err| format!("Can't read {path}: {err}"))?;

    let mut lines = Vec::new();
    let mut invalid = 0;
    for (index, line) in source.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match text::parse_command_text(line) {
            Ok((name, params)) => lines.push((index + 1, name, params)),
            Err(err) => {
                invalid += 1;
                let error = json!({ "code": "InvalidParams", "message": err });
                println!("{}", json!({ "line": index + 1, "command": "", "ok": false, "error": error }));
            }
        }
    }

    if invalid > 0 {
        println!("{}", json!({ "script": path, "ok": false, "invalid_lines": invalid }));
        return Err(format!("Script '{path}' has {invalid} invalid line(s). Nothing ran."));
    }

    let mut client = options.connect()?;
    let (mut succeeded, mut failed, mut skipped) = (0, 0, 0);
    let mut last_frame = None;

    for (position, (number, name, params)) in lines.iter().enumerate() {
        let reply = client.call(name, params.clone());

        let line = match reply {
            Ok(reply) => {
                succeeded += 1;
                last_frame = reply.frame.or(last_frame);
                json!({ "line": number, "command": name, "ok": true, "result": reply.result, "frame": reply.frame })
            }
            Err(err) => {
                failed += 1;
                json!({ "line": number, "command": name, "ok": false, "error": err.to_json() })
            }
        };
        println!("{line}");

        let quit = name == "quit" && line["ok"] == true;
        if quit || (line["ok"] == false && !continue_on_error) {
            skipped = lines.len() - position - 1;
            break;
        }
    }

    println!(
        "{}",
        json!({ "script": path, "ok": failed == 0, "succeeded": succeeded, "failed": failed, "skipped": skipped, "frame": last_frame })
    );
    Ok(if failed == 0 { 0 } else { 1 })
}

// ----------------------------------------------
// quit
// ----------------------------------------------

fn cmd_quit(options: &Options, args: &[String]) -> Result<u8, String> {
    let exit_code: i32 = match args {
        [] => 0,
        [code] => code.parse().map_err(|_| "quit takes a numeric exit code.")?,
        _ => return Err("quit takes at most an exit code.".into()),
    };

    let instance = options.instance()?;
    let mut client = Client::connect(&instance, options.timeout)?;

    if let Err(err) = client.call("quit", json!({ "exit_code": exit_code })) {
        options.print(&json!({ "error": err.to_json() }));
        return Ok(1);
    }

    // The game removes its instance file as it shuts down.
    let instance_path = client::instance_file(&options.game_dir, instance.pid);
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut exited = instance.pid == 0; // Can't tell for --port.
    while !exited && Instant::now() < deadline {
        exited = !instance_path.exists();
        if !exited {
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    options.print(&json!({ "quit": true, "pid": instance.pid, "exited": exited }));
    Ok(0)
}
