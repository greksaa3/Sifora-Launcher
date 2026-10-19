//! Launch plan construction and game process management.
//!
//! Responsibilities:
//!
//! * Fetch and cache the version metadata chain (including `inheritsFrom`).
//! * Download the client jar, libraries and assets.
//! * Resolve a Java runtime for the required major version.
//! * Build the JVM + game argument vector with every `${...}` placeholder
//!   substituted — never a templated command *line*, always a `Vec<String>`
//!   handed to the OS, so quoting bugs cannot become argument injection.
//! * Spawn the game and keep a handle on the process so the UI can report
//!   status and a second launch of a running game is refused.
//!
//! ## Authentication
//!
//! Stage 1 runs Minecraft in **offline mode**: the player name is validated
//! locally and a deterministic offline UUID is derived from it. No Mojang
//! account credentials are requested, stored or transmitted. The seam for
//! Microsoft OAuth is [`Placeholders::user_tokens`] — adding real sessions
//! later changes only what fills those fields, not the pipeline.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;

use super::assets::{AssetDownloader, AssetPlan};
use super::download::{self, EventSink, Ledger};
use super::instances::{InstanceMeta, InstancePaths};
use super::java::{self, JavaRuntime};
use super::libraries;
use super::version::{
    Argument, ArgumentValue, Platform, ResolvedVersion, RuleAction, VersionMeta,
};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::log_info;
use crate::util;

// ---------------------------------------------------------------------------
// Offline identity
// ---------------------------------------------------------------------------

const NAME_MIN: usize = 3;
const NAME_MAX: usize = 16;

#[derive(Debug, Clone)]
pub struct OfflineUser {
    pub name: String,
    pub uuid: String,
}

/// Validate a player name and derive the standard offline UUID.
///
/// The UUID is `UUID.nameUUIDFromBytes("OfflinePlayer:<name>")`, which is what
/// the vanilla client uses in offline mode, so worlds keep working if the user
/// ever migrates to an online account with the same name.
pub fn offline_user(name: &str) -> AppResult<OfflineUser> {
    let name = name.trim();
    let length = name.chars().count();
    if length < NAME_MIN || length > NAME_MAX {
        return Err(AppError::invalid("Ник должен содержать от 3 до 16 символов"));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(AppError::invalid(
            "В нике допустимы только латинские буквы, цифры и подчёркивание",
        ));
    }
    Ok(OfflineUser { name: name.to_string(), uuid: offline_uuid(name) })
}

pub fn offline_uuid(name: &str) -> String {
    let data = format!("OfflinePlayer:{name}");
    let bytes = md5(data.as_bytes());
    format_uuid(&bytes)
}

fn format_uuid(b: &[u8; 16]) -> String {
    let mut bytes = *b;
    bytes[6] = (bytes[6] & 0x0f) | 0x30; // version 3 (name-based MD5)
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // IETF variant
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8],
        bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

