//! The per-version metadata document (`https://launchermeta.mojang.com/.../<id>.json`).
//!
//! The schema has drifted several times over Minecraft's history, so the types
//! here keep *every* legacy shape:
//!
//! * `minecraftArguments` (pre-1.13) alongside `arguments` (1.13+),
//! * `assets` string alongside `assetIndex`,
//! * libraries with and without explicit `downloads.artifact`,
//! * optional `javaVersion`.
//!
//! Anything missing simply stays `None`; the launcher decides what is
//! mandatory when it builds a launch plan.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// Architecture token used inside `${arch}` native classifier patterns.
pub fn arch_token(platform: Platform) -> &'static str {
    match platform {
        Platform::Windows => "64",
        Platform::MacOs | Platform::Linux => {
            if cfg!(target_arch = "aarch64") {
                "arm64"
            } else {
                "64"
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionMeta {
    pub id: String,
    #[serde(rename = "type")]
    pub release_type: Option<String>,
    #[serde(rename = "mainClass")]
    pub main_class: Option<String>,
    #[serde(rename = "minecraftArguments", default)]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(rename = "assetIndex", default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub downloads: Option<Downloads>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(rename = "javaVersion", default)]
    pub java_version: Option<JavaVersion>,
    #[serde(rename = "inheritsFrom", default)]
    pub inherits_from: Option<String>,
    #[serde(rename = "releaseTime", default)]
    pub release_time: Option<String>,
    #[serde(rename = "complianceLevel", default)]
    pub compliance_level: Option<u32>,
    #[serde(rename = "minimumLauncherVersion", default)]
    pub minimum_launcher_version: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional {
        #[serde(default)]
        rules: Vec<Rule>,
        value: ArgumentValue,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    Single(String),
    Many(Vec<String>),
}

impl ArgumentValue {
    pub fn into_vec(self) -> Vec<String> {
        match self {
            ArgumentValue::Single(s) => vec![s],
            ArgumentValue::Many(v) => v,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Rule {
    pub action: RuleAction,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<serde_json::Value>,
}

impl Rule {
    /// A rule is satisfied when *any* of its features is enabled (all features
    /// must be active for a single object; Mojang emits one object per feature).
    pub fn matches(&self, platform: Platform, feature_flags: &[&str]) -> bool {
        if let Some(os) = &self.os {
            if !os.matches(platform) {
                return false;
            }
        }
        match &self.features {
            None => true,
            Some(features) => match features.as_object() {
                Some(map) => map.iter().all(|(key, value)| {
                    let enabled = value.as_bool().unwrap_or(false);
                    let is_on = feature_flags.contains(&key.as_str());
                    enabled == is_on
                }),
                None => false,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    Allow,
    Disallow,
}

/// The `os` part of a Mojang rule.
///
/// Every field is optional in the real data. Mojang ships rules that constrain
/// only the architecture (`{"arch": "x86"}`) and rules that constrain only the
/// platform (`{"name": "windows"}`), so requiring `name` made the whole version
/// document fail to parse. `versionRange` (Mojang's newer structured build
/// range) is read but not evaluated, and only ever appears inside the
/// `arguments.default-user-jvm` block, which Sifora does not consume.
#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOs,
    Linux,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else {
            Platform::Linux
        }
    }

    fn os_name(self) -> &'static str {
        match self {
            Platform::Windows => "windows",
            Platform::MacOs => "osx",
            Platform::Linux => "linux",
        }
    }
}

/// The host CPU architecture as Mojang spells it in `os.arch`.
///
/// This used to answer `"x86"` for every Windows host, which made the
/// 32-bit-only rule `{"arch": "x86"}` match on 64-bit machines and inject the
/// wrong JVM arguments.
fn current_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "x86_64"
    }
}

impl OsRule {
    pub fn matches(&self, platform: Platform) -> bool {
        if let Some(name) = &self.name {
            if !name.eq_ignore_ascii_case(platform.os_name()) {
                return false;
            }
        }
        if let Some(arch) = &self.arch {
            if !arch.is_empty() && !arch.eq_ignore_ascii_case(current_arch()) {
                return false;
            }
        }
        if let Some(version) = &self.version {
            if !os_version_matches(version) {
                return false;
            }
        }
        true
    }
}

/// Match the `os.version` part of a Mojang rule.
///
/// Mojang only ever emits two shapes: `*` (any) and an anchored regular
/// expression such as `^10\.` (Windows 10+), plus an OSX variant. Only Windows
/// version patterns affect behaviour in practice, and both Windows 10 and
/// Windows 11 report major version 10 through the same API, so the pattern is
/// evaluated against that. Non-Windows patterns are accepted rather than
/// guessed at, because every non-Windows rule Mojang ships today guards
/// cosmetic arguments.
pub fn os_version_matches(pattern: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern == "*" {
        return true;
    }
    if !cfg!(target_os = "windows") {
        return true;
    }
    anchored_prefix_match(pattern, "10.0")
}

/// Minimal matcher for the `^literal\.` style patterns Mojang uses.
///
/// Supports an optional leading `^`, literal characters and `\.` escapes — and
/// nothing else. Anything more exotic fails closed, so an unknown pattern can
/// never silently enable an argument.
fn anchored_prefix_match(pattern: &str, value: &str) -> bool {
    let bytes = pattern.as_bytes();
    let target = value.as_bytes();
    if bytes.first() != Some(&b'^') {
        return pattern == value;
    }
    let mut i = 1usize;
    let mut v = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 1;
                match bytes.get(i) {
                    Some(expected) => {
                        if target.get(v).copied() != Some(*expected) {
                            return false;
                        }
                        v += 1;
                        i += 1;
                    }
                    None => return false,
                }
            }
            expected => {
                if target.get(v).copied() != Some(expected) {
                    return false;
                }
                v += 1;
                i += 1;
            }
        }
    }
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
    #[serde(rename = "totalSize", default)]
    pub total_size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Downloads {
    pub client: Option<FileMeta>,
    pub server: Option<FileMeta>,
    #[serde(rename = "client_mappings", default)]
    pub client_mappings: Option<FileMeta>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileMeta {
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub natives: Option<Natives>,
    #[serde(default)]
    pub rules: Option<Vec<Rule>>,
    /// Maven coordinates for libraries that ship without explicit downloads.
    #[serde(rename = "extract", default)]
    pub _extract: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<FileMeta>,
    pub classifiers: Option<std::collections::HashMap<String, FileMeta>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Natives {
    #[serde(default)]
    pub linux: Option<NativeSpec>,
    #[serde(default)]
    pub windows: Option<NativeSpec>,
    #[serde(default)]
    pub osx: Option<NativeSpec>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum NativeSpec {
    /// `"natives": "natives-windows"` — one classifier for every platform.
    Replacement(String),
    /// `"natives": { "windows": "natives-windows-${arch}", ... }`
    Map(std::collections::HashMap<String, String>),
}

/// Pick the native specification that applies to this platform.
pub fn natives_for_platform<'a>(natives: &'a Natives, platform: Platform) -> Option<&'a NativeSpec> {
    match platform {
        Platform::Windows => natives.windows.as_ref(),
        Platform::MacOs => natives.osx.as_ref(),
        Platform::Linux => natives.linux.as_ref(),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JavaVersion {
    #[serde(rename = "component")]
    pub _component: String,
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

impl VersionMeta {
    pub fn parse(raw: &str) -> AppResult<Self> {
        serde_json::from_str(raw).map_err(|e| {
            // `detail` is never shown to the user, so mirror it into the log
            // bus. Without this a malformed document is undiagnosable.
            crate::logbus::error(
                "version",
                format!("не удалось разобрать метаданные версии: {e}"),
            );
            AppError::config("Метаданные версии повреждены").with_detail(e.to_string())
        })
    }

    /// Java feature version this build expects, defaulting to 8 for the many
    /// pre-1.17 documents that omit it.
    pub fn required_java(&self) -> u32 {
        self.java_version.as_ref().map(|j| j.major_version).unwrap_or(8)
    }

    pub fn asset_index_id(&self) -> Option<&str> {
        self.asset_index.as_ref().map(|a| a.id.as_str()).or(self.assets.as_deref())
    }
}

/// Merged view of a version and everything it inherits from.
#[derive(Debug, Clone)]
pub struct ResolvedVersion {
    pub id: String,
    pub main_class: String,
    pub libraries: Vec<Library>,
    pub arguments: Arguments,
    pub minecraft_arguments: Option<String>,
    pub asset_index: Option<AssetIndexRef>,
    pub client: Option<FileMeta>,
    pub java_major: u32,
    pub release_time: Option<String>,
    pub release_type: String,
    pub chain: Vec<String>,
}

impl ResolvedVersion {
    pub fn resolve(id: &str, chain: &[VersionMeta]) -> AppResult<Self> {
        let mut libraries: Vec<Library> = Vec::new();
        let mut arguments = Arguments::default();
        let mut minecraft_arguments = None;
        let mut asset_index = None;
        let mut client = None;
        let mut main_class = None;
        let mut java_major = 8u32;
        let mut release_time = None;
        let mut release_type = String::new();
        let mut seen: Vec<String> = Vec::new();

        // `chain` is ordered oldest ancestor first.
        for meta in chain.iter().chain(std::iter::once(&chain[0])) {
            let _ = &meta;
            break;
        }

        for meta in chain {
            if seen.contains(&meta.id) {
                continue;
            }
            seen.push(meta.id.clone());
            if meta.main_class.is_some() {
                main_class = meta.main_class.clone();
            }
            if let Some(a) = &meta.arguments {
                if !a.game.is_empty() {
                    arguments.game = a.game.clone();
                }
                if !a.jvm.is_empty() {
                    arguments.jvm = a.jvm.clone();
                }
            }
            if meta.minecraft_arguments.is_some() {
                minecraft_arguments = meta.minecraft_arguments.clone();
            }
            if meta.asset_index.is_some() {
                asset_index = meta.asset_index.clone();
            } else if meta.assets.is_some() && asset_index.is_none() {
                // Legacy `assets: "legacy"` without an index document: the index
                // id still matters for the `--assetsIndex` argument.
                asset_index = None;
            }
            if let Some(d) = &meta.downloads {
                if d.client.is_some() {
                    client = d.client.clone();
                }
            }
            if meta.java_version.is_some() {
                java_major = meta.java_version.as_ref().map(|j| j.major_version).unwrap_or(8);
            }
            if meta.release_time.is_some() {
                release_time = meta.release_time.clone();
            }
            if meta.release_type.is_some() {
                release_type = meta.release_type.clone().unwrap_or_default();
            }
            for lib in &meta.libraries {
                libraries.retain(|existing| !same_artifact(existing, lib));
                libraries.push(lib.clone());
            }
        }

        let main_class = main_class.ok_or_else(|| {
            AppError::unsupported(format!("У версии {id} отсутствует главный класс"))
                .with_detail("документ версии не содержит mainClass")
        })?;

        Ok(ResolvedVersion {
            id: id.to_string(),
            main_class,
            libraries,
            arguments,
            minecraft_arguments,
            asset_index,
            client,
            java_major,
            release_time,
            release_type,
            chain: seen,
        })
    }
}

/// Two libraries are "the same artifact" when they share `group:artifact` *and
/// the classifier*.
///
/// A child document (Forge, Fabric, …) that bumps the version must replace the
/// parent's entry rather than adding a second copy to the classpath. The
/// classifier has to be part of the key: `org.lwjgl:lwjgl:3.3.1` and
/// `org.lwjgl:lwjgl:3.3.1:natives-windows` are different files, and collapsing
/// them silently dropped every LWJGL core jar while keeping one native.
fn same_artifact(a: &Library, b: &Library) -> bool {
    fn key(lib: &Library) -> Option<(&str, &str, Option<&str>)> {
        let name = lib.name.split('@').next().unwrap_or(&lib.name);
        let mut parts = name.split(':');
        let group = parts.next()?;
        let artifact = parts.next()?;
        // The version is deliberately skipped; the classifier is not.
        parts.next()?;
        let classifier = parts.next();
        Some((group, artifact, classifier))
    }
    key(a).is_some() && key(a) == key(b)
}

#[derive(Debug, Clone, Serialize)]
pub struct ParsedCoordinate {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub classifier: Option<String>,
    pub extension: String,
    /// `group/artifact/version/artifact-version[-classifier].ext`
    pub file_name: String,
    pub relative_path: String,
}

/// Parse a Maven coordinate: `group:artifact:version[:classifier][@ext]`.
pub fn parse_coordinate(name: &str) -> Option<ParsedCoordinate> {
    let (coords, extension) = match name.split_once('@') {
        Some((c, e)) => (c, e.to_ascii_lowercase()),
        None => (name, "jar".to_string()),
    };
    let parts: Vec<&str> = coords.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0];
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).map(|s| s.to_string());
    if group.is_empty() || artifact.is_empty() || version.is_empty() {
        return None;
    }
    let file_name = match &classifier {
        Some(c) => format!("{artifact}-{version}-{c}.{extension}"),
        None => format!("{artifact}-{version}.{extension}"),
    };
    Some(ParsedCoordinate {
        group: group.to_string(),
        artifact: artifact.to_string(),
        version: version.to_string(),
        classifier,
        extension,
        relative_path: format!("{}/{}/{}/{}", group.replace('.', "/"), artifact, version, file_name),
        file_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(json: &str) -> VersionMeta {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn parses_modern_arguments_document() {
        let m = meta(
            r#"{"id":"1.20.4","type":"release","mainClass":"net.minecraft.client.main.Main",
            "arguments":{"game":["--demo"],"jvm":["-Djava.library.path=${natives_directory}"]},
            "libraries":[],"downloads":{"client":{"url":"u","sha1":"s","size":1}},
            "assetIndex":{"id":"12","sha1":"s","size":1,"url":"u","totalSize":10},
            "javaVersion":{"component":"java-runtime-gamma","majorVersion":17}}"#,
        );
        assert_eq!(m.main_class.as_deref(), Some("net.minecraft.client.main.Main"));
        assert_eq!(m.required_java(), 17);
        assert_eq!(m.asset_index_id(), Some("12"));
        assert_eq!(m.downloads.unwrap().client.unwrap().size, 1);
    }

    #[test]
    fn parses_legacy_arguments_document() {
        let m = meta(
            r#"{"id":"b1.7.3","type":"old_beta","mainClass":"net.minecraft.launchwrapper.Launch",
            "minecraftArguments":"${auth_player_name} --gameDir ${game_directory}",
            "libraries":[],"downloads":{"client":{"url":"u","sha1":"s","size":1}},
            "assets":"pre-1.6","javaVersion":{"component":"jre-legacy","majorVersion":8}}"#,
        );
        assert!(m.arguments.is_none());
        assert_eq!(m.asset_index_id(), Some("pre-1.6"));
        assert_eq!(m.required_java(), 8);
    }

    #[test]
    fn missing_java_version_defaults_to_eight() {
        let m = meta(r#"{"id":"a1.2.6","mainClass":"x","libraries":[]}"#);
        assert_eq!(m.required_java(), 8);
    }

    #[test]
    fn conditional_arguments_deserialise() {
        let m = meta(
            r#"{"id":"1.21","mainClass":"x","libraries":[],"arguments":{"game":[
              "--always",
              {"rules":[{"action":"allow","features":{"is_demo_user":true}}],"value":"--demo"},
              {"rules":[{"action":"allow","os":{"name":"osx"}}],"value":["-XstartOnFirstThread"]}
            ]}}"#,
        );
        let game = m.arguments.unwrap().game;
        assert_eq!(game.len(), 3);
        match &game[1] {
            Argument::Conditional { rules, value } => {
                assert_eq!(rules.len(), 1);
                assert!(!rules[0].matches(Platform::Windows, &[]));
                assert!(rules[0].matches(Platform::Windows, &["is_demo_user"]));
                assert_eq!(value.clone().into_vec(), vec!["--demo".to_string()]);
            }
            _ => panic!("expected conditional"),
        }
    }

    #[test]
    fn os_rules_only_match_their_platform() {
        let osx = OsRule { name: Some("osx".into()), version: None, arch: None };
        let windows =
            OsRule { name: Some("windows".into()), version: Some("*".into()), arch: None };
        assert!(osx.matches(Platform::MacOs));
        assert!(!osx.matches(Platform::Windows));
        assert!(windows.matches(Platform::Windows));
        assert!(!windows.matches(Platform::Linux));
    }

    /// Regression: Mojang ships rules whose `os` object carries only `arch`.
    /// A required `name` made `VersionMeta::parse` reject the entire version
    /// document, so the launcher answered "Метаданные версии повреждены" and
    /// no version could ever be installed or started.
    #[test]
    fn an_os_rule_without_a_name_still_parses() {
        let meta = meta(
            r#"{"id":"26.3","mainClass":"net.minecraft.client.main.Main",
                "arguments":{"jvm":[
                    {"rules":[{"action":"allow","os":{"arch":"x86"}}],"value":"-Xss1M"},
                    {"rules":[{"action":"allow","os":{"name":"windows"}}],"value":"-Dwin=true"}
                ]}}"#,
        );
        let jvm = meta.arguments.unwrap().jvm;
        assert_eq!(jvm.len(), 2);

        // An arch-only rule must not constrain the platform: it depends on the
        // host architecture alone, and never rejects a platform by name.
        let arch_only = match &jvm[0] {
            Argument::Conditional { rules, .. } => rules[0].clone(),
            _ => panic!("expected conditional"),
        };
        let x86_host = cfg!(target_arch = "x86");
        assert_eq!(arch_only.matches(Platform::Windows, &[]), x86_host);
        assert_eq!(arch_only.matches(Platform::Linux, &[]), x86_host);

        // A name-only rule must still filter platforms.
        let windows_only = match &jvm[1] {
            Argument::Conditional { rules, .. } => rules[0].clone(),
            _ => panic!("expected conditional"),
        };
        assert!(windows_only.matches(Platform::Windows, &[]));
        assert!(!windows_only.matches(Platform::MacOs, &[]));
    }

    #[test]
    fn inheritance_merges_libraries_with_child_priority() {
        let parent = meta(
            r#"{"id":"1.7.10","mainClass":"Parent","libraries":[{"name":"a:b:1"}],
               "assetIndex":{"id":"1.7.10","sha1":"s","size":1,"url":"u"},"javaVersion":{"component":"c","majorVersion":8}}"#,
        );
        let child = meta(
            r#"{"id":"forge","inheritsFrom":"1.7.10","mainClass":"Child",
               "libraries":[{"name":"a:b:2"},{"name":"c:d:1"}]}"#,
        );
        let resolved = ResolvedVersion::resolve("forge", &[parent, child]).unwrap();
        assert_eq!(resolved.main_class, "Child");
        assert_eq!(resolved.java_major, 8);
        assert_eq!(resolved.libraries.len(), 2);
        assert!(resolved.libraries.iter().any(|l| l.name == "a:b:2"));
        assert!(!resolved.libraries.iter().any(|l| l.name == "a:b:1"));
    }

    /// A native is a *different artifact*, not a newer version of the core jar.
    ///
    /// Keying the merge on `group:artifact` alone collapsed
    /// `org.lwjgl:lwjgl:3.3.1` and its six `natives-*` siblings into the last
    /// one seen, which silently removed every LWJGL core jar from the classpath
    /// and left the game unable to load `MemoryUtil`.
    #[test]
    fn a_native_does_not_replace_the_core_jar() {
        let m = meta(
            r#"{"id":"1.20.1","mainClass":"Main","libraries":[
                 {"name":"org.lwjgl:lwjgl:3.3.1"},
                 {"name":"org.lwjgl:lwjgl:3.3.1:natives-windows"},
                 {"name":"org.lwjgl:lwjgl:3.3.1:natives-windows-arm64"},
                 {"name":"org.lwjgl:lwjgl:3.3.1:natives-windows-x86"}]}"#,
        );
        let resolved = ResolvedVersion::resolve("1.20.1", &[m]).unwrap();
        assert_eq!(resolved.libraries.len(), 4, "all four entries must survive");
        assert!(resolved.libraries.iter().any(|l| l.name == "org.lwjgl:lwjgl:3.3.1"));
        assert!(resolved
            .libraries
            .iter()
            .any(|l| l.name == "org.lwjgl:lwjgl:3.3.1:natives-windows"));
    }

    /// The child still wins when it bumps the *same* classifier.
    #[test]
    fn a_child_replaces_the_same_coordinate_only() {
        let parent = meta(
            r#"{"id":"p","mainClass":"P","libraries":[
                 {"name":"g:a:1"},{"name":"g:a:1:natives-windows"}]}"#,
        );
        let child = meta(
            r#"{"id":"c","inheritsFrom":"p","mainClass":"C",
               "libraries":[{"name":"g:a:2"}]}"#,
        );
        let resolved = ResolvedVersion::resolve("c", &[parent, child]).unwrap();
        assert!(resolved.libraries.iter().any(|l| l.name == "g:a:2"));
        assert!(resolved.libraries.iter().any(|l| l.name == "g:a:1:natives-windows"));
        assert!(!resolved.libraries.iter().any(|l| l.name == "g:a:1"));
    }

    #[test]
    fn resolve_without_main_class_is_unsupported() {
        let m = meta(r#"{"id":"broken","libraries":[]}"#);
        let err = ResolvedVersion::resolve("broken", &[m]).unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::Unsupported);
    }

    #[test]
    fn maven_coordinates_parse() {
        let c = parse_coordinate("net.minecraft:launchwrapper:1.5").unwrap();
        assert_eq!(c.relative_path, "net/minecraft/launchwrapper/1.5/launchwrapper-1.5.jar");
        let n = parse_coordinate("org.lwjgl:lwjgl-platform:3.3.3:natives-windows").unwrap();
        assert_eq!(n.classifier.as_deref(), Some("natives-windows"));
        assert!(n.file_name.contains("natives-windows"));
        assert!(parse_coordinate("broken").is_none());
    }

    #[test]
    fn asset_index_reference_survives_missing_field() {
        let m = meta(r#"{"id":"x","mainClass":"y","libraries":[]}"#);
        assert!(m.asset_index.is_none());
        assert!(m.asset_index_id().is_none());
    }
}