//! Version manifest commands.
//!
//! The list is fetched from Mojang, cached on disk and enriched with local
//! knowledge (is the version cached, can we launch it, is there an instance).

use std::collections::HashMap;

use serde::Serialize;
use tauri::State;

use crate::error::AppResult;
use crate::mc::manifest::{self, ManifestSource, ReleaseType, VersionRow};
use crate::mc::version::VersionMeta;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionListResponse {
    pub versions: Vec<VersionRow>,
    pub latest_release: String,
    pub latest_snapshot: String,
    pub from_cache: bool,
    pub loaded_at: String,
}

#[tauri::command]
pub async fn load_versions(
    state: State<'_, AppState>,
    force_refresh: Option<bool>,
) -> AppResult<VersionListResponse> {
    let loaded = manifest::load(
        &state.data_dir,
        &state.json_http,
        force_refresh.unwrap_or(false),
    )
    .await?;

    let m = &loaded.manifest;
    let latest_release = m.latest.release.clone();
    let latest_snapshot = m.latest.snapshot.clone();

    // Version jars and documents we already have on disk.
    let versions_dir = state.versions_dir();
    let instances = crate::mc::instances::list(&state.instances_dir());
    let instance_for: HashMap<String, String> = instances
        .iter()
        .map(|i| (i.version_id.clone(), i.id.clone()))
        .collect();

    // `javaMajor` comes from the version document; reading 900 cached documents
    // on every call would be wasteful, so only the metadata we can infer from
    // the id is reported here and the exact value arrives with `version_details`.
    let rows: Vec<VersionRow> = m
        .versions
        .iter()
        .map(|v| {
            let json_path = crate::mc::launcher::version_json_path(&versions_dir, &v.id);
            let jar_path = crate::mc::launcher::client_jar_path(&versions_dir, &v.id);
            let installed = jar_path.exists() && json_path.exists();
            let (supported, reason) = manifest::is_supported(v.release_type, &v.id);
            let java_major = if json_path.exists() {
                std::fs::read_to_string(&json_path)
                    .ok()
                    .and_then(|raw| VersionMeta::parse(&raw).ok())
                    .map(|meta| meta.required_java())
                    .unwrap_or(0)
            } else {
                0
            };
            VersionRow {
                id: v.id.clone(),
                release_type: v.release_type,
                type_label: v.release_type.label(),
                release_time: v.release_time.clone(),
                is_latest_release: v.id == latest_release,
                is_latest_snapshot: v.id == latest_snapshot,
                installed,
                supported,
                unsupported_reason: reason.to_string(),
                java_major,
                instance_id: instance_for.get(&v.id).cloned(),
            }
        })
        .collect();

    let mut rows = rows;
    manifest::sort_versions(&mut rows);

    let from_cache = loaded.source == ManifestSource::Cache;
    *state.manifest.lock() = Some(m.clone());

    Ok(VersionListResponse {
        versions: rows,
        latest_release,
        latest_snapshot,
        from_cache,
        loaded_at: crate::util::now_rfc3339(),
    })
}

/// Cached version documents already present on disk, so the picker can mark
/// them without waiting for the manifest to be re-fetched.
#[tauri::command]
pub fn cached_version_ids(state: State<'_, AppState>) -> Vec<String> {
    let dir = state.versions_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(id) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if path.join(format!("{id}.json")).exists() && path.join(format!("{id}.jar")).exists() {
            out.push(id.to_string());
        }
    }
    out.sort();
    out
}

/// Group the rows into the buckets the picker shows. Derived from the manifest
/// that is already cached in Rust, so the renderer never has to walk 900 rows
/// to draw three counters.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionGroups {
    pub latest_release: Option<String>,
    pub latest_snapshot: Option<String>,
    pub releases: usize,
    pub snapshots: usize,
    pub betas: usize,
    pub alphas: usize,
    pub installed: usize,
}

#[tauri::command]
pub fn summarise_versions(state: State<'_, AppState>) -> VersionGroups {
    let mut groups = VersionGroups {
        latest_release: None,
        latest_snapshot: None,
        releases: 0,
        snapshots: 0,
        betas: 0,
        alphas: 0,
        installed: 0,
    };

    let installed_ids: std::collections::HashSet<String> = std::fs::read_dir(state.versions_dir())
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let manifest = state.manifest.lock().clone();
    if let Some(manifest) = manifest {
        groups.latest_release = Some(manifest.latest.release.clone());
        groups.latest_snapshot = Some(manifest.latest.snapshot.clone());
        for version in &manifest.versions {
            match version.release_type {
                ReleaseType::Release => groups.releases += 1,
                ReleaseType::Snapshot => groups.snapshots += 1,
                ReleaseType::Beta => groups.betas += 1,
                ReleaseType::Alpha => groups.alphas += 1,
                ReleaseType::Other => {}
            }
            if installed_ids.contains(&version.id) {
                groups.installed += 1;
            }
        }
    }
    groups
}