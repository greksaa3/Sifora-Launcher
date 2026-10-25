//! Library (Maven artifact) resolution and native extraction.
//!
//! Libraries are shared between every instance — they are content-addressed
//! and identical across versions, so storing them once in `<game-data>/libraries`
//! keeps instances small and makes reinstalls cheap.
//!
//! Native libraries are the exception: they are *extracted* (flattened, with
//! `META-INF/` stripped) into `<instance>/natives` because the game loads them
//! through `java.library.path` rather than the classpath.

use std::path::{Path, PathBuf};
use std::sync::atomic::{Ordering};
use std::sync::Arc;

use super::download::{self, EventSink, FileTask};
use super::instances::InstancePaths;
use super::version::{
    arch_token, natives_for_platform, parse_coordinate, Argument, Library, NativeSpec, OsRule,
    Platform, Rule, RuleAction,
};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::log_warn;

pub const DEFAULT_REPO: &str = "https://libraries.minecraft.net/";

/// Evaluate a library's rule list for the current platform.
pub fn library_allowed(lib: &Library, platform: Platform) -> bool {
    let Some(rules) = &lib.rules else { return true };
    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule, platform) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

fn rule_matches(rule: &Rule, platform: Platform) -> bool {
    if let Some(os) = &rule.os {
        if !os_matches(os, platform) {
            return false;
        }
    }
    match &rule.features {
        None => true,
        Some(features) => match features.as_object() {
            // Sifora never enables Mojang's optional launch features, so any
            // feature-gated argument is skipped.
            Some(map) => map.is_empty(),
            None => false,
        },
    }
}

fn os_matches(os: &OsRule, platform: Platform) -> bool {
    if let Some(name) = &os.name {
        if !name.eq_ignore_ascii_case(os_name(platform)) {
            return false;
        }
    }
    if let Some(arch) = &os.arch {
        if !arch.is_empty() && !arch.eq_ignore_ascii_case(current_arch()) {
            return false;
        }
    }
    if let Some(version) = &os.version {
        if !version.is_empty() && version != "*" {
            // Mojang only ever emits `*` or an exact major.minor; there is no
            // range syntax, and refusing anything else is the safe default.
            return false;
        }
    }
    true
}

fn os_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Windows => "windows",
        Platform::MacOs => "osx",
        Platform::Linux => "linux",
    }
}

fn current_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "x86_64"
    }
}

fn maven_repo_of(platform: Platform) -> &'static str {
    match platform {
        Platform::Windows => "https://libraries.minecraft.net/",
        Platform::MacOs => "https://libraries.minecraft.net/",
        Platform::Linux => "https://libraries.minecraft.net/",
    }
}

/// A concrete file the launcher needs on disk.
#[derive(Debug, Clone)]
pub struct LibraryFile {
    pub id: String,
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    /// Path relative to the shared libraries directory.
    pub relative: String,
    /// `true` for native jars, which must be extracted rather than added to
    /// the classpath.
    pub is_native: bool,
}

