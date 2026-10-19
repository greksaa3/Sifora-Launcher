//! Sifora Launcher — backend.
//!
//! ## Layering
//!
//! ```text
//!  main.rs            thin entry point
//!    lib.rs           app wiring, window setup, error handling
//!      state.rs       AppState: settings, caches, HTTP clients, process slot
//!      commands/      the IPC surface (input validation + orchestration only)
//!        mc/          Minecraft: manifest, versions, downloads, java, launch
//!        theme/       closed design-token schema, validation, composition
//!      settings.rs    persisted, range-checked preferences
//!      error.rs       typed errors that cross the IPC boundary
//!      logbus.rs      zero-dependency ring-buffer logger
//! ```
//!
//! The renderer (Vue) owns presentation and nothing else. It never touches the
//! filesystem, the network or a process directly; every one of those operations
//! is a Rust command with explicit validation.

pub mod commands;
pub mod error;
pub mod logbus;
pub mod mc;
pub mod settings;
pub mod state;
pub mod theme;
pub mod util;

use std::path::Path;

use tauri::Manager;

use crate::error::AppError;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(commands::handler())
        .setup(|app| {
            commands::setup(app)?;
            let handle = app.handle().clone();

            // The window is configured hidden so the first frame the user sees
            // is the finished UI rather than an empty WebView. It is revealed
            // once the renderer reports that it has mounted.
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.set_title("Sifora Launcher");
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let handle = window.app_handle();
                if let Some(state) = handle.try_state::<state::AppState>() {
                    let settings = state.settings();
                    if settings.minimise_to_tray_on_close {
                        // Hide instead of quitting: the process keeps running
                        // only while a game is alive, otherwise it exits.
                        if state.process.is_running() {
                            if let Some(main) = handle.get_webview_window("main") {
                                let _ = main.hide();
                            }
                            api.prevent_close();
                            return;
                        }
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| {
            // The binary is a GUI subsystem application, so there is no console
            // to print to. Write the reason next to the data directory instead
            // of spawning a console the user did not ask for.
            report_fatal(&error.to_string());
        });
}

/// Record a start-up failure in a file the user can find and attach to a bug
/// report. Best effort: if this fails too, there is nothing further to try.
fn report_fatal(message: &str) {
    log_error!("lib", "fatal: {message}");
    let path = std::env::temp_dir().join("sifora-launcher-startup-error.log");
    let _ = util::write_atomic(&path, format!("{message}\n").as_bytes());
    if let Ok(home) = std::env::var("USERPROFILE") {
        let desktop = Path::new(&home).join("Desktop");
        if desktop.is_dir() {
            let _ = util::write_atomic(
                &desktop.join("sifora-launcher-startup-error.log"),
                format!("{message}\n").as_bytes(),
            );
        }
    }
}

/// Shared helper used by tests and by `run` in single-instance scenarios.
pub fn ensure_dir(path: &std::path::Path) -> Result<(), AppError> {
    std::fs::create_dir_all(path).map_err(|e| AppError::io("Не удалось создать папку", &e))
}