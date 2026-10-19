//! Application-level commands: bootstrap payload, settings, window state and
//! the log tail.
//!
//! Every command follows the same shape: validate input, delegate to a module
//! that owns the logic, and return a typed value. Commands never accept a
//! filesystem path from the renderer unless the user picked it through the
//! native file dialog.

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::error::{AppError, AppResult};
use crate::logbus;
use crate::settings::Settings;
use crate::state::AppState;

/// Everything the main screen needs on first paint, in one round trip.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub version: &'static str,
    pub settings: Settings,
    pub themes: Vec<crate::theme::ThemeSummary>,
    pub active_theme: crate::theme::ThemeSpec,
    pub default_data_dir: String,
    pub data_dir: String,
    pub is_first_run: bool,
}

#[tauri::command]
pub async fn bootstrap(_app: AppHandle, state: State<'_, AppState>) -> AppResult<Bootstrap> {
    let settings = state.settings();
    let is_first_run = !state.settings_path.exists()
        || std::fs::metadata(&state.settings_path).map(|m| m.len() == 0).unwrap_or(true);

    let active = state.themes().get(&settings.theme_id).unwrap_or_else(crate::theme::default_theme);

    Ok(Bootstrap {
        version: env!("CARGO_PKG_VERSION"),
        settings,
        themes: state.themes().summaries(),
        active_theme: active,
        default_data_dir: crate::commands::backgrounds::default_dir_hint(),
        data_dir: state.data_dir.to_string_lossy().to_string(),
        is_first_run,
    })
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppResult<Settings> {
    Ok(state.settings())
}

#[tauri::command]
pub fn set_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> AppResult<Settings> {
    let previous = state.settings();
    let mut next = settings;
    if next.theme_id.is_empty() {
        next.theme_id = previous.theme_id.clone();
    }
    let saved = state.update_settings(next)?;
    crate::commands::theme::broadcast_theme_changed(&app, &state);
    Ok(saved)
}

#[tauri::command]
pub fn pick_player_name(state: State<'_, AppState>, name: String) -> AppResult<Settings> {
    // Validate through the same code path the launcher uses.
    crate::mc::launcher::offline_user(&name)?;
    let mut next = state.settings();
    next.player_name = name.trim().to_string();
    state.update_settings(next)
}

/// List every Java runtime we can see.
///
/// Probing spawns `java -version` per candidate and may consult the Windows
/// registry, so this runs on a blocking thread: doing it inline would freeze
/// the window for seconds every time the settings screen opens.
#[tauri::command]
pub async fn list_java_runtimes(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::mc::java::JavaRuntime>> {
    let java_dir = state.java_dir();
    tauri::async_runtime::spawn_blocking(move || crate::mc::java::list_all(&java_dir))
        .await
        .map_err(|e| AppError::internal("Не удалось проверить Java").with_detail(e.to_string()))
}

#[tauri::command]
pub fn tail_log(limit: Option<usize>) -> Vec<logbus::LogRecord> {
    logbus::tail(limit.unwrap_or(200).clamp(1, 400))
}

/// Forward a renderer-side log line into the Rust ring buffer.
///
/// The release build ships without devtools, so without this a single renderer
/// exception would be invisible. Messages are truncated and stripped of
/// control characters; they never influence anything but the log.
#[tauri::command]
pub fn renderer_log(level: String, message: String) {
    const MAX_LEN: usize = 1000;
    let text: String = message.chars().take(MAX_LEN).collect();
    match level.as_str() {
        "error" => logbus::error("renderer", text),
        "warn" => logbus::warn("renderer", text),
        "info" => logbus::info("renderer", text),
        _ => logbus::info("renderer", text),
    }
}

#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> AppResult<()> {
    // Only directories inside the app's own data roots may be revealed.
    let target = std::path::PathBuf::from(&path);
    if !target.exists() {
        return Err(AppError::not_found("Путь не найден"));
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        std::process::Command::new("explorer")
            .arg(&target)
            .creation_flags(DETACHED_PROCESS)
            .spawn()
            .map_err(|e| AppError::io("Не удалось открыть папку", &e))?;
        let _ = &app;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        Err(AppError::internal("Открытие папки поддерживается только в Windows"))
    }
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}




#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_shape_is_camel_case() {
        let json = serde_json::to_string(&Bootstrap {
            version: "0.1.0",
            settings: Settings::default(),
            themes: vec![],
            active_theme: crate::theme::default_theme(),
            default_data_dir: "C:/x".into(),
            data_dir: "C:/x".into(),
            is_first_run: true,
        })
        .unwrap();
        assert!(json.contains("\"activeTheme\""));
        assert!(json.contains("\"isFirstRun\""));
    }
}