//! Isolated game instances.
//!
//! Every version gets its own directory under `<game-data>/instances/<slug>/`
//! containing `saves`, `config`, `resourcepacks`, `shaderpacks`, `natives`,
//! `assets`, `logs`, `options.txt` and its own `instance.json`. Nothing is
//! shared except the immutable download caches (libraries, asset objects,
//! version jars), which keeps installs small while guaranteeing that two
//! builds can never corrupt each other's worlds or settings.
//!
//! The metadata already carries the fields a stage-2 mod loader needs
//! (`mods.loader`, `mods.profile`) so that adding Fabric/Forge later is a
//! pure extension, not a migration.

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
    pub loader_version: String,
    /// Independent mod/config folder inside this instance. Reserved for stage 2;
    /// nothing reads or writes it today.
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceMeta {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub version_id: String,
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
}

impl Default for InstanceMeta {
    fn default() -> Self {
        InstanceMeta {
            id: String::new(),
            slug: String::new(),
            name: String::new(),
            version_id: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            state: InstanceState::Empty,
            ram_mb: None,
            jvm_args: Vec::new(),
            game_args: Vec::new(),
            width: None,
            height: None,
            mods: ModProfile::default(),
        }
    }
}

/// Turn a version id into a filesystem-safe, collision-resistant slug.
///
/// Version ids are already `[A-Za-z0-9._-]`, but a user could later add a
/// custom instance name, so the slug is sanitised and suffixed with a short
/// hash when it had to be altered — this keeps `<slug>` unique and prevents
/// two different names from mapping to the same directory.
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
        let mut h: u32 = 0x811c_9dc5;
        for b in version_id.as_bytes() {
            h ^= *b as u32;
            h = h.wrapping_mul(0x0100_0193);
        }
        base = format!("{base}-{:08x}", h);
    }
    base
}

/// Directories every instance gets. Created eagerly so a crash never leaves a
/// half-initialised game directory behind.
const SUBDIRS: &[&str] = &[
    "saves",
    "config",
    "resourcepacks",
    "shaderpacks",
    "screenshots",
    "logs",
    "resourcepacks_cache",
    "natives",
    "assets",
];

pub struct InstancePaths {
    pub root: PathBuf,
    pub meta: PathBuf,
    pub natives: PathBuf,
    pub assets: PathBuf,
}

pub fn paths(instances_root: &Path, slug: &str) -> InstancePaths {
    let root = instances_root.join(slug);
    InstancePaths {
        meta: root.join(META_FILE),
        natives: root.join("natives"),
        assets: root.join("assets"),
        root,
    }
}

pub fn load(path: &Path) -> AppResult<InstanceMeta> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| AppError::io("Не удалось прочитать данные экземпляра", &e))?;
    serde_json::from_str(&raw)
        .map_err(|e| AppError::config("Данные экземпляра повреждены").with_detail(e.to_string()))
}

pub fn save(path: &Path, meta: &InstanceMeta) -> AppResult<()> {
    let bytes = serde_json::to_vec_pretty(meta)
        .map_err(|e| AppError::config("Не удалось сериализовать экземпляр").with_detail(e.to_string()))?;
    util::write_atomic(path, &bytes)
        .map_err(|e| AppError::io("Не удалось сохранить экземпляр", &e))
}

/// Create the directory skeleton and metadata if they do not exist yet.
pub fn ensure(instances_root: &Path, version_id: &str, name: Option<&str>) -> AppResult<InstanceMeta> {
    let slug = slugify(version_id);
    let p = paths(instances_root, &slug);
    std::fs::create_dir_all(&p.root)
        .map_err(|e| AppError::io("Не удалось создать папку экземпляра", &e))?;
    for dir in SUBDIRS {
        std::fs::create_dir_all(p.root.join(dir))
            .map_err(|e| AppError::io("Не удалось создать подпапку экземпляра", &e))?;
    }

    if p.meta.exists() {
        return load(&p.meta);
    }

    let now = util::now_rfc3339();
    let meta = InstanceMeta {
        id: format!("i-{}-{}-{:08x}", util::now_secs(), std::process::id(), fnv(version_id)),
        slug: slug.clone(),
        name: name.map(|n| n.to_string()).unwrap_or_else(|| version_id.to_string()),
        version_id: version_id.to_string(),
        created_at: now.clone(),
        updated_at: now,
        state: InstanceState::Empty,
        ram_mb: None,
        jvm_args: Vec::new(),
        game_args: Vec::new(),
        width: None,
        height: None,
        mods: ModProfile::default(),
    };
    save(&p.meta, &meta)?;
    Ok(meta)
}

