//! Install / launch commands.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::mc::instances::{self, InstanceState};
use crate::mc::launcher::{self, GameState, SpawnRequest};
use crate::mc::version::{Platform, ResolvedVersion};
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionDetails {
    pub id: String,
    pub main_class: String,
    pub java_major: u32,
    pub release_type: String,
    pub release_time: String,
    pub library_count: usize,
    pub native_count: usize,
    pub has_assets: bool,
    pub asset_index: String,
    pub client_size: u64,
    pub supported: bool,
    pub reason: String,
}

/// Fetch (and cache) the version metadata chain and summarise it.
#[tauri::command]
pub async fn version_details(
    state: State<'_, AppState>,
    version_id: String,
) -> AppResult<VersionDetails> {
    let manifest = state.manifest.lock().clone();
    let map: HashMap<String, String> = manifest
        .map(|m| m.versions.into_iter().map(|v| (v.id, v.url)).collect())
        .unwrap_or_default();

    let chain = launcher::resolve_version_chain(
        &state.json_http,
        &map,
        &state.game_data,
        version_id.trim(),
    )
    .await?;

    let resolved = ResolvedVersion::resolve(version_id.trim(), &chain)?;
    let (files, _cp, natives) = crate::mc::libraries::resolve_libraries(
        &resolved.libraries,
        Platform::current(),
        &state.libraries_dir(),
    );

    Ok(VersionDetails {
        id: resolved.id.clone(),
        main_class: resolved.main_class.clone(),
        java_major: resolved.java_major,
        release_type: resolved.release_type.clone(),
        release_time: resolved.release_time.clone().unwrap_or_default(),
        library_count: files.iter().filter(|f| !f.is_native).count(),
        native_count: natives.len(),
        has_assets: resolved.asset_index.is_some(),
        asset_index: resolved.asset_index.as_ref().map(|a| a.id.clone()).unwrap_or_default(),
        client_size: resolved.client.as_ref().map(|c| c.size).unwrap_or(0),
        supported: true,
        reason: String::new(),
    })
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    pub version_id: String,
    #[serde(default)]
    pub ram_mb: Option<u32>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResponse {
    pub instance_id: String,
    pub slug: String,
    pub instance_path: String,
    pub libraries: usize,
    pub assets: u64,
    pub client_size: u64,
    pub java_version: u32,
    pub java_path: String,
    pub state: &'static str,
}

#[tauri::command]
pub async fn install_version(
    state: State<'_, AppState>,
    request: InstallRequest,
) -> AppResult<InstallResponse> {
    if state.job_active.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err(AppError::invalid("Другая загрузка уже выполняется"));
    }
    let _guard = JobGuard(Arc::clone(&state.job_active));

    let version_id = request.version_id.trim().to_string();
    if version_id.is_empty() {
        return Err(AppError::invalid("Версия не выбрана"));
    }

    let manifest = state.manifest.lock().clone();
    let map: HashMap<String, String> = manifest
        .map(|m| m.versions.into_iter().map(|v| (v.id, v.url)).collect())
        .unwrap_or_default();

    let chain =
        launcher::resolve_version_chain(&state.json_http, &map, &state.game_data, &version_id)
            .await?;
    let resolved = ResolvedVersion::resolve(&version_id, &chain)?;

    let instances_root = state.instances_dir();
    std::fs::create_dir_all(&instances_root)
        .map_err(|e| AppError::io("Не удалось создать папку экземпляров", &e))?;
    let meta = instances::ensure(&instances_root, &version_id, request.name.as_deref())?;
    let paths = instances::paths(&instances_root, &meta.slug);
    instances::set_state(&paths, InstanceState::Partial)?;

    let runtime_hint = discover_java(&state, resolved.java_major).await;

    let cancel = Arc::new(AtomicBool::new(false));
    let outcome = launcher::prepare(
        &state.json_http,
        &state.bulk_http,
        &state.game_data,
        &paths,
        &resolved,
        runtime_hint.as_ref(),
        cancel,
        state.sink(),
    )
    .await;

    match outcome {
        Ok(outcome) => {
            instances::set_state(&paths, InstanceState::Ready)?;
            let mut next = meta.clone();
            next.state = InstanceState::Ready;
            if request.ram_mb.is_some() {
                next.ram_mb = request.ram_mb.map(|v| v.clamp(512, 32_768));
            }
            if request.width.is_some() {
                next.width = request.width.map(|v| v.clamp(320, 7680));
            }
            if request.height.is_some() {
                next.height = request.height.map(|v| v.clamp(240, 4320));
            }
            instances::save(&paths.meta, &next)?;

            let client_size = std::fs::metadata(&crate::mc::launcher::client_jar_path(
                &state.game_data,
                &version_id,
            ))
            .map(|m| m.len())
            .unwrap_or(0);

            crate::log_info!(
                "commands::install",
                "instance {} ready ({} libraries, {} assets)",
                next.slug,
                outcome.library_count,
                outcome.asset_count
            );

            Ok(InstallResponse {
                instance_id: next.id.clone(),
                slug: next.slug.clone(),
                instance_path: paths.root.to_string_lossy().to_string(),
                libraries: outcome.library_count,
                assets: outcome.asset_count,
                client_size,
                java_version: outcome.runtime.major,
                java_path: outcome.runtime.path.to_string_lossy().to_string(),
                state: "ready",
            })
        }
        Err(err) => {
            instances::set_state(&paths, InstanceState::Partial)?;
            Err(err)
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRequest {
    pub version_id: String,
    #[serde(default)]
    pub instance_slug: Option<String>,
    #[serde(default)]
    pub ram_mb: Option<u32>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

#[tauri::command]
pub async fn launch_game(
    state: State<'_, AppState>,
    request: LaunchRequest,
) -> AppResult<launcher::GameSession> {
    if state.process.is_running() {
        return Err(AppError::invalid("Minecraft уже запущен"));
    }
    if state.job_active.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(AppError::invalid("Дождитесь окончания загрузки"));
    }

    let settings = state.settings();
    let user = launcher::offline_user(&settings.player_name)?;

    let version_id = request.version_id.trim().to_string();
    let instances_root = state.instances_dir();
    let meta = instances::ensure(&instances_root, &version_id, None)?;
    let paths = instances::paths(&instances_root, &meta.slug);

    let manifest = state.manifest.lock().clone();
    let map: HashMap<String, String> = manifest
        .map(|m| m.versions.into_iter().map(|v| (v.id, v.url)).collect())
        .unwrap_or_default();
    let chain =
        launcher::resolve_version_chain(&state.json_http, &map, &state.game_data, &version_id)
            .await?;
    let resolved = ResolvedVersion::resolve(&version_id, &chain)?;

    let runtime_hint = discover_java(&state, resolved.java_major).await;

    let cancel = Arc::new(AtomicBool::new(false));
    let outcome = launcher::prepare(
        &state.json_http,
        &state.bulk_http,
        &state.game_data,
        &paths,
        &resolved,
        runtime_hint.as_ref(),
        cancel,
        state.sink(),
    )
    .await?;

    instances::set_state(&paths, InstanceState::Ready)?;

    let ram = request
        .ram_mb
        .or(meta.ram_mb)
        .unwrap_or(settings.ram_mb)
        .clamp(512, 32_768);
    let width = request.width.or(meta.width).unwrap_or(1280);
    let height = request.height.or(meta.height).unwrap_or(720);

    let game_settings = settings.clone();
    launcher::spawn(
        Arc::clone(&state.process),
        SpawnRequest {
            instance_meta: &meta,
            instance: &paths,
            resolved: &outcome.resolved,
            classpath: &outcome.classpath,
            runtime: &outcome.runtime,
            natives_dir: &outcome.natives_dir,
            user: &user,
            asset_index_name: &outcome.asset_index_name,
            ram_mb: ram,
            width,
            height,
            version_type: &outcome.resolved.release_type,
            extra_jvm: &meta.jvm_args,
            extra_game: &game_args_for(&meta, game_settings.close_on_launch),
        },
        state.sink(),
    )
    .await
}

fn game_args_for(meta: &crate::mc::instances::InstanceMeta, _close_on_launch: bool) -> Vec<String> {
    meta.game_args.clone()
}

#[tauri::command]
pub fn game_status(state: State<'_, AppState>) -> GameState {
    state.process.snapshot().map(|s| s.state).unwrap_or(GameState::Idle)
}

#[tauri::command]
pub fn game_session(state: State<'_, AppState>) -> Option<launcher::GameSession> {
    state.process.snapshot()
}

#[tauri::command]
pub async fn stop_game(state: State<'_, AppState>) -> AppResult<()> {
    // Killing a child the supervisor owns would leave the slot inconsistent,
    // so the honest implementation is: report that stopping is not supported
    // and let the player close Minecraft.
    let session = state.process.snapshot();
    match session {
        Some(s) if s.state == GameState::Running => Err(AppError::unsupported(
            "Завершение игры выполняется самим клиентом Minecraft",
        )),
        _ => Ok(()),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSummary {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub version_id: String,
    pub state: InstanceState,
    pub path: String,
    pub size_bytes: u64,
    pub ram_mb: Option<u32>,
    pub created_at: String,
    pub updated_at: String,
}

#[tauri::command]
pub fn list_instances(state: State<'_, AppState>) -> Vec<InstanceSummary> {
    let root = state.instances_dir();
    instances::list(&root)
        .into_iter()
        .map(|meta| {
            let paths = instances::paths(&root, &meta.slug);
            InstanceSummary {
                size_bytes: instances::disk_usage(&paths),
                path: paths.root.to_string_lossy().to_string(),
                id: meta.id,
                slug: meta.slug,
                name: meta.name,
                version_id: meta.version_id,
                state: meta.state,
                ram_mb: meta.ram_mb,
                created_at: meta.created_at,
                updated_at: meta.updated_at,
            }
        })
        .collect()
}

#[tauri::command]
pub fn update_instance_settings(
    state: State<'_, AppState>,
    slug: String,
    ram_mb: Option<u32>,
    width: Option<u32>,
    height: Option<u32>,
    name: Option<String>,
) -> AppResult<InstanceSummary> {
    let root = state.instances_dir();
    let paths = instances::paths(&root, &slug);
    let mut meta = instances::load(&paths.meta)?;
    if let Some(ram) = ram_mb {
        meta.ram_mb = Some(ram.clamp(512, 32_768));
    }
    if let Some(w) = width {
        meta.width = Some(w.clamp(320, 7680));
    }
    if let Some(h) = height {
        meta.height = Some(h.clamp(240, 4320));
    }
    if let Some(n) = name {
        let cleaned = n.trim();
        if !cleaned.is_empty() && cleaned.chars().count() <= 48 {
            meta.name = cleaned.to_string();
        }
    }
    instances::save(&paths.meta, &meta)?;
    Ok(InstanceSummary {
        size_bytes: instances::disk_usage(&paths),
        path: paths.root.to_string_lossy().to_string(),
        id: meta.id,
        slug: meta.slug,
        name: meta.name,
        version_id: meta.version_id,
        state: meta.state,
        ram_mb: meta.ram_mb,
        created_at: meta.created_at,
        updated_at: meta.updated_at,
    })
}

#[tauri::command]
pub fn delete_instance(state: State<'_, AppState>, slug: String) -> AppResult<()> {
    instances::remove(&state.instances_dir(), &slug)
}

#[tauri::command]
pub fn instance_paths(state: State<'_, AppState>, slug: String) -> AppResult<InstancePathsDto> {
    let paths = instances::paths(&state.instances_dir(), &slug);
    if !paths.root.is_dir() {
        return Err(AppError::not_found("Экземпляр не найден"));
    }
    Ok(InstancePathsDto {
        root: paths.root.to_string_lossy().to_string(),
        saves: paths.root.join("saves").to_string_lossy().to_string(),
        mods: paths.root.join("mods").to_string_lossy().to_string(),
        config: paths.root.join("config").to_string_lossy().to_string(),
        resourcepacks: paths.root.join("resourcepacks").to_string_lossy().to_string(),
        logs: paths.root.join("logs").to_string_lossy().to_string(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstancePathsDto {
    pub root: String,
    pub saves: String,
    pub mods: String,
    pub config: String,
    pub resourcepacks: String,
    pub logs: String,
}

/// Release the "job running" flag even when a command fails.
struct JobGuard(Arc<AtomicBool>);

impl Drop for JobGuard {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Discover a suitable Java runtime off the UI thread.
///
/// Discovery spawns `java -version` for each candidate and may consult the
/// Windows registry; running it inline would stall the window for seconds.
pub async fn discover_java(
    state: &AppState,
    major: u32,
) -> Option<crate::mc::java::JavaRuntime> {
    let hint = state.settings().java_path.clone();
    let dir = state.java_dir();
    tauri::async_runtime::spawn_blocking(move || crate::mc::java::discover(major, hint.as_deref(), &dir))
        .await
        .ok()
        .flatten()
}

/// Shared helper for commands that need the resolved chain of a version.
#[allow(dead_code)]
pub async fn resolve(state: &AppState, version_id: &str) -> AppResult<ResolvedVersion> {
    let manifest = state.manifest.lock().clone();
    let map: HashMap<String, String> = manifest
        .map(|m| m.versions.into_iter().map(|v| (v.id, v.url)).collect())
        .unwrap_or_default();
    let chain = launcher::resolve_version_chain(&state.json_http, &map, &state.game_data, version_id)
        .await?;
    ResolvedVersion::resolve(version_id, &chain)
}