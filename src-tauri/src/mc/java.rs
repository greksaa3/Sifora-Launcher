//! Java runtime discovery and provisioning.
//!
//! Order of preference:
//!
//! 1. An explicit path from settings.
//! 2. `JAVA_HOME`.
//! 3. `java` on `PATH`.
//! 4. Installed runtimes found in the usual OS locations / registry.
//! 5. A Temurin (Eclipse Adoptium) JRE downloaded into the game-data folder.
//!
//! The first runtime whose **major version is >= the one the version requires**
//! wins. An exact match is preferred over a newer one because old Minecraft
//! versions crash on modern JVMs (removed flags, changed LWJGL requirements),
//! but never a *too old* runtime — that just fails to start.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Serialize;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::log_info;

/// Runtime flavour, used only for reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    Jdk,
    Jre,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaRuntime {
    pub path: PathBuf,
    pub version: String,
    pub major: u32,
    pub vendor: String,
    pub kind: RuntimeKind,
    pub source: &'static str,
}

impl JavaRuntime {
    pub fn executable(&self) -> PathBuf {
        if cfg!(target_os = "windows") {
            self.path.join("bin").join("javaw.exe")
        } else {
            self.path.join("bin").join("java")
        }
    }

    pub fn cli_executable(&self) -> PathBuf {
        if cfg!(target_os = "windows") {
            self.path.join("bin").join("java.exe")
        } else {
            self.path.join("bin").join("java")
        }
    }

    pub fn describe(&self) -> String {
        format!("{} {} ({})", self.vendor, self.version, self.source)
    }
}

/// Parse `java -version` output.
///
/// The line looks like:
/// `openjdk version "21.0.3" 2024-04-16` (newer) or
/// `java version "1.8.0_392"` (legacy).
pub fn parse_version(output: &str) -> Option<(u32, String)> {
    for line in output.lines() {
        let line = line.trim();
        let Some(rest) = line.split("version \"").nth(1) else { continue };
        let Some(raw) = rest.split('"').next() else { continue };
        let raw = raw.trim();
        let major = if let Some(stripped) = raw.strip_prefix("1.") {
            stripped.split('.').next().and_then(|p| p.parse::<u32>().ok())?
        } else {
            raw.split('.').next().and_then(|p| p.parse::<u32>().ok())?
        };
        return Some((major, raw.to_string()));
    }
    None
}

fn classify(path: &Path) -> RuntimeKind {
    if path.join("include").join(if cfg!(target_os = "windows") { "windows.h" } else { "jni.h" }).exists() {
        RuntimeKind::Jdk
    } else {
        RuntimeKind::Jre
    }
}

/// Probe a candidate JDK/JRE root by running `java -version`.
pub fn probe(root: &Path, source: &'static str) -> Option<JavaRuntime> {
    let cli = if cfg!(target_os = "windows") {
        root.join("bin").join("java.exe")
    } else {
        root.join("bin").join("java")
    };
    if !cli.is_file() {
        return None;
    }
    let output = std::process::Command::new(&cli)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stderr).to_string();
    let (major, version) = parse_version(&text)?;
    Some(JavaRuntime {
        path: root.to_path_buf(),
        version,
        major,
        vendor: vendor_from(&text),
        kind: classify(root),
        source,
    })
}

fn vendor_from(text: &str) -> String {
    let lower = text.to_lowercase();
    for (needle, name) in [
        ("temurin", "Eclipse Temurin"),
        ("adoptium", "Eclipse Temurin"),
        ("corretto", "Amazon Corretto"),
        ("graalvm", "GraalVM"),
        ("zulu", "Azul Zulu"),
        ("microsoft", "Microsoft Build of OpenJDK"),
        ("openjdk", "OpenJDK"),
        ("oracle", "Oracle Java"),
        ("ibm", "IBM Semeru"),
    ] {
        if lower.contains(needle) {
            return name.to_string();
        }
    }
    "Java".to_string()
}

