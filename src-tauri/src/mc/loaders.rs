//! Mod-loader discovery and installation.
//!
//! Three loaders, two mechanisms:
//!
//! * **Fabric** publishes a ready-made version document, so "installing" is
//!   fetching one JSON file and caching it beside the vanilla versions. No
//!   process starts and nothing can fail halfway.
//! * **Forge** and **NeoForge** only ship an installer jar that has to be run.
//!   Its output is a version document in `<game-data>/versions/`, so this
//!   module runs it once and then *looks at what appeared* rather than guessing
//!   the version id from its inputs — Forge appends the game version again for
//!   old releases (`1.7.10-10.13.4.1614-1.7.10`) and NeoForge uses a four-part
//!   number, and a wrong guess would silently launch the wrong profile.
//!
//! Discovery is honest about what does not exist: Fabric has no 1.12.2 release
//! and NeoForge has nothing before 1.20.2, so those answers come back as an
//! empty list plus a note, not as an error the user has to decode.

use std::path::Path;

use serde::Serialize;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::mc::download;
use crate::mc::instances::ModLoader;

const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";

/// How many loader versions the picker offers per loader.
const MAX_OPTIONS: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderOption {
    pub loader: ModLoader,
    /// The exact version string the loader's own metadata uses. This is what
    /// goes into a Maven path and into the profile id, so it is never
    /// reformatted.
    pub version: String,
    /// A short form for the picker: for Forge the part after the game version.
    pub label: String,
    pub stable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderCatalog {
    pub game_version: String,
    pub fabric: Vec<LoaderOption>,
    pub forge: Vec<LoaderOption>,
    pub neoforge: Vec<LoaderOption>,
    /// Plain-language reasons a loader offers nothing for this version.
    pub notes: Vec<String>,
}

/// The result of a successful install.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledLoader {
    pub loader: ModLoader,
    pub version: String,
    /// The Minecraft version id the build must now be launched as.
    pub version_id: String,
}

// ---------------------------------------------------------------------------
// Version-list helpers
// ---------------------------------------------------------------------------

/// Fetch JSON, treating "no such thing" as an empty result rather than an error.
///
/// Fabric answers `400` with `[]` for game versions it has never supported
/// (1.12.2, for instance). That is not a failure to report — it *is* the answer.
async fn get_json_or_none<T: for<'de> serde::Deserialize<'de>>(
    http: &reqwest::Client,
    url: &str,
) -> Option<T> {
    let response = http.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let text = response.text().await.ok()?;
    serde_json::from_str(&text).ok()
}

/// Every `<version>` value in a Maven `maven-metadata.xml`, in document order.
pub fn parse_maven_versions(xml: &str) -> Vec<String> {
    const OPEN: &str = "<version>";
    const CLOSE: &str = "</version";
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(OPEN) {
        let after = &rest[start + OPEN.len()..];
        let Some(end) = after.find(CLOSE) else { break };
        let value = after[..end].trim();
        if !value.is_empty() && value.len() <= 64 {
            out.push(value.to_string());
        }
        rest = &after[end + CLOSE.len()..];
    }
    out
}

/// Every run of digits in a version, in order.
///
/// Comparing these lexicographically is what puts `.10` above `.9` — Maven
/// emits versions in document order, which is not numeric. Runs are compared as
/// whole numbers, so `1.20.4` also beats `1.2.4`.
fn numeric_parts(version: &str) -> Vec<u64> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in version.chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else if !current.is_empty() {
            out.push(current.parse().unwrap_or(0));
            current.clear();
        }
    }
    if !current.is_empty() {
        out.push(current.parse().unwrap_or(0));
    }
    out
}

/// Newest first, judged numerically rather than by string order — Maven emits
/// `.9` before `.10` in document order and grouping by game version shuffles
/// releases of the same game version apart.
fn sort_newest_first(mut versions: Vec<String>) -> Vec<String> {
    versions.sort_by(|a, b| {
        numeric_parts(b)
            .cmp(&numeric_parts(a))
            .then_with(|| b.len().cmp(&a.len()))
            .then_with(|| b.cmp(a))
    });
    versions.truncate(MAX_OPTIONS);
    versions
}