/// Turn the library list of a resolved version into concrete files.
pub fn resolve_libraries(
    libraries: &[Library],
    platform: Platform,
    libraries_dir: &Path,
) -> (Vec<LibraryFile>, Vec<String>, Vec<Library>) {
    let mut files: Vec<LibraryFile> = Vec::new();
    let mut classpath: Vec<String> = Vec::new();
    let mut native_libs: Vec<Library> = Vec::new();

    for lib in libraries {
        if !library_allowed(lib, platform) {
            continue;
        }

        let coordinate = parse_coordinate(&lib.name);

        // Modern metadata (1.19+) declares a native as its own library entry —
        // `org.lwjgl:lwjgl:3.3.1:natives-windows`, one per OS *and*
        // architecture. The rules on those entries only name the OS, so the
        // classifier is the only thing that can pick the right file; without
        // this check all of `natives-windows`, `natives-windows-arm64` and
        // `natives-windows-x86` are downloaded and none is extracted.
        let declared_native = coordinate
            .as_ref()
            .and_then(|c| c.classifier.as_deref())
            .filter(|c| c.starts_with("natives-"));
        if let Some(classifier) = declared_native {
            if !native_classifier_matches(classifier, platform) {
                continue;
            }
        }
        let is_native = declared_native.is_some();

        // 1. Main artifact.
        if let Some(downloads) = &lib.downloads {
            if let Some(artifact) = &downloads.artifact {
                let relative = coordinate
                    .as_ref()
                    .map(|c| c.relative_path.clone())
                    .unwrap_or_else(|| {
                        artifact
                            .url
                            .rsplit('/')
                            .next()
                            .unwrap_or(&lib.name)
                            .to_string()
                    });
                files.push(LibraryFile {
                    id: format!("lib:{}", lib.name),
                    url: artifact.url.clone(),
                    sha1: Some(artifact.sha1.clone()),
                    size: Some(artifact.size),
                    relative: relative.clone(),
                    is_native,
                });
                if is_native {
                    native_libs.push(lib.clone());
                } else {
                    classpath.push(libraries_dir.join(&relative).to_string_lossy().to_string());
                }
            }

            // 2. Native classifier, if the platform declares one.
            if let Some(native_spec) = &lib.natives {
                let spec = natives_for_platform(native_spec, platform);
                if let Some(classifier) = spec.and_then(|s| native_classifier(s, platform)) {
                    let file_meta = downloads
                        .classifiers
                        .as_ref()
                        .and_then(|c| c.get(&classifier))
                        .cloned()
                        .or_else(|| {
                            // Some libraries declare no classifier map; fall
                            // back to the canonical Maven URL + coordinate path.
                            coordinate.as_ref().map(|c| super::version::FileMeta {
                                url: format!(
                                    "{}{}",
                                    maven_repo_of(platform),
                                    native_relative_path(c, &classifier)
                                ),
                                // No checksum available -> size check only.
                                sha1: String::new(),
                                size: 0,
                            })
                        });

                    if let Some(meta) = file_meta {
                        let relative = coordinate
                            .as_ref()
                            .map(|c| native_relative_path(c, &classifier))
                            .unwrap_or_default();
                        if !relative.is_empty() {
                            files.push(LibraryFile {
                                id: format!("native:{}:{}", lib.name, classifier),
                                url: meta.url,
                                sha1: if meta.sha1.is_empty() { None } else { Some(meta.sha1) },
                                size: if meta.size == 0 { None } else { Some(meta.size) },
                                relative,
                                is_native: true,
                            });
                            native_libs.push(lib.clone());
                        }
                    }
                }
            }
        } else if let Some(c) = &coordinate {
            // No explicit download block: resolve against the public repo.
            files.push(LibraryFile {
                id: format!("lib:{}", lib.name),
                url: format!("{}{}", maven_repo_of(platform), c.relative_path),
                sha1: None,
                size: None,
                relative: c.relative_path.clone(),
                is_native,
            });
            if is_native {
                native_libs.push(lib.clone());
            } else {
                classpath.push(libraries_dir.join(&c.relative_path).to_string_lossy().to_string());
            }
        }
    }

    files.sort_by(|a, b| a.relative.cmp(&b.relative));
    files.dedup_by(|a, b| a.relative == b.relative);
    (files, classpath, native_libs)
}

