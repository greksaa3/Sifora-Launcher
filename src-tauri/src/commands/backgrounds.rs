//! Background image commands.
//!
//! The only way a background image enters the application is the native file
//! dialog. The chosen file is **copied into the app's own `backgrounds/`
//! folder**, and only that folder is exposed through the asset protocol — so a
//! stale or hostile path in a saved theme can never make the renderer read an
//! arbitrary file off the machine.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const ALLOWED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "avif"];

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundImage {
    /// Stable id stored inside the theme (`ImageRef::Local`).
    pub id: String,
    /// Absolute path inside the app data directory.
    pub path: String,
    /// Tauri asset-protocol URL for the renderer.
    pub asset_url: String,
    pub file_name: String,
    pub size_bytes: u64,
}

#[tauri::command]
pub async fn pick_background_image(app: AppHandle, state: State<'_, AppState>) -> AppResult<Option<BackgroundImage>> {
    let picked = app
        .dialog()
        .file()
        .set_title("Выберите фон")
        .add_filter("Изображения", ALLOWED_EXTENSIONS)
        .blocking_pick_file();

    let Some(picked) = picked else { return Ok(None) };
    let source = picked
        .into_path()
        .map_err(|e| AppError::invalid("Не удалось получить путь к изображению").with_detail(e.to_string()))?;
    let imported = import_image(&state.backgrounds_dir, &source)?;
    Ok(Some(imported))
}

/// Import (copy) an image into the managed folder and describe it.
pub fn import_image(directory: &Path, source: &Path) -> AppResult<BackgroundImage> {
    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .ok_or_else(|| AppError::invalid("Файл не является изображением"))?;
    if !ALLOWED_EXTENSIONS.contains(&extension.as_str()) {
        return Err(AppError::invalid(format!("Неподдерживаемый формат: {extension}")));
    }

    let metadata = std::fs::metadata(source)
        .map_err(|e| AppError::io("Не удалось прочитать изображение", &e))?;
    // A wallpaper bigger than this is never useful and wastes disk.
    const MAX_BYTES: u64 = 48 * 1024 * 1024;
    if metadata.len() > MAX_BYTES {
        return Err(AppError::invalid("Изображение слишком большое (максимум 48 МБ)"));
    }

    std::fs::create_dir_all(directory).map_err(|e| AppError::io("Не удалось создать папку фонов", &e))?;
    let id = format!("{}.{extension}", crate::theme::defaults::new_id());
    let destination: PathBuf = directory.join(&id);
    std::fs::copy(source, &destination)
        .map_err(|e| AppError::io("Не удалось скопировать изображение", &e))?;

    Ok(BackgroundImage {
        asset_url: format!("asset:localhost/backgrounds/{id}"),
        id,
        path: destination.to_string_lossy().to_string(),
        file_name: source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("background")
            .to_string(),
        size_bytes: metadata.len(),
    })
}

/// List previously imported backgrounds.
#[tauri::command]
pub fn list_backgrounds(state: State<'_, AppState>) -> Vec<BackgroundImage> {
    let Ok(entries) = std::fs::read_dir(&state.backgrounds_dir) else { return Vec::new() };
    let mut out: Vec<BackgroundImage> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let id = path.file_name()?.to_str()?.to_string();
            let extension = path.extension()?.to_str()?.to_ascii_lowercase();
            if !ALLOWED_EXTENSIONS.contains(&extension.as_str()) {
                return None;
            }
            let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            Some(BackgroundImage {
                asset_url: format!("asset:localhost/backgrounds/{id}"),
                path: path.to_string_lossy().to_string(),
                file_name: id.clone(),
                size_bytes,
                id,
            })
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Remove an imported background. Refuses anything outside the managed folder.
#[tauri::command]
pub fn delete_background(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let safe = crate::mc::assets::safe_key(&id)
        .filter(|_| !id.contains('/') && !id.contains('\\'))
        .ok_or_else(|| AppError::invalid("Некорректный идентификатор фона"))?;
    let path = state.backgrounds_dir.join(&safe);
    let inside = state
        .backgrounds_dir
        .canonicalize()
        .ok()
        .zip(path.canonicalize().ok())
        .map(|(base, real)| real.starts_with(base))
        .unwrap_or(false);
    if !inside {
        return Err(AppError::invalid("Файл находится вне папки фонов"));
    }
    std::fs::remove_file(&path).map_err(|e| AppError::io("Не удалось удалить фон", &e))
}

pub fn default_dir_hint() -> String {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(|home| PathBuf::from(home).join("Pictures").to_string_lossy().to_string())
        .unwrap_or_else(|_| "Изображения".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn import_rejects_unsupported_extensions() {
        let dir = temp_dir("sifora-bg-ext");
        let source = dir.join("evil.exe");
        std::fs::write(&source, b"MZ").unwrap();
        let err = import_image(&dir.join("store"), &source).unwrap_err();
        assert!(err.message.contains("Неподдерживаемый формат"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_copies_the_file_and_returns_an_asset_url() {
        let dir = temp_dir("sifora-bg-ok");
        let store = dir.join("store");
        let source = dir.join("wall.png");
        std::fs::write(&source, b"\x89PNG\r\n\x1a\n").unwrap();
        let imported = import_image(&store, &source).unwrap();
        assert!(imported.id.ends_with(".png"));
        assert!(Path::new(&imported.path).starts_with(&store));
        assert!(imported.asset_url.starts_with("asset:localhost/backgrounds/"));
        assert!(Path::new(&imported.path).is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_rejects_oversized_files() {
        let dir = temp_dir("sifora-bg-big");
        let source = dir.join("huge.png");
        let handle = std::fs::File::create(&source).unwrap();
        handle.set_len(49 * 1024 * 1024).unwrap();
        drop(handle);
        let err = import_image(&dir.join("store"), &source).unwrap_err();
        assert!(err.message.contains("слишком большое"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_refuses_path_traversal() {
        let dir = temp_dir("sifora-bg-delete");
        let store = dir.join("store");
        std::fs::create_dir_all(&store).unwrap();
        for hostile in ["../../secret.txt", "a/b.png", "..\\x.png"] {
            let err = delete_background_from(&store, hostile);
            assert!(err.is_err(), "must refuse {hostile}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn delete_background_from(store: &Path, id: &str) -> AppResult<()> {
        let safe = crate::mc::assets::safe_key(id)
            .filter(|_| !id.contains('/') && !id.contains('\\'))
            .ok_or_else(|| AppError::invalid("Некорректный идентификатор фона"))?;
        let path = store.join(&safe);
        let inside = store.canonicalize().ok().zip(path.canonicalize().ok()).map(|(b, r)| r.starts_with(b)).unwrap_or(false);
        if !inside {
            return Err(AppError::invalid("Файл находится вне папки фонов"));
        }
        std::fs::remove_file(&path).map_err(|e| AppError::io("Не удалось удалить фон", &e))
    }
}