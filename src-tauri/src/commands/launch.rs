//! Version metadata and game-process commands.
//!
//! Installing and launching a *build* lives in [`super::builds`]; what remains
//! here is the shared machinery both paths need — summarising a version, Java
//! resolution (including first-run provisioning) and the process slot the UI
//! polls.

use std::collections::HashMap;

use serde::Serialize;
use tauri::State;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::mc::download;
use crate::mc::java::JavaRuntime;
use crate::mc::launcher::{self, GameState};
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
    let resolved = resolve(&state, version_id.trim()).await?;
    let (files, _classpath, natives) = crate::mc::libraries::resolve_libraries(
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
    // Killing a child the supervisor owns would leave the slot inconsistent, so
    // the honest implementation is: report that stopping is not supported and
    // let the player close Minecraft.
    match state.process.snapshot() {
        Some(session) if session.state == GameState::Running => Err(AppError::unsupported(
            "Завершение игры выполняется самим клиентом Minecraft",
        )),
        _ => Ok(()),
    }
}

/// Discover a Java runtime off the UI thread.
///
/// Discovery spawns `java -version` for each candidate and may consult the
/// Windows registry; running it inline would stall the window for seconds.
pub async fn discover_java(state: &AppState, major: u32) -> Option<JavaRuntime> {
    let hint = state.settings().java_path.clone();
    let dir = state.java_dir();
    tauri::async_runtime::spawn_blocking(move || crate::mc::java::discover(major, hint.as_deref(), &dir))
        .await
        .ok()
        .flatten()
}

/// A Java runtime for `major`, downloading one if the user allowed it.
///
/// Provisioning was previously unreachable code: a machine without a suitable
/// Java simply failed, with `auto_provision_java` sitting at `true` and doing
/// nothing. This is the single place that honours that setting.
pub async fn ensure_java(state: &AppState, major: u32) -> AppResult<JavaRuntime> {
    if let Some(runtime) = discover_java(state, major).await {
        return Ok(runtime);
    }

    let settings = state.settings();
    if !settings.auto_provision_java {
        return Err(AppError::new(
            ErrorKind::Java,
            format!("Не найдена Java {major}, а автоматическая загрузка выключена в настройках"),
        ));
    }

    crate::log_info!("commands::launch", "provisioning Java {major} from Adoptium");
    let sink = state.sink();
    let job = format!("java-{major}");
    let reported = std::sync::atomic::AtomicU64::new(0);
    let progress = move |done: u64, total: u64| {
        // The callback fires per chunk; report at coarse milestones so the UI
        // is not flooded with hundreds of events per second.
        let step = (total / 20).max(1);
        let previous = reported.load(std::sync::atomic::Ordering::Relaxed);
        if done >= previous.saturating_add(step) || done == total {
            reported.store(done, std::sync::atomic::Ordering::Relaxed);
            sink(download::DownloadEvent::Aggregate {
                job: job.clone(),
                done_files: 0,
                total_files: 1,
                bytes: done,
                total_bytes: total,
            });
        }
    };

    crate::mc::java::provision(
        &state.bulk_http,
        major,
        &state.java_dir(),
        &std::sync::atomic::AtomicBool::new(false),
        &progress,
    )
    .await
}

/// Shared helper for commands that need the resolved chain of a version.
pub async fn resolve(state: &AppState, version_id: &str) -> AppResult<ResolvedVersion> {
    let versions = ensure_manifest(state).await?;
    let chain = launcher::resolve_version_chain(
        &state.json_http,
        &versions,
        &state.game_data,
        version_id,
    )
    .await?;
    ResolvedVersion::resolve(version_id, &chain)
}

/// The id → URL map of the Mojang manifest, guaranteed to be populated.
///
/// The launch screen used to be the only thing that called `load_versions`, and
/// that command is what filled this cache. A build carries its own version now,
/// so installing or launching can be the very first thing that needs the
/// manifest — hence the load here rather than "версия отсутствует в манифесте"
/// on a build that is perfectly valid.
pub async fn ensure_manifest(state: &AppState) -> AppResult<HashMap<String, String>> {
    let cached = state.manifest.lock().clone();
    if let Some(manifest) = cached {
        return Ok(manifest
            .versions
            .into_iter()
            .map(|v| (v.id, v.url))
            .collect());
    }

    let loaded = crate::mc::manifest::load(&state.data_dir, &state.json_http, false).await?;
    let map = loaded
        .manifest
        .versions
        .iter()
        .map(|v| (v.id.clone(), v.url.clone()))
        .collect();
    *state.manifest.lock() = Some(loaded.manifest);
    Ok(map)
}
