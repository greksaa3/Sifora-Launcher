//! Asset index handling.
//!
//! Mojang changed the asset storage layout twice. All three are supported:
//!
//! | Index flavour      | Where files live                        | Versions |
//! |--------------------|-----------------------------------------|----------|
//! | object store       | `assets/objects/<xx>/<sha1>`            | 1.7.10+   |
//! | virtual            | `assets/virtual/<index id>/<path>`      | 1.6–1.7.2 |
//! | `map_to_resources` | `assets/resources/<path>`               | ≤ 1.5.x   |
//!
//! The index document itself is downloaded once and cached; objects are
//! content-addressed, so they are shared by every version that shares an index
//! id — which is why the object store lives in the shared game-data directory
//! while the legacy layouts are materialised per instance.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::Deserialize;

use super::download::{self, EventSink, FileTask, Ledger};
use crate::error::{AppError, AppResult, ErrorKind};

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    #[serde(default)]
    pub objects: std::collections::BTreeMap<String, AssetObject>,
    #[serde(default, rename = "map_to_resources")]
    pub map_to_resources: bool,
    #[serde(default, rename = "virtual")]
    pub virtual_resource: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetLayout {
    ObjectStore,
    Virtual,
    Resources,
}

impl AssetIndex {
    pub fn layout(&self) -> AssetLayout {
        if self.map_to_resources {
            AssetLayout::Resources
        } else if self.virtual_resource {
            AssetLayout::Virtual
        } else {
            AssetLayout::ObjectStore
        }
    }

    /// Index docs are cached per index id and verified against the sha1 Mojang
    /// publishes in the version document.
    pub fn parse(raw: &str) -> AppResult<Self> {
        serde_json::from_str(raw)
            .map_err(|e| AppError::new(ErrorKind::Config, "Индекс ресурсов повреждён").with_detail(e.to_string()))
    }
}

/// Normalise an asset key coming from Mojang's index.
///
/// Keys are relative resource paths. A malicious or corrupted index could try
/// to escape the assets root with `..`, a drive prefix or a UNC path, so the
/// key is rebuilt component-by-component and anything suspicious is dropped.
pub fn safe_key(key: &str) -> Option<String> {
    if key.is_empty() || key.len() > 512 {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in key.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => return None,
            p if p.contains(':') || p.starts_with('~') => return None,
            p if p.chars().any(|c| c.is_control()) => return None,
            p => parts.push(p),
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

/// Where one asset has to end up inside an instance.
pub fn asset_destination(assets_root: &Path, layout: AssetLayout, index_id: &str, key: &str) -> Option<PathBuf> {
    let key = safe_key(key)?;
    match layout {
        // The object store is content-addressed; callers pass `<xx>/<hash>`.
        AssetLayout::ObjectStore => Some(assets_root.join("objects").join(key)),
        AssetLayout::Virtual => {
            let id = safe_key(index_id)?;
            Some(assets_root.join("virtual").join(id).join(key))
        }
        AssetLayout::Resources => Some(assets_root.join("resources").join(key)),
    }
}

pub struct AssetPlan {
    pub index_id: String,
    pub index_url: String,
    pub index_sha1: String,
    pub index_size: u64,
}

pub struct AssetDownloader<'a> {
    pub assets_store: &'a Path,
    pub http: &'a reqwest::Client,
    pub cancel: Arc<AtomicBool>,
    pub sink: EventSink,
}

impl<'a> AssetDownloader<'a> {
    /// Fetch + cache the index document and return the parsed contents.
    pub async fn load_index(&self, plan: &AssetPlan) -> AppResult<(AssetIndex, PathBuf)> {
        let indexes_dir = self.assets_store.join("indexes");
        let index_path = indexes_dir.join(format!("{}.json", plan.index_id));

        if index_path.exists() {
            if let Ok(raw) = std::fs::read_to_string(&index_path) {
                if download::sha1_file(&index_path)
                    .map(|h| h.eq_ignore_ascii_case(&plan.index_sha1))
                    .unwrap_or(false)
                {
                    if let Ok(index) = AssetIndex::parse(&raw) {
                        return Ok((index, index_path));
                    }
                }
            }
        }

        let task = FileTask::new(format!("asset-index:{}", plan.index_id), &plan.index_url, &index_path)
            .with_sha1(Some(plan.index_sha1.clone()))
            .with_size(Some(plan.index_size));
        download::ensure_file(self.http, &task, &self.cancel, &self.sink).await?;

        let raw = std::fs::read_to_string(&index_path)
            .map_err(|e| AppError::io("Не удалось прочитать индекс ресурсов", &e))?;
        let index = AssetIndex::parse(&raw)?;
        Ok((index, index_path))
    }

    /// Create hard links (or copies as a fallback) for every asset that the
    /// instance needs but does not have yet.
    pub async fn materialise(
        &self,
        index: &AssetIndex,
        layout: AssetLayout,
        index_id: &str,
        instance_assets: &Path,
        ledger: &Arc<parking_lot::Mutex<Ledger>>,
    ) -> AppResult<u64> {
        std::fs::create_dir_all(instance_assets)
            .map_err(|e| AppError::io("Не удалось создать папку ресурсов", &e))?;

        let objects_dir = self.assets_store.join("objects");
        let mut linked = 0u64;
        let mut failures: Vec<String> = Vec::new();

        for (key, object) in &index.objects {
            let Some(destination) = asset_destination(instance_assets, layout, index_id, key) else {
                crate::log_warn!("mc::assets", "skipping unsafe asset key {key:?}");
                continue;
            };
            if object.hash.len() < 2 || !object.hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                continue;
            }
            let source = objects_dir.join(&object.hash[..2]).join(&object.hash);

            if destination.exists() {
                continue;
            }
            if !source.exists() {
                failures.push(format!("resource {key}"));
                continue;
            }
            if let Some(parent) = destination.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    crate::log_warn!("mc::assets", "mkdir failed for {}: {e}", destination.display());
                    failures.push(key.clone());
                    continue;
                }
            }
            match link_or_copy(&source, &destination) {
                Ok(()) => {
                    linked += 1;
                    if linked % 200 == 0 {
                        ledger.lock().note(format!("Подготовлено ресурсов: {linked}"));
                    }
                }
                Err(e) => {
                    crate::log_warn!("mc::assets", "link {} -> {}: {e}", source.display(), destination.display());
                    failures.push(key.clone());
                }
            }
        }

        if !failures.is_empty() && failures.len() == index.objects.len() {
            return Err(AppError::new(
                ErrorKind::NotFound,
                "Ресурсы не найдены в кэше — запустите загрузку ещё раз",
            ));
        }
        Ok(linked)
    }
}

fn link_or_copy(source: &Path, destination: &Path) -> std::io::Result<()> {
    if destination.exists() {
        return Ok(());
    }
    // Hard links keep the instance tiny on NTFS; fall back to a copy on FAT32,
    // exFAT or when linking is denied.
    match std::fs::hard_link(source, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(source, destination)?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_detection_matches_real_indices() {
        let pre16 = AssetIndex::parse(
            r#"{"objects":{"icons/icon_16x16.png":{"hash":"aa","size":1}},"map_to_resources":true}"#,
        )
        .unwrap();
        assert_eq!(pre16.layout(), AssetLayout::Resources);

        let legacy = AssetIndex::parse(
            r#"{"objects":{"lang/en_US.lang":{"hash":"bb","size":1}},"virtual":true}"#,
        );
        // Modern indices call the flag `virtual`; serde maps `virtual_resource`.
        let legacy = legacy.unwrap_or_else(|_| AssetIndex::parse(r#"{"objects":{}}"#).unwrap());
        assert_ne!(legacy.layout(), AssetLayout::ObjectStore, "virtual index must not use the object store");

        let modern = AssetIndex::parse(r#"{"objects":{"a.png":{"hash":"cc","size":1}}}"#).unwrap();
        assert_eq!(modern.layout(), AssetLayout::ObjectStore);
    }

    #[test]
    fn destinations_are_contained_inside_the_assets_root() {
        let root = Path::new("C:/game/assets");
        assert!(asset_destination(root, AssetLayout::ObjectStore, "12", "aa/bb")
            .unwrap()
            .starts_with(root));
        assert!(asset_destination(root, AssetLayout::Virtual, "legacy", "lang/en_US.lang")
            .unwrap()
            .starts_with(root));
        assert!(asset_destination(root, AssetLayout::Resources, "pre-1.6", "pack.png")
            .unwrap()
            .starts_with(root));
    }

    #[test]
    fn path_traversal_in_an_asset_key_is_refused() {
        let root = Path::new("assets");
        for hostile in [
            "../../../evil.txt",
            "..\\..\\windows\\system32\\x.dll",
            "C:/windows/system32/cmd.exe",
            "",
            "./../x",
        ] {
            assert!(
                asset_destination(root, AssetLayout::Resources, "pre-1.6", hostile).is_none(),
                "must refuse {hostile:?}"
            );
        }
    }

    #[test]
    fn unc_style_keys_are_normalised_but_stay_inside_the_root() {
        let root = Path::new("assets");
        // A leading `\\` collapses to an ordinary relative path, so the result
        // is contained rather than refused.
        let destination =
            asset_destination(root, AssetLayout::Resources, "pre-1.6", "\\\\server\\share\\x.png")
                .expect("normalised");
        assert!(destination.starts_with(root));
        assert!(destination.components().all(|c| c != std::path::Component::RootDir));
    }

    #[test]
    fn safe_key_normalises_redundant_separators() {
        assert_eq!(safe_key("lang//en_US.lang").as_deref(), Some("lang/en_US.lang"));
        assert_eq!(safe_key("./a/b.png").as_deref(), Some("a/b.png"));
        assert!(safe_key("a\u{0}b").is_none());
        assert!(safe_key(&"a".repeat(600)).is_none());
    }

    #[test]
    fn unsafe_index_id_is_refused_for_virtual_layout() {
        let root = Path::new("assets");
        assert!(asset_destination(root, AssetLayout::Virtual, "../escape", "a/b").is_none());
    }

    #[test]
    fn corrupt_index_is_reported_as_config_error() {
        let err = AssetIndex::parse("{not json").unwrap_err();
        assert_eq!(err.kind, ErrorKind::Config);
    }
}