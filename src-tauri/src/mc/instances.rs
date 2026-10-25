//! Builds — isolated, named game setups.
//!
//! A **build** (сборка) is what the user plays: a name, an icon, a Minecraft
//! version, an optional mod loader, and its own memory/window/JVM settings. Each
//! one gets its own directory under `<game-data>/instances/<slug>/` containing
//! `saves`, `config`, `resourcepacks`, `shaderpacks`, `mods`, `natives`,
//! `assets`, `logs`, `options.txt` and its own `build.json`. Nothing is shared
//! except the immutable download caches (libraries, asset objects, version
//! jars), which keeps builds small while guaranteeing that two builds can never
//! corrupt each other's worlds, mods or settings.
//!
//! ## Why the slug comes from the name and not the version
//!
//! Earlier this module keyed a directory on the Minecraft version, which meant
//! exactly one build per version. A build system needs the opposite: two
//! different builds on 1.12.2, one Vanilla and one Forge. The slug is therefore
//! derived from the build name, transliterated to ASCII so a Cyrillic name still
//! produces a readable folder, and de-duplicated with a numeric suffix.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::util;

pub const META_FILE: &str = "instance.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceState {
    /// Created, nothing downloaded yet.
    Empty,
    /// Partially downloaded; safe to resume.
    Partial,
    /// Fully installed and launchable.
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModLoader {
    #[default]
    None,
    Fabric,
    Forge,
    Quilt,
    NeoForge,
    OptiFine,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
pub struct ModProfile {
    pub loader: ModLoader,
    /// Loader version string as published by the loader's own metadata.
    pub loader_version: String,
    /// Legacy field, kept so build files written before loader support still
    /// deserialise (`deny_unknown_fields` would otherwise reject them).
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceMeta {
    pub id: String,
    pub slug: String,
    pub name: String,
    /// The Minecraft version this build is based on.
    pub version_id: String,
    /// The version document the game is actually launched with. Equal to
    /// `version_id` for a Vanilla build, and the loader profile id once a
    /// loader has been installed. `None` means "not resolved yet".
    #[serde(default)]
    pub launch_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub state: InstanceState,
    /// Own RAM limit, overriding the global default when set.
    pub ram_mb: Option<u32>,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    /// Window size requested by the game.
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub mods: ModProfile,
    /// File name of the build icon inside the app's `build-icons` folder.
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub last_played_at: Option<String>,
}

impl Default for InstanceMeta {
    fn default() -> Self {
        InstanceMeta {
            id: String::new(),
            slug: String::new(),
            name: String::new(),
            version_id: String::new(),
            launch_id: None,
            created_at: String::new(),
            updated_at: String::new(),
            state: InstanceState::Empty,
            ram_mb: None,
            jvm_args: Vec::new(),
            game_args: Vec::new(),
            width: None,
            height: None,
            mods: ModProfile::default(),
            icon: None,
            last_played_at: None,
        }
    }
}

/// Turn a version id into a filesystem-safe, collision-resistant slug.
pub fn slugify(version_id: &str) -> String {
    let mut base = String::with_capacity(version_id.len());
    let mut changed = false;
    for ch in version_id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
            base.push(ch);
        } else {
            base.push('_');
            changed = true;
        }
    }
    if base.is_empty() || base == "." || base == ".." {
        base.push_str("instance");
        changed = true;
    }
    if base.len() > 80 {
        base.truncate(80);
        changed = true;
    }
    if changed {
        base = format!("{base}-{:08x}", fnv(version_id));
    }
    base
}

/// Transliterate the few Cyrillic letters a Russian build name is likely to
/// use, so `Моя сборка` becomes `moya-sborka` rather than `_`.
fn transliterate(ch: char) -> Option<&'static str> {
    const TABLE: &[(char, &str)] = &[
        ('а', "a"), ('б', "b"), ('в', "v"), ('г', "g"), ('д', "d"), ('е', "e"),
        ('ё', "e"), ('ж', "zh"), ('з', "z"), ('и', "i"), ('й', "y"), ('к', "k"),
        ('л', "l"), ('м', "m"), ('н', "n"), ('о', "o"), ('п', "p"), ('р', "r"),
        ('с', "s"), ('т', "t"), ('у', "u"), ('ф', "f"), ('х', "h"), ('ц', "c"),
        ('ч', "ch"), ('ш', "sh"), ('щ', "sch"), ('ъ', ""), ('ы', "y"), ('ь', ""),
        ('э', "e"), ('ю', "yu"), ('я', "ya"),
    ];
    let lower: char = ch.to_lowercase().next().unwrap_or(ch);
    TABLE.iter().find(|(c, _)| *c == lower).map(|(_, latin)| *latin)
}

