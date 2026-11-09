// SPDX-License-Identifier: MIT OR Apache-2.0
//! Sidecar lifecycle: spawn the bundled `lkb serve` on a free loopback port,
//! wait for `/health`, and keep the child handle for shutdown.

use crate::setup::LaunchPlan;
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

/// First run runs migrations and a remote PostgreSQL may be slow to answer;
/// the user sees a "starting" state until then.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
const HEALTH_POLL: Duration = Duration::from_millis(250);
const HEALTH_PROBE_TIMEOUT: Duration = Duration::from_millis(600);
const LOG_TAIL_LINES: usize = 100;
const LOG_LINE_CHARS: usize = 2000;

#[derive(Default)]
pub struct ShellState {
    child: Mutex<Option<CommandChild>>,
    port: Mutex<Option<u16>>,
    logs: Mutex<LogBuffer>,
    /// `None` while the sidecar runs (or was never started); `Some(code)`
    /// after it terminated.
    exit: Mutex<Option<Option<i32>>>,
    last_error: Mutex<Option<String>>,
}

#[derive(Default)]
struct LogBuffer {
    file: Option<File>,
    lines: VecDeque<String>,
}

pub fn last_error(app: &AppHandle) -> Option<String> {
    app.state::<ShellState>()
        .last_error
        .lock()
        .expect("shell state")
        .clone()
}

pub fn set_last_error(app: &AppHandle, message: &str) {
    let state = app.state::<ShellState>();
    *state.last_error.lock().expect("shell state") = Some(message.to_string());
}

/// Spawn the sidecar and block until `/health` answers 200 or the startup
/// deadline passes. Runs on a blocking thread, never on the main thread.
pub fn start(app: &AppHandle, plan: &LaunchPlan) -> Result<u16, String> {
    stop(app);
    let port = free_port()?;
    let state = app.state::<ShellState>();
    {
        let mut logs = state.logs.lock().expect("shell state");
        logs.lines.clear();
        logs.file = open_log_file(&plan.log_file);
    }
    *state.exit.lock().expect("shell state") = None;

    let mut command = app
        .shell()
        .sidecar("lkb")
        .map_err(|err| format!("无法定位 lkb sidecar：{err}"))?
        .args(["serve", "--config"])
        .arg(&plan.config_file)
        .env("LKB_CONFIG_FILE", &plan.config_file)
        .env("LKB_BIND", format!("127.0.0.1:{port}"))
        .env("LKB_PROFILE", &plan.profile)
        .kill_on_drop(true);
    if let Some(web_dir) = &plan.web_dir {
        command = command.env("LKB_WEB_DIR", web_dir);
    }
    if let Some(url) = &plan.database_url {
        command = command.env("LKB_DATABASE_URL", url);
    }
    let (mut events, child) = command
        .spawn()
        .map_err(|err| format!("无法启动 lkb sidecar：{err}"))?;
    *state.child.lock().expect("shell state") = Some(child);

    let pump = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Stdout(bytes) | CommandEvent::Stderr(bytes) => record(&pump, bytes),
                CommandEvent::Error(message) => record(&pump, message.into_bytes()),
                CommandEvent::Terminated(payload) => {
                    record(
                        &pump,
                        format!("[lkb exited: code={:?}, signal={:?}]", payload.code, payload.signal)
                            .into_bytes(),
                    );
                    let state = pump.state::<ShellState>();
                    *state.exit.lock().expect("shell state") = Some(payload.code);
                    break;
                }
                _ => {}
            }
        }
    });

    let deadline = Instant::now() + STARTUP_TIMEOUT;
    loop {
        if let Some(code) = *state.exit.lock().expect("shell state") {
            let message = format!(
                "lkb serve 启动失败（退出码 {:?}）\n\n{}",
                code,
                tail(app)
            );
            stop(app);
            set_last_error(app, &message);
            return Err(message);
        }
        if health_ok(port) {
            *state.port.lock().expect("shell state") = Some(port);
            return Ok(port);
        }
        if Instant::now() >= deadline {
            let message = format!(
                "lkb serve 在 {} 秒内未就绪\n\n{}",
                STARTUP_TIMEOUT.as_secs(),
                tail(app)
            );
            stop(app);
            set_last_error(app, &message);
            return Err(message);
        }
        std::thread::sleep(HEALTH_POLL);
    }
}

/// Kill the sidecar (whole process group) and forget the port.
pub fn stop(app: &AppHandle) {
    let state = app.state::<ShellState>();
    let child = state.child.lock().expect("shell state").take();
    *state.port.lock().expect("shell state") = None;
    state.logs.lock().expect("shell state").file = None;
    if let Some(child) = child {
        // Kill after releasing the log lock: the event pump records the exit
        // into the same buffer.
        let _ = child.kill();
    }
}

fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|err| format!("无法分配本地端口：{err}"))?;
    let port = listener
        .local_addr()
        .map_err(|err| format!("无法读取本地端口：{err}"))?
        .port();
    drop(listener);
    Ok(port)
}

/// Plain HTTP probe; loopback only, so a raw request beats pulling in a client.
fn health_ok(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, HEALTH_PROBE_TIMEOUT) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(HEALTH_PROBE_TIMEOUT));
    let _ = stream.set_write_timeout(Some(HEALTH_PROBE_TIMEOUT));
    if stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut buffer = [0u8; 32];
    let read = stream.read(&mut buffer).unwrap_or(0);
    let head = String::from_utf8_lossy(&buffer[..read]);
    head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.0 200")
}

fn open_log_file(path: &Path) -> Option<File> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Truncated per run: the file is a diagnostic tail, not an archive.
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .ok()
}

fn record(app: &AppHandle, bytes: Vec<u8>) {
    let text = String::from_utf8_lossy(&bytes);
    let state = app.state::<ShellState>();
    let mut logs = state.logs.lock().expect("shell state");
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let line: String = line.chars().take(LOG_LINE_CHARS).collect();
        if let Some(file) = logs.file.as_mut() {
            let _ = writeln!(file, "{line}");
        }
        logs.lines.push_back(line);
        while logs.lines.len() > LOG_TAIL_LINES {
            logs.lines.pop_front();
        }
    }
}

fn tail(app: &AppHandle) -> String {
    let state = app.state::<ShellState>();
    let logs = state.logs.lock().expect("shell state");
    let lines: Vec<&str> = logs.lines.iter().map(String::as_str).collect();
    if lines.is_empty() {
        "(sidecar 没有输出)".to_string()
    } else {
        lines.join("\n")
    }
}
