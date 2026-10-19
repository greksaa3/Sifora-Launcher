//! Persistence for themes plus the preview / undo model.
//!
//! Themes live in `themes/` inside the application data directory:
//!
//! ```text
//! themes/
//!   index.json        -- lightweight list used by the UI
//!   <id>.sifora-theme.json
//! ```
//!
//! The active theme is referenced by id from `settings.json`; the previous
//! theme id is kept as well so "undo" survives a restart.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::defaults;
use super::model::{ThemeMeta, ThemeSpec};
use super::validate::validate_theme;
use crate::error::{AppError, AppResult};
use crate::util;

const THEME_FILE_EXT: &str = "sifora-theme.json";

#[derive(Serialize, Deserialize)]
struct ThemeFile {
    spec: ThemeSpec,
}

/// On-disk list. Kept separate so the UI can render a gallery without reading
/// every token of every theme.
#[derive(Serialize, Deserialize, Default)]
struct ThemeIndex {
    #[serde(default)]
    order: Vec<String>,
}

pub struct ThemeStore {
    dir: PathBuf,
    /// All known themes, keyed by id. Built-in themes are injected at start.
    themes: Mutex<HashMap<String, ThemeSpec>>,
    /// Theme to restore if the user rejects the current one.
    undo: Mutex<Option<ThemeSpec>>,
    /// Theme currently shown but not yet committed (preview).
    preview: Mutex<Option<ThemeSpec>>,
}