/// The remainder of a NeoForge version after the part that mirrors the game
/// version: a build number, optionally zero-padded.
///
/// NeoForge numbers releases as `<game with the leading 1. dropped>.<build>`,
/// so game `1.21.1` is covered by `21.1.72` and game `26.3` by `26.3.0.94` —
/// the `0` is padding for the new four-part scheme. Accepting only a bare build
/// number or a `0.`-padded one is what keeps game `26.1` from being offered the
/// `26.1.2.*` releases that belong to game `26.1.2`.
fn is_neoforge_build(remainder: &str) -> bool {
    let trimmed = remainder.trim();
    // A single leading `0.` is padding for the four-part scheme.
    let body = trimmed.strip_prefix("0.").unwrap_or(trimmed);
    let digits: String = body.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 6 {
        return false;
    }
    // What follows the build number may only be a pre-release suffix.
    let rest = &body[digits.len()..];
    rest.is_empty() || rest.starts_with('-') || rest.starts_with('+')
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

pub async fn catalog(http: &reqwest::Client, game_version: &str) -> AppResult<LoaderCatalog> {
    let game = safe_component(game_version)?;
    let mut notes: Vec<String> = Vec::new();

    // ---- Fabric ----------------------------------------------------------
    #[derive(serde::Deserialize)]
    struct FabricEntry {
        loader: FabricLoader,
    }
    #[derive(serde::Deserialize)]
    struct FabricLoader {
        version: String,
        #[serde(default)]
        stable: bool,
    }

    let fabric = match get_json_or_none::<Vec<FabricEntry>>(
        http,
        &format!("{FABRIC_META}/versions/loader/{game}"),
    )
    .await
    {
        Some(entries) if !entries.is_empty() => entries
            .into_iter()
            .take(MAX_OPTIONS)
            .map(|e| LoaderOption {
                loader: ModLoader::Fabric,
                label: e.loader.version.clone(),
                version: e.loader.version,
                stable: e.loader.stable,
            })
            .collect(),
        _ => {
            notes.push(format!(
                "Fabric не поддерживает {game}: загрузчик появился только с версии 1.14"
            ));
            Vec::new()
        }
    };

    // ---- Forge -----------------------------------------------------------
    let forge = match download::fetch_text(
        http,
        &format!("{FORGE_MAVEN}/net/minecraftforge/forge/maven-metadata.xml"),
    )
    .await
    {
        Ok(xml) => {
            let prefix = format!("{game}-");
            let versions: Vec<String> = parse_maven_versions(&xml)
                .into_iter()
                .filter(|v| v.starts_with(&prefix))
                .collect();
            if versions.is_empty() {
                notes.push(format!("Forge не выпущен для {game}"));
            }
            sort_newest_first(versions)
                .into_iter()
                .map(|version| {
                    let label = version
                        .strip_prefix(&prefix)
                        .unwrap_or(&version)
                        // The legacy scheme repeats the game version.
                        .trim_end_matches(&format!("-{game}"))
                        .to_string();
                    LoaderOption { loader: ModLoader::Forge, version, label, stable: true }
                })
                .collect()
        }
        Err(err) => {
            notes.push(format!("Не удалось получить список Forge: {}", err.message));
            Vec::new()
        }
    };

    // ---- NeoForge --------------------------------------------------------
    let neoforge = match download::fetch_text(
        http,
        &format!("{NEOFORGE_MAVEN}/net/neoforged/neoforge/maven-metadata.xml"),
    )
    .await
    {
        Ok(xml) => {
            let stem = game.strip_prefix("1.").unwrap_or(&game).to_string();
            let stem_prefix = format!("{stem}.");
            let versions: Vec<(String, bool)> = parse_maven_versions(&xml)
                .into_iter()
                .filter(|v| v.starts_with(&stem_prefix))
                .filter(|v| is_neoforge_build(&v[stem_prefix.len()..]))
                .map(|v| {
                    let prerelease =
                        v.contains("alpha") || v.contains("beta") || v.contains("snapshot");
                    (v, !prerelease)
                })
                .collect();
            if versions.is_empty() {
                notes.push(format!(
                    "NeoForge не поддерживает {game}: загрузчик появился с версии 1.20.2"
                ));
            }
            let mut versions = versions;
            versions.sort_by(|a, b| {
                numeric_parts(b.0.as_str())
                    .cmp(&numeric_parts(a.0.as_str()))
                    .then_with(|| b.0.len().cmp(&a.0.len()))
            });
            versions.truncate(MAX_OPTIONS);
            versions
                .into_iter()
                .map(|(version, stable)| LoaderOption {
                    loader: ModLoader::NeoForge,
                    label: version.clone(),
                    version,
                    stable,
                })
                .collect()
        }
        Err(err) => {
            notes.push(format!("Не удалось получить список NeoForge: {}", err.message));
            Vec::new()
        }
    };

    Ok(LoaderCatalog { game_version: game, fabric, forge, neoforge, notes })
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

/// Fabric: write the published profile into the version cache.
pub async fn install_fabric(
    http: &reqwest::Client,
    game_data: &Path,
    game_version: &str,
    loader_version: &str,
) -> AppResult<InstalledLoader> {
    let game = safe_component(game_version)?;
    let loader = safe_component(loader_version)?;
    let fallback_id = format!("fabric-loader-{loader}-{game}");

    let url = format!("{FABRIC_META}/versions/loader/{game}/{loader}/profile/json");
    let text = download::fetch_text(http, &url)
        .await
        .map_err(|e| AppError::new(e.kind, "Не удалось получить профиль Fabric"))?;

    // The document names itself; trust the document over our own construction.
    #[derive(serde::Deserialize)]
    struct Profile {
        #[serde(default)]
        id: Option<String>,
    }
    let id = serde_json::from_str::<Profile>(&text)
        .ok()
        .and_then(|p| p.id)
        .filter(|id| !id.trim().is_empty())
        .unwrap_or(fallback_id);

    let dir = game_data.join("versions").join(&id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::io("Не удалось создать папку профиля Fabric", &e))?;
    crate::util::write_atomic(&dir.join(format!("{id}.json")), text.as_bytes())
        .map_err(|e| AppError::io("Не удалось сохранить профиль Fabric", &e))?;

    Ok(InstalledLoader {
        loader: ModLoader::Fabric,
        version: loader_version.to_string(),
        version_id: id,
    })
}

/// Installer-based loaders.
#[derive(Debug, Clone, Copy)]
enum Installer {
    Forge,
    NeoForge,
}

impl Installer {
    fn loader(self) -> ModLoader {
        match self {
            Installer::Forge => ModLoader::Forge,
            Installer::NeoForge => ModLoader::NeoForge,
        }
    }

    fn base(self) -> &'static str {
        match self {
            Installer::Forge => FORGE_MAVEN,
            Installer::NeoForge => NEOFORGE_MAVEN,
        }
    }

    fn group_path(self) -> &'static str {
        match self {
            Installer::Forge => "net/minecraftforge/forge",
            Installer::NeoForge => "net/neoforged/neoforge",
        }
    }

    /// The Maven artifact id, e.g. `1.20.1-47.2.0` or `26.3.0.94`. It is used
    /// verbatim: the catalogue stores exactly this string, so no guesswork is
    /// involved in rebuilding the URL.
    fn file_stem(self, artifact: &str) -> String {
        match self {
            Installer::Forge => format!("forge-{artifact}-installer"),
            Installer::NeoForge => format!("neoforge-{artifact}-installer"),
        }
    }

    fn installer_url(self, artifact: &str) -> String {
        format!(
            "{}/{}/{}/{}.jar",
            self.base(),
            self.group_path(),
            artifact,
            self.file_stem(artifact)
        )
    }
}