/// Every candidate location we are willing to look at.
fn candidate_roots(explicit: Option<&str>) -> Vec<(PathBuf, &'static str)> {
    let mut out: Vec<(PathBuf, &'static str)> = Vec::new();

    if let Some(p) = explicit {
        out.push((normalise_root(PathBuf::from(p)), "settings"));
    }
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        out.push((normalise_root(PathBuf::from(home)), "JAVA_HOME"));
    }
    // Sifora-managed runtimes take precedence over the system ones: they were
    // provisioned for a specific Minecraft major version.
    out.push((PathBuf::from("__managed__"), "managed"));

    if cfg!(target_os = "windows") {
        for path in registry_java_homes() {
            out.push((normalise_root(PathBuf::from(path)), "registry"));
        }
        for base in ["C:\\Program Files\\Java", "C:\\Program Files\\Eclipse Adoptium", "C:\\Program Files\\Microsoft", "C:\\Program Files\\Amazon Corretto", "C:\\Program Files (x86)\\Java"] {
            if let Ok(entries) = std::fs::read_dir(base) {
                for entry in entries.flatten() {
                    out.push((normalise_root(entry.path()), "filesystem"));
                }
            }
        }
    } else {
        if let Ok(path) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path) {
                let bin = dir.join("java");
                if let Ok(real) = std::fs::canonicalize(&bin) {
                    if let Some(root) = real.parent().and_then(|p| p.parent()) {
                        out.push((root.to_path_buf(), "PATH"));
                    }
                }
                let javac = dir.join("javac");
                if let Ok(real) = std::fs::canonicalize(&javac) {
                    if let Some(root) = real.parent().and_then(|p| p.parent()) {
                        out.push((root.to_path_buf(), "PATH"));
                    }
                }
            }
        }
        for base in ["/usr/lib/jvm", "/usr/java", "/Library/Java/JavaVirtualMachines"] {
            if let Ok(entries) = std::fs::read_dir(base) {
                for entry in entries.flatten() {
                    out.push((entry.path(), "filesystem"));
                    // macOS bundles put the real home in `Contents/Home`.
                    let nested = entry.path().join("Contents").join("Home");
                    if nested.is_dir() {
                        out.push((nested, "filesystem"));
                    }
                }
            }
        }
    }

    out
}

fn normalise_root(path: PathBuf) -> PathBuf {
    // Accept `.../bin/java.exe` as well as the runtime root.
    let text = path.to_string_lossy();
    if text.contains("java.exe") || text.ends_with("\\bin\\java") || text.ends_with("/bin/java") {
        if let Some(root) = path.parent().and_then(|p| p.parent()) {
            return root.to_path_buf();
        }
    }
    path
}

/// Registry-installed Java homes.
///
/// Uses the `reg.exe` that ships with Windows instead of linking a registry
/// crate. Each query spawns a process, which is far too slow to repeat, so the
/// result is computed once and cached for the lifetime of the process.
#[cfg(windows)]
fn registry_java_homes() -> Vec<String> {
    static CACHE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        const KEYS: &[&str] = &[
            r"SOFTWARE\JavaSoft\JDK",
            r"SOFTWARE\JavaSoft\Java Development Kit",
            r"SOFTWARE\JavaSoft\JRE",
            r"SOFTWARE\Eclipse Adoptium",
            r"SOFTWARE\Amazon Corretto",
            r"SOFTWARE\Zulu",
            r"SOFTWARE\Microsoft\JDK",
            r"SOFTWARE\Oracle\Java",
        ];
        const VALUE_NAMES: &[&str] = &["JavaHome", "Path", "RuntimeLib"];

        let mut out: Vec<String> = Vec::new();
        for key in KEYS {
            for hive in ["HKEY_LOCAL_MACHINE", "HKEY_CURRENT_USER"] {
                let Ok(output) = std::process::Command::new("reg")
                    .args(["query", &format!("{hive}\\{key}"), "/s"])
                    .stdin(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .output()
                else {
                    continue;
                };
                // `reg query` writes in the OEM code page; the interesting parts
                // (`REG_SZ` and the path) are ASCII, so lossy decoding is fine.
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    let mut fields = line.split_whitespace();
                    let Some(name) = fields.next() else { continue };
                    if !VALUE_NAMES.iter().any(|v| v.eq_ignore_ascii_case(name)) {
                        continue;
                    }
                    let rest: Vec<&str> = fields.collect();
                    let Some(type_pos) = rest.iter().position(|t| t.starts_with("REG_")) else {
                        continue;
                    };
                    let value = rest[type_pos + 1..].join(" ").trim().to_string();
                    if value.len() > 2 && !out.contains(&value) {
                        out.push(value);
                    }
                }
            }
        }
        out
    })
    .clone()
}

#[cfg(not(windows))]
fn registry_java_homes() -> Vec<String> {
    Vec::new()
}

