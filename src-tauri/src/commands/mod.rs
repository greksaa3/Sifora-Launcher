//! Command surface.
//!
//! Every function registered here is part of the product's functional contract:
//! the UI can be hidden, a theme can be minimal, but the capability set itself
//! is fixed at compile time and cannot be changed by any theme, any generated
//! file or any renderer input.

pub mod app;
pub mod backgrounds;
pub mod launch;
pub mod theme;
pub mod versions;

use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::error::AppResult;
use crate::mc::download::{self, DownloadEvent};
use crate::state::AppState;

/// Install the managed state and wire the progress event bridge.
///
/// `AppState` is managed **by value**, not behind an `Arc`. Tauri's `State<'_, T>`
/// looks the type up verbatim, so registering `Arc<AppState>` while commands
/// declare `State<'_, AppState>` makes every command fail at runtime with
/// "state not managed" — the type has to match on both sides.
pub fn setup(app: &mut tauri::App) -> AppResult<()> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| {
            crate::error::AppError::new(
                crate::error::ErrorKind::Config,
                "Не удалось определить папку данных приложения",
            )
            .with_detail(e.to_string())
        })?;
    let state = AppState::init(&data_dir)?;

    // Progress events are forwarded to the renderer through the Tauri event
    // system. The closure is cheap: it only serialises when something is
    // actually happening.
    let emit_handle = app.handle().clone();
    let sink: download::EventSink = Arc::new(move |event: DownloadEvent| {
        let _ = emit_handle.emit("download://progress", event);
    });
    state.set_sink(sink);

    app.manage(state);
    Ok(())
}

/// The complete command list, in one place, so it is easy to audit.
///
/// `generate_handler!` decides, at compile time, whether a command name is a
/// real command; adding a function here is the only way to expose new
/// behaviour to the renderer.
pub fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        // application
        app::bootstrap,
        app::get_settings,
        app::set_settings,
        app::pick_player_name,
        app::list_java_runtimes,
        app::tail_log,
        app::renderer_log,
        app::reveal_path,
        app::show_main_window,
        app::quit,
        // versions
        versions::load_versions,
        versions::cached_version_ids,
        versions::summarise_versions,
        launch::version_details,
        // install / launch
        launch::install_version,
        launch::launch_game,
        launch::game_status,
        launch::game_session,
        launch::stop_game,
        launch::list_instances,
        launch::update_instance_settings,
        launch::delete_instance,
        launch::instance_paths,
        // themes
        theme::list_themes,
        theme::get_theme,
        theme::get_builtin_backgrounds,
        theme::preview_theme,
        theme::clear_preview,
        theme::current_preview,
        theme::apply_theme,
        theme::apply_theme_by_id,
        theme::undo_theme,
        theme::reset_theme,
        theme::delete_theme,
        theme::generate_theme,
        theme::get_ai_config,
        theme::set_ai_config,
        theme::derive_theme,
        theme::validate_theme_command,
        theme::theme_contrast,
        theme::default_theme_spec,
        theme::set_force_reduce_motion,
        // backgrounds
        backgrounds::pick_background_image,
        backgrounds::list_backgrounds,
        backgrounds::delete_background,
    ]
}