/// A readable, filesystem-safe base slug for a build name.
pub fn slug_base(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending_dash = false;
    for ch in name.chars() {
        let piece: String = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase().to_string()
        } else if let Some(latin) = transliterate(ch) {
            latin.to_string()
        } else {
            pending_dash = true;
            continue;
        };
        if pending_dash && !out.is_empty() {
            out.push('-');
        }
        pending_dash = false;
        out.push_str(&piece);
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > 60 {
        out.truncate(60);
        while out.ends_with('-') {
            out.pop();
        }
    }
    if out.is_empty() {
        out.push_str("build");
    }
    out
}

/// Directories every build gets. Created eagerly so a crash never leaves a
/// half-initialised game directory behind.
const SUBDIRS: &[&str] = &[
    "saves",
    "config",
    "mods",
    "resourcepacks",
    "shaderpacks",
    "screenshots",
    "logs",
    "resourcepacks_cache",
    "natives",
    "assets",
];

#[derive(Debug, Clone)]
pub struct InstancePaths {
    pub root: PathBuf,
    pub meta: PathBuf,
    pub natives: PathBuf,
    pub assets: PathBuf,
    pub mods: PathBuf,
}

pub fn paths(instances_root: &Path, slug: &str) -> InstancePaths {
    let root = instances_root.join(slug);
    InstancePaths {
        meta: root.join(META_FILE),
        natives: root.join("natives"),
        assets: root.join("assets"),
        mods: root.join("mods"),
        root,
    }
}

/// The record of which mods this build installed, and from where.
pub fn mods_manifest_path(p: &InstancePaths) -> PathBuf {
    p.root.join("mods.json")
}

pub fn load(path: &Path) -> AppResult<InstanceMeta> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| AppError::io("Не удалось прочитать данные сборки", &e))?;
    serde_json::from_str(&raw)
        .map_err(|e| AppError::config("Данные сборки повреждены").with_detail(e.to_string()))
}

pub fn save(path: &Path, meta: &InstanceMeta) -> AppResult<()> {
    let bytes = serde_json::to_vec_pretty(meta)
        .map_err(|e| AppError::config("Не удалось сериализовать сборку").with_detail(e.to_string()))?;
    util::write_atomic(path, &bytes)
        .map_err(|e| AppError::io("Не удалось сохранить сборку", &e))
}

/// Everything needed to create a build.
pub struct NewBuild<'a> {
    pub name: &'a str,
    pub version_id: &'a str,
    pub loader: ModLoader,
    pub loader_version: &'a str,
    pub ram_mb: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
}