/// Find a usable runtime for `required_major`.
///
/// `managed_dir` is where Sifora keeps the runtimes it downloads itself.
pub fn discover(required_major: u32, explicit: Option<&str>, managed_dir: &Path) -> Option<JavaRuntime> {
    let mut best_exact: Option<JavaRuntime> = None;
    let mut best_newer: Option<JavaRuntime> = None;

    for (root, source) in candidate_roots(explicit) {
        let root = if root == PathBuf::from("__managed__") {
            match managed_dir.read_dir() {
                Ok(entries) => {
                    let mut found = None;
                    for entry in entries.flatten() {
                        let candidate = entry.path();
                        if let Some(rt) = probe(&candidate, "managed") {
                            if rt.major >= required_major
                                && (found.as_ref().map(|f: &JavaRuntime| rt.major < f.major).unwrap_or(true))
                            {
                                found = Some(rt);
                            }
                        }
                    }
                    match found {
                        Some(rt) => {
                            remember(&mut best_exact, &mut best_newer, rt, required_major);
                            continue;
                        }
                        None => continue,
                    }
                }
                Err(_) => continue,
            }
        } else {
            root
        };

        if let Some(rt) = probe(&root, source) {
            remember(&mut best_exact, &mut best_newer, rt, required_major);
        }
    }

    log_info!(
        "mc::java",
        "selected runtime for Java {}: {:?}",
        required_major,
        best_exact.as_ref().map(|r| r.describe()).or_else(|| best_newer.as_ref().map(|r| r.describe()))
    );
    best_exact.or(best_newer)
}

fn remember(
    best_exact: &mut Option<JavaRuntime>,
    best_newer: &mut Option<JavaRuntime>,
    candidate: JavaRuntime,
    required_major: u32,
) {
    if candidate.major < required_major {
        return;
    }
    if candidate.major == required_major {
        if best_exact.as_ref().map(|b| candidate.major < b.major).unwrap_or(true) {
            *best_exact = Some(candidate);
        }
    } else if best_newer.as_ref().map(|b| candidate.major < b.major).unwrap_or(true) {
        *best_newer = Some(candidate);
    }
}

/// Adoptium download endpoint for a given feature version.
///
/// Uses the public redirect API; no key, no account, no tracking identifier.
pub fn temurin_url(required_major: u32) -> String {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };
    format!(
        "https://api.adoptium.net/v3/binary/latest/{required_major}/ga/{os}/{arch}/jre/hotspot/normal/eclipse"
    )
}

/// Unpack a downloaded JRE archive into `destination`.
pub async fn provision(
    http: &reqwest::Client,
    required_major: u32,
    managed_dir: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    on_progress: &(dyn Fn(u64, u64) + Sync),
) -> AppResult<JavaRuntime> {
    std::fs::create_dir_all(managed_dir)
        .map_err(|e| AppError::io("Не удалось создать папку Java", &e))?;

    let url = temurin_url(required_major);
    let response = http
        .get(&url)
        .send()
        .await
        .map_err(|e| {
            AppError::new(ErrorKind::Unreachable, "Не удалось скачать Java")
                .with_detail(e.to_string())
                .retryable(true)
        })?;
    if !response.status().is_success() {
        return Err(AppError::new(
            ErrorKind::Network,
            format!("Сервер Java вернул {}", response.status().as_u16()),
        )
        .with_detail(url)
        .retryable(true));
    }
    let total = response.content_length().unwrap_or(0);

    let archive_path = managed_dir.join(format!("temurin-{required_major}.tmp"));
    let mut file = tokio::fs::File::create(&archive_path)
        .await
        .map_err(|e| AppError::io("Не удалось создать временный файл Java", &e))?;
    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();
    let mut written = 0u64;
    while let Some(chunk) = stream.next().await {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(AppError::cancelled());
        }
        let chunk = chunk
            .map_err(|e| AppError::new(ErrorKind::Network, "Ошибка загрузки Java").with_detail(e.to_string()))?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|e| AppError::io("Ошибка записи файла Java", &e))?;
        written += chunk.len() as u64;
        on_progress(written, total);
    }
    tokio::io::AsyncWriteExt::flush(&mut file).await.ok();
    drop(file);

    let target = managed_dir.join(format!("temurin-{required_major}"));
    let _ = std::fs::remove_dir_all(&target);
    let extracted = unpack_archive(&archive_path, &target).await?;
    let _ = std::fs::remove_file(&archive_path);
    if !extracted {
        return Err(AppError::new(ErrorKind::Java, "Не удалось распаковать среду Java"));
    }

    // The archive usually contains a single top-level directory.
    let root = std::fs::read_dir(&target)
        .ok()
        .and_then(|mut entries| entries.next().and_then(|e| e.ok()))
        .map(|e| e.path())
        .filter(|p| p.join("bin").is_dir())
        .unwrap_or(target.clone());

    probe(&root, "managed").ok_or_else(|| {
        AppError::new(ErrorKind::Java, "Загруженная среда Java не распознана")
    })
}

