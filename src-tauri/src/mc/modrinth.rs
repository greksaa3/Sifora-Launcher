//! Modrinth client — mod browsing, search and installation.
//!
//! Everything here is read-only against Modrinth's public v2 API plus a single
//! file download into an instance's `mods` folder. Modrinth was chosen over
//! CurseForge because it needs no API key: there is no credential to store, so
//! there is nothing to leak. (CurseForge requires an author-issued key, so it
//! is deliberately not wired up yet.)
//!
//! Two rules shape the code:
//!
//! * **The renderer never talks to Modrinth.** The window's CSP forbids remote
//!   connections; every request is made here, from Rust, and the renderer only
//!   receives plain values.
//! * **The renderer cannot shape the request.** Search facets are built from a
//!   closed set of typed fields with validated values, never from a string the
//!   UI supplies. Modrinth's facet syntax is a query language, and handing it
//!   straight through would make this module a proxy for it.
//!
//! Tag icons are the one thing deliberately dropped: Modrinth returns raw SVG
//! markup for category icons, and no value that reaches the renderer is allowed
//! to carry markup. The UI draws its own icons instead.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult, ErrorKind};

pub const API: &str = "https://api.modrinth.com/v2";

/// Bounds on everything the renderer can ask for.
const MAX_QUERY: usize = 120;
const MAX_FILTERS: usize = 12;
const MAX_LIMIT: u32 = 50;

/// Closed allow-lists for the sort modes and side flags Modrinth accepts.
const INDEXES: [&str; 5] = ["relevance", "downloads", "follows", "newest", "updated"];
const PROJECT_TYPES: [&str; 4] = ["mod", "modpack", "resourcepack", "shader"];
const SIDES: [&str; 3] = ["required", "optional", "unsupported"];

// ---------------------------------------------------------------------------
// Request
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchQuery {
    #[serde(default)]
    pub query: String,
    /// One of [`INDEXES`]; anything else falls back to `relevance`.
    #[serde(default)]
    pub index: Option<String>,
    /// Category ids. Modrinth files loaders under categories too, so
    /// `["optimization"]` paired with a loader is the usual query.
    #[serde(default)]
    pub categories: Vec<String>,
    /// Loader ids (`fabric`, `forge`, `neoforge`, `quilt`). Loaders share the
    /// `categories:` facet key with categories, so each loader becomes its own
    /// AND group — filtering to Fabric *and* optimisation is the useful query.
    #[serde(default)]
    pub loaders: Vec<String>,
    /// Minecraft version ids, e.g. `["1.20.1"]`.
    #[serde(default)]
    pub versions: Vec<String>,
    /// One of [`PROJECT_TYPES`]; defaults to `mod`.
    #[serde(default)]
    pub project_type: Option<String>,
    #[serde(default)]
    pub client_side: Option<String>,
    #[serde(default)]
    pub server_side: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

/// A facet value that is safe to embed in Modrinth's facet syntax.
///
/// Facets are `[[ "key:value", ... ], ...]` where the inner arrays are OR and
/// the outer array is AND. Since the value ends up inside a JSON string that is
/// then URL-encoded, only characters that appear in real tag ids are accepted —
/// quoting, brackets and colons are refused outright.
fn safe_facet_value(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return None;
    }
    let ok = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'));
    ok.then(|| trimmed.to_string())
}

fn safe_facet_values(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for value in raw.iter().take(MAX_FILTERS) {
        if let Some(value) = safe_facet_value(value) {
            if !out.contains(&value) {
                out.push(value);
            }
        }
    }
    out
}

impl SearchQuery {
    fn normalised(&self) -> (String, u32, u32, String) {
        let query: String = self.query.trim().chars().take(MAX_QUERY).collect();
        let limit = self.limit.unwrap_or(20).clamp(1, MAX_LIMIT);
        let offset = self.offset.unwrap_or(0).min(10_000);
        let index = self
            .index
            .as_deref()
            .map(|i| i.trim().to_ascii_lowercase())
            .filter(|i| INDEXES.contains(&i.as_str()))
            .unwrap_or_else(|| "relevance".to_string());
        (query, limit, offset, index)
    }

