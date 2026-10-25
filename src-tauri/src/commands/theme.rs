//! Theme commands.
//!
//! The renderer can:
//!
//! * list themes and read the active one,
//! * preview a theme (rendered but not persisted),
//! * apply a theme (persisted, with the previous one remembered for undo),
//! * undo the last apply,
//! * reset to the built-in default,
//! * create a custom theme derived from an existing one, and edit it freely,
//! * export a theme to a file and import one back.
//!
//! Every write goes through `ThemeStore`, which re-validates the spec. A
//! theme that fails validation is refused — it is never "partially applied".

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::{AppError, AppResult};
use crate::settings::Settings;
use crate::state::AppState;
use crate::theme::{validate::validate_theme, Color, ThemeSpec, ThemeSummary, ValidationReport};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeList {
    pub themes: Vec<ThemeSummary>,
    pub active_id: String,
    pub can_undo: bool,
    pub default_id: &'static str,
    pub custom_count: usize,
}

#[tauri::command]
pub fn list_themes(state: State<'_, AppState>) -> ThemeList {
    let store = state.themes();
    let settings = state.settings();
    ThemeList {
        themes: store.summaries(),
        active_id: settings.theme_id.clone(),
        can_undo: store.can_undo(),
        default_id: crate::theme::defaults::DEFAULT_THEME_ID,
        custom_count: store.custom_count(),
    }
}

#[tauri::command]
pub fn get_theme(state: State<'_, AppState>, theme_id: Option<String>) -> AppResult<ThemeSpec> {
    let store = state.themes();
    let id = theme_id.unwrap_or_else(|| state.settings().theme_id);
    store.get(&id).ok_or_else(|| AppError::not_found("Тема не найдена"))
}

