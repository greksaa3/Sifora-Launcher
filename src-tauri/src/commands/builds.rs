//! Build commands: create, configure, install, launch — plus the mod library.
//!
//! A **build** is the unit a player actually plays. This module is the only
//! place that turns a build into a running game, and it always follows the same
//! order:
//!
//! 1. **Resolve the loader.** Fabric publishes a profile document; Forge and
//!    NeoForge ship an installer that has to be run. Either way the build ends
//!    up with a `launch_id` naming a version document on disk, and that id is
//!    remembered so the next launch skips the step entirely.
//! 2. **Prepare the files.** Client jar, libraries, natives, assets and Java —
//!    all resumable and checksum-verified, in shared caches.
//! 3. **Spawn.** Arguments are handed to the OS as a `Vec<String>`.
//!
//! Installation is idempotent: pressing Play on a ready build re-checks the
//! files and starts the game without downloading anything again.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::{AppError, AppResult};
use crate::mc::instances::{
    self, BuildPatch, InstanceMeta, InstancePaths, InstanceState, ModLoader, NewBuild,
};
use crate::mc::launcher::{self, SpawnRequest};
use crate::mc::loaders;
use crate::mc::modrinth;
use crate::mc::version::ResolvedVersion;
use crate::state::AppState;

/// Image formats accepted for a build icon.
const ICON_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp"];

// ---------------------------------------------------------------------------
// Build summaries
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildSummary {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub version_id: String,
    pub launch_id: Option<String>,
    pub loader: ModLoader,
    pub loader_version: String,
    pub state: InstanceState,
    pub path: String,
    pub size_bytes: u64,
    pub ram_mb: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    pub icon_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_played_at: Option<String>,
}

fn summarise(root: &Path, meta: InstanceMeta) -> BuildSummary {
    let paths = instances::paths(root, &meta.slug);
    BuildSummary {
        // Icons live in the app data folder, so the URL does not depend on
        // where the game data directory points.
        icon_url: meta.icon.as_ref().map(|file| format!("asset:localhost/build-icons/{file}")),
        size_bytes: instances::disk_usage(&paths),
        path: paths.root.to_string_lossy().to_string(),
        id: meta.id,
        slug: meta.slug,
        name: meta.name,
        version_id: meta.version_id,
        launch_id: meta.launch_id,
        loader: meta.mods.loader,
        loader_version: meta.mods.loader_version,
        state: meta.state,
        ram_mb: meta.ram_mb,
        width: meta.width,
        height: meta.height,
        jvm_args: meta.jvm_args,
        game_args: meta.game_args,
        created_at: meta.created_at,
        updated_at: meta.updated_at,
        last_played_at: meta.last_played_at,
    }
}

