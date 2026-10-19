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
    if !os.name.eq_ignore_ascii_case(os_name(platform)) {
        return false;
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

        // 1. Main artifact.
        let coordinate = parse_coordinate(&lib.name);
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
                    is_native: false,
                });
                classpath.push(libraries_dir.join(&relative).to_string_lossy().to_string());
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
                is_native: false,
            });
            classpath.push(libraries_dir.join(&c.relative_path).to_string_lossy().to_string());
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
    let ledger = Arc::clone(ledger);
    let results = download::run_bounded(pending, 4, cancel, move |file: LibraryFile| {
        let http = http.clone();
        let dir = dir.clone();
        let sink = sink2.clone();
        let cancel = cancel_for_closure.clone();
        let ledger = ledger.clone();
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

/// Extract `META-INF/natives/**` from a native jar into a flat directory.
pub fn extract_natives(jar: &Path, destination: &Path) -> AppResult<u32> {
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
        let Some(relative) = entry.enclosed_name() else {
            log_warn!("mc::libraries", "skipping unsafe zip entry in {}", jar.display());
            continue;
        };
        let relative = relative.to_string_lossy().to_string();
        if !relative.starts_with("META-INF/natives/") {
            continue;
        }
        let Some(name) = relative.strip_prefix("META-INF/natives/") else { continue };
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
            continue;
        }
        let target = destination.join(name);
        if target.exists() {
            extracted += 1;
            continue;
        }
        let mut out = std::fs::File::create(&target)
            .map_err(|e| AppError::io("Не удалось записать native-библиотеку", &e))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| AppError::io("Ошибка распаковки natives", &e))?;
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
    fn native_extraction_flattens_and_ignores_non_natives() {
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
        assert_eq!(count, 1);
        assert_eq!(std::fs::read_to_string(out.join("lwjgl.dll")).unwrap(), "native-bytes");
        assert!(!out.join("MANIFEST.MF").exists());
        assert!(!out.join("evil.dll").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}