async fn unpack_archive(archive: &Path, destination: &Path) -> AppResult<bool> {
    std::fs::create_dir_all(destination)
        .map_err(|e| AppError::io("Не удалось создать папку Java", &e))?;
    let file = std::fs::File::open(archive).map_err(|e| AppError::io("Не удалось открыть архив Java", &e))?;

    if cfg!(target_os = "windows") {
        // Adoptium serves a zip for Windows.
        let mut zip = zip::ZipArchive::new(file)
            .map_err(|e| AppError::new(ErrorKind::Java, "Повреждён архив Java").with_detail(e.to_string()))?;
        for i in 0..zip.len() {
            let mut entry = match zip.by_index(i) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let Some(relative) = entry.enclosed_name() else { continue };
            let is_dir = entry.is_dir();
            let target = destination.join(relative);
            if is_dir {
                let _ = std::fs::create_dir_all(&target);
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            if let Ok(mut out) = std::fs::File::create(&target) {
                std::io::copy(&mut entry, &mut out).ok();
            }
        }
        return Ok(true);
    }

    // macOS / Linux: tar.gz. Decompress with the `flate2` decompressor and
    // walk the tar manually would need another crate, so we shell out to the
    // system `tar`, which is present on every supported host.
    drop(file);
    let status = tokio::process::Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(destination)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| AppError::new(ErrorKind::Java, "Не найден tar для распаковки Java").with_detail(e.to_string()))?;
    Ok(status.status.success())
}

/// List everything we can see, for the settings screen.
pub fn list_all(managed_dir: &Path) -> Vec<JavaRuntime> {
    let mut out = Vec::new();
    for (root, source) in candidate_roots(None) {
        let root = if root == PathBuf::from("__managed__") {
            match std::fs::read_dir(managed_dir) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        if let Some(rt) = probe(&entry.path(), source) {
                            out.push(rt);
                        }
                    }
                    continue;
                }
                Err(_) => continue,
            }
        } else {
            root
        };
        if let Some(rt) = probe(&root, source) {
            out.push(rt);
        }
    }
    out.sort_by(|a, b| b.major.cmp(&a.major));
    out.dedup_by(|a, b| a.path == b.path);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_and_legacy_version_strings() {
        assert_eq!(parse_version("openjdk version \"21.0.3\" 2024-04-16"), Some((21, "21.0.3".into())));
        assert_eq!(
            parse_version("java version \"1.8.0_392\"\nJava(TM) SE Runtime Environment"),
            Some((8, "1.8.0_392".into()))
        );
        assert_eq!(parse_version("openjdk 17.0.9 2023-10-17"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn vendor_detection_is_best_effort() {
        assert_eq!(vendor_from("OpenJDK Runtime Environment (build 17)"), "OpenJDK");
        assert_eq!(vendor_from("Eclipse Temurin"), "Eclipse Temurin");
        assert_eq!(vendor_from("something else"), "Java");
    }

    #[test]
    fn adoptium_urls_are_https_and_platform_specific() {
        let url = temurin_url(17);
        assert!(url.starts_with("https://api.adoptium.net/v3/binary/latest/17/ga/"));
        assert!(!url.contains(' '));
    }

    #[test]
    fn runtime_root_normalisation_handles_bin_paths() {
        let root = normalise_root(PathBuf::from(r"C:\Program Files\Java\jre1.8.0\bin\java.exe"));
        assert_eq!(root, PathBuf::from(r"C:\Program Files\Java\jre1.8.0"));
    }

    #[test]
    fn discovery_never_returns_a_runtime_older_than_required() {
        // Simulate: nothing exists on this machine, so the result is None
        // rather than a crash or an exception.
        let empty = std::env::temp_dir().join("sifora-java-empty");
        let _ = std::fs::create_dir_all(&empty);
        if let Some(rt) = discover(8, Some("C:/definitely/not/here"), &empty) {
            assert!(rt.major >= 8);
        }
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn prefer_exact_match_over_newer_runtime() {
        let mut exact = None;
        let mut newer = None;
        let mk = |major: u32| JavaRuntime {
            path: PathBuf::from("x"),
            version: format!("{major}.0"),
            major,
            vendor: "test".into(),
            kind: RuntimeKind::Jre,
            source: "test",
        };
        remember(&mut exact, &mut newer, mk(21), 17);
        remember(&mut exact, &mut newer, mk(17), 17);
        remember(&mut exact, &mut newer, mk(8), 17);
        assert_eq!(exact.unwrap().major, 17);
        assert_eq!(newer.unwrap().major, 21);
    }
}