#[tauri::command]
pub fn list_builds(state: State<'_, AppState>) -> Vec<BuildSummary> {
    let root = state.instances_dir();
    instances::list(&root)
        .into_iter()
        .map(|meta| summarise(&root, meta))
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateBuildRequest {
    pub name: String,
    pub version_id: String,
    #[serde(default)]
    pub loader: ModLoader,
    #[serde(default)]
    pub loader_version: String,
    #[serde(default)]
    pub ram_mb: Option<u32>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub jvm_args: Vec<String>,
    #[serde(default)]
    pub game_args: Vec<String>,
}

#[tauri::command]
pub fn create_build(
    state: State<'_, AppState>,
    request: CreateBuildRequest,
) -> AppResult<BuildSummary> {
    if request.loader != ModLoader::None && request.loader_version.trim().is_empty() {
        return Err(AppError::invalid("Выберите версию загрузчика"));
    }
    let root = state.instances_dir();
    let meta = instances::create(
        &root,
        NewBuild {
            name: &request.name,
            version_id: &request.version_id,
            loader: request.loader,
            loader_version: &request.loader_version,
            ram_mb: request.ram_mb,
            width: request.width,
            height: request.height,
            jvm_args: request.jvm_args,
            game_args: request.game_args,
        },
    )?;
    crate::log_info!("commands::builds", "build {} created ({})", meta.slug, meta.version_id);
    Ok(summarise(&root, meta))
}

#[tauri::command]
pub fn update_build(
    state: State<'_, AppState>,
    slug: String,
    patch: BuildPatch,
) -> AppResult<BuildSummary> {
    let root = state.instances_dir();
    let meta = instances::update(&root, &slug, &patch)?;
    Ok(summarise(&root, meta))
}

#[tauri::command]
pub fn delete_build(state: State<'_, AppState>, slug: String) -> AppResult<()> {
    let root = state.instances_dir();
    let meta = instances::load(&instances::paths(&root, &slug).meta).ok();
    instances::remove(&root, &slug)?;
    // The icon is a separate file; remove it too rather than leave an orphan.
    if let Some(icon) = meta.and_then(|m| m.icon) {
        let _ = std::fs::remove_file(state.build_icons_dir().join(icon));
    }
    Ok(())
}

#[tauri::command]
pub fn pick_build_icon(
    app: AppHandle,
    state: State<'_, AppState>,
    slug: String,
) -> AppResult<Option<BuildSummary>> {
    let root = state.instances_dir();
    let paths = instances::paths(&root, &slug);
    let mut meta = instances::load(&paths.meta)?;

    let picked = app
        .dialog()
        .file()
        .set_title("Выберите иконку сборки")
        .add_filter("Изображения", ICON_EXTENSIONS)
        .blocking_pick_file();
    let Some(picked) = picked else { return Ok(None) };
    let source = picked.into_path().map_err(|e| {
        AppError::invalid("Не удалось получить путь к изображению").with_detail(e.to_string())
    })?;

    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .ok_or_else(|| AppError::invalid("Файл не является изображением"))?;
    if !ICON_EXTENSIONS.contains(&extension.as_str()) {
        return Err(AppError::invalid(format!("Неподдерживаемый формат: {extension}")));
    }
    let size = std::fs::metadata(&source)
        .map_err(|e| AppError::io("Не удалось прочитать изображение", &e))?
        .len();
    if size > 8 * 1024 * 1024 {
        return Err(AppError::invalid("Иконка слишком большая (максимум 8 МБ)"));
    }

    let icons_dir = state.build_icons_dir();
    std::fs::create_dir_all(&icons_dir)
        .map_err(|e| AppError::io("Не удалось создать папку иконок", &e))?;
    // One file per build, so the folder cannot accumulate stale images.
    if let Some(previous) = meta.icon.clone() {
        let _ = std::fs::remove_file(icons_dir.join(previous));
    }
    let file = format!("{slug}.{extension}");
    std::fs::copy(&source, icons_dir.join(&file))
        .map_err(|e| AppError::io("Не удалось скопировать иконку", &e))?;

    meta.icon = Some(file);
    meta.updated_at = crate::util::now_rfc3339();
    instances::save(&paths.meta, &meta)?;
    Ok(Some(summarise(&root, meta)))
}

#[tauri::command]
pub fn clear_build_icon(state: State<'_, AppState>, slug: String) -> AppResult<BuildSummary> {
    let root = state.instances_dir();
    let paths = instances::paths(&root, &slug);
    let mut meta = instances::load(&paths.meta)?;
    if let Some(previous) = meta.icon.take() {
        let _ = std::fs::remove_file(state.build_icons_dir().join(previous));
    }
    meta.updated_at = crate::util::now_rfc3339();
    instances::save(&paths.meta, &meta)?;
    Ok(summarise(&root, meta))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildPathsDto {
    pub root: String,
    pub saves: String,
    pub mods: String,
    pub config: String,
    pub resourcepacks: String,
    pub logs: String,
}

#[tauri::command]
pub fn build_paths(state: State<'_, AppState>, slug: String) -> AppResult<BuildPathsDto> {
    let paths = instances::paths(&state.instances_dir(), &slug);
    if !paths.root.is_dir() {
        return Err(AppError::not_found("Сборка не найдена"));
    }
    let join = |name: &str| paths.root.join(name).to_string_lossy().to_string();
    Ok(BuildPathsDto {
        root: paths.root.to_string_lossy().to_string(),
        saves: join("saves"),
        mods: join("mods"),
        config: join("config"),
        resourcepacks: join("resourcepacks"),
        logs: join("logs"),
    })
}

// ---------------------------------------------------------------------------
// Loaders
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn loader_catalog(
    state: State<'_, AppState>,
    game_version: String,
) -> AppResult<loaders::LoaderCatalog> {
    loaders::catalog(&state.json_http, &game_version).await
}

// ---------------------------------------------------------------------------
// Install / launch
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    pub slug: String,
    pub launch_id: String,
    pub libraries: usize,
    pub assets: u64,
    pub java_version: u32,
    pub java_path: String,
}

/// Release the "job running" flag even when a command fails.
struct JobGuard(Arc<AtomicBool>);

impl Drop for JobGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Install the build's loader, if it has one, and return the version id to run.
async fn resolve_launch_id(
    state: &AppState,
    meta: &mut InstanceMeta,
    paths: &InstancePaths,
) -> AppResult<String> {
    if let Some(id) = meta.launch_id.clone() {
        // A profile removed from disk by a cleanup must not be trusted.
        if state.versions_dir().join(&id).join(format!("{id}.json")).is_file() {
            return Ok(id);
        }
        crate::log_warn!("commands::builds", "профиль {id} пропал, ставлю загрузчик заново");
        meta.launch_id = None;
    }

    if meta.mods.loader == ModLoader::None {
        return Ok(meta.version_id.clone());
    }

    let game_version = meta.version_id.clone();
    let loader_version = meta.mods.loader_version.clone();
    if loader_version.is_empty() {
        return Err(AppError::invalid(
            "У сборки не указана версия загрузчика — создайте её заново",
        ));
    }

    // The installer has to run on *some* Java, and the vanilla metadata is the
    // only document that states the requirement.
    if meta.mods.loader != ModLoader::Fabric {
        let vanilla = super::launch::resolve(state, &game_version).await?;
        let java = super::launch::ensure_java(state, vanilla.java_major).await?;
        let installed = match meta.mods.loader {
            ModLoader::Forge => {
                loaders::install_forge(
                    &state.bulk_http,
                    &state.game_data,
                    &java.path,
                    &game_version,
                    &loader_version,
                    state.sink(),
                )
                .await?
            }
            _ => {
                loaders::install_neoforge(
                    &state.bulk_http,
                    &state.game_data,
                    &java.path,
                    &game_version,
                    &loader_version,
                    state.sink(),
                )
                .await?
            }
        };
        return record_loader(state, meta, paths, installed);
    }

    let installed =
        loaders::install_fabric(&state.json_http, &state.game_data, &game_version, &loader_version)
            .await?;
    record_loader(state, meta, paths, installed)
}

fn record_loader(
    _state: &AppState,
    meta: &mut InstanceMeta,
    paths: &InstancePaths,
    installed: loaders::InstalledLoader,
) -> AppResult<String> {
    meta.launch_id = Some(installed.version_id.clone());
    meta.updated_at = crate::util::now_rfc3339();
    instances::save(&paths.meta, meta)?;
    crate::log_info!(
        "commands::builds",
        "{} {} установлен как {}",
        installed.loader.label(),
        installed.version,
        installed.version_id
    );
    Ok(installed.version_id)
}

/// Install everything the build needs, then hand back what a spawn requires.
#[allow(clippy::type_complexity)]
async fn prepare_build(
    state: &AppState,
    slug: &str,
) -> AppResult<(InstanceMeta, InstancePaths, launcher::PrepareOutcome)> {
    let root = state.instances_dir();
    let paths = instances::paths(&root, slug);
    let mut meta = instances::load(&paths.meta)?;

    let launch_id = resolve_launch_id(state, &mut meta, &paths).await?;
    let versions = super::launch::ensure_manifest(state).await?;
    let chain = launcher::resolve_version_chain(
        &state.json_http,
        &versions,
        &state.game_data,
        &launch_id,
    )
    .await?;
    let resolved = ResolvedVersion::resolve(&launch_id, &chain)?;

    instances::set_state(&paths, InstanceState::Partial)?;
    // A build can be launched before anything else runs, so the runtime is
    // fetched here rather than assumed: `ensure_java` provisions Temurin when
    // the setting allows it, and fails with a plain message when it does not.
    let runtime = super::launch::ensure_java(state, resolved.java_major).await?;

    let cancel = Arc::new(AtomicBool::new(false));
    let outcome = launcher::prepare(
        &state.json_http,
        &state.bulk_http,
        &state.game_data,
        &paths,
        &resolved,
        Some(&runtime),
        cancel,
        state.sink(),
    )
    .await;

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(err) => {
            instances::set_state(&paths, InstanceState::Partial)?;
            return Err(err);
        }
    };

    instances::set_state(&paths, InstanceState::Ready)?;
    meta.state = InstanceState::Ready;
    Ok((meta, paths, outcome))
}