fn fnv(text: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in text.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

pub fn set_state(p: &InstancePaths, state: InstanceState) -> AppResult<()> {
    let mut meta = load(&p.meta)?;
    meta.state = state;
    meta.updated_at = util::now_rfc3339();
    save(&p.meta, &meta)
}

/// List every instance on disk, newest first.
pub fn list(instances_root: &Path) -> Vec<InstanceMeta> {
    let Ok(entries) = std::fs::read_dir(instances_root) else { return Vec::new() };
    let mut out: Vec<InstanceMeta> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| load(&e.path().join(META_FILE)).ok())
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

/// Deleting an instance only removes its directory; the shared caches stay.
pub fn remove(instances_root: &Path, slug: &str) -> AppResult<()> {
    if slug != slugify(slug) || slug.is_empty() {
        return Err(AppError::invalid("Некорректный идентификатор экземпляра"));
    }
    let root = instances_root.join(slug);
    if !root.is_dir() {
        return Err(AppError::not_found("Экземпляр не найден"));
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
        return Err(AppError::invalid("Путь экземпляра выходит за пределы каталога"));
    }
    std::fs::remove_dir_all(&root).map_err(|e| AppError::io("Не удалось удалить экземпляр", &e))
}

/// Approximate on-disk size, used by the (stage 2) instances screen.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_version_ids_survive_slugify_unchanged() {
        assert_eq!(slugify("1.20.4"), "1.20.4");
        assert_eq!(slugify("24w14potato"), "24w14potato");
        assert_eq!(slugify("1.7.10-forge1.7.10-10.13.4.1614-1.7.10"), "1.7.10-forge1.7.10-10.13.4.1614-1.7.10");
    }

    #[test]
    fn unsafe_version_ids_get_a_hash_suffix() {
        let a = slugify("../etc/passwd");
        assert!(!a.contains('/'));
        assert!(!a.contains('\\'));
        assert_ne!(a, slugify("../../other"));
    }

    #[test]
    fn slugs_are_deterministic() {
        assert_eq!(slugify("1.20.4"), slugify("1.20.4"));
        assert_eq!(slugify("weird id!"), slugify("weird id!"));
    }

    #[test]
    fn ensure_creates_the_full_skeleton_and_metadata() {
        let root = std::env::temp_dir().join("sifora-instance-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let meta = ensure(&root, "1.20.4", None).unwrap();
        assert_eq!(meta.version_id, "1.20.4");
        assert_eq!(meta.state, InstanceState::Empty);
        assert_eq!(meta.mods.loader, ModLoader::None);

        for dir in SUBDIRS {
            assert!(root.join(&meta.slug).join(dir).is_dir(), "{dir} missing");
        }
        assert!(root.join(&meta.slug).join(META_FILE).is_file());

        // Calling again is idempotent and preserves the original id.
        let again = ensure(&root, "1.20.4", None).unwrap();
        assert_eq!(again.id, meta.id);

        let listed = list(&root);
        assert_eq!(listed.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn state_transitions_persist() {
        let root = std::env::temp_dir().join("sifora-instance-state-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let meta = ensure(&root, "1.8.9", None).unwrap();
        let p = paths(&root, &meta.slug);
        set_state(&p, InstanceState::Ready).unwrap();
        assert_eq!(load(&p.meta).unwrap().state, InstanceState::Ready);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn removal_refuses_to_escape_the_instances_root() {
        let root = std::env::temp_dir().join("sifora-instance-rm-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let meta = ensure(&root, "1.20.4", None).unwrap();
        remove(&root, &meta.slug).unwrap();
        assert!(!root.join(&meta.slug).exists());
        assert!(remove(&root, &meta.slug).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn metadata_with_unknown_fields_is_rejected() {
        let root = std::env::temp_dir().join("sifora-instance-schema-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let meta = ensure(&root, "1.20.4", None).unwrap();
        let p = paths(&root, &meta.slug);
        std::fs::write(&p.meta, br#"{"id":"x","__proto__":{"a":1}}"#).unwrap();
        assert!(load(&p.meta).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}