/// Pick a slug nothing else is using.
fn free_slug(instances_root: &Path, base: &str) -> String {
    let taken = |candidate: &str| instances_root.join(candidate).join(META_FILE).exists();
    if !taken(base) {
        return base.to_string();
    }
    for n in 2..500u32 {
        let candidate = format!("{base}-{n}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    format!("{base}-{}", util::now_secs())
}

pub fn create(instances_root: &Path, req: NewBuild<'_>) -> AppResult<InstanceMeta> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::invalid("Укажите название сборки"));
    }
    let version_id = req.version_id.trim();
    if version_id.is_empty() {
        return Err(AppError::invalid("Выберите версию Minecraft"));
    }

    std::fs::create_dir_all(instances_root)
        .map_err(|e| AppError::io("Не удалось создать папку сборок", &e))?;
    let slug = free_slug(instances_root, &slug_base(name));
    let p = paths(instances_root, &slug);
    std::fs::create_dir_all(&p.root).map_err(|e| AppError::io("Не удалось создать папку сборки", &e))?;
    for dir in SUBDIRS {
        std::fs::create_dir_all(p.root.join(dir))
            .map_err(|e| AppError::io("Не удалось создать подпапку сборки", &e))?;
    }

    let now = util::now_rfc3339();
    let meta = InstanceMeta {
        id: format!("b-{}-{}-{:08x}", util::now_secs(), std::process::id(), fnv(&slug)),
        slug: slug.clone(),
        name: name.chars().take(48).collect(),
        version_id: version_id.to_string(),
        launch_id: None,
        created_at: now.clone(),
        updated_at: now,
        state: InstanceState::Empty,
        ram_mb: req.ram_mb.map(|v| v.clamp(512, 32_768)),
        jvm_args: req.jvm_args,
        game_args: req.game_args,
        width: req.width.map(|v| v.clamp(320, 7680)),
        height: req.height.map(|v| v.clamp(240, 4320)),
        mods: ModProfile {
            loader: req.loader,
            loader_version: req.loader_version.trim().to_string(),
            enabled: false,
        },
        icon: None,
        last_played_at: None,
    };
    save(&p.meta, &meta)?;
    Ok(meta)
}

/// Fields a user may change after creation. `None` leaves the value alone.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct BuildPatch {
    pub name: Option<String>,
    pub ram_mb: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub jvm_args: Option<Vec<String>>,
    pub game_args: Option<Vec<String>>,
}

pub fn update(instances_root: &Path, slug: &str, patch: &BuildPatch) -> AppResult<InstanceMeta> {
    let p = paths(instances_root, slug);
    let mut meta = load(&p.meta)?;
    if let Some(name) = patch.name.as_deref() {
        let cleaned = name.trim();
        if cleaned.is_empty() {
            return Err(AppError::invalid("Название сборки не может быть пустым"));
        }
        meta.name = cleaned.chars().take(48).collect();
    }
    if let Some(ram) = patch.ram_mb {
        meta.ram_mb = Some(ram.clamp(512, 32_768));
    }
    if let Some(w) = patch.width {
        meta.width = Some(w.clamp(320, 7680));
    }
    if let Some(h) = patch.height {
        meta.height = Some(h.clamp(240, 4320));
    }
    if let Some(args) = patch.jvm_args.as_ref() {
        meta.jvm_args = sanitise_args(args);
    }
    if let Some(args) = patch.game_args.as_ref() {
        meta.game_args = sanitise_args(args);
    }
    meta.updated_at = util::now_rfc3339();
    save(&p.meta, &meta)?;
    Ok(meta)
}

/// Arguments are passed to the game as a `Vec<String>`, never as a shell line,
/// but they are still bounded so a pasted blob cannot balloon the launch plan.
fn sanitise_args(args: &[String]) -> Vec<String> {
    args.iter()
        .map(|a| a.trim())
        .filter(|a| !a.is_empty() && a.len() <= 256 && !a.contains('\0'))
        .take(64)
        .map(|a| a.to_string())
        .collect()
}

/// List every build on disk, most recently played (then newest) first.
pub fn list(instances_root: &Path) -> Vec<InstanceMeta> {
    let Ok(entries) = std::fs::read_dir(instances_root) else { return Vec::new() };
    let mut out: Vec<InstanceMeta> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| load(&e.path().join(META_FILE)).ok())
        .collect();
    out.sort_by(|a, b| {
        let ka = a.last_played_at.as_deref().unwrap_or(&a.created_at);
        let kb = b.last_played_at.as_deref().unwrap_or(&b.created_at);
        kb.cmp(ka)
    });
    out
}

/// Deleting a build only removes its directory; the shared caches stay.
pub fn remove(instances_root: &Path, slug: &str) -> AppResult<()> {
    if slug != slugify(slug) || slug.is_empty() {
        return Err(AppError::invalid("Некорректный идентификатор сборки"));
    }
    let root = instances_root.join(slug);
    if !root.is_dir() {
        return Err(AppError::not_found("Сборка не найдена"));
    }
    // Defence in depth: the slug is already sanitised, but confirm the resolved
    // directory really sits inside the instances root before deleting anything.
    let contained = root
        .canonicalize()
        .ok()
        .zip(instances_root.canonicalize().ok())
        .map(|(real, base)| real.starts_with(&base))
        .unwrap_or(false);
    if !contained {
        return Err(AppError::invalid("Путь сборки выходит за пределы каталога"));
    }
    std::fs::remove_dir_all(&root).map_err(|e| AppError::io("Не удалось удалить сборку", &e))
}