#[tauri::command]
pub fn get_builtin_backgrounds() -> Vec<crate::theme::defaults::ThemeSummary> {
    crate::theme::builtin_themes()
        .iter()
        .map(crate::theme::defaults::summarize)
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResponse {
    pub theme: ThemeSpec,
    pub report: ValidationReport,
}

/// Show a theme without committing it.
#[tauri::command]
pub fn preview_theme(state: State<'_, AppState>, theme: ThemeSpec) -> AppResult<PreviewResponse> {
    let mut candidate = theme;
    let report = validate_theme(&mut candidate);
    if !report.is_valid() {
        return Err(AppError::new(
            crate::error::ErrorKind::Theme,
            "Тема не прошла проверку дизайн-системы",
        )
        .with_detail(format!("{:?}", report.errors)));
    }
    state.themes().set_preview(Some(candidate.clone()));
    Ok(PreviewResponse { theme: candidate, report })
}

#[tauri::command]
pub fn clear_preview(state: State<'_, AppState>) {
    state.themes().clear_preview();
}

/// Read the currently previewed theme, if any.
#[tauri::command]
pub fn current_preview(state: State<'_, AppState>) -> Option<ThemeSpec> {
    state.themes().preview()
}

/// Commit a theme: remember the current one, store the new one and persist the
/// selection so it survives a restart.
#[tauri::command]
pub fn apply_theme(
    app: AppHandle,
    state: State<'_, AppState>,
    theme: ThemeSpec,
) -> AppResult<ThemeSummary> {
    let store = state.themes();
    let mut candidate = theme.clone();
    let report = validate_theme(&mut candidate);
    if !report.is_valid() {
        return Err(AppError::new(
            crate::error::ErrorKind::Theme,
            "Тема не прошла проверку дизайн-системы",
        )
        .with_detail(format!("{:?}", report.errors)));
    }

    // A built-in id is applied directly; anything else is stored first.
    let stored = if store.get(&candidate.meta.id).is_some() {
        store.replace(candidate)?
    } else {
        store.insert(candidate)?
    };

    let settings = state.settings();
    let mut next = settings.clone();
    next.previous_theme_id = Some(settings.theme_id.clone());
    next.theme_id = stored.meta.id.clone();
    state.update_settings(next)?;
    store.clear_preview();

    broadcast_theme_changed(&app, &state);
    Ok(crate::theme::defaults::summarize(&stored))
}

#[tauri::command]
pub fn apply_theme_by_id(app: AppHandle, state: State<'_, AppState>, theme_id: String) -> AppResult<ThemeSpec> {
    let store = state.themes();
    let theme = store
        .get(&theme_id)
        .ok_or_else(|| AppError::not_found("Тема не найдена"))?;

    let settings = state.settings();
    let mut next = settings.clone();
    next.previous_theme_id = Some(settings.theme_id.clone());
    next.theme_id = theme_id;
    state.update_settings(next)?;
    store.clear_preview();

    broadcast_theme_changed(&app, &state);
    Ok(theme)
}

#[tauri::command]
pub fn undo_theme(app: AppHandle, state: State<'_, AppState>) -> AppResult<ThemeSpec> {
    let store = state.themes();
    let settings = state.settings();
    let previous = settings
        .previous_theme_id
        .clone()
        .or_else(|| store.take_undo().map(|t| t.meta.id))
        .ok_or_else(|| AppError::invalid("Нечего отменять"))?;

    let theme = store
        .get(&previous)
        .ok_or_else(|| AppError::not_found("Предыдущая тема не найдена"))?;

    let mut next = state.settings();
    next.previous_theme_id = None;
    next.theme_id = previous;
    state.update_settings(next)?;
    store.clear_preview();

    broadcast_theme_changed(&app, &state);
    Ok(theme)
}

#[tauri::command]
pub fn reset_theme(app: AppHandle, state: State<'_, AppState>) -> AppResult<ThemeSpec> {
    let theme = crate::theme::default_theme();
    let settings = state.settings();
    let mut next = settings.clone();
    next.previous_theme_id = Some(settings.theme_id.clone());
    next.theme_id = theme.meta.id.clone();
    state.update_settings(next)?;
    state.themes().clear_preview();
    broadcast_theme_changed(&app, &state);
    Ok(theme)
}

#[tauri::command]
pub fn delete_theme(app: AppHandle, state: State<'_, AppState>, theme_id: String) -> AppResult<()> {
    let settings = state.settings();
    state.themes().delete(&theme_id)?;
    if settings.theme_id == theme_id {
        let mut next = settings;
        next.theme_id = crate::theme::defaults::DEFAULT_THEME_ID.into();
        state.update_settings(next)?;
        // Deleting the active theme silently changed what is on screen, so the
        // renderer has to be told rather than left wearing a theme that is gone.
        broadcast_theme_changed(&app, &state);
    }
    Ok(())
}

/// Export a theme to a file the user picks.
///
/// Built-in themes are exportable too: sharing a preset is exactly what export
/// is for, and the file carries a `builtin` flag that import deliberately
/// ignores.
#[tauri::command]
pub fn export_theme(
    app: AppHandle,
    state: State<'_, AppState>,
    theme_id: String,
) -> AppResult<Option<String>> {
    let theme = state
        .themes()
        .get(&theme_id)
        .ok_or_else(|| AppError::not_found("Тема не найдена"))?;

    let suggested = format!("{}.sifora-theme.json", crate::theme::store::file_stem(&theme));
    let picked = app
        .dialog()
        .file()
        .set_title("Куда сохранить тему")
        .set_file_name(suggested)
        .add_filter("Тема Sifora", &["json"])
        .blocking_save_file();
    let Some(picked) = picked else { return Ok(None) };
    let target = picked.into_path().map_err(|e| {
        AppError::invalid("Не удалось получить путь для сохранения").with_detail(e.to_string())
    })?;

    let bytes = crate::theme::store::to_file_bytes(&theme)?;
    std::fs::write(&target, bytes)
        .map_err(|e| AppError::io("Не удалось записать файл темы", &e))?;
    Ok(Some(target.display().to_string()))
}

/// Import a theme from a file and register it as a new custom theme.
///
/// The file's id is discarded by the store, so an import can never overwrite an
/// existing theme or impersonate a built-in one.
#[tauri::command]
pub fn import_theme(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<Option<ThemeSummary>> {
    let picked = app
        .dialog()
        .file()
        .set_title("Выберите файл темы")
        .add_filter("Тема Sifora", &["json"])
        .blocking_pick_file();
    let Some(picked) = picked else { return Ok(None) };
    let source = picked.into_path().map_err(|e| {
        AppError::invalid("Не удалось получить путь к файлу темы").with_detail(e.to_string())
    })?;

    let raw = std::fs::read(&source).map_err(|e| AppError::io("Не удалось прочитать файл", &e))?;
    let spec = crate::theme::store::from_file_bytes(&raw)?;
    let stored = state.themes().import(spec)?;
    Ok(Some(crate::theme::defaults::summarize(&stored)))
}

#[tauri::command]
pub fn derive_theme(
    state: State<'_, AppState>,
    theme: ThemeSpec,
    name: String,
) -> AppResult<ThemeSummary> {
    let store = state.themes();
    // Fork *the spec that was sent*, not a fresh copy of the stored theme: this
    // is how "edit a built-in" works, and re-reading the stored version would
    // silently discard every edit the user just made.
    let mut derived = crate::theme::store::derive_from(&theme, &name, "sifora:user", None);
    let report = validate_theme(&mut derived);
    if !report.is_valid() {
        return Err(AppError::new(
            crate::error::ErrorKind::Theme,
            "Новая тема не прошла проверку",
        )
        .with_detail(format!("{:?}", report.errors)));
    }
    let stored = store.insert(derived)?;
    Ok(crate::theme::defaults::summarize(&stored))
}

/// Validate a theme without storing it — used by the stage-2 editor.
#[tauri::command]
pub fn validate_theme_command(theme: ThemeSpec) -> ValidationReport {
    let mut candidate = theme;
    validate_theme(&mut candidate)
}

/// The contrast helpers the editor needs, exposed so the UI does not have to
/// re-implement them (and drift from the Rust behaviour).
#[tauri::command]
pub fn theme_contrast(foreground: String, background: String) -> Option<f32> {
    let fg = Color::parse(&foreground)?;
    let bg = Color::parse(&background)?;
    Some(Color::contrast_ratio(fg, bg))
}

/// Default values for a fresh theme, so an editor never has to invent them.
#[tauri::command]
pub fn default_theme_spec() -> ThemeSpec {
    crate::theme::default_theme()
}

/// Notify the renderer that the active theme changed.
pub fn broadcast_theme_changed(app: &AppHandle, state: &AppState) {
    let id = state.settings().theme_id;
    let theme = state.themes().get(&id).unwrap_or_else(crate::theme::default_theme);
    let _ = app.emit("theme://changed", &theme);
}

/// Reduce-motion is a user accessibility setting that outranks any theme.
#[tauri::command]
pub fn set_force_reduce_motion(state: State<'_, AppState>, value: bool) -> AppResult<Settings> {
    let mut next = state.settings();
    next.force_reduce_motion = value;
    state.update_settings(next)
}

/// Trade frosted glass for flat translucency.
///
/// Rendering quality is a performance decision, not a theme decision, so it
/// lives in the settings rather than in the theme tokens — a theme must not be
/// able to switch the expensive effect back on behind the user's back.
#[tauri::command]
pub fn set_light_rendering(state: State<'_, AppState>, value: bool) -> AppResult<Settings> {
    let mut next = state.settings();
    next.light_rendering = value;
    state.update_settings(next)
}