impl ThemeStore {
    pub fn load(data_dir: &Path) -> AppResult<Self> {
        let dir = data_dir.join("themes");
        std::fs::create_dir_all(&dir)
            .map_err(|e| AppError::io("Не удалось создать папку тем", &e))?;

        let mut themes: HashMap<String, ThemeSpec> = HashMap::new();
        // `default_theme()` is the `mist` preset, so inserting the preset list
        // already registers it. Registering it a second time under a different
        // id is what produced two "Горы в тумане" entries in the gallery.
        for builtin in defaults::builtin_themes() {
            themes.insert(builtin.meta.id.clone(), builtin);
        }
        if let Some(fallback) = themes.get_mut(defaults::DEFAULT_THEME_ID) {
            fallback.meta.name = defaults::default_theme().meta.name;
        }

        let mut order: Vec<String> = Vec::new();
        if let Ok(raw) = std::fs::read_to_string(dir.join("index.json")) {
            if let Ok(index) = serde_json::from_str::<ThemeIndex>(&raw) {
                order = index.order;
            }
        }

        let entries = std::fs::read_dir(&dir)
            .map_err(|e| AppError::io("Не удалось прочитать папку тем", &e))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") || path == dir.join("index.json") {
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(raw) => match serde_json::from_str::<ThemeFile>(&raw) {
                    Ok(file) => {
                        let mut spec = file.spec;
                        // Never trust the file: re-validate on every load.
                        if validate_theme(&mut spec).is_valid() {
                            order.push(spec.meta.id.clone());
                            themes.insert(spec.meta.id.clone(), spec);
                        } else {
                            util::write_atomic(&path, b"").ok();
                            let _ = std::fs::remove_file(&path);
                            crate::log_warn!("theme::store", "dropped invalid theme file {}", path.display());
                        }
                    }
                    Err(err) => {
                        crate::log_warn!("theme::store", "unreadable theme {}: {}", path.display(), err);
                    }
                },
                Err(_) => continue,
            }
        }

        Ok(ThemeStore {
            dir,
            themes: Mutex::new(themes),
            undo: Mutex::new(None),
            preview: Mutex::new(None),
        })
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.{THEME_FILE_EXT}"))
    }

    pub fn summaries(&self) -> Vec<defaults::ThemeSummary> {
        let guard = self.themes.lock();
        let mut list: Vec<defaults::ThemeSummary> = guard.values().map(defaults::summarize).collect();
        list.sort_by(|a, b| b.builtin.cmp(&a.builtin).then_with(|| a.name.cmp(&b.name)));
        list
    }

    pub fn get(&self, id: &str) -> Option<ThemeSpec> {
        self.themes.lock().get(id).cloned()
    }

    /// Persist a new theme. It is validated first; an invalid theme is refused.
    pub fn insert(&self, mut spec: ThemeSpec) -> AppResult<ThemeSpec> {
        let report = validate_theme(&mut spec);
        if !report.is_valid() {
            return Err(AppError::new(
                crate::error::ErrorKind::Theme,
                "Тема не прошла проверку дизайн-системы",
            )
            .with_detail(format!("{:?}", report.errors)));
        }
        let id = spec.meta.id.clone();
        if id.is_empty() {
            return Err(AppError::config("У темы отсутствует идентификатор"));
        }
        let payload = serde_json::to_vec_pretty(&ThemeFile { spec: spec.clone() })
            .map_err(|e| AppError::config("Не удалось сериализовать тему").with_detail(e.to_string()))?;
        util::write_atomic(&self.path_for(&id), &payload)
            .map_err(|e| AppError::io("Не удалось сохранить тему", &e))?;

        {
            let mut guard = self.themes.lock();
            guard.insert(id, spec.clone());
            drop(guard);
        }
        self.write_index();
        Ok(spec)
    }

    /// Overwrite an existing *user* theme. Built-ins are immutable.
    pub fn replace(&self, mut spec: ThemeSpec) -> AppResult<ThemeSpec> {
        let id = spec.meta.id.clone();
        if self.get(&id).map(|t| t.meta.builtin).unwrap_or(false) {
            return Err(AppError::invalid("Встроенные темы нельзя изменять"));
        }
        spec.meta.builtin = false;
        self.insert(spec)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let builtin = self.get(id).map(|t| t.meta.builtin).unwrap_or(true);
        if builtin {
            return Err(AppError::invalid("Встроенную тему нельзя удалить"));
        }
        self.themes.lock().remove(id);
        let path = self.path_for(id);
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| AppError::io("Не удалось удалить файл темы", &e))?;
        }
        self.write_index();
        Ok(())
    }

    /// Stage a theme for preview without committing it.
    pub fn set_preview(&self, spec: Option<ThemeSpec>) {
        *self.preview.lock() = spec;
    }

    pub fn preview(&self) -> Option<ThemeSpec> {
        self.preview.lock().clone()
    }

    pub fn clear_preview(&self) {
        *self.preview.lock() = None;
    }

    /// Remember the currently active theme so it can be restored.
    pub fn push_undo(&self, current: &ThemeSpec) {
        *self.undo.lock() = Some(current.clone());
    }

    pub fn take_undo(&self) -> Option<ThemeSpec> {
        self.undo.lock().take()
    }

    pub fn can_undo(&self) -> bool {
        self.undo.lock().is_some()
    }

    fn write_index(&self) {
        let guard = self.themes.lock();
        let mut order: Vec<String> = guard
            .values()
            .filter(|t| !t.meta.builtin)
            .map(|t| t.meta.id.clone())
            .collect();
        drop(guard);
        order.sort();
        let index = ThemeIndex { order };
        if let Ok(bytes) = serde_json::to_vec_pretty(&index) {
            util::write_atomic(&self.dir.join("index.json"), &bytes).ok();
        }
    }

    /// Number of custom (non built-in) themes.
    pub fn custom_count(&self) -> usize {
        self.themes.lock().values().filter(|t| !t.meta.builtin).count()
    }

    /// Display names must be unique.
    ///
    /// A duplicate here is what makes a gallery look broken — two identical
    /// entries under different ids, with the user unable to tell them apart.
    pub fn duplicate_names(&self) -> Vec<String> {
        let guard = self.themes.lock();
        let mut seen: Vec<String> = Vec::new();
        let mut duplicates: Vec<String> = Vec::new();
        for theme in guard.values() {
            let key = theme.meta.name.to_lowercase();
            if seen.contains(&key) {
                if !duplicates.contains(&key) {
                    duplicates.push(key);
                }
            } else {
                seen.push(key);
            }
        }
        duplicates
    }
}

/// Small helper the commands use to build a brand-new custom theme from an
/// existing one.
pub fn derive_from(base: &ThemeSpec, name: &str, author: &str, prompt: Option<String>) -> ThemeSpec {
    let mut spec = base.clone();
    spec.meta = ThemeMeta {
        id: defaults::new_id(),
        name: super::model::sanitize_text(name, 48, "Sifora theme"),
        author: super::model::sanitize_text(author, 48, "sifora"),
        description: base.meta.description.clone(),
        prompt,
        created_at: util::now_rfc3339(),
        builtin: false,
        schema: super::model::THEME_SCHEMA,
    };
    spec
}