#[tauri::command]
pub async fn install_build(state: State<'_, AppState>, slug: String) -> AppResult<InstallResult> {
    if state.job_active.swap(true, Ordering::SeqCst) {
        return Err(AppError::invalid("Другая загрузка уже выполняется"));
    }
    let _guard = JobGuard(Arc::clone(&state.job_active));

    let (meta, _paths, outcome) = prepare_build(&state, &slug).await?;
    let launch_id = meta.launch_id.clone().unwrap_or_else(|| meta.version_id.clone());
    Ok(InstallResult {
        slug: meta.slug,
        launch_id,
        libraries: outcome.library_count,
        assets: outcome.asset_count,
        java_version: outcome.runtime.major,
        java_path: outcome.runtime.path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub async fn launch_build(
    state: State<'_, AppState>,
    slug: String,
) -> AppResult<launcher::GameSession> {
    if state.process.is_running() {
        return Err(AppError::invalid("Minecraft уже запущен"));
    }
    if state.job_active.swap(true, Ordering::SeqCst) {
        return Err(AppError::invalid("Другая загрузка уже выполняется"));
    }
    let _guard = JobGuard(Arc::clone(&state.job_active));

    let settings = state.settings();
    let user = launcher::offline_user(&settings.player_name)?;

    let (mut meta, paths, outcome) = prepare_build(&state, &slug).await?;

    let ram = meta.ram_mb.unwrap_or(settings.ram_mb).clamp(512, 32_768);
    let width = meta.width.unwrap_or(1280);
    let height = meta.height.unwrap_or(720);

    meta.last_played_at = Some(crate::util::now_rfc3339());
    instances::save(&paths.meta, &meta)?;

    let session = launcher::spawn(
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
            extra_game: &meta.game_args,
        },
        state.sink(),
    )
    .await?;

    Ok(session)
}

// ---------------------------------------------------------------------------
// Mods
// ---------------------------------------------------------------------------

/// A mod this build installed, recorded so the list survives a restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledMod {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub version_id: String,
    pub version_number: String,
    pub filename: String,
    /// `true` for a jar found in `mods/` that Sifora did not install.
    #[serde(default)]
    pub manual: bool,
    pub size_bytes: u64,
    pub installed_at: String,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ModsManifest {
    pub mods: Vec<InstalledMod>,
}

impl ModsManifest {
    fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) -> AppResult<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| {
            AppError::config("Не удалось сохранить список модов").with_detail(e.to_string())
        })?;
        crate::util::write_atomic(path, &bytes)
            .map_err(|e| AppError::io("Не удалось сохранить список модов", &e))
    }
}