fn native_relative_path(coordinate: &super::version::ParsedCoordinate, classifier: &str) -> String {
    let file_name = format!(
        "{}-{}-{}.{}",
        coordinate.artifact, coordinate.version, classifier, coordinate.extension
    );
    format!(
        "{}/{}/{}/{}",
        coordinate.group.replace('.', "/"),
        coordinate.artifact,
        coordinate.version,
        file_name
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostArch {
    X86,
    X86_64,
    Aarch64,
}

fn host_arch() -> HostArch {
    if cfg!(target_arch = "x86") {
        HostArch::X86
    } else if cfg!(target_arch = "aarch64") {
        HostArch::Aarch64
    } else {
        HostArch::X86_64
    }
}

/// Does a `natives-*` classifier describe *this* machine?
///
/// Mojang uses `natives-windows`, `natives-windows-arm64`, `natives-windows-x86`,
/// `natives-macos`, `natives-macos-arm64`, `natives-linux`, … The plain name is
/// the 64-bit build. Anything with an architecture we do not recognise is
/// refused rather than guessed: shipping the wrong DLL is worse than shipping
/// none, because the failure is a crash inside the game instead of a message.
pub fn native_classifier_matches(classifier: &str, platform: Platform) -> bool {
    let Some(rest) = classifier.strip_prefix("natives-") else {
        return false;
    };
    let (file_os, arch) = match rest.split_once('-') {
        Some((os, arch)) => (os, Some(arch)),
        None => (rest, None),
    };
    // Rules call macOS "osx"; classifiers call it "macos".
    let os_matches = file_os == os_name(platform)
        || (matches!(platform, Platform::MacOs) && file_os == "macos");
    if !os_matches {
        return false;
    }
    match (host_arch(), arch) {
        (HostArch::X86_64, None) => true,
        (HostArch::Aarch64, Some("arm64")) => true,
        (HostArch::X86, Some("x86")) => true,
        // Pre-LWJGL-3 metadata spelled the architecture as a bit width.
        (HostArch::X86_64, Some("64")) => true,
        (HostArch::X86, Some("32")) => true,
        _ => false,
    }
}

/// Resolve the native classifier pattern for this platform.
pub fn native_classifier(spec: &NativeSpec, platform: Platform) -> Option<String> {
    match spec {
        NativeSpec::Replacement(raw) => Some(raw.replace("${arch}", arch_token(platform))),
        NativeSpec::Map(map) => {
            let os = os_name(platform);
            // Mojang sometimes keys by `<os>-<arch>`; try the specific key first.
            let candidates = [
                format!("{os}-{}", current_arch()),
                format!("{os}-x86_64"),
                format!("{os}-x86"),
                os.to_string(),
            ];
            for key in candidates {
                if let Some(value) = map.get(&key) {
                    let resolved = value.replace("${arch}", arch_token(platform));
                    if !resolved.is_empty() {
                        return Some(resolved);
                    }
                }
            }
            None
        }
    }
}


/// Download every missing library.
pub async fn fetch_libraries(
    http: &reqwest::Client,
    files: Vec<LibraryFile>,
    libraries_dir: &Path,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    sink: EventSink,
    ledger: &Arc<parking_lot::Mutex<download::Ledger>>,
) -> AppResult<()> {
    let pending: Vec<LibraryFile> = files
        .into_iter()
        .filter(|f| !libraries_dir.join(&f.relative).exists())
        .collect();
    if pending.is_empty() {
        return Ok(());
    }

    for file in &pending {
        ledger.lock().add_file(file.size.unwrap_or(0));
    }

    let http = http.clone();
    let dir = libraries_dir.to_path_buf();
    let sink2 = sink.clone();
    let cancel_for_closure = cancel.clone();
    let ledger_for_workers = Arc::clone(ledger);
    let results = download::run_bounded(pending, 4, cancel, move |file: LibraryFile| {
        let http = http.clone();
        let dir = dir.clone();
        let sink = sink2.clone();
        let cancel = cancel_for_closure.clone();
        let ledger = ledger_for_workers.clone();
        async move {
            let task = FileTask::new(file.id, file.url, dir.join(&file.relative))
                .with_sha1(file.sha1)
                .with_size(file.size);
            let len = download::ensure_file(&http, &task, &cancel, &sink).await?;
            ledger.lock().set_file_bytes(len);
            Ok(())
        }
    })
    .await;

    // The throttle may have swallowed the last increment; the UI must still see
    // the batch finished.
    ledger.lock().flush();

    let mut first_error: Option<AppError> = None;
    let mut cancelled = false;
    for result in results {
        match result {
            Ok(()) => {}
            Err(e) if e.kind == ErrorKind::Cancelled => cancelled = true,
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
    }
    if cancelled {
        return Err(AppError::cancelled());
    }
    match first_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Extract the native payload of a jar into a flat directory.
///
/// Three layouts are in the wild, and all three are handled by writing the
/// **last path component** of every payload entry:
///
/// * `META-INF/natives/lwjgl.dll` — the oldest convention;
/// * `lwjgl.dll` at the archive root;
/// * `windows/x64/org/lwjgl/lwjgl.dll` — what LWJGL 3.3 actually ships, and the
///   layout that made the game die on `UnsatisfiedLinkError: lwjgl.dll` while
///   every other factory looked fine.
///
/// `META-INF/` entries other than `META-INF/natives/` are metadata (`MANIFEST.MF`,
/// `INDEX.LIST`, multi-release `module-info.class`) and are skipped. Flattening
/// is not a convenience: the game loads natives from a single directory passed
/// through `-Djava.library.path`, and it is also what makes a hostile archive
/// unable to escape the destination through a nested path or `..`.
pub fn extract_natives(jar: &Path, destination: &Path) -> AppResult<u32> {
    extract_natives_collect(jar, destination, &mut Vec::new())
}

/// [`extract_natives`], also reporting the file names it wrote.
///
/// The names feed the freshness marker in [`mark_natives_ready`], so the next
/// launch can prove the folder is intact instead of unpacking everything again.
pub fn extract_natives_collect(
    jar: &Path,
    destination: &Path,
    names: &mut Vec<String>,
) -> AppResult<u32> {
    std::fs::create_dir_all(destination)
        .map_err(|e| AppError::io("Не удалось создать папку natives", &e))?;

    let file = std::fs::File::open(jar)
        .map_err(|e| AppError::io("Не удалось открыть native-библиотеку", &e))?;
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(e) => {
            log_warn!("mc::libraries", "not a zip: {} ({e})", jar.display());
            return Ok(0);
        }
    };

    let mut extracted = 0u32;
    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entry.is_dir() {
            continue;
        }
        let Some(relative) = entry.enclosed_name() else {
            log_warn!("mc::libraries", "skipping unsafe zip entry in {}", jar.display());
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        let payload = match relative.strip_prefix("META-INF/natives/") {
            Some(name) => name,
            None => {
                // Any other `META-INF/` entry is metadata, not a library.
                if relative.starts_with("META-INF/") {
                    continue;
                }
                relative.as_str()
            }
        };
        // Only the final component is written: `windows/x64/org/lwjgl/lwjgl.dll`
        // and `lwjgl.dll` must both land as `lwjgl.dll`.
        let name = payload.rsplit('/').next().unwrap_or("");
        if name.is_empty() || name.contains("..") || name.contains('\\') {
            continue;
        }
        let target = destination.join(name);
        if target.exists() {
            names.push(name.to_string());
            extracted += 1;
            continue;
        }
        let mut out = std::fs::File::create(&target)
            .map_err(|e| AppError::io("Не удалось записать native-библиотеку", &e))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| AppError::io("Ошибка распаковки natives", &e))?;
        names.push(name.to_string());
        extracted += 1;
    }
    Ok(extracted)
}

/// Resolve the `-Djava.library.path` argument value for an instance.
pub fn natives_dir_for(instance: &InstancePaths) -> PathBuf {
    instance.natives.clone()
}

/// Drop a stale natives folder so a removed library cannot linger.
pub fn clear_natives(instance: &InstancePaths) -> AppResult<()> {
    let dir = instance.natives.clone();
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| AppError::io("Не удалось очистить natives", &e))?;
    }
    Ok(())
}

