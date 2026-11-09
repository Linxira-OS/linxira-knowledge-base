// SPDX-License-Identifier: MIT OR Apache-2.0
//! Linxira Knowledge Base desktop shell (Windows / Linux).
//!
//! The shell writes a first-run configuration, spawns the bundled
//! `lkb serve` sidecar on a free loopback port, waits for `/health` and only
//! then opens the main window on `http://127.0.0.1:<port>`. Closing the last
//! window stops the sidecar and exits.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;
mod setup;

use setup::{SaveOutcome, SetupPayload, SetupState};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
async fn setup_state(app: AppHandle) -> Result<SetupState, String> {
    let paths = setup::paths()?;
    Ok(setup::state(&app, &paths))
}

/// Save the wizard payload, start the sidecar and open the main window.
/// Errors carry the sidecar's log tail so the wizard can show what failed and
/// the user can adjust the configuration and retry.
#[tauri::command]
async fn save_and_launch(app: AppHandle, payload: SetupPayload) -> Result<SaveOutcome, String> {
    let worker = app.clone();
    let task = tauri::async_runtime::spawn_blocking(move || {
        let paths = setup::paths()?;
        let outcome = setup::save(&paths, &payload)?;
        let launch = setup::load_plan(&worker, &paths)
            .and_then(|plan| engine::start(&worker, &plan));
        Ok::<_, String>((outcome, launch))
    })
    .await
    .map_err(|err| format!("启动任务异常终止：{err}"))?;
    let (outcome, launch) = task?;

    match launch {
        Ok(port) => {
            open_main_window(&app, port).await?;
            Ok(outcome)
        }
        Err(message) => {
            let text = match &outcome.warning {
                Some(warning) => format!("{warning}\n\n{message}"),
                None => message,
            };
            engine::set_last_error(&app, &text);
            Err(text)
        }
    }
}

/// Boot with the saved configuration: start the engine, then show the main
/// window; on failure fall back to the setup window with the error.
fn boot(app: &AppHandle) {
    if !app.webview_windows().is_empty() {
        return;
    }
    let paths = match setup::paths() {
        Ok(paths) => paths,
        Err(message) => {
            show_setup_error(app, &message);
            return;
        }
    };
    if !paths.config_file.is_file() {
        let _ = create_setup_window(app);
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        let outcome = setup::load_plan(&handle, &paths).and_then(|plan| engine::start(&handle, &plan));
        let dispatcher = handle.clone();
        let _ = handle.run_on_main_thread(move || match outcome {
            Ok(port) => {
                if let Err(message) = create_main_window(&dispatcher, port) {
                    show_setup_error(&dispatcher, &message);
                }
            }
            Err(message) => show_setup_error(&dispatcher, &message),
        });
    });
}

async fn open_main_window(app: &AppHandle, port: u16) -> Result<(), String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let result = create_main_window(&handle, port).map(|_| close_setup_window(&handle));
        let _ = sender.send(result);
    })
    .map_err(|err| format!("无法调度主线程：{err}"))?;
    receiver
        .recv()
        .map_err(|_| "主窗口创建未返回结果".to_string())?
}

fn create_main_window(app: &AppHandle, port: u16) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    let local_origin = format!("http://127.0.0.1:{port}");
    let url = tauri::Url::parse(&format!("{local_origin}/"))
        .map_err(|err| format!("无法构造本地地址：{err}"))?;
    let opener = app.clone();
    WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
        .title("Linxira 知识库")
        .inner_size(1280.0, 820.0)
        .min_inner_size(960.0, 640.0)
        .on_navigation(move |url| {
            if url.as_str().starts_with(&local_origin) {
                return true;
            }
            // Anything else must not navigate the app window (doc §8: the
            // webview only ever shows the local app). http(s) links go to the
            // system browser; every other scheme is dropped.
            if matches!(url.scheme(), "http" | "https") {
                let _ = opener.opener().open_url(url.as_str(), None::<&str>);
            }
            false
        })
        .build()
        .map_err(|err| format!("无法创建主窗口：{err}"))?;
    Ok(())
}

fn create_setup_window(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("setup") {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(app, "setup", WebviewUrl::App(PathBuf::from("index.html")))
        .title("Linxira 桌面设置")
        .inner_size(820.0, 760.0)
        .min_inner_size(680.0, 560.0)
        .center()
        .build()
        .map_err(|err| format!("无法创建设置窗口：{err}"))?;
    Ok(())
}

fn close_setup_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("setup") {
        let _ = window.close();
    }
}

fn show_setup_error(app: &AppHandle, message: &str) {
    engine::set_last_error(app, message);
    if let Err(err) = create_setup_window(app) {
        eprintln!("lkb-desktop: {message}\n设置窗口创建失败：{err}");
    }
}

/// Second launch: raise the existing window (or boot again if none is left).
fn focus_or_boot(app: &AppHandle) {
    let windows = app.webview_windows();
    if windows.is_empty() {
        boot(app);
        return;
    }
    for window in windows.values() {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn main() {
    tauri::Builder::default()
        // Must be registered first: the second instance exits before any other
        // plugin runs.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_or_boot(app);
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .manage(engine::ShellState::default())
        .invoke_handler(tauri::generate_handler![setup_state, save_and_launch])
        .setup(|app| {
            boot(app.handle());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Linxira 桌面壳初始化失败")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                engine::stop(app);
            }
        });
}