/// The loader name Modrinth uses for a build's loader.
fn modrinth_loader(loader: ModLoader) -> Option<&'static str> {
    match loader {
        ModLoader::Fabric => Some("fabric"),
        ModLoader::Forge => Some("forge"),
        ModLoader::NeoForge => Some("neoforge"),
        ModLoader::Quilt => Some("quilt"),
        ModLoader::None | ModLoader::OptiFine => None,
    }
}

/// What a build's mod list looks like right now.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModList {
    pub slug: String,
    pub game_version: String,
    pub loader: ModLoader,
    pub loader_version: String,
    pub mods: Vec<InstalledMod>,
    /// Jars sitting in `mods/` that were not installed through Sifora.
    pub manual_count: usize,
    /// A build with no loader cannot load mods at all; the UI says so.
    pub can_load_mods: bool,
}

fn collect_mods(state: &AppState, slug: &str) -> AppResult<ModList> {
    let paths = instances::paths(&state.instances_dir(), slug);
    let meta = instances::load(&paths.meta)?;
    let manifest = ModsManifest::load(&instances::mods_manifest_path(&paths));

    // Anything in `mods/` the manifest does not know about is reported too:
    // pretending those jars are absent is a lie the game contradicts.
    let known: std::collections::HashSet<String> =
        manifest.mods.iter().map(|m| m.filename.clone()).collect();
    let mut mods = manifest.mods;
    let mut manual_count = 0usize;
    if let Ok(entries) = std::fs::read_dir(&paths.mods) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.to_ascii_lowercase().ends_with(".jar") || known.contains(&name) {
                continue;
            }
            manual_count += 1;
            mods.push(InstalledMod {
                project_id: String::new(),
                slug: String::new(),
                title: name.trim_end_matches(".jar").to_string(),
                version_id: String::new(),
                version_number: String::new(),
                size_bytes: entry.metadata().map(|m| m.len()).unwrap_or(0),
                filename: name,
                manual: true,
                installed_at: String::new(),
                icon_url: None,
            });
        }
    }
    mods.sort_by_key(|m| m.title.to_lowercase());

    Ok(ModList {
        can_load_mods: meta.mods.loader != ModLoader::None,
        slug: meta.slug,
        game_version: meta.version_id,
        loader: meta.mods.loader,
        loader_version: meta.mods.loader_version,
        mods,
        manual_count,
    })
}

