//! Version manifest handling.
//!
//! The list of versions always comes from Mojang's official
//! `version_manifest_v2.json`; nothing is hard-coded. The manifest is ~280 KB
//! of JSON, so it is fetched once, parsed on a blocking thread and cached on
//! disk. Only a compact summary crosses the IPC boundary.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult, ErrorKind};
use crate::util;

pub const MANIFEST_URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
const CACHE_FILE: &str = "version-manifest.json";
const CACHE_TTL_MS: u64 = 6 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseType {
    Release,
    Snapshot,
    #[serde(rename = "old_beta")]
    Beta,
    #[serde(rename = "old_alpha")]
    Alpha,
    /// Mojang occasionally ships a type we do not know about. Keeping it as a
    /// distinct variant means an unknown type is *shown* rather than dropped.
    #[serde(other)]
    Other,
}

impl ReleaseType {
    pub fn label(self) -> &'static str {
        match self {
            ReleaseType::Release => "Релиз",
            ReleaseType::Snapshot => "Снапшот",
            ReleaseType::Beta => "Beta",
            ReleaseType::Alpha => "Alpha",
            ReleaseType::Other => "Прочее",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub release_type: ReleaseType,
    pub url: String,
    pub time: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
    pub sha1: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<ManifestVersion>,
}

/// Compact row sent to the renderer.
#[derive(Debug, Clone, Serialize)]
pub struct VersionRow {
    pub id: String,
    pub release_type: ReleaseType,
    pub type_label: &'static str,
    pub release_time: String,
    /// `true` for the newest release / snapshot respectively.
    pub is_latest_release: bool,
    pub is_latest_snapshot: bool,
    /// Whether the client jar / version json are already cached.
    pub installed: bool,
    /// Whether this launcher build can construct a launch configuration.
    pub supported: bool,
    /// Empty when supported; otherwise a short, user-facing reason.
    pub unsupported_reason: String,
    /// Java feature version required, if the manifest declares one.
    pub java_major: u32,
    /// Instance already created for this version, if any.
    pub instance_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestSource {
    Network,
    Cache,
}

pub struct LoadedManifest {
    pub manifest: VersionManifest,
    pub source: ManifestSource,
}

fn cache_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(CACHE_FILE)
}

fn cache_is_fresh(path: &Path) -> bool {
    match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(modified) => modified
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| util::now_ms().saturating_sub(d.as_millis() as u64) < CACHE_TTL_MS)
            .unwrap_or(false),
        Err(_) => false,
    }
}

pub async fn load(
    data_dir: &Path,
    http: &reqwest::Client,
    force_refresh: bool,
) -> AppResult<LoadedManifest> {
    let cache = cache_path(data_dir);

    if !force_refresh && cache_is_fresh(&cache) {
        if let Ok(raw) = std::fs::read_to_string(&cache) {
            if let Ok(manifest) = serde_json::from_str::<VersionManifest>(&raw) {
                return Ok(LoadedManifest { manifest, source: ManifestSource::Cache });
            }
        }
    }

    let response = http
        .get(MANIFEST_URL)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| {
            AppError::new(ErrorKind::Unreachable, "Не удалось получить список версий Mojang")
                .with_detail(e.to_string())
                .retryable(true)
        })?;

    if !response.status().is_success() {
        // Network is down — fall back to a stale cache rather than failing.
        if let Ok(raw) = std::fs::read_to_string(&cache) {
            if let Ok(manifest) = serde_json::from_str::<VersionManifest>(&raw) {
                return Ok(LoadedManifest { manifest, source: ManifestSource::Cache });
            }
        }
        return Err(AppError::new(
            ErrorKind::Network,
            format!("Mojang вернул ошибку {}", response.status().as_u16()),
        )
        .retryable(true));
    }

    let manifest: VersionManifest = response.json().await.map_err(|e| {
        AppError::new(ErrorKind::Network, "Список версий повреждён").with_detail(e.to_string())
    })?;

    if let Ok(bytes) = serde_json::to_vec(&manifest) {
        util::write_atomic(&cache, &bytes).ok();
    }

    Ok(LoadedManifest { manifest, source: ManifestSource::Network })
}

/// Decide whether this launcher build can launch a given version, without
/// downloading anything.
///
/// The only two cases we knowingly refuse are the very first alpha builds that
/// predate the launch wrapper (they cannot be started from a normal classpath)
/// and anything Mojang declares with a type we do not model yet.
pub fn is_supported(release_type: ReleaseType, id: &str) -> (bool, &'static str) {
    match release_type {
        ReleaseType::Release | ReleaseType::Snapshot | ReleaseType::Beta => (true, ""),
        ReleaseType::Alpha => {
            // `rd-*` and `c0.0.*` builds run a single hard-coded main class
            // with no arguments and no launch wrapper.
            if id.starts_with("rd-") || id.starts_with("c0.0") || id.starts_with("in-") {
                (false, "Сборка старше launch wrapper — не поддерживается запуск")
            } else {
                (true, "")
            }
        }
        ReleaseType::Other => (false, "Неизвестный тип версии"),
    }
}

/// Group ordering used by the picker: newest first, with the type preserved.
pub fn sort_versions(rows: &mut [VersionRow]) {
    rows.sort_by(|a, b| b.release_time.cmp(&a.release_time).then_with(|| a.id.cmp(&b.id)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_types_with_renames() {
        let json = r#"{"latest":{"release":"1.0","snapshot":"2.0"},"versions":[
            {"id":"1.0","type":"release","url":"u","time":"t","releaseTime":"t","sha1":"s"},
            {"id":"b1","type":"old_beta","url":"u","time":"t","releaseTime":"t","sha1":"s"},
            {"id":"a1","type":"old_alpha","url":"u","time":"t","releaseTime":"t","sha1":"s"},
            {"id":"x","type":"whatever","url":"u","time":"t","releaseTime":"t","sha1":"s"}
        ]}"#;
        let m: VersionManifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.versions[0].release_type, ReleaseType::Release);
        assert_eq!(m.versions[1].release_type, ReleaseType::Beta);
        assert_eq!(m.versions[2].release_type, ReleaseType::Alpha);
        assert_eq!(m.versions[3].release_type, ReleaseType::Other);
    }

    #[test]
    fn legacy_alpha_builds_are_flagged_unsupported() {
        assert_eq!(is_supported(ReleaseType::Alpha, "rd-132211").0, false);
        assert_eq!(is_supported(ReleaseType::Alpha, "c0.0.11a").0, false);
        assert_eq!(is_supported(ReleaseType::Alpha, "a1.2.6").0, true);
        assert_eq!(is_supported(ReleaseType::Beta, "b1.7.3").0, true);
        assert_eq!(is_supported(ReleaseType::Release, "1.21.1").0, true);
        assert_eq!(is_supported(ReleaseType::Snapshot, "24w14potato").0, true);
        assert_eq!(is_supported(ReleaseType::Other, "x").0, false);
    }

    #[test]
    fn sorting_puts_newest_first() {
        let mk = |id: &str, t: &str| VersionRow {
            id: id.into(),
            release_type: ReleaseType::Release,
            type_label: "Релиз",
            release_time: t.into(),
            is_latest_release: false,
            is_latest_snapshot: false,
            installed: false,
            supported: true,
            unsupported_reason: String::new(),
            java_major: 0,
            instance_id: None,
        };
        let mut rows = vec![mk("old", "2009-01-01T00:00:00Z"), mk("new", "2026-01-01T00:00:00Z")];
        sort_versions(&mut rows);
        assert_eq!(rows[0].id, "new");
    }
}