pub fn set_state(p: &InstancePaths, state: InstanceState) -> AppResult<()> {
    let mut meta = load(&p.meta)?;
    meta.state = state;
    meta.updated_at = util::now_rfc3339();
    save(&p.meta, &meta)
}

/// Approximate on-disk size, shown on the builds screen.
pub fn disk_usage(p: &InstancePaths) -> u64 {
    fn walk(dir: &Path) -> u64 {
        let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
        entries
            .flatten()
            .map(|e| match e.file_type() {
                Ok(t) if t.is_dir() => walk(&e.path()),
                Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
                _ => 0,
            })
            .sum()
    }
    walk(&p.root)
}

fn fnv(text: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in text.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The renderer's `ModLoader` union mirrors these strings exactly, so the
    /// spelling is pinned rather than inferred from serde's rules.
    #[test]
    fn loader_names_serialise_the_way_the_ui_expects() {
        let cases = [
            (ModLoader::None, "none"),
            (ModLoader::Fabric, "fabric"),
            (ModLoader::Forge, "forge"),
            (ModLoader::Quilt, "quilt"),
            (ModLoader::NeoForge, "neo_forge"),
            (ModLoader::OptiFine, "opti_fine"),
        ];
        for (loader, expected) in cases {
            let json = serde_json::to_string(&loader).unwrap();
            assert_eq!(json, format!("\"{expected}\""));
            let back: ModLoader = serde_json::from_str(&json).unwrap();
            assert_eq!(back, loader);
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn request<'a>(name: &'a str, version: &'a str) -> NewBuild<'a> {
        NewBuild {
            name,
            version_id: version,
            loader: ModLoader::None,
            loader_version: "",
            ram_mb: None,
            width: None,
            height: None,
            jvm_args: Vec::new(),
            game_args: Vec::new(),
        }
    }

    #[test]
    fn plain_version_ids_survive_slugify_unchanged() {
        assert_eq!(slugify("1.20.4"), "1.20.4");
        assert_eq!(slugify("24w14potato"), "24w14potato");
    }

    #[test]
    fn unsafe_version_ids_get_a_hash_suffix() {
        let a = slugify("../etc/passwd");
        assert!(!a.contains('/'));
        assert!(!a.contains('\\'));
        assert_ne!(a, slugify("../../other"));
    }

    #[test]
    fn build_names_become_readable_slugs() {
        assert_eq!(slug_base("Моя сборка"), "moya-sborka");
        assert_eq!(slug_base("Fabric 1.21.1"), "fabric-1-21-1");
        assert_eq!(slug_base("  ...  "), "build");
        assert_eq!(slug_base("Привет, мир!"), "privet-mir");
        assert!(slug_base(&"x".repeat(200)).len() <= 60);
    }

    /// The whole point of the builds rework: two builds on one version.
    #[test]
    fn two_builds_can_share_a_version() {
        let root = temp("sifora-builds-two");
        let vanilla = create(&root, request("Vanilla survival", "1.12.2")).unwrap();
        let modded = create(&root, request("Modded forge", "1.12.2")).unwrap();

        assert_ne!(vanilla.slug, modded.slug);
        assert_eq!(vanilla.version_id, "1.12.2");
        assert_eq!(modded.version_id, "1.12.2");
        assert_eq!(list(&root).len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn identical_names_get_distinct_slugs() {
        let root = temp("sifora-builds-dup");
        let first = create(&root, request("Test", "1.20.1")).unwrap();
        let second = create(&root, request("Test", "1.20.1")).unwrap();
        assert_eq!(first.slug, "test");
        assert_eq!(second.slug, "test-2");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn create_makes_the_full_skeleton_and_survives_a_reload() {
        let root = temp("sifora-builds-skeleton");
        let mut req = request("Skyblock", "1.20.4");
        req.loader = ModLoader::Fabric;
        req.loader_version = "0.20.0";
        req.ram_mb = Some(4096);
        let meta = create(&root, req).unwrap();

        for dir in SUBDIRS {
            assert!(root.join(&meta.slug).join(dir).is_dir(), "{dir} missing");
        }
        let reloaded = load(&paths(&root, &meta.slug).meta).unwrap();
        assert_eq!(reloaded.name, "Skyblock");
        assert_eq!(reloaded.mods.loader, ModLoader::Fabric);
        assert_eq!(reloaded.mods.loader_version, "0.20.0");
        assert_eq!(reloaded.ram_mb, Some(4096));
        assert_eq!(reloaded.launch_id, None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_names_and_versions_are_refused() {
        let root = temp("sifora-builds-invalid");
        assert!(create(&root, request("   ", "1.20.1")).is_err());
        assert!(create(&root, request("Ok", "  ")).is_err());
        assert!(list(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn patches_only_touch_what_they_name() {
        let root = temp("sifora-builds-patch");
        let meta = create(&root, request("Patch me", "1.20.1")).unwrap();
        let patch = BuildPatch {
            name: Some("Renamed".into()),
            ram_mb: Some(6144),
            ..BuildPatch::default()
        };
        let updated = update(&root, &meta.slug, &patch).unwrap();
        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.ram_mb, Some(6144));
        assert_eq!(updated.version_id, "1.20.1", "the version must not move");
        assert_eq!(updated.id, meta.id);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn blank_player_arguments_are_dropped() {
        let root = temp("sifora-builds-args");
        let meta = create(&root, request("Args", "1.20.1")).unwrap();
        let patch = BuildPatch {
            jvm_args: Some(vec!["  -Xss2M ".into(), "".into(), "   ".into()]),
            ..BuildPatch::default()
        };
        let updated = update(&root, &meta.slug, &patch).unwrap();
        assert_eq!(updated.jvm_args, vec!["-Xss2M".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn state_transitions_persist() {
        let root = temp("sifora-builds-state");
        let meta = create(&root, request("State", "1.8.9")).unwrap();
        let p = paths(&root, &meta.slug);
        set_state(&p, InstanceState::Ready).unwrap();
        assert_eq!(load(&p.meta).unwrap().state, InstanceState::Ready);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn removal_refuses_to_escape_the_instances_root() {
        let root = temp("sifora-builds-rm");
        let meta = create(&root, request("Delete me", "1.20.4")).unwrap();
        remove(&root, &meta.slug).unwrap();
        assert!(!root.join(&meta.slug).exists());
        assert!(remove(&root, &meta.slug).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn metadata_with_unknown_fields_is_rejected() {
        let root = temp("sifora-builds-schema");
        let meta = create(&root, request("Schema", "1.20.4")).unwrap();
        let p = paths(&root, &meta.slug);
        std::fs::write(&p.meta, br#"{"id":"x","__proto__":{"a":1}}"#).unwrap();
        assert!(load(&p.meta).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Build files written before loaders, icons and `launch_id` existed must
    /// keep loading — `deny_unknown_fields` only guards against *invented*
    /// fields, not against a build that predates a feature.
    #[test]
    fn an_older_build_file_still_loads() {
        let root = temp("sifora-builds-migrate");
        let dir = root.join("legacy");
        std::fs::create_dir_all(&dir).unwrap();
        let legacy = r#"{
            "id": "i-1", "slug": "legacy", "name": "Legacy", "version_id": "1.12.2",
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
            "state": "ready", "ram_mb": null, "jvm_args": [], "game_args": [],
            "width": null, "height": null,
            "mods": { "loader": "none", "loader_version": "", "enabled": false }
        }"#;
        std::fs::write(dir.join(META_FILE), legacy).unwrap();
        let loaded = load(&dir.join(META_FILE)).unwrap();
        assert_eq!(loaded.version_id, "1.12.2");
        assert_eq!(loaded.launch_id, None);
        assert_eq!(loaded.icon, None);
        assert_eq!(loaded.last_played_at, None);
        assert_eq!(list(&root).len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
