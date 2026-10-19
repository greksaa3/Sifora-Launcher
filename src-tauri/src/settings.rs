//! Persistent application settings.
//!
//! Stored as `settings.json` in the OS application-data directory. Values are
//! range-checked on load as well as on write, so a hand-edited or truncated
//! file degrades to defaults instead of breaking the launcher.
//!
//! Unlike themes, `Settings` deliberately *ignores* unknown fields: a settings
//! file written by a newer build must not wipe an older build's preferences.
//! (Themes are the opposite — see `theme::model`.)

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::util;

pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema: u32,

    /// Offline-mode player name.
    pub player_name: String,
    /// Default heap size for new instances, in megabytes.
    pub ram_mb: u32,
    /// Custom game-data root. `None` keeps everything inside app data.
    pub game_data_dir: Option<String>,
    /// Explicit Java home, bypassing discovery.
    pub java_path: Option<String>,
    /// Download a managed Temurin runtime when none is found.
    pub auto_provision_java: bool,

    pub selected_version: Option<String>,
    pub last_background: Option<String>,

    pub concurrent_downloads: u32,
    pub close_on_launch: bool,
    pub minimise_to_tray_on_close: bool,

    pub window: WindowState,

    /// Actively applied theme id.
    pub theme_id: String,
    /// Theme that was applied before the current one — powers "undo".
    pub previous_theme_id: Option<String>,

    /// Force the "always reduce motion" policy regardless of the theme.
    pub force_reduce_motion: bool,
    /// Show the frametime/hover debug readout in development builds.
    pub debug_overlay: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            schema: SCHEMA,
            player_name: "Player".to_string(),
            ram_mb: 2048,
            game_data_dir: None,
            java_path: None,
            auto_provision_java: true,
            selected_version: None,
            last_background: None,
            concurrent_downloads: 8,
            close_on_launch: false,
            minimise_to_tray_on_close: false,
            window: WindowState::default(),
            theme_id: crate::theme::defaults::DEFAULT_THEME_ID.to_string(),
            previous_theme_id: None,
            force_reduce_motion: false,
            debug_overlay: cfg!(debug_assertions),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct WindowState {
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        WindowState { width: 1180, height: 760, maximized: false }
    }
}

impl Settings {
    pub fn normalise(&mut self) {
        self.schema = SCHEMA;
        let name = self.player_name.trim();
        self.player_name = if name.is_empty() { "Player".into() } else { name.chars().take(16).collect() };
        self.ram_mb = self.ram_mb.clamp(512, 32_768);
        self.concurrent_downloads = self.concurrent_downloads.clamp(1, 32);
        self.window.width = self.window.width.clamp(880, 10_000);
        self.window.height = self.window.height.clamp(560, 10_000);
        if self.theme_id.trim().is_empty() {
            self.theme_id = crate::theme::defaults::DEFAULT_THEME_ID.into();
        }
        self.java_path = self
            .java_path
            .take()
            .filter(|p| !p.trim().is_empty())
            .map(|p| PathBuf::from(p).to_string_lossy().to_string());
        self.game_data_dir = self
            .game_data_dir
            .take()
            .filter(|p| !p.trim().is_empty())
            .map(|p| PathBuf::from(p).to_string_lossy().to_string());
        // Only accept ids that look like our own slugs; anything else is a
        // leftover from a hand-edited file. `sifora-default` was the id of the
        // shipped default before it became the `mist` preset, so old settings
        // files are migrated rather than silently reset.
        if !self.theme_id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            self.theme_id = crate::theme::defaults::DEFAULT_THEME_ID.into();
        } else if self.theme_id == "sifora-default" {
            self.theme_id = crate::theme::defaults::DEFAULT_THEME_ID.into();
        }
    }

    pub fn load(path: &Path) -> Self {
        let mut settings = std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Settings>(&raw).ok())
            .unwrap_or_default();
        settings.normalise();
        settings
    }

    pub fn save(&self, path: &Path) -> AppResult<()> {
        let mut copy = self.clone();
        copy.normalise();
        let bytes = serde_json::to_vec_pretty(&copy)
            .map_err(|e| AppError::config("Не удалось сериализовать настройки").with_detail(e.to_string()))?;
        util::write_atomic(path, &bytes)
            .map_err(|e| AppError::io("Не удалось сохранить настройки", &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let mut s = Settings::default();
        s.normalise();
        assert_eq!(s.schema, SCHEMA);
        assert_eq!(s.theme_id, crate::theme::defaults::DEFAULT_THEME_ID);
        assert!((512..=32_768).contains(&s.ram_mb));
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let mut s = Settings { ram_mb: 0, concurrent_downloads: 9999, ..Settings::default() };
        s.normalise();
        assert_eq!(s.ram_mb, 512);
        assert_eq!(s.concurrent_downloads, 32);
    }

    #[test]
    fn hostile_theme_id_falls_back_to_the_default() {
        let mut s = Settings { theme_id: "../../evil".into(), ..Settings::default() };
        s.normalise();
        assert_eq!(s.theme_id, crate::theme::defaults::DEFAULT_THEME_ID);
    }

    #[test]
    fn unknown_fields_are_ignored_on_load_and_not_written_back() {
        let dir = std::env::temp_dir().join("sifora-settings-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("settings.json");
        std::fs::write(&path, br#"{"schema":1,"player_name":"Steve","__proto__":{"x":1},"futureField":7}"#)
            .unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(loaded.player_name, "Steve");
        loaded.save(&path).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("__proto__"));
        assert!(!raw.contains("futureField"));
        assert!(raw.contains("Steve"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join("sifora-settings-corrupt");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("settings.json");
        std::fs::write(&path, b"{ this is not json").unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(loaded.player_name, "Player");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn roundtrip_preserves_values() {
        let dir = std::env::temp_dir().join("sifora-settings-roundtrip");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("settings.json");
        let original = Settings {
            player_name: "Steve".into(),
            ram_mb: 4096,
            theme_id: "aurora".into(),
            selected_version: Some("1.20.4".into()),
            ..Settings::default()
        };
        original.save(&path).unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(loaded.player_name, "Steve");
        assert_eq!(loaded.ram_mb, 4096);
        assert_eq!(loaded.theme_id, "aurora");
        assert_eq!(loaded.selected_version.as_deref(), Some("1.20.4"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}