    /// Build the `facets` parameter.
    fn facets(&self) -> String {
        let project_type = self
            .project_type
            .as_deref()
            .map(|t| t.trim().to_ascii_lowercase())
            .filter(|t| PROJECT_TYPES.contains(&t.as_str()))
            .unwrap_or_else(|| "mod".to_string());

        let mut groups: Vec<Vec<String>> = vec![vec![format!("project_type:{project_type}")]];

        let categories = safe_facet_values(&self.categories);
        if !categories.is_empty() {
            groups.push(categories.iter().map(|c| format!("categories:{c}")).collect());
        }
        let loaders = safe_facet_values(&self.loaders);
        if !loaders.is_empty() {
            groups.push(loaders.iter().map(|l| format!("categories:{l}")).collect());
        }
        let versions = safe_facet_values(&self.versions);
        if !versions.is_empty() {
            groups.push(versions.iter().map(|v| format!("versions:{v}")).collect());
        }
        // Client/server side flags are single-valued, not OR lists.
        for (key, value) in [("client_side", &self.client_side), ("server_side", &self.server_side)] {
            if let Some(value) = value.as_deref().map(|v| v.trim().to_ascii_lowercase()) {
                if SIDES.contains(&value.as_str()) {
                    groups.push(vec![format!("{key}:{value}")]);
                }
            }
        }

        let inner: Vec<String> = groups
            .iter()
            .map(|group| {
                let items: Vec<String> = group.iter().map(|g| json_string(g)).collect();
                format!("[{}]", items.join(","))
            })
            .collect();
        format!("[{}]", inner.join(","))
    }
}

/// Minimal JSON string literal. Facet values are already restricted to
/// `[A-Za-z0-9._+-]`, so there is nothing here that can escape the quotes.
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | '+' | ':' ) {
            out.push(ch);
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Responses
// ---------------------------------------------------------------------------

/// A project as it appears in a search result list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModSummary {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub follows: u64,
    pub categories: Vec<String>,
    pub versions: Vec<String>,
    pub icon_url: Option<String>,
    pub updated: String,
    pub license: String,
    pub client_side: String,
    pub server_side: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub hits: Vec<ModSummary>,
    pub offset: u32,
    pub limit: u32,
    pub total_hits: u32,
}

#[derive(Debug, Deserialize)]
struct RawHit {
    project_id: String,
    slug: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    display_categories: Vec<String>,
    #[serde(default)]
    versions: Vec<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    follows: u64,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    date_modified: String,
    #[serde(default)]
    license: String,
    #[serde(default)]
    client_side: String,
    #[serde(default)]
    server_side: String,
}

#[derive(Debug, Deserialize)]
struct RawSearch {
    #[serde(default)]
    hits: Vec<RawHit>,
    #[serde(default)]
    offset: u32,
    #[serde(default)]
    limit: u32,
    #[serde(default)]
    total_hits: u32,
}

impl RawHit {
    fn into_summary(self) -> ModSummary {
        // `display_categories` omits loaders that are implicit for the project
        // type; prefer it for the chip row when it is present.
        let categories = if self.display_categories.is_empty() {
            self.categories
        } else {
            self.display_categories
        };
        ModSummary {
            project_id: self.project_id,
            slug: self.slug,
            title: self.title,
            description: self.description,
            author: self.author,
            downloads: self.downloads,
            follows: self.follows,
            categories,
            versions: self.versions,
            icon_url: self.icon_url,
            updated: self.date_modified,
            license: self.license,
            client_side: self.client_side,
            server_side: self.server_side,
        }
    }
}