async fn install_with_installer(
    bulk_http: &reqwest::Client,
    game_data: &Path,
    java: &Path,
    game_version: &str,
    loader_version: &str,
    kind: Installer,
    sink: download::EventSink,
) -> AppResult<InstalledLoader> {
    let game = safe_component(game_version)?;
    let artifact = safe_component(loader_version)?;
    let url = kind.installer_url(&artifact);

    let dir = game_data.join("installers");
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::io("Не удалось создать папку установщиков", &e))?;
    let jar = dir.join(format!("{}.jar", kind.file_stem(&artifact)));

    let task = download::FileTask::new(format!("installer:{artifact}"), url, &jar);
    download::ensure_file(bulk_http, &task, &std::sync::atomic::AtomicBool::new(false), &sink)
        .await
        .map_err(|e| {
            AppError::new(
                e.kind,
                format!("Не удалось скачать установщик {}", kind.loader().label()),
            )
            .with_detail(e.detail.unwrap_or_default())
        })?;

    let before = version_ids(game_data);

    // The installer runs for minutes; keeping it off the async worker pool is
    // what stops a launch from stalling every other command.
    let target = game_data.to_path_buf();
    let java = java.to_path_buf();
    let jar_for_run = jar.clone();
    tokio::task::spawn_blocking(move || run_installer(&java, &jar_for_run, &target))
        .await
        .map_err(|e| {
            AppError::internal("Установщик загрузчика был прерван").with_detail(e.to_string())
        })??;

    let after = version_ids(game_data);
    let mut added: Vec<String> = after
        .iter()
        .filter(|id| !before.contains(*id))
        .cloned()
        .collect();
    added.sort();

    let version_id = match added.into_iter().next() {
        Some(id) => id,
        None => {
            // The profile may already exist from an install whose result was
            // never recorded (a crash between the installer and the save). Find
            // it rather than failing after a multi-minute run.
            let marker = kind.loader().label().to_ascii_lowercase();
            let game_lower = game.to_ascii_lowercase();
            let mut candidates: Vec<String> = after
                .into_iter()
                .filter(|id| {
                    let lower = id.to_ascii_lowercase();
                    lower.contains(&marker) && lower.contains(&game_lower)
                })
                .collect();
            candidates.sort();
            match candidates.pop() {
                Some(id) => id,
                None => {
                    return Err(AppError::new(
                        ErrorKind::Internal,
                        format!(
                            "Установщик {} завершился, но новой версии в профилях не появилось",
                            kind.loader().label()
                        ),
                    )
                    .with_detail(format!("{game} / {artifact}")));
                }
            }
        }
    };

    Ok(InstalledLoader {
        loader: kind.loader(),
        version: loader_version.to_string(),
        version_id,
    })
}