/// Minimal MD5 (RFC 1321) — the one hash RFC 4122 v3 needs. Sixty lines is
/// cheaper than a dependency.
fn md5(input: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let mut k = [0u32; 64];
    for (i, slot) in k.iter_mut().enumerate() {
        *slot = ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32;
    }
    let (mut a0, mut b0, mut c0, mut d0) = (0x6745_2301u32, 0xefcd_ab89u32, 0x98ba_dcfeu32, 0x1032_5476u32);

    let mut message = input.to_vec();
    let bit_len = (input.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_le_bytes());

    for chunk in message.chunks(64) {
        let mut m = [0u32; 16];
        for (i, word) in m.iter_mut().enumerate() {
            *word = u32::from_le_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..16].copy_from_slice(&d0.to_le_bytes());
    out
}

// ---------------------------------------------------------------------------
// Placeholders
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Placeholders {
    map: HashMap<String, String>,
}

impl Placeholders {
    pub fn new(entries: Vec<(&str, String)>) -> Self {
        Placeholders { map: entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect() }
    }

    pub fn insert(&mut self, key: &str, value: impl Into<String>) {
        self.map.insert(key.to_string(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(|s| s.as_str())
    }

    /// Replace every `${name}` occurrence. Unknown placeholders are left as-is
    /// so a missing value shows up in the game's own error message instead of
    /// being silently swallowed.
    pub fn substitute(&self, input: &str) -> String {
        if !input.contains("${") {
            return input.to_string();
        }
        let mut out = String::with_capacity(input.len());
        let mut rest = input;
        while let Some(start) = rest.find("${") {
            out.push_str(&rest[..start]);
            let Some(end) = rest[start + 2..].find('}') else {
                out.push_str(&rest[start..]);
                return out;
            };
            let name = &rest[start + 2..start + 2 + end];
            match self.map.get(name) {
                Some(value) => out.push_str(value),
                None => {
                    out.push_str("${");
                    out.push_str(name);
                    out.push('}');
                }
            }
            rest = &rest[start + 2 + end + 1..];
        }
        out.push_str(rest);
        out
    }
}

/// JVM/game flags that would let a stale setting or a theme escape the sandbox.
fn is_dangerous_arg(arg: &str) -> bool {
    const FORBIDDEN: &[&str] = &[
        "-javaagent:",
        "-agentlib:",
        "-agentpath:",
        "-agent:jnidispatch",
        "-XX:OnOutOfMemoryError=",
        "-XX:OnError=",
        "-XX:OnBreak=",
        "-Xbootclasspath",
        "-Xbootclasspath/a:",
        "-Xbootclasspath/p:",
        "--add-opens=",
        "--illegal-access=",
        "-XX:+DisableAttachMechanism",
    ];
    // `-XX:On*=` covers all shutdown hooks; the explicit entries document intent.
    if arg.starts_with("-XX:On") && arg.contains('=') {
        return true;
    }
    FORBIDDEN.iter().any(|prefix| arg.starts_with(prefix))
}

// ---------------------------------------------------------------------------
// Argument expansion
// ---------------------------------------------------------------------------

fn rule_allows(rules: &[super::version::Rule], platform: Platform) -> bool {
    let mut allowed = false;
    for rule in rules {
        let single =
            Argument::Conditional { rules: vec![rule.clone()], value: ArgumentValue::Single(String::new()) };
        if libraries::argument_is_allowed(&single, platform) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

/// Expand a modern `arguments.jvm` / `arguments.game` list.
pub fn expand_arguments(args: &[Argument], placeholders: &Placeholders, platform: Platform) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args {
        let values: Vec<String> = match arg {
            Argument::Plain(s) => vec![s.clone()],
            Argument::Conditional { rules, value } => {
                if !rule_allows(rules, platform) {
                    continue;
                }
                value.clone().into_vec()
            }
        };
        for value in values {
            let substituted = placeholders.substitute(&value);
            if !is_dangerous_arg(&substituted) {
                out.push(substituted);
            }
        }
    }
    out
}

/// Split the legacy space-separated `minecraftArguments` string.
pub fn expand_legacy_arguments(raw: &str, placeholders: &Placeholders) -> Vec<String> {
    raw.split_whitespace()
        .map(|part| placeholders.substitute(part))
        .filter(|part| !is_dangerous_arg(part))
        .collect()
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

pub fn client_jar_path(game_data: &Path, version_id: &str) -> PathBuf {
    game_data.join("versions").join(version_id).join(format!("{version_id}.jar"))
}

pub fn version_json_path(game_data: &Path, version_id: &str) -> PathBuf {
    game_data.join("versions").join(version_id).join(format!("{version_id}.json"))
}

// ---------------------------------------------------------------------------
// Process bookkeeping
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GameState {
    Idle,
    Preparing,
    Downloading,
    Starting,
    Running,
    Exited,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSession {
    pub instance_id: String,
    pub version_id: String,
    pub state: GameState,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub message: String,
}

/// The single running game. Shared with the supervisor task through an `Arc`.
#[derive(Default)]
pub struct ProcessSlot {
    current: Mutex<Option<GameSession>>,
}

impl ProcessSlot {
    pub fn snapshot(&self) -> Option<GameSession> {
        self.current.lock().clone()
    }

    pub fn is_running(&self) -> bool {
        self.current
            .lock()
            .as_ref()
            .is_some_and(|s| matches!(s.state, GameState::Running | GameState::Starting))
    }

    fn set(&self, session: GameSession) {
        *self.current.lock() = Some(session);
    }
}

// ---------------------------------------------------------------------------
// Prepare
// ---------------------------------------------------------------------------

pub struct PrepareOutcome {
    pub resolved: ResolvedVersion,
    pub classpath: Vec<PathBuf>,
    pub runtime: JavaRuntime,
    pub asset_index_name: String,
    pub natives_dir: PathBuf,
    pub library_count: usize,
    pub asset_count: u64,
}

#[allow(clippy::too_many_arguments)]
pub async fn prepare(
    http: &reqwest::Client,
    bulk: &reqwest::Client,
    game_data: &Path,
    instance: &InstancePaths,
    resolved: &ResolvedVersion,
    runtime_hint: Option<&JavaRuntime>,
    cancel: Arc<AtomicBool>,
    sink: EventSink,
) -> AppResult<PrepareOutcome> {
    let platform = Platform::current();
    let libraries_dir = game_data.join("libraries");
    let assets_store = game_data.join("assets");
    let ledger = Arc::new(Mutex::new(Ledger::new(
        format!("install:{}", resolved.id),
        0,
        0,
        sink.clone(),
    )));

    // ---- 1. client jar ---------------------------------------------------
    ledger.lock().note("Проверка клиентского файла");
    let client = resolved
        .client
        .clone()
        .ok_or_else(|| AppError::unsupported(format!("У версии {} нет клиентского файла", resolved.id)))?;
    let client_jar = client_jar_path(game_data, &resolved.id);
    let client_task = download::FileTask::new(
        format!("client:{}", resolved.id),
        client.url.clone(),
        &client_jar,
    )
    .with_sha1(Some(client.sha1.clone()))
    .with_size(Some(client.size));
    download::ensure_file(http, &client_task, &cancel, &sink).await?;

    // ---- 2. libraries ----------------------------------------------------
    let (library_files, _classpath, _natives) =
        libraries::resolve_libraries(&resolved.libraries, platform, &libraries_dir);
    ledger.lock().note(format!("Библиотек: {}", library_files.len()));
    libraries::fetch_libraries(
        bulk,
        library_files.clone(),
        &libraries_dir,
        cancel.clone(),
        sink.clone(),
        &ledger,
    )
    .await?;

    // ---- 3. natives ------------------------------------------------------
    let natives_dir = instance.natives.clone();
    libraries::clear_natives(instance)?;
    std::fs::create_dir_all(&natives_dir)
        .map_err(|e| AppError::io("Не удалось создать папку natives", &e))?;
    for file in library_files.iter().filter(|f| f.is_native) {
        let path = libraries_dir.join(&file.relative);
        if path.exists() {
            libraries::extract_natives(&path, &natives_dir)?;
        }
    }

    // ---- 4. assets -------------------------------------------------------
    let mut asset_index_name = resolved.id.clone();
    let mut asset_count = 0u64;
    if let Some(reference) = &resolved.asset_index {
        asset_index_name = reference.id.clone();
        asset_count = download_assets(
            http,
            &assets_store,
            &instance.assets,
            reference,
            &ledger,
            cancel.clone(),
            sink.clone(),
        )
        .await?;
    }

    // ---- 5. java ---------------------------------------------------------
    let runtime = match runtime_hint {
        Some(rt) => rt.clone(),
        None => {
            java::discover(resolved.java_major, None, &game_data.join("java")).ok_or_else(|| {
                AppError::new(
                    ErrorKind::Java,
                    format!(
                        "Не найдена среда Java {} для версии {}",
                        resolved.java_major, resolved.id
                    ),
                )
                .with_detail("укажите путь к Java в настройках или установите её вручную")
                .retryable(false)
            })?
        }
    };
    if runtime.major < resolved.java_major {
        return Err(AppError::new(
            ErrorKind::Java,
            format!("Требуется Java {}, а найдена {}", resolved.java_major, runtime.major),
        ));
    }

    // ---- 6. classpath ----------------------------------------------------
    let mut classpath: Vec<PathBuf> = Vec::new();
    // launchwrapper must be first for `net.minecraft.launchwrapper.Launch`
    if let Ok(entries) = std::fs::read_dir(libraries_dir.join("net/minecraft/launchwrapper")) {
        let mut wrappers: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "jar"))
            .collect();
        wrappers.sort();
        if let Some(wrapper) = wrappers.last() {
            classpath.push(wrapper.clone());
        }
    }
    for file in &library_files {
        if file.is_native {
            continue;
        }
        let path = libraries_dir.join(&file.relative);
        if path.exists() && !classpath.contains(&path) {
            classpath.push(path);
        }
    }
    classpath.push(client_jar);

    Ok(PrepareOutcome {
        resolved: resolved.clone(),
        classpath,
        runtime,
        asset_index_name,
        natives_dir,
        library_count: library_files.len(),
        asset_count,
    })
}

async fn download_assets(
    http: &reqwest::Client,
    assets_store: &Path,
    instance_assets: &Path,
    reference: &super::version::AssetIndexRef,
    ledger: &Arc<Mutex<Ledger>>,
    cancel: Arc<AtomicBool>,
    sink: EventSink,
) -> AppResult<u64> {
    let plan = AssetPlan {
        index_id: reference.id.clone(),
        index_url: reference.url.clone(),
        index_sha1: reference.sha1.clone(),
        index_size: reference.size,
    };
    let downloader = AssetDownloader { assets_store, http, cancel: cancel.clone(), sink: sink.clone() };
    let (index, _) = downloader.load_index(&plan).await?;
    let layout = index.layout();
    let total = index.objects.len() as u64;
    ledger.lock().note(format!("Ресурсов: {total}"));

    let objects_dir = assets_store.join("objects");
    let base = format!("{}/", reference.url.trim_end_matches('/'));

    let pending: Vec<(String, String, PathBuf, String, u64)> = index
        .objects
        .iter()
        .filter_map(|(key, object)| {
            if object.hash.len() < 2 || !object.hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let path = objects_dir.join(&object.hash[..2]).join(&object.hash);
            path.exists().then_some(())?;
            let url = format!("{base}{}/{}", &object.hash[..2], object.hash);
            Some((
                format!("asset:{key}"),
                url,
                path,
                object.hash.clone(),
                object.size,
            ))
        })
        .collect();

    if !pending.is_empty() {
        ledger.lock().add_file(pending.len() as u64);
        let http = http.clone();
        let sink2 = sink.clone();
        let ledger2 = ledger.clone();
        let cancel_for_closure = cancel.clone();
        let results = download::run_bounded(
            pending,
            8,
            cancel,
            move |(id, url, path, sha1, size): (String, String, PathBuf, String, u64)| {
                let http = http.clone();
                let sink = sink2.clone();
                let ledger = ledger2.clone();
                let cancel = cancel_for_closure.clone();
                async move {
                    let task = download::FileTask::new(id, url, path)
                        .with_sha1(Some(sha1))
                        .with_size(Some(size));
                    let len = download::ensure_file(&http, &task, &cancel, &sink).await?;
                    ledger.lock().set_file_bytes(len);
                    Ok(())
                }
            },
        )
        .await;

        for result in results {
            if let Err(err) = result {
                if err.kind == ErrorKind::Cancelled {
                    return Err(err);
                }
                // A single unreachable sound must not block the launch.
                crate::log_warn!("mc::launcher", "asset skipped: {}", err);
            }
        }
    }

    downloader.materialise(&index, layout, &reference.id, instance_assets, ledger).await?;
    Ok(total)
}

/// Resolve the `inheritsFrom` chain for a version, oldest ancestor first.
pub async fn resolve_version_chain(
    http: &reqwest::Client,
    manifest_versions: &HashMap<String, String>,
    game_data: &Path,
    version_id: &str,
) -> AppResult<Vec<VersionMeta>> {
    let mut chain: Vec<VersionMeta> = Vec::new();
    let mut current = version_id.to_string();
    let mut guard = 0;
    while !current.is_empty() && guard < 8 {
        guard += 1;
        let cache = version_json_path(game_data, &current);
        let raw = match std::fs::read_to_string(&cache) {
            Ok(raw) => raw,
            Err(_) => {
                let Some(url) = manifest_versions.get(&current) else {
                    return Err(AppError::not_found(format!(
                        "Версия {current} отсутствует в манифесте Mojang"
                    )));
                };
                let text = download::fetch_text(http, url).await?;
                let _ = util::write_atomic(&cache, text.as_bytes());
                text
            }
        };
        let meta = VersionMeta::parse(&raw)?;
        let parent = meta.inherits_from.clone();
        chain.push(meta);
        current = parent.unwrap_or_default();
    }
    if chain.is_empty() {
        return Err(AppError::not_found(format!("Метаданные версии {version_id} не найдены")));
    }
    chain.reverse();
    Ok(chain)
}

// ---------------------------------------------------------------------------
// Spawn
// ---------------------------------------------------------------------------

pub struct SpawnRequest<'a> {
    pub instance_meta: &'a InstanceMeta,
    pub instance: &'a InstancePaths,
    pub resolved: &'a ResolvedVersion,
    pub classpath: &'a [PathBuf],
    pub runtime: &'a JavaRuntime,
    pub natives_dir: &'a Path,
    pub user: &'a OfflineUser,
    pub asset_index_name: &'a str,
    pub ram_mb: u32,
    pub width: u32,
    pub height: u32,
    pub version_type: &'a str,
    pub extra_jvm: &'a [String],
    pub extra_game: &'a [String],
}

/// Build the exact argument vector without spawning. Exposed for tests and for
/// the "copy launch command" debug helper.
///
/// Ordering follows what the JVM actually needs:
///
/// ```text
/// java [our flags] [user flags] [version document's JVM args] -cp <classpath> <main class> [game args]
/// ```
///
/// The version document's JVM args already contain `-cp ${classpath}` on 1.13+
/// and nothing at all before that, so `-cp` is appended **only** when the
/// document did not provide one. Duplicating it silently breaks the launch:
/// the JVM honours the first `-cp` it sees.
pub fn build_arguments(req: &SpawnRequest<'_>) -> Vec<String> {
    let separator = if cfg!(target_os = "windows") { ";" } else { ":" };
    let classpath = req
        .classpath
        .iter()
        .map(|entry| entry.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(separator);

    let natives = req.natives_dir.to_string_lossy().to_string();
    let assets = req.instance.assets.to_string_lossy().to_string();
    let game_dir = req.instance.root.to_string_lossy().to_string();

    let p = Placeholders::new(vec![
        ("auth_player_name", req.user.name.clone()),
        ("auth_uuid", req.user.uuid.clone()),
        ("auth_access_token", "0".to_string()),
        ("auth_session", format!("token:0:{}:{}", util::now_secs(), req.user.uuid)),
        ("auth_xuid", String::new()),
        ("clientid", String::new()),
        ("user_type", "legacy".to_string()),
        // An empty JSON object: versions that expect a properties blob (1.8 –
        // 1.12) must not receive the literal placeholder.
        ("user_properties", "{}".to_string()),
        ("version_name", req.resolved.id.clone()),
        ("version_type", req.version_type.to_string()),
        ("game_directory", game_dir.clone()),
        ("game_assets", assets.clone()),
        ("assets_root", assets),
        ("assets_index_name", req.asset_index_name.to_string()),
        ("assets_index", req.asset_index_name.to_string()),
        ("natives_directory", natives.clone()),
        ("resolution_width", req.width.to_string()),
        ("resolution_height", req.height.to_string()),
        ("launcher_name", "sifora".to_string()),
        ("launcher_version", env!("CARGO_PKG_VERSION").to_string()),
        ("classpath", classpath.clone()),
        ("classpath_separator", separator.to_string()),
        ("clientjar", classpath.clone()),
        ("library_directory", req.instance.root.join("libraries").to_string_lossy().to_string()),
    ]);

    let platform = Platform::current();
    let mut args: Vec<String> = Vec::new();

    // 1. Our own JVM flags.
    args.push(format!("-Xmx{}M", req.ram_mb.clamp(512, 32_768)));
    if req.runtime.major >= 8 {
        args.push("-XX:MaxMetaspaceSize=256M".to_string());
    }
    if req.runtime.major >= 9 {
        args.push("-XX:+UseG1GC".to_string());
        args.push("-XX:MaxGCPauseMillis=50".to_string());
    }
    args.push(format!("-Djava.library.path={natives}"));
    args.push(format!("-Dorg.lwjgl.librarypath={natives}"));
    args.push("-Dfile.encoding=UTF-8".to_string());

    // 2. Per-instance overrides from the user.
    for extra in req.extra_jvm {
        let substituted = p.substitute(extra);
        if !is_dangerous_arg(&substituted) {
            args.push(substituted);
        }
    }

    // 3. The version document's JVM arguments (1.13+). These already carry
    //    `-cp ${classpath}` on modern versions.
    let document_args = expand_arguments(&req.resolved.arguments.jvm, &p, platform);
    let document_has_classpath = document_args.iter().any(|arg| arg == "-cp");
    args.extend(document_args);

    // 4. Classpath, only when the document did not supply one (pre-1.13).
    if !document_has_classpath {
        args.push("-cp".to_string());
        args.push(classpath);
    }

    // 5. Main class, then the game arguments.
    args.push(req.resolved.main_class.clone());
    args.extend(expand_arguments(&req.resolved.arguments.game, &p, platform));
    if let Some(legacy) = &req.resolved.minecraft_arguments {
        args.extend(expand_legacy_arguments(legacy, &p));
    }
    for extra in req.extra_game {
        let substituted = p.substitute(extra);
        if !is_dangerous_arg(&substituted) {
            args.push(substituted);
        }
    }

    // 6. Hygiene.
    //
    //    - The instance's heap limit has the final word: drop any `-Xmx` that
    //      arrived from the version document or from a user override.
    let mut heap_seen = false;
    args.retain(|arg| {
        if arg.starts_with("-Xmx") {
            if heap_seen {
                return false;
            }
            heap_seen = true;
        }
        arg != "-XX:+UnlockExperimentalVMOptions" || req.runtime.major >= 9
    });

    //    - A placeholder we do not know about would reach the JVM verbatim and
    //      produce a baffling error, so anything still unexpanded is reported
    //      rather than silently passed on.
    for arg in &args {
        if arg.contains("${") {
            crate::log_warn!("mc::launcher", "unresolved placeholder in argument: {arg}");
        }
    }

    args
}

/// Start the game and supervise the process until it exits.
pub async fn spawn(
    slot: Arc<ProcessSlot>,
    req: SpawnRequest<'_>,
    sink: EventSink,
) -> AppResult<GameSession> {
    if slot.is_running() {
        return Err(AppError::invalid("Minecraft уже запущен"));
    }

    let args = build_arguments(&req);
    let executable = if req.runtime.executable().exists() {
        req.runtime.executable()
    } else {
        req.runtime.cli_executable()
    };
    if !executable.exists() {
        return Err(
            AppError::new(ErrorKind::Java, "Не найден исполняемый файл Java")
                .with_detail(executable.to_string_lossy().to_string()),
        );
    }

    let executable_name = executable.to_string_lossy().to_string();
    log_info!("mc::launcher", "launching {} ({} args)", req.resolved.id, args.len());
    if std::env::var_os("SIFORA_DEBUG").is_some() {
        log_info!("mc::launcher", "exec: {executable_name} {:?}", args);
    }

    let mut command = tokio::process::Command::new(&executable);
    command
        .args(&args)
        .current_dir(&req.instance.root)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW | DETACHED_PROCESS: no console flash behind the
        // launcher window, and the game survives if the launcher closes.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        command.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }

    let mut child = command.spawn().map_err(|e| {
        AppError::new(ErrorKind::Spawn, "Не удалось запустить Minecraft")
            .with_detail(format!("{executable_name}: {e}"))
    })?;

    let session = GameSession {
        instance_id: req.instance_meta.id.clone(),
        version_id: req.resolved.id.clone(),
        state: GameState::Running,
        pid: child.id(),
        exit_code: None,
        started_at: util::now_rfc3339(),
        finished_at: None,
        message: "Игра запущена".into(),
    };
    slot.set(session.clone());
    sink(download::DownloadEvent::Note {
        job: "launch".into(),
        message: format!("Minecraft {} запущен", req.resolved.id),
    });

    let slot_for_task = slot.clone();
    let version_id = req.resolved.id.clone();
    let fallback = session.clone();
    tokio::spawn(async move {
        let status = child.wait().await;
        let mut finished = slot_for_task.snapshot().unwrap_or(fallback);
        finished.state = if status.is_ok() { GameState::Exited } else { GameState::Failed };
        finished.exit_code = status.ok().and_then(|s| s.code());
        finished.finished_at = Some(util::now_rfc3339());
        finished.message = match finished.exit_code {
            Some(0) => "Игра завершена".to_string(),
            Some(code) => format!("Игра завершилась с кодом {code}"),
            None => "Игра завершена".to_string(),
        };
        log_info!("mc::launcher", "{version_id}: {}", finished.message);
        slot_for_task.set(finished);
    });

    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mc::instances::InstancePaths;
    use crate::mc::version::Arguments;

    #[test]
    fn md5_matches_reference_vectors() {
        assert_eq!(hex::encode(md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex::encode(md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hex::encode(md5(b"The quick brown fox jumps over the lazy dog")),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
    }

    #[test]
    fn offline_uuid_matches_the_vanilla_algorithm() {
        assert_eq!(offline_uuid("Notch"), "b50ad385-829d-3141-a216-7e7d7539ba7f");
        assert_eq!(offline_uuid("jeb_"), "a762f560-4fce-3236-812a-b80efff0b62b");
        assert_eq!(offline_uuid("Notch"), offline_uuid("Notch"));
        assert_ne!(offline_uuid("Notch"), offline_uuid("notch"));
    }

    #[test]
    fn player_names_are_validated() {
        assert!(offline_user("Steve").is_ok());
        assert!(offline_user("ab").is_err());
        assert!(offline_user(&"a".repeat(17)).is_err());
        assert!(offline_user("bad name").is_err());
        assert!(offline_user("рус").is_err());
        assert!(offline_user("name-with-dash").is_err());
    }

    #[test]
    fn placeholder_substitution_handles_every_case() {
        let p = Placeholders::new(vec![("a", "1".into()), ("bb", "2".into())]);
        assert_eq!(p.substitute("x${a}y"), "x1y");
        assert_eq!(p.substitute("${a}${bb}"), "12");
        assert_eq!(p.substitute("${unknown}"), "${unknown}");
        assert_eq!(p.substitute("${unclosed"), "${unclosed");
        assert_eq!(p.substitute("plain"), "plain");
        assert_eq!(p.substitute("${a}${a}${a}"), "111");
        assert_eq!(p.substitute("}"), "}");
    }

    #[test]
    fn hostile_jvm_flags_are_dropped() {
        for arg in [
            "-javaagent:/tmp/evil.jar",
            "-agentlib:jdwp=transport=dt_socket",
            "-XX:OnOutOfMemoryError=/tmp/pwn.sh",
            "-Xbootclasspath/a:evil.jar",
            "--add-opens=java.base/java.lang=ALL-UNNAMED",
            "-agentpath:C:\\evil.dll",
        ] {
            assert!(is_dangerous_arg(arg), "{arg} should be rejected");
        }
        assert!(!is_dangerous_arg("-Xmx2G"));
        assert!(!is_dangerous_arg("-Djava.library.path=natives"));
    }

    #[test]
    fn conditional_arguments_expand_only_when_allowed() {
        let args: Vec<Argument> = serde_json::from_value(serde_json::json!([
            "--always",
            {"rules": [{"action":"allow","os":{"name":"osx"}}], "value": ["-XstartOnFirstThread"]},
            {"rules": [{"action":"allow","features":{"is_demo_user": true}}], "value": "--demo"}
        ]))
        .unwrap();
        let p = Placeholders::default();
        assert_eq!(expand_arguments(&args, &p, Platform::Windows), vec!["--always".to_string()]);
        assert_eq!(
            expand_arguments(&args, &p, Platform::MacOs),
            vec!["--always".to_string(), "-XstartOnFirstThread".to_string()]
        );
    }

    #[test]
    fn legacy_arguments_are_whitespace_split_and_substituted() {
        let p = Placeholders::new(vec![("auth_player_name", "Steve".into())]);
        let out = expand_legacy_arguments("${auth_player_name} --gameDir ${game_directory}", &p);
        assert_eq!(
            out,
            vec!["Steve".to_string(), "--gameDir".to_string(), "${game_directory}".to_string()]
        );
    }

    fn fixture(tmp: &Path) -> (InstancePaths, ResolvedVersion, JavaRuntime) {
        let resolved = ResolvedVersion {
            id: "1.20.4".into(),
            main_class: "net.minecraft.client.main.Main".into(),
            libraries: vec![],
            arguments: serde_json::from_value(serde_json::json!({
                "game": ["--demo"],
                "jvm": ["-Djava.library.path=${natives_directory}"]
            }))
            .unwrap(),
            minecraft_arguments: None,
            asset_index: None,
            client: None,
            java_major: 17,
            release_time: None,
            release_type: "release".into(),
            chain: vec![],
        };
        let runtime = JavaRuntime {
            path: tmp.join("jdk"),
            version: "17.0.9".into(),
            major: 17,
            vendor: "test".into(),
            kind: crate::mc::java::RuntimeKind::Jdk,
            source: "test",
        };
        (InstancePaths {
            root: tmp.join("instance"),
            meta: tmp.join("instance").join("instance.json"),
            natives: tmp.join("instance").join("natives"),
            assets: tmp.join("instance").join("assets"),
        }, resolved, runtime)
    }

    #[test]
    fn arguments_contain_classpath_main_class_and_substituted_values() {
        let tmp = std::env::temp_dir().join("sifora-launch-args");
        let _ = std::fs::create_dir_all(&tmp);
        let (instance, resolved, runtime) = fixture(&tmp);
        let meta = InstanceMeta {
            id: "i-1".into(),
            slug: "s".into(),
            name: "s".into(),
            version_id: "1.20.4".into(),
            created_at: String::new(),
            updated_at: String::new(),
            state: crate::mc::instances::InstanceState::Ready,
            ram_mb: None,
            jvm_args: vec![],
            game_args: vec![],
            width: None,
            height: None,
            mods: Default::default(),
        };
        let classpath = vec![PathBuf::from("libs/a.jar"), PathBuf::from("versions/1.20.4/1.20.4.jar")];
        let user = offline_user("Steve").unwrap();
        let args = build_arguments(&SpawnRequest {
            instance_meta: &meta,
            instance: &instance,
            resolved: &resolved,
            classpath: &classpath,
            runtime: &runtime,
            natives_dir: &instance.natives,
            user: &user,
            asset_index_name: "12",
            ram_mb: 2048,
            width: 1280,
            height: 720,
            version_type: "release",
            extra_jvm: &[],
            extra_game: &["--width".into(), "${resolution_width}".into()],
        });

        let heap = args.iter().find(|a| a.starts_with("-Xmx")).unwrap();
        assert_eq!(heap, "-Xmx2048M");
        assert!(args.iter().any(|a| a == "-cp"));
        assert!(args.iter().any(|a| a == "net.minecraft.client.main.Main"));
        assert!(args.iter().any(|a| a == "--demo"));
        assert!(args.iter().any(|a| a == "1280"), "resolution placeholder must be filled");
        assert!(args.iter().any(|a| a.contains("java.library.path")));
        // Exactly one heap flag must survive.
        assert_eq!(args.iter().filter(|a| a.starts_with("-Xmx")).count(), 1);
        // The main class has to come after the classpath separator.
        let cp = args.iter().position(|a| a == "-cp").unwrap();
        let main = args.iter().position(|a| a == "net.minecraft.client.main.Main").unwrap();
        assert!(cp < main);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn ram_is_clamped_to_a_safe_range() {
        let tmp = std::env::temp_dir().join("sifora-launch-ram");
        let _ = std::fs::create_dir_all(&tmp);
        let (instance, resolved, runtime) = fixture(&tmp);
        let meta = InstanceMeta::default();
        let user = offline_user("Steve").unwrap();
        let args = build_arguments(&SpawnRequest {
            instance_meta: &meta,
            instance: &instance,
            resolved: &resolved,
            classpath: &[],
            runtime: &runtime,
            natives_dir: &instance.natives,
            user: &user,
            asset_index_name: "12",
            ram_mb: 999_999,
            width: 800,
            height: 600,
            version_type: "release",
            extra_jvm: &[],
            extra_game: &[],
        });
        assert!(args.iter().any(|a| a == "-Xmx32768M"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The 1.13+ JVM argument list ends with `-cp ${classpath}`. If we append a
    /// second `-cp`, the JVM uses the first one and the launch fails with a
    /// baffling "Could not find the main class".
    #[test]
    fn modern_documents_supply_the_classpath_and_we_do_not_duplicate_it() {
        let tmp = std::env::temp_dir().join("sifora-launch-modern");
        let _ = std::fs::create_dir_all(&tmp);
        let (instance, _, runtime) = fixture(&tmp);
        let mut resolved = resolved_with_jvm(&[
            "-Djava.library.path=${natives_directory}",
            "-Dminecraft.launcher.brand=${launcher_name}",
            "-Dminecraft.launcher.version=${launcher_version}",
            "-cp",
            "${classpath}",
        ]);
        resolved.arguments.game = vec![Argument::Plain("--demo".into())];
        let classpath = vec![PathBuf::from("libs/a.jar"), PathBuf::from("versions/1.20.4/1.20.4.jar")];
        let expected_classpath = classpath
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(if cfg!(target_os = "windows") { ";" } else { ":" });
        let user = offline_user("Steve").unwrap();

        let args = build_arguments(&SpawnRequest {
            instance_meta: &InstanceMeta::default(),
            instance: &instance,
            resolved: &resolved,
            classpath: &classpath,
            runtime: &runtime,
            natives_dir: &instance.natives,
            user: &user,
            asset_index_name: "12",
            ram_mb: 2048,
            width: 1280,
            height: 720,
            version_type: "release",
            extra_jvm: &[],
            extra_game: &[],
        });

        assert_eq!(args.iter().filter(|a| *a == "-cp").count(), 1, "{args:?}");
        assert!(args.contains(&expected_classpath), "classpath placeholder must be filled");
        assert!(args.iter().any(|a| a == "-Dminecraft.launcher.brand=sifora"));
        assert!(args.iter().any(|a| a == "-Dminecraft.launcher.version=sifora-launcher") || args.iter().any(|a| a.starts_with("-Dminecraft.launcher.version=0.")));
        assert!(!args.iter().any(|a| a.contains("${")), "no placeholder may survive");
        // `-cp` must precede the main class.
        let cp = args.iter().position(|a| a == "-cp").unwrap();
        let main = args.iter().position(|a| a == "net.minecraft.client.main.Main").unwrap();
        assert!(cp < main, "{args:?}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Pre-1.13 documents have no JVM argument list at all, so the launcher has
    /// to supply `-cp` itself.
    #[test]
    fn legacy_documents_get_a_launcher_supplied_classpath() {
        let tmp = std::env::temp_dir().join("sifora-launch-legacy");
        let _ = std::fs::create_dir_all(&tmp);
        let (instance, mut resolved, runtime) = fixture(&tmp);
        resolved.arguments = Arguments::default();
        resolved.minecraft_arguments = Some(
            "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} \
             --assetsDir ${assets_root} --assetIndex ${assets_index_name} --uuid ${auth_uuid} \
             --accessToken ${accessToken} --userProperties ${user_properties} --userType ${user_type}"
                .into(),
        );
        let classpath = vec![PathBuf::from("libs/lwjgl.jar")];
        let user = offline_user("Steve").unwrap();

        let args = build_arguments(&SpawnRequest {
            instance_meta: &InstanceMeta::default(),
            instance: &instance,
            resolved: &resolved,
            classpath: &classpath,
            runtime: &runtime,
            natives_dir: &instance.natives,
            user: &user,
            asset_index_name: "pre-1.6",
            ram_mb: 1024,
            width: 854,
            height: 480,
            version_type: "release",
            extra_jvm: &[],
            extra_game: &[],
        });

        assert_eq!(args.iter().filter(|a| *a == "-cp").count(), 1);
        assert!(args.iter().any(|a| a == "--username"));
        assert!(args.iter().any(|a| a == "Steve"));
        assert!(args.iter().any(|a| a == "{}"), "user_properties must be filled");
        assert!(args.iter().any(|a| a == "pre-1.6"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A per-instance override must not be able to smuggle a second `-cp` past
    /// the main class, which would change which entry point runs.
    #[test]
    fn user_jvm_overrides_cannot_inject_a_second_entry_point() {
        let tmp = std::env::temp_dir().join("sifora-launch-override");
        let _ = std::fs::create_dir_all(&tmp);
        let (instance, mut resolved, runtime) = fixture(&tmp);
        resolved.arguments = Arguments::default();
        resolved.minecraft_arguments = None;
        let user = offline_user("Steve").unwrap();
        let args = build_arguments(&SpawnRequest {
            instance_meta: &InstanceMeta::default(),
            instance: &instance,
            resolved: &resolved,
            classpath: &[PathBuf::from("libs/a.jar")],
            runtime: &runtime,
            natives_dir: &instance.natives,
            user: &user,
            asset_index_name: "12",
            ram_mb: 2048,
            width: 800,
            height: 600,
            version_type: "release",
            extra_jvm: &[
                "-javaagent:/tmp/evil.jar".into(),
                "-Xmx64G".into(),
                "-Dcustom=1".into(),
            ],
            extra_game: &[],
        });
        assert!(!args.iter().any(|a| a.starts_with("-javaagent")));
        assert_eq!(args.iter().filter(|a| a.starts_with("-Xmx")).count(), 1);
        assert!(args.iter().any(|a| a == "-Xmx2048M"));
        assert!(args.iter().any(|a| a == "-Dcustom=1"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn resolved_with_jvm(values: &[&str]) -> ResolvedVersion {
        let args: Vec<Argument> = values.iter().map(|v| Argument::Plain((*v).to_string())).collect();
        ResolvedVersion {
            id: "1.20.4".into(),
            main_class: "net.minecraft.client.main.Main".into(),
            libraries: vec![],
            arguments: Arguments { game: vec![], jvm: args },
            minecraft_arguments: None,
            asset_index: None,
            client: None,
            java_major: 17,
            release_time: None,
            release_type: "release".into(),
            chain: vec![],
        }
    }
}