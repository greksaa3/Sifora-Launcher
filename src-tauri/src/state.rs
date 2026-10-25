//! Shared application state.
//!
//! One `AppState` is created at startup and handed to every command. It owns
//! the settings, the theme store, the HTTP clients, the running game slot and
//! the caches that must survive a settings reload.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::error::{AppError, AppResult};
use crate::mc::download::{self, EventSink};
use crate::mc::launcher::ProcessSlot;
use crate::settings::Settings;
use crate::theme::ThemeStore;

pub const USER_AGENT: &str = concat!("SiforaLauncher/", env!("CARGO_PKG_VERSION"), " (+https://github.com/greksaa3/Sifora-Launcher)");

pub struct AppState {
    /// Small, text-only client for Mojang metadata.
    pub json_http: reqwest::Client,
    /// Streaming client for jars and asset objects.
    pub bulk_http: reqwest::Client,

    pub data_dir: PathBuf,
    pub settings_path: PathBuf,
    pub backgrounds_dir: PathBuf,

    settings: Mutex<Settings>,
    themes: Arc<ThemeStore>,

    /// Root for downloads: versions, libraries, assets, java, instances.
    pub game_data: PathBuf,

    pub process: Arc<ProcessSlot>,
    /// Set to true while a download/install job is running.
    pub job_active: Arc<AtomicBool>,
    /// Cached manifest, refreshed on demand.
    pub manifest: Mutex<Option<crate::mc::manifest::VersionManifest>>,
    /// Forwarded to the renderer as `download://progress` events.
    pub event_sink: Mutex<Option<EventSink>>,
}

impl AppState {
    pub fn init(data_dir: &Path) -> AppResult<Self> {
        std::fs::create_dir_all(data_dir)
            .map_err(|e| AppError::io("Не удалось создать папку данных приложения", &e))?;
        let backgrounds_dir = data_dir.join("backgrounds");
        std::fs::create_dir_all(&backgrounds_dir)
            .map_err(|e| AppError::io("Не удалось создать папку фонов", &e))?;
        std::fs::create_dir_all(data_dir.join("build-icons"))
            .map_err(|e| AppError::io("Не удалось создать папку иконок сборок", &e))?;

        let settings_path = data_dir.join("settings.json");
        let settings = Settings::load(&settings_path);

        let game_data = match settings.game_data_dir.as_deref().map(PathBuf::from) {
            Some(custom) if custom.is_absolute() => custom,
            _ => data_dir.join("game"),
        };
        std::fs::create_dir_all(&game_data)
            .map_err(|e| AppError::io("Не удалось создать папку игровых данных", &e))?;

        let themes = ThemeStore::load(data_dir)?;
        let (json_http, bulk_http) = download::build_clients(USER_AGENT);

        Ok(AppState {
            json_http,
            bulk_http,
            data_dir: data_dir.to_path_buf(),
            settings_path,
            backgrounds_dir,
            settings: Mutex::new(settings),
            themes: Arc::new(themes),
            game_data,
            process: Arc::new(ProcessSlot::default()),
            job_active: Arc::new(AtomicBool::new(false)),
            manifest: Mutex::new(None),
            event_sink: Mutex::new(None),
        })
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().clone()
    }

    pub fn update_settings(&self, mut next: Settings) -> AppResult<Settings> {
        next.normalise();
        next.save(&self.settings_path)?;
        *self.settings.lock() = next;
        Ok(self.settings())
    }

    pub fn themes(&self) -> Arc<ThemeStore> {
        self.themes.clone()
    }

    pub fn instances_dir(&self) -> PathBuf {
        self.game_data.join("instances")
    }

    /// Build icons live under the app data directory rather than inside a build,
    /// so their asset URL never depends on where the game data directory points.
    pub fn build_icons_dir(&self) -> PathBuf {
        self.data_dir.join("build-icons")
    }

    pub fn versions_dir(&self) -> PathBuf {
        self.game_data.join("versions")
    }

    pub fn libraries_dir(&self) -> PathBuf {
        self.game_data.join("libraries")
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.game_data.join("assets")
    }

    pub fn java_dir(&self) -> PathBuf {
        self.game_data.join("java")
    }

    pub fn sink(&self) -> EventSink {
        match self.event_sink.lock().clone() {
            Some(sink) => sink,
            None => Arc::new(|_| {}),
        }
    }

    pub fn set_sink(&self, sink: EventSink) {
        *self.event_sink.lock() = Some(sink);
    }
}

/// Normalise a path coming from the renderer and make sure it is absolute.
pub fn resolve_absolute(input: &str) -> AppResult<PathBuf> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid("Путь не указан"));
    }
    let path = PathBuf::from(trimmed);
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir().map_err(|e| AppError::io("Не удалось определить текущий каталог", &e))?.join(path)
    };
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_initialises_with_defaults() {
        let dir = std::env::temp_dir().join("sifora-state-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let state = AppState::init(&dir).unwrap();
        assert_eq!(state.settings().theme_id, crate::theme::defaults::DEFAULT_THEME_ID);
        assert!(state.backgrounds_dir.is_dir());
        assert!(state.game_data.is_dir());
        assert!(state.themes().get("sifora-default").is_none());
        assert!(state.themes().get(crate::theme::defaults::DEFAULT_THEME_ID).is_some());
        assert!(state.themes().custom_count() == 0);
        assert!(
            state.themes().duplicate_names().is_empty(),
            "duplicate theme names: {:?}",
            state.themes().duplicate_names()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_updates_persist() {
        let dir = std::env::temp_dir().join("sifora-state-settings");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let state = AppState::init(&dir).unwrap();
        let mut next = state.settings();
        next.player_name = "Steve".into();
        next.ram_mb = 8192;
        state.update_settings(next).unwrap();
        assert_eq!(AppState::init(&dir).unwrap().settings().player_name, "Steve");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn relative_paths_are_resolved() {
        assert!(resolve_absolute("C:\\games").is_ok());
        assert!(resolve_absolute("  ").is_err());
    }
}