/// File name of the freshness marker inside an instance's `natives` folder.
const NATIVES_MARKER: &str = ".sifora-natives";

fn natives_marker_path(instance: &InstancePaths) -> PathBuf {
    instance.natives.join(NATIVES_MARKER)
}

/// Which jars feed the natives folder, and how big each one is.
fn natives_fingerprint(jars: &[LibraryFile]) -> String {
    let mut lines: Vec<String> = jars
        .iter()
        .map(|f| format!("{}|{}", f.relative, f.size.unwrap_or(0)))
        .collect();
    lines.sort();
    lines.join("\n")
}

/// Can the existing `natives` folder be reused instead of being rebuilt?
///
/// Unpacking three dozen jars on every single launch is pure repetition: the
/// inputs only change when the version's library list changes. The marker
/// records both the inputs (jar path + size) and the outputs (every file name
/// that was written), and the folder is only trusted when both still agree —
/// so deleting a DLL by hand quietly gets it back.
pub fn natives_are_ready(instance: &InstancePaths, jars: &[LibraryFile]) -> bool {
    if jars.is_empty() {
        return false;
    }
    let Ok(marker) = std::fs::read_to_string(natives_marker_path(instance)) else {
        return false;
    };
    let Some((inputs, outputs)) = marker.split_once("--\n") else {
        return false;
    };
    if inputs != natives_fingerprint(jars) {
        return false;
    }
    let mut seen = 0usize;
    for name in outputs.lines().filter(|l| !l.is_empty()) {
        if !instance.natives.join(name).is_file() {
            return false;
        }
        seen += 1;
    }
    seen > 0
}