#[tauri::command]
pub fn list_mods(state: State<'_, AppState>, slug: String) -> AppResult<ModList> {
    collect_mods(&state, &slug)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallModRequest {
    pub slug: String,
    pub project_id: String,
    /// A specific version, or `None` for the newest that fits the build.
    #[serde(default)]
    pub version_id: Option<String>,
}

/// Pick the version of a project that fits a build.
fn choose_version<'a>(
    versions: &'a [modrinth::ModVersion],
    wanted: Option<&str>,
) -> Option<&'a modrinth::ModVersion> {
    if let Some(wanted) = wanted {
        return versions.iter().find(|v| v.id == wanted);
    }
    versions.iter().find(|v| modrinth::primary_file(v).is_some())
}

#[tauri::command]
pub async fn install_mod(
    state: State<'_, AppState>,
    request: InstallModRequest,
) -> AppResult<InstalledMod> {
    if state.job_active.swap(true, Ordering::SeqCst) {
        return Err(AppError::invalid("Другая загрузка уже выполняется"));
    }
    let _guard = JobGuard(Arc::clone(&state.job_active));

    let paths = instances::paths(&state.instances_dir(), &request.slug);
    let meta = instances::load(&paths.meta)?;
    if meta.mods.loader == ModLoader::None {
        return Err(AppError::invalid(
            "Это Vanilla-сборка: моды требуют Fabric, Forge или NeoForge",
        ));
    }

    let details = modrinth::project(&state.json_http, &request.project_id).await?;
    let versions = modrinth::versions(
        &state.json_http,
        &request.project_id,
        Some(&meta.version_id),
        modrinth_loader(meta.mods.loader),
    )
    .await?;

    let chosen = choose_version(&versions, request.version_id.as_deref()).ok_or_else(|| {
        AppError::not_found(format!(
            "У мода «{}» нет версии для Minecraft {}",
            details.title, meta.version_id
        ))
    })?;

    let file = modrinth::primary_file(chosen)
        .ok_or_else(|| AppError::not_found("У выбранной версии мода нет файлов"))?;
    if !modrinth::download_url_allowed(&file.url) {
        return Err(AppError::invalid(
            "Мод предлагает скачивание с недопустимого адреса",
        ));
    }
    let filename = modrinth::safe_filename(&file.filename)
        .ok_or_else(|| AppError::invalid("Некорректное имя файла мода"))?;

    std::fs::create_dir_all(&paths.mods)
        .map_err(|e| AppError::io("Не удалось создать папку модов", &e))?;

    let task = crate::mc::download::FileTask::new(
        format!("mod:{}", details.slug),
        file.url.clone(),
        paths.mods.join(&filename),
    )
    .with_sha1(file.sha1.clone())
    .with_size(Some(file.size).filter(|s| *s > 0));

    crate::mc::download::ensure_file(
        &state.bulk_http,
        &task,
        &AtomicBool::new(false),
        &state.sink(),
    )
    .await?;

    let installed = InstalledMod {
        project_id: details.project_id.clone(),
        slug: details.slug.clone(),
        title: details.title.clone(),
        version_id: chosen.id.clone(),
        version_number: chosen.version_number.clone(),
        filename,
        manual: false,
        size_bytes: file.size,
        installed_at: crate::util::now_rfc3339(),
        icon_url: details.icon_url.clone(),
    };

    // Replacing a mod with a newer version must not leave the old jar behind —
    // the game would load both.
    let manifest_path = instances::mods_manifest_path(&paths);
    let mut manifest = ModsManifest::load(&manifest_path);
    let stale: Vec<String> = manifest
        .mods
        .iter()
        .filter(|m| m.project_id == installed.project_id && m.filename != installed.filename)
        .map(|m| m.filename.clone())
        .collect();
    for filename in stale {
        let _ = std::fs::remove_file(paths.mods.join(filename));
    }
    manifest.mods.retain(|m| m.project_id != installed.project_id);
    manifest.mods.push(installed.clone());
    manifest.save(&manifest_path)?;

    crate::log_info!(
        "commands::builds",
        "mod {} installed into {}",
        details.slug,
        meta.slug
    );
    Ok(installed)
}