pub async fn install_forge(
    bulk_http: &reqwest::Client,
    game_data: &Path,
    java: &Path,
    game_version: &str,
    loader_version: &str,
    sink: download::EventSink,
) -> AppResult<InstalledLoader> {
    install_with_installer(
        bulk_http,
        game_data,
        java,
        game_version,
        loader_version,
        Installer::Forge,
        sink,
    )
    .await
}

pub async fn install_neoforge(
    bulk_http: &reqwest::Client,
    game_data: &Path,
    java: &Path,
    game_version: &str,
    loader_version: &str,
    sink: download::EventSink,
) -> AppResult<InstalledLoader> {
    install_with_installer(
        bulk_http,
        game_data,
        java,
        game_version,
        loader_version,
        Installer::NeoForge,
        sink,
    )
    .await
}

fn version_ids(game_data: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(game_data.join("versions")) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .collect()
}

/// Run a loader installer headlessly and wait for it.
///
/// The installer refuses to continue without a `launcher_profiles.json`, which
/// a vanilla run of Minecraft would have created — so one is written first.
/// The process is started with `CREATE_NO_WINDOW`, so no console flashes in
/// front of the user while a multi-minute install runs.
fn run_installer(java: &Path, jar: &Path, target: &Path) -> AppResult<()> {
    let profiles = target.join("launcher_profiles.json");
    if !profiles.is_file() {
        let seed = br#"{"profiles":{},"selectedProfile":"","clientToken":"","launcherVersion":{"name":"sifora","format":0}}"#;
        let _ = crate::util::write_atomic(&profiles, seed);
    }

    let mut command = std::process::Command::new(java);
    command
        .arg("-jar")
        .arg(jar)
        .arg("--installClient")
        .arg(target)
        .current_dir(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    crate::util::hide_console(&mut command);

    let output = command
        .output()
        .map_err(|e| AppError::io("Не удалось запустить установщик загрузчика", &e))?;

    if !output.status.success() {
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let lines: Vec<&str> = combined.lines().filter(|l| !l.trim().is_empty()).collect();
        let tail = lines[lines.len().saturating_sub(8)..].join("\n");
        return Err(AppError::new(
            ErrorKind::Internal,
            "Установщик загрузчика завершился с ошибкой",
        )
        .with_detail(if tail.is_empty() {
            format!("код {:?}", output.status.code())
        } else {
            tail
        }));
    }
    Ok(())
}

/// Loader versions and game versions both end up inside URLs and folder names.
pub fn safe_component(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    let ok = !trimmed.is_empty()
        && trimmed.len() <= 64
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'));
    if ok {
        Ok(trimmed.to_string())
    } else {
        Err(AppError::invalid("Некорректный номер версии"))
    }
}

impl ModLoader {
    /// Display name, used in messages.
    pub fn label(self) -> &'static str {
        match self {
            ModLoader::None => "Vanilla",
            ModLoader::Fabric => "Fabric",
            ModLoader::Forge => "Forge",
            ModLoader::NeoForge => "NeoForge",
            ModLoader::Quilt => "Quilt",
            ModLoader::OptiFine => "OptiFine",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maven_versions_are_read_in_order() {
        let xml = r#"<?xml version="1.0"?><metadata><versioning><versions>
            <version>1.21-51.0.33</version>
            <version>26.3-66.0.13</version>
        </versions></versioning></metadata>"#;
        assert_eq!(parse_maven_versions(xml), vec!["1.21-51.0.33", "26.3-66.0.13"]);
    }

    #[test]
    fn a_truncated_maven_document_does_not_panic() {
        // An unterminated element yields nothing rather than a bogus value.
        assert!(parse_maven_versions("<version>1.2").is_empty());
        assert_eq!(parse_maven_versions("<version>1.2</version"), vec!["1.2"]);
        assert!(parse_maven_versions("").is_empty());
    }

    /// Maven lists versions in document order, which puts `.9` before `.10` and
    /// interleaves different game versions — so the sort has to be numeric.
    #[test]
    fn versions_are_sorted_numerically_newest_first() {
        let sorted = sort_newest_first(vec![
            "1.21.1-52.0.9".into(),
            "1.21.1-52.0.10".into(),
            "1.21.1-52.1.2".into(),
        ]);
        assert_eq!(
            sorted,
            vec!["1.21.1-52.1.2", "1.21.1-52.0.10", "1.21.1-52.0.9"]
        );
    }

    #[test]
    fn prerelease_suffixes_do_not_break_numeric_sorting() {
        let sorted = sort_newest_first(vec![
            "26.3.0.9-beta".into(),
            "26.3.0.10".into(),
        ]);
        assert_eq!(sorted[0], "26.3.0.10");
    }

    /// The rule that stops game `26.1` being offered game `26.1.2`'s releases.
    #[test]
    fn neoforge_build_numbers_are_recognised_precisely() {
        // Bare build numbers, as used for the 1.20.2 – 1.21.x scheme.
        assert!(is_neoforge_build("72"));
        assert!(is_neoforge_build("263"));
        // Zero-padded build numbers, as used from the four-part scheme.
        assert!(is_neoforge_build("0.94"));
        assert!(is_neoforge_build("0.0-alpha.1+snapshot-1"));
        // A real game-version component, which must never be accepted.
        assert!(!is_neoforge_build("2.121"));
        assert!(!is_neoforge_build(""));
        assert!(!is_neoforge_build("beta"));
        assert!(!is_neoforge_build("123456789"));
    }

    #[test]
    fn neoforge_option_falls_out_of_the_game_version() {
        let catalog = |xml: &str, game: &str| {
            let stem = game.strip_prefix("1.").unwrap_or(game).to_string();
            let prefix = format!("{stem}.");
            parse_maven_versions(xml)
                .into_iter()
                .filter(|v| v.starts_with(&prefix))
                .filter(|v| is_neoforge_build(&v[prefix.len()..]))
                .collect::<Vec<_>>()
        };
        let xml = "<versions>\
            <version>21.1.72</version>\
            <version>26.1.2.121</version>\
            <version>26.3.0.94</version>\
            <version>20.4.253</version>\
            </versions>";
        assert_eq!(catalog(xml, "1.21.1"), vec!["21.1.72"]);
        assert_eq!(catalog(xml, "26.3"), vec!["26.3.0.94"]);
        assert_eq!(catalog(xml, "1.20.4"), vec!["20.4.253"]);
        // 26.1 must not be handed the releases that belong to 26.1.2.
        assert!(catalog(xml, "26.1").is_empty());
    }

    #[test]
    fn forge_urls_use_the_artifact_verbatim() {
        assert_eq!(
            Installer::Forge.installer_url("1.20.1-47.2.0"),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-installer.jar"
        );
        // The legacy scheme keeps its trailing game version, which is exactly
        // why the artifact is stored rather than rebuilt from parts.
        assert_eq!(
            Installer::Forge.installer_url("1.7.10-10.13.4.1614-1.7.10"),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10-installer.jar"
        );
    }

    #[test]
    fn neoforge_urls_use_the_artifact_verbatim() {
        assert_eq!(
            Installer::NeoForge.installer_url("26.3.0.94"),
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/26.3.0.94/neoforge-26.3.0.94-installer.jar"
        );
    }

    #[test]
    fn version_components_reject_path_characters() {
        assert!(safe_component("0.20.0").is_ok());
        assert!(safe_component("1.20.1-47.2.0").is_ok());
        assert!(safe_component("26.3.0.94").is_ok());
        assert!(safe_component("../../evil").is_err());
        assert!(safe_component("a/b").is_err());
        assert!(safe_component("").is_err());
    }
}