/// Record that the natives folder now matches this set of jars.
pub fn mark_natives_ready(instance: &InstancePaths, jars: &[LibraryFile], names: &[String]) {
    let mut out = String::with_capacity(names.len() * 24 + 128);
    out.push_str(&natives_fingerprint(jars));
    out.push_str("--\n");
    let mut sorted: Vec<&String> = names.iter().collect();
    sorted.sort();
    sorted.dedup();
    for name in sorted {
        out.push_str(name);
        out.push('\n');
    }
    let _ = std::fs::write(natives_marker_path(instance), out);
}

/// Collect the JVM/game arguments contributed by libraries (e.g. LWJGL's
/// `-Djava.library.path` is handled separately, but Forge adds classpath
/// entries). Kept as a seam for stage 2 mod loaders.
pub fn library_classpath_entries(_lib: &Library) -> Vec<String> {
    Vec::new()
}

/// Whether the argument list of this version contains a library-injected JVM
/// argument that must be preserved verbatim.
pub fn is_library_argument(arg: &Argument) -> bool {
    matches!(arg, Argument::Conditional { .. })
}

pub fn argument_is_allowed(arg: &Argument, platform: Platform) -> bool {
    match arg {
        Argument::Plain(_) => true,
        Argument::Conditional { rules, .. } => {
            let mut allowed = false;
            for rule in rules {
                if rule_matches(rule, platform) {
                    allowed = rule.action == RuleAction::Allow;
                }
            }
            allowed
        }
    }
}