#[tauri::command]
pub fn remove_mod(state: State<'_, AppState>, slug: String, filename: String) -> AppResult<ModList> {
    let paths = instances::paths(&state.instances_dir(), &slug);
    let safe = modrinth::safe_filename(&filename)
        .ok_or_else(|| AppError::invalid("Некорректное имя файла мода"))?;
    let target = paths.mods.join(&safe);
    if target.is_file() {
        std::fs::remove_file(&target).map_err(|e| AppError::io("Не удалось удалить мод", &e))?;
    }
    let manifest_path = instances::mods_manifest_path(&paths);
    let mut manifest = ModsManifest::load(&manifest_path);
    manifest.mods.retain(|m| m.filename != safe);
    manifest.save(&manifest_path)?;
    collect_mods(&state, &slug)
}

// ---------------------------------------------------------------------------
// Modrinth browsing
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn modrinth_tags(state: State<'_, AppState>) -> AppResult<modrinth::TagBundle> {
    modrinth::tags(&state.json_http).await
}

#[tauri::command]
pub async fn modrinth_search(
    state: State<'_, AppState>,
    query: modrinth::SearchQuery,
) -> AppResult<modrinth::SearchPage> {
    modrinth::search(&state.json_http, &query).await
}

#[tauri::command]
pub async fn modrinth_project(
    state: State<'_, AppState>,
    project_id: String,
) -> AppResult<modrinth::ModDetails> {
    modrinth::project(&state.json_http, &project_id).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectVersionsQuery {
    pub project_id: String,
    #[serde(default)]
    pub game_version: Option<String>,
    #[serde(default)]
    pub loader: Option<String>,
}

#[tauri::command]
pub async fn modrinth_versions(
    state: State<'_, AppState>,
    query: ProjectVersionsQuery,
) -> AppResult<Vec<modrinth::ModVersion>> {
    modrinth::versions(
        &state.json_http,
        &query.project_id,
        query.game_version.as_deref(),
        query.loader.as_deref(),
    )
    .await
}

/// Version and loader availability for a build, so the mod browser can filter
/// to what the build can actually run.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildModContext {
    pub slug: String,
    pub name: String,
    pub game_version: String,
    pub loader: ModLoader,
    pub loader_version: String,
    /// The Modrinth loader name, when the build has one.
    pub modrinth_loader: Option<String>,
}

#[tauri::command]
pub fn build_mod_context(state: State<'_, AppState>, slug: String) -> AppResult<BuildModContext> {
    let meta = instances::load(&instances::paths(&state.instances_dir(), &slug).meta)?;
    Ok(BuildModContext {
        modrinth_loader: modrinth_loader(meta.mods.loader).map(|s| s.to_string()),
        slug: meta.slug,
        name: meta.name,
        game_version: meta.version_id,
        loader: meta.mods.loader,
        loader_version: meta.mods.loader_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(id: &str, with_file: bool) -> modrinth::ModVersion {
        modrinth::ModVersion {
            id: id.to_string(),
            name: String::new(),
            version_number: id.to_string(),
            game_versions: vec![],
            loaders: vec![],
            version_type: "release".into(),
            downloads: 0,
            published: String::new(),
            files: if with_file {
                vec![modrinth::ModFile {
                    url: "https://cdn.modrinth.com/x.jar".into(),
                    filename: "x.jar".into(),
                    size: 1,
                    sha1: None,
                    primary: true,
                }]
            } else {
                vec![]
            },
            dependencies: vec![],
        }
    }

    #[test]
    fn the_newest_version_with_a_file_is_chosen() {
        let versions = vec![version("new", false), version("older", true)];
        assert_eq!(choose_version(&versions, None).map(|v| v.id.as_str()), Some("older"));
    }

    #[test]
    fn an_explicit_version_wins_even_without_a_file() {
        let versions = vec![version("new", true), version("old", false)];
        assert_eq!(
            choose_version(&versions, Some("old")).map(|v| v.id.as_str()),
            Some("old")
        );
        assert!(choose_version(&versions, Some("missing")).is_none());
    }

    #[test]
    fn loader_names_match_modrinth() {
        assert_eq!(modrinth_loader(ModLoader::Fabric), Some("fabric"));
        assert_eq!(modrinth_loader(ModLoader::Forge), Some("forge"));
        assert_eq!(modrinth_loader(ModLoader::NeoForge), Some("neoforge"));
        assert_eq!(modrinth_loader(ModLoader::None), None);
    }

    #[test]
    fn the_mods_manifest_round_trips() {
        let dir = std::env::temp_dir().join("sifora-mods-manifest");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mods.json");
        let manifest = ModsManifest {
            mods: vec![InstalledMod {
                project_id: "AANobbMI".into(),
                slug: "sodium".into(),
                title: "Sodium".into(),
                version_id: "v1".into(),
                version_number: "0.5.13".into(),
                filename: "sodium.jar".into(),
                manual: false,
                size_bytes: 10,
                installed_at: "2026-01-01T00:00:00Z".into(),
                icon_url: None,
            }],
        };
        manifest.save(&path).unwrap();
        let reloaded = ModsManifest::load(&path);
        assert_eq!(reloaded.mods.len(), 1);
        assert_eq!(reloaded.mods[0].slug, "sodium");
        assert_eq!(reloaded.mods[0].filename, "sodium.jar");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_manifest_degrades_to_empty() {
        let dir = std::env::temp_dir().join("sifora-mods-manifest-bad");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mods.json");
        std::fs::write(&path, b"{ not json").unwrap();
        assert!(ModsManifest::load(&path).mods.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