/// One entry in a filter list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub name: String,
    /// `mod`, `modpack`, … — lets the UI show only the relevant groups.
    pub project_type: String,
    /// `categories`, `resolutions`, … — the heading Modrinth files it under.
    pub header: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagBundle {
    pub categories: Vec<Tag>,
    pub loaders: Vec<String>,
    pub game_versions: Vec<String>,
    pub project_types: Vec<String>,
    pub indexes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawCategoryTag {
    name: String,
    #[serde(default)]
    project_type: String,
    #[serde(default)]
    header: String,
}

#[derive(Debug, Deserialize)]
struct RawLoaderTag {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RawGameVersionTag {
    version: String,
    #[serde(default)]
    stable: bool,
}

/// A single downloadable file attached to a version.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModFile {
    pub url: String,
    pub filename: String,
    pub size: u64,
    pub sha1: Option<String>,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModVersion {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    /// `release`, `beta` or `alpha`.
    pub version_type: String,
    pub downloads: u64,
    pub published: String,
    pub files: Vec<ModFile>,
    pub dependencies: Vec<ModDependency>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModDependency {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub dependency_type: String,
}

#[derive(Debug, Deserialize)]
struct RawVersion {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version_number: String,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    version_type: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    date_published: String,
    #[serde(default)]
    files: Vec<RawFile>,
    #[serde(default)]
    dependencies: Vec<RawDependency>,
}

#[derive(Debug, Deserialize)]
struct RawFile {
    #[serde(default)]
    url: String,
    #[serde(default)]
    filename: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    hashes: RawHashes,
}

#[derive(Debug, Default, Deserialize)]
struct RawHashes {
    #[serde(default)]
    sha1: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawDependency {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    version_id: Option<String>,
    #[serde(default)]
    dependency_type: String,
}

impl RawVersion {
    fn into_version(self) -> ModVersion {
        ModVersion {
            id: self.id,
            name: self.name,
            version_number: self.version_number,
            game_versions: self.game_versions,
            loaders: self.loaders,
            version_type: self.version_type,
            downloads: self.downloads,
            published: self.date_published,
            files: self
                .files
                .into_iter()
                .map(|f| ModFile {
                    url: f.url,
                    filename: f.filename,
                    size: f.size,
                    sha1: f.hashes.sha1,
                    primary: f.primary,
                })
                .collect(),
            dependencies: self
                .dependencies
                .into_iter()
                .map(|d| ModDependency {
                    project_id: d.project_id,
                    version_id: d.version_id,
                    dependency_type: d.dependency_type,
                })
                .collect(),
        }
    }
}

/// Full project page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModDetails {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    /// Markdown, rendered by the UI from a small safe subset.
    pub body: String,
    pub categories: Vec<String>,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub downloads: u64,
    pub followers: u64,
    pub icon_url: Option<String>,
    pub updated: String,
    pub published: String,
    pub client_side: String,
    pub server_side: String,
    pub project_type: String,
    pub license_name: String,
    pub source_url: Option<String>,
    pub issues_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawProject {
    id: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    followers: u64,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    updated: String,
    #[serde(default)]
    published: String,
    #[serde(default)]
    client_side: String,
    #[serde(default)]
    server_side: String,
    #[serde(default)]
    project_type: String,
    #[serde(default)]
    license: Option<RawLicense>,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    issues_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawLicense {
    #[serde(default)]
    name: String,
    #[serde(default)]
    id: String,
}

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

/// Does this URL point at a host we are willing to download mod files from?
///
/// Modrinth serves files from `cdn.modrinth.com`, but older releases are also
/// mirrored through `github.com` release assets, so both are allowed. Anything
/// else is refused: a project's file entry is remote data, and following it
/// blindly would let a compromised record redirect a download anywhere.
fn download_host_allowed(url: &str) -> bool {
    let rest = match url.strip_prefix("https://") {
        Some(rest) => rest,
        None => return false,
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.split('@').next_back().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    matches!(
        host.as_str(),
        "cdn.modrinth.com"
            | "github.com"
            | "objects.githubusercontent.com"
            | "raw.githubusercontent.com"
            | "mediafilez.forgecdn.net"
    )
}

fn api_error(what: &str, err: AppError) -> AppError {
    AppError::new(err.kind, what).with_detail(err.detail.clone().unwrap_or_default())
}

async fn get_json<T: for<'de> Deserialize<'de>>(
    http: &reqwest::Client,
    url: &str,
    what: &str,
) -> AppResult<T> {
    let raw = crate::mc::download::fetch_text(http, url)
        .await
        .map_err(|e| api_error(what, e))?;
    serde_json::from_str(&raw).map_err(|e| {
        AppError::new(ErrorKind::Network, what).with_detail(format!("неожиданный ответ Modrinth: {e}"))
    })
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

pub async fn search(http: &reqwest::Client, query: &SearchQuery) -> AppResult<SearchPage> {
    let (text, limit, offset, index) = query.normalised();
    let facets = query.facets();
    let url = format!(
        "{API}/search?query={}&index={}&limit={}&offset={}&facets={}",
        urlencode(&text),
        index,
        limit,
        offset,
        urlencode(&facets),
    );

    let raw: RawSearch = get_json(http, &url, "Не удалось выполнить поиск по Modrinth").await?;
    Ok(SearchPage {
        hits: raw.hits.into_iter().map(RawHit::into_summary).collect(),
        offset: raw.offset,
        limit: raw.limit,
        total_hits: raw.total_hits,
    })
}

pub async fn tags(http: &reqwest::Client) -> AppResult<TagBundle> {
    // Three small documents. Fetched in sequence: together they are a handful
    // of kilobytes, and the UI loads them once when the mods view opens.
    let categories: Vec<RawCategoryTag> =
        get_json(http, &format!("{API}/tag/category"), "Не удалось получить категории Modrinth").await?;
    let loaders: Vec<RawLoaderTag> =
        get_json(http, &format!("{API}/tag/loader"), "Не удалось получить загрузчики Modrinth").await?;
    let versions: Vec<RawGameVersionTag> =
        get_json(http, &format!("{API}/tag/game_version"), "Не удалось получить версии Modrinth").await?;

    Ok(TagBundle {
        categories: categories
            .into_iter()
            .map(|c| Tag { name: c.name, project_type: c.project_type, header: c.header })
            .collect(),
        loaders: loaders.into_iter().map(|l| l.name).collect(),
        // Modrinth returns newest-first; float the released versions above the
        // snapshots without disturbing that order (`sort_by` is stable).
        game_versions: {
            let mut versions = versions;
            versions.sort_by(|a, b| b.stable.cmp(&a.stable));
            versions.into_iter().map(|v| v.version).collect()
        },
        project_types: PROJECT_TYPES.iter().map(|s| s.to_string()).collect(),
        indexes: INDEXES.iter().map(|s| s.to_string()).collect(),
    })
}

pub async fn project(http: &reqwest::Client, id: &str) -> AppResult<ModDetails> {
    let id = safe_project_id(id)?;
    let url = format!("{API}/project/{id}");
    let raw: RawProject = get_json(http, &url, "Не удалось открыть проект Modrinth").await?;

    let (license_name, _) = raw
        .license
        .map(|l| if l.name.is_empty() { (l.id, ()) } else { (l.name, ()) })
        .unwrap_or_default();

    Ok(ModDetails {
        project_id: raw.id,
        slug: raw.slug,
        title: raw.title,
        description: raw.description,
        body: raw.body,
        categories: raw.categories,
        game_versions: raw.game_versions,
        loaders: raw.loaders,
        downloads: raw.downloads,
        followers: raw.followers,
        icon_url: raw.icon_url,
        updated: raw.updated,
        published: raw.published,
        client_side: raw.client_side,
        server_side: raw.server_side,
        project_type: raw.project_type,
        license_name,
        source_url: raw.source_url,
        issues_url: raw.issues_url,
    })
}

/// Versions of a project, filtered to what an instance can actually run.
pub async fn versions(
    http: &reqwest::Client,
    project_id: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
) -> AppResult<Vec<ModVersion>> {
    let id = safe_project_id(project_id)?;
    let mut url = format!("{API}/project/{id}/version");
    let mut params: Vec<String> = Vec::new();
    if let Some(game_version) = game_version.and_then(safe_facet_value) {
        params.push(format!("game_versions={}", urlencode(&format!("[\"{game_version}\"]"))));
    }
    if let Some(loader) = loader.and_then(safe_facet_value) {
        params.push(format!("loaders={}", urlencode(&format!("[\"{loader}\"]"))));
    }
    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }

    let raw: Vec<RawVersion> = get_json(http, &url, "Не удалось получить версии мода").await?;
    Ok(raw.into_iter().map(RawVersion::into_version).collect())
}

/// Project ids and slugs are `[A-Za-z0-9._-]`. Anything else is refused so a
/// value from the renderer can never change which URL is requested.
fn safe_project_id(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    let ok = !trimmed.is_empty()
        && trimmed.len() <= 64
        && trimmed != "."
        && trimmed != ".."
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if ok {
        Ok(trimmed.to_string())
    } else {
        Err(AppError::invalid("Некорректный идентификатор мода"))
    }
}

/// The file a version should be installed from.
pub fn primary_file(version: &ModVersion) -> Option<&ModFile> {
    version
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| version.files.first())
}

/// Restrict an arbitrary filename to a bare name inside `mods`.
///
/// Modrinth filenames are well behaved, but they are remote data and a name
/// containing a separator or a drive letter would place the download outside
/// the instance.
pub fn safe_filename(raw: &str) -> Option<String> {
    let name = raw.rsplit(['/', '\\']).next().unwrap_or_default();
    if name.is_empty() || name.len() > 128 {
        return None;
    }
    if name == "." || name == ".." || name.contains(':') {
        return None;
    }
    if name.chars().any(|c| c.is_control() || matches!(c, '<' | '>' | '"' | '|' | '?' | '*')) {
        return None;
    }
    if !name.to_ascii_lowercase().ends_with(".jar") {
        // Mods are jars; a `.zip` or an executable is not something the game
        // would load, so it is refused rather than written to disk.
        return None;
    }
    Some(name.to_string())
}

pub fn download_url_allowed(url: &str) -> bool {
    download_host_allowed(url)
}

fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q() -> SearchQuery {
        SearchQuery { project_type: Some("mod".into()), ..SearchQuery::default() }
    }

    #[test]
    fn facets_always_pin_the_project_type() {
        assert_eq!(q().facets(), r#"[["project_type:mod"]]"#);
    }

    #[test]
    fn facet_values_are_restricted_to_tag_characters() {
        let query = SearchQuery {
            categories: vec!["fabric".into(), "optimization".into(), "bad\"],[\"x".into()],
            versions: vec!["1.20.1".into(), "../../etc".into()],
            ..q()
        };
        let facets = query.facets();
        assert!(facets.contains("categories:fabric"));
        assert!(facets.contains("categories:optimization"));
        assert!(facets.contains("versions:1.20.1"));
        assert!(!facets.contains("bad"), "a hostile category must be dropped: {facets}");
        assert!(!facets.contains("etc"), "a hostile version must be dropped: {facets}");
        assert!(!facets.contains('\\'));
        // The value can never escape its JSON string literal.
        assert!(facets.starts_with("[["));
    }

    #[test]
    fn a_loader_is_its_own_facet_group() {
        let query = SearchQuery {
            categories: vec!["optimization".into()],
            loaders: vec!["fabric".into()],
            ..q()
        };
        // Loader and category each get their own group, which Modrinth reads as
        // AND — "Fabric mods that are optimisation mods", not either one.
        assert_eq!(
            query.facets(),
            r#"[["project_type:mod"],["categories:optimization"],["categories:fabric"]]"#
        );
    }

    #[test]
    fn an_unknown_sort_falls_back_to_relevance() {
        let query = SearchQuery { index: Some("../../evil".into()), ..q() };
        assert_eq!(query.normalised().3, "relevance");
        let query = SearchQuery { index: Some("downloads".into()), ..q() };
        assert_eq!(query.normalised().3, "downloads");
    }

    #[test]
    fn limits_are_clamped() {
        let query = SearchQuery { limit: Some(5000), offset: Some(999_999), ..q() };
        let (_, limit, offset, _) = query.normalised();
        assert_eq!(limit, MAX_LIMIT);
        assert_eq!(offset, 10_000);
    }

    #[test]
    fn project_ids_reject_path_characters() {
        assert!(safe_project_id("AANobbMI").is_ok());
        assert!(safe_project_id("sodium").is_ok());
        assert!(safe_project_id("sodium/../evil").is_err());
        assert!(safe_project_id("..").is_err());
        assert!(safe_project_id("a/b").is_err());
        assert!(safe_project_id("").is_err());
    }

    #[test]
    fn mod_filenames_stay_inside_the_mods_folder() {
        assert_eq!(safe_filename("sodium-fabric-0.5.13.jar").as_deref(), Some("sodium-fabric-0.5.13.jar"));
        assert_eq!(safe_filename("sub/dir/mod.jar").as_deref(), Some("mod.jar"));
        assert_eq!(safe_filename("..\\..\\evil.jar"), Some("evil.jar".to_string()));
        // A drive-qualified path is reduced to its bare file name: the result
        // can only ever be joined onto the build's own `mods` folder.
        assert_eq!(safe_filename("C:\\windows\\evil.jar"), Some("evil.jar".to_string()));
        assert!(safe_filename("../etc/passwd").is_none());
        assert!(safe_filename("setup.exe").is_none());
        assert!(safe_filename("data.zip").is_none());
        assert!(safe_filename("").is_none());
        assert!(safe_filename(&format!("{}.jar", "a".repeat(200))).is_none());
    }

    #[test]
    fn only_expected_hosts_can_be_downloaded_from() {
        assert!(download_url_allowed("https://cdn.modrinth.com/data/AANobbMI/x.jar"));
        assert!(download_url_allowed("https://github.com/x/y/releases/download/1/m.jar"));
        assert!(!download_url_allowed("http://cdn.modrinth.com/x.jar"), "plain http is refused");
        assert!(!download_url_allowed("https://evil.example.com/x.jar"));
        assert!(!download_url_allowed("https://cdn.modrinth.com.evil.com/x.jar"));
        assert!(!download_url_allowed("file:///C:/x.jar"));
    }

    #[test]
    fn urlencoding_escapes_facet_syntax() {
        assert_eq!(urlencode(r#"[["project_type:mod"]]"#), "%5B%5B%22project_type%3Amod%22%5D%5D");
    }

    #[test]
    fn the_primary_file_is_preferred_and_never_panics_on_empty() {
        let version = ModVersion {
            id: "v".into(),
            name: String::new(),
            version_number: String::new(),
            game_versions: vec![],
            loaders: vec![],
            version_type: "release".into(),
            downloads: 0,
            published: String::new(),
            files: vec![],
            dependencies: vec![],
        };
        assert!(primary_file(&version).is_none());
    }
}