/// Relative time helper used by callers that want to poll.
pub fn cancel_requested(cancel: &std::sync::atomic::AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::version::{Library, NativeSpec};

    fn lib(json: &str) -> Library {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn libraries_without_rules_are_allowed() {
        let l = lib(r#"{"name":"a:b:1"}"#);
        assert!(library_allowed(&l, Platform::Windows));
    }

    #[test]
    fn os_rules_filter_other_platforms() {
        let l = lib(r#"{"name":"a:b:1","rules":[{"action":"allow","os":{"name":"osx"}}]}"#);
        assert!(!library_allowed(&l, Platform::Windows));
        assert!(library_allowed(&l, Platform::MacOs));
    }

    #[test]
    fn later_rules_win() {
        let l = lib(
            r#"{"name":"a:b:1","rules":[
                 {"action":"allow"},
                 {"action":"disallow","os":{"name":"windows"}}]}"#,
        );
        assert!(!library_allowed(&l, Platform::Windows));
        assert!(library_allowed(&l, Platform::Linux));
    }

    #[test]
    fn feature_gated_libraries_are_skipped() {
        let l = lib(
            r#"{"name":"a:b:1","rules":[{"action":"allow","features":{"is_demo_user":true}}]}"#,
        );
        assert!(!library_allowed(&l, Platform::Windows));
    }

    #[test]
    fn native_classifier_resolves_arch_placeholders() {
        let spec = NativeSpec::Map(
            [("windows".to_string(), "natives-windows-${arch}".to_string())].into_iter().collect(),
        );
        assert_eq!(native_classifier(&spec, Platform::Windows).as_deref(), Some("natives-windows-64"));

        let plain = NativeSpec::Replacement("natives-linux".to_string());
        assert_eq!(native_classifier(&plain, Platform::Linux).as_deref(), Some("natives-linux"));
    }

    #[test]
    fn arch_specific_map_keys_are_preferred() {
        let spec = NativeSpec::Map(
            [
                ("windows-x86_64".to_string(), "natives-windows-x86_64".to_string()),
                ("windows".to_string(), "natives-windows-x86".to_string()),
            ]
            .into_iter()
            .collect(),
        );
        assert_eq!(native_classifier(&spec, Platform::Windows).as_deref(), Some("natives-windows-x86_64"));
    }

    #[test]
    fn artifact_paths_come_from_mojang_when_available() {
        let l = lib(
            r#"{"name":"net.minecraft:launchwrapper:1.5",
                "downloads":{"artifact":{"url":"https://x/launchwrapper-1.5.jar","sha1":"ab","size":10}}}"#,
        );
        let (files, classpath, _) = resolve_libraries(&[l], Platform::Windows, Path::new("libs"));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative, "net/minecraft/launchwrapper/1.5/launchwrapper-1.5.jar");
        assert_eq!(classpath.len(), 1);
    }

    #[test]
    fn libraries_without_downloads_fall_back_to_the_maven_repo() {
        let l = lib(r#"{"name":"org.example:thing:1.2.3"}"#);
        let (files, classpath, _) = resolve_libraries(&[l], Platform::Windows, Path::new("libs"));
        assert_eq!(files[0].url, "https://libraries.minecraft.net/org/example/thing/1.2.3/thing-1.2.3.jar");
        assert!(files[0].sha1.is_none(), "no fabricated checksums");
        assert_eq!(classpath.len(), 1);
    }

    #[test]
    fn native_jars_are_marked_and_kept_off_the_classpath() {
        // Shape taken from the real LWJGL entry in Mojang metadata: one
        // artifact plus per-platform classifiers under `natives`.
        let l = lib(
            r#"{"name":"org.lwjgl.lwjgl:lwjgl-platform:2.9.4",
                "downloads":{
                  "artifact":{"path":"org/lwjgl/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4.jar","url":"https://x/p.jar","sha1":"aa","size":100},
                  "classifiers":{
                    "natives-windows":{"path":"org/lwjgl/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4-natives-windows.jar","url":"https://x/pn.jar","sha1":"bb","size":50}
                  }},
                "natives":{"windows":"natives-windows","linux":"natives-linux","osx":"natives-osx"}}"#,
        );
        let (files, classpath, natives) =
            resolve_libraries(&[l], Platform::Windows, Path::new("libs"));
        assert_eq!(files.len(), 2, "artifact + native");
        assert_eq!(files.iter().filter(|f| f.is_native).count(), 1);
        // The native jar must not appear on the classpath.
        assert_eq!(classpath.len(), 1);
        assert!(!classpath[0].contains("natives-windows"));
        assert_eq!(natives.len(), 1);
        assert!(files.iter().any(|f| f.relative.ends_with("lwjgl-platform-2.9.4-natives-windows.jar")));
    }

    #[test]
    fn modern_native_entries_are_picked_by_os_and_arch() {
        // Shape taken from 1.20.1: each native is its own library entry, the
        // rules only mention the OS, and the architecture lives in the
        // classifier. All three Windows entries therefore pass the rules.
        let core = lib(
            r#"{"name":"org.lwjgl:lwjgl:3.3.1",
                "downloads":{"artifact":{"url":"https://x/core.jar","sha1":"aa","size":1}}}"#,
        );
        let win = lib(
            r#"{"name":"org.lwjgl:lwjgl:3.3.1:natives-windows",
                "rules":[{"action":"allow","os":{"name":"windows"}}],
                "downloads":{"artifact":{"url":"https://x/win.jar","sha1":"bb","size":1}}}"#,
        );
        let arm = lib(
            r#"{"name":"org.lwjgl:lwjgl:3.3.1:natives-windows-arm64",
                "rules":[{"action":"allow","os":{"name":"windows"}}],
                "downloads":{"artifact":{"url":"https://x/arm.jar","sha1":"cc","size":1}}}"#,
        );
        let x86 = lib(
            r#"{"name":"org.lwjgl:lwjgl:3.3.1:natives-windows-x86",
                "rules":[{"action":"allow","os":{"name":"windows"}}],
                "downloads":{"artifact":{"url":"https://x/x86.jar","sha1":"dd","size":1}}}"#,
        );

        let (files, classpath, natives) =
            resolve_libraries(&[core, win, arm, x86], Platform::Windows, Path::new("libs"));

        assert_eq!(classpath.len(), 1, "only the core jar belongs on the classpath");
        assert!(classpath[0].ends_with("lwjgl-3.3.1.jar"));

        let native: Vec<&LibraryFile> = files.iter().filter(|f| f.is_native).collect();
        assert_eq!(native.len(), 1, "exactly one native jar for this machine");
        assert_eq!(natives.len(), 1);
        let expected = match host_arch() {
            HostArch::X86_64 => "lwjgl-3.3.1-natives-windows.jar",
            HostArch::Aarch64 => "lwjgl-3.3.1-natives-windows-arm64.jar",
            HostArch::X86 => "lwjgl-3.3.1-natives-windows-x86.jar",
        };
        assert_eq!(native[0].relative, format!("org/lwjgl/lwjgl/3.3.1/{expected}"));
    }

    #[test]
    fn native_classifiers_of_other_systems_are_refused() {
        assert!(!native_classifier_matches("natives-linux", Platform::Windows));
        assert!(!native_classifier_matches("natives-windows-arm64", Platform::Linux));
        assert!(native_classifier_matches("natives-osx", Platform::MacOs));
        assert!(native_classifier_matches("natives-macos", Platform::MacOs));
        assert!(!native_classifier_matches("natives-macos-arm64", Platform::Windows));
        assert!(!native_classifier_matches("natives-windows-riscv64", Platform::Windows));
        assert!(!native_classifier_matches("plain", Platform::Windows));
    }

    #[test]
    fn native_extraction_flattens_and_cannot_escape_the_destination() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("sifora-natives-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let jar_path = dir.join("n.jar");
        {
            let file = std::fs::File::create(&jar_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<'_, ()> =
                zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("META-INF/natives/lwjgl.dll", options).unwrap();
            zip.write_all(b"native-bytes").unwrap();
            zip.start_file("META-INF/MANIFEST.MF", options).unwrap();
            zip.write_all(b"Manifest-Version: 1.0").unwrap();
            zip.start_file("META-INF/natives/../evil.dll", options).unwrap();
            zip.write_all(b"evil").unwrap();
            zip.finish().unwrap();
        }

        let out = dir.join("out");
        let count = extract_natives(&jar_path, &out).unwrap();
        assert_eq!(count, 2, "both payload entries land in the destination");
        assert_eq!(std::fs::read_to_string(out.join("lwjgl.dll")).unwrap(), "native-bytes");
        assert!(!out.join("MANIFEST.MF").exists());
        // Only the final path component is ever written, so the `..` cannot
        // reach anything: the file must be inside `out`, never beside it.
        assert!(!dir.join("evil.dll").exists());
        assert!(out.join("evil.dll").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The layout LWJGL 3.3 actually ships: the payload sits under
    /// `windows/x64/org/lwjgl/…`. Requiring a flat entry (or the legacy
    /// `META-INF/natives/` prefix) produced an empty `natives/` folder and the
    /// game died on `UnsatisfiedLinkError: lwjgl.dll`.
    #[test]
    fn nested_lwjgl3_native_jars_are_flattened() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("sifora-natives-nested-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let jar_path = dir.join("n.jar");
        {
            let file = std::fs::File::create(&jar_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<'_, ()> =
                zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("META-INF/MANIFEST.MF", options).unwrap();
            zip.write_all(b"Manifest-Version: 1.0").unwrap();
            zip.start_file("windows/x64/org/lwjgl/lwjgl.dll", options).unwrap();
            zip.write_all(b"lwjgl-x64").unwrap();
            zip.start_file("windows/x64/org/lwjgl/lwjgl.dll.sha1", options).unwrap();
            zip.write_all(b"deadbeef").unwrap();
            zip.start_file("META-INF/versions/9/module-info.class", options).unwrap();
            zip.write_all(b"not-a-native").unwrap();
            zip.finish().unwrap();
        }

        let out = dir.join("out");
        let mut names = Vec::new();
        let count = extract_natives_collect(&jar_path, &out, &mut names).unwrap();
        assert_eq!(count, 2, "both payload files must land: {names:?}");
        assert_eq!(std::fs::read_to_string(out.join("lwjgl.dll")).unwrap(), "lwjgl-x64");
        assert!(out.join("lwjgl.dll.sha1").exists());
        assert!(!out.join("MANIFEST.MF").exists());
        assert!(!out.join("module-info.class").exists());
        assert!(names.contains(&"lwjgl.dll".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Root-level payloads (older jars) still work, and metadata is still dropped.
    #[test]
    fn root_level_native_jars_are_extracted() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("sifora-natives-root-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let jar_path = dir.join("n.jar");
        {
            let file = std::fs::File::create(&jar_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<'_, ()> =
                zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("lwjgl.dll", options).unwrap();
            zip.write_all(b"lwjgl").unwrap();
            zip.start_file("glfw.dll", options).unwrap();
            zip.write_all(b"glfw").unwrap();
            zip.start_file("META-INF/MANIFEST.MF", options).unwrap();
            zip.write_all(b"Manifest-Version: 1.0").unwrap();
            zip.start_file("META-INF/versions/9/module-info.class", options).unwrap();
            zip.write_all(b"not-a-native").unwrap();
            zip.finish().unwrap();
        }

        let out = dir.join("out");
        let count = extract_natives(&jar_path, &out).unwrap();
        assert_eq!(count, 2, "only the two root-level DLLs");
        assert_eq!(std::fs::read_to_string(out.join("lwjgl.dll")).unwrap(), "lwjgl");
        assert!(out.join("glfw.dll").exists());
        assert!(!out.join("MANIFEST.MF").exists());
        assert!(!out.join("module-info.class").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The freshness marker must be evidence-based: `natives_are_ready` is only
    /// true while the jars are unchanged *and* every extracted file is present.
    #[test]
    fn the_natives_folder_is_only_reused_while_it_is_intact() {
        let dir = std::env::temp_dir().join("sifora-natives-marker-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("natives")).unwrap();

        let instance = InstancePaths {
            root: dir.clone(),
            meta: dir.join("instance.json"),
            natives: dir.join("natives"),
            assets: dir.join("assets"),
            mods: dir.join("mods"),
        };
        let jars = vec![LibraryFile {
            id: "native:a".into(),
            url: "https://x/a.jar".into(),
            relative: "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar".into(),
            sha1: None,
            size: Some(1234),
            is_native: true,
        }];

        assert!(!natives_are_ready(&instance, &jars), "no marker means no shortcut");
        assert!(!natives_are_ready(&instance, &[]), "no jars means no shortcut");

        std::fs::write(instance.natives.join("lwjgl.dll"), b"x").unwrap();
        mark_natives_ready(&instance, &jars, &["lwjgl.dll".to_string()]);
        assert!(natives_are_ready(&instance, &jars));

        // A different library set invalidates it.
        let mut other = jars.clone();
        other[0].size = Some(4321);
        assert!(!natives_are_ready(&instance, &other));

        // Deleting an extracted file invalidates it too.
        std::fs::remove_file(instance.natives.join("lwjgl.dll")).unwrap();
        assert!(!natives_are_ready(&instance, &jars));

        let _ = std::fs::remove_dir_all(&dir);
    }
}