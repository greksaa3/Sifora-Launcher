//! Resumable, checksum-verified download engine.
//!
//! Design notes:
//!
//! * **Everything is resumable.** Partial data lands in a sibling `.part` file.
//!   If the launcher dies mid-download, the next run sends `Range:` and picks
//!   up where it stopped. A finished file is verified before the `.part` is
//!   promoted, so a corrupted cache never masquerades as a good one.
//! * **Concurrency is bounded** by a semaphore (assets use a wider window than
//!   large jars), because a few hundred simultaneous TLS handshakes is the
//!   fastest way to get rate-limited by Mojang.
//! * **Retries are bounded and backed off**, and only for errors that can
//!   plausibly succeed later (timeouts, 5xx, 429).
//! * **Cancellation** is checked between chunks, so a cancelled install exits
//!   in milliseconds and leaves a valid `.part` behind.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;
use serde::Serialize;
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;

use crate::error::{AppError, AppResult, ErrorKind};

/// Number of retries *after* the first attempt.
const MAX_RETRIES: u32 = 4;
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum DownloadEvent {
    /// A single file changed state.
    File { id: String, bytes: u64, total: u64, done: bool, failed: bool },
    /// Aggregate view of a whole job.
    Aggregate { job: String, done_files: u32, total_files: u32, bytes: u64, total_bytes: u64 },
    /// Human readable note (e.g. "verifying checksum").
    Note { job: String, message: String },
}

/// Receives progress events. Implemented by the Tauri bridge.
pub type EventSink = Arc<dyn Fn(DownloadEvent) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct FileTask {
    pub id: String,
    pub url: String,
    pub path: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

impl FileTask {
    pub fn new(id: impl Into<String>, url: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        FileTask { id: id.into(), url: url.into(), path: path.into(), sha1: None, size: None }
    }

    pub fn with_sha1(mut self, sha1: Option<String>) -> Self {
        self.sha1 = sha1;
        self
    }

    pub fn with_size(mut self, size: Option<u64>) -> Self {
        self.size = size;
        self
    }

    fn part_path(&self) -> PathBuf {
        let mut name = self.path.file_name().map(|s| s.to_os_string()).unwrap_or_default();
        name.push(".part");
        self.path.with_file_name(name)
    }
}

/// Aggregated progress across many files.
pub struct Ledger {
    job: String,
    total_files: u32,
    done_files: u32,
    total_bytes: u64,
    done_bytes: u64,
    sink: EventSink,
    last_emit: std::time::Instant,
}

/// How often an aggregate update may cross the IPC boundary.
///
/// A 5000-file asset install calls `set_file_bytes` 5000 times; forwarding every
/// one of those to the webview is what made the window stutter while a large
/// download was running. The final state is always delivered by [`Ledger::flush`].
const EMIT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(120);

impl Ledger {
    pub fn new(job: impl Into<String>, total_files: u32, total_bytes: u64, sink: EventSink) -> Self {
        let mut ledger =
            Ledger { job: job.into(), total_files, done_files: 0, total_bytes, done_bytes: 0, sink, last_emit: std::time::Instant::now() };
        ledger.emit(true);
        ledger
    }

    pub fn add_file(&mut self, bytes: u64) {
        self.total_files += 1;
        self.total_bytes += bytes;
        self.emit(false);
    }

    pub fn set_file_bytes(&mut self, bytes: u64) {
        self.done_files = self.done_files.saturating_add(1);
        self.done_bytes = self.done_bytes.saturating_add(bytes);
        self.emit(false);
    }

    /// Send the current totals immediately, ignoring the throttle.
    ///
    /// Called once a batch finishes so a throttled last update cannot leave the
    /// progress bar stuck just short of the end.
    pub fn flush(&mut self) {
        self.emit(true);
    }

    pub fn note(&self, message: impl Into<String>) {
        (self.sink)(DownloadEvent::Note { job: self.job.clone(), message: message.into() });
    }

    fn emit(&mut self, force: bool) {
        if !force && self.last_emit.elapsed() < EMIT_INTERVAL {
            return;
        }
        self.last_emit = std::time::Instant::now();
        (self.sink)(DownloadEvent::Aggregate {
            job: self.job.clone(),
            done_files: self.done_files,
            total_files: self.total_files,
            bytes: self.done_bytes,
            total_bytes: self.total_bytes,
        });
    }
}

/// Shared HTTP client with sensible connection pooling.
///
/// Two clients exist: `json` for Mojang metadata (small, text) and `bulk` for
/// CDN payloads. Pool sizes are small on purpose — a low-end PC benefits more
/// from staying idle than from saturating the uplink.
pub fn build_clients(user_agent: &str) -> (reqwest::Client, reqwest::Client) {
    let builder = || {
        reqwest::Client::builder()
            .user_agent(user_agent)
            .connect_timeout(CONNECT_TIMEOUT)
            .pool_max_idle_per_host(8)
            .https_only(false)
            .redirect(reqwest::redirect::Policy::limited(5))
    };
    let json = builder().timeout(std::time::Duration::from_secs(30)).build().expect("valid client");
    let bulk = builder().build().expect("valid client");
    (json, bulk)
}

fn is_retryable(status: reqwest::StatusCode) -> bool {
    status.is_server_error() || status.as_u16() == 429 || status.as_u16() == 408
}

fn backoff(attempt: u32) -> std::time::Duration {
    let base = 400u64.saturating_mul(1u64 << attempt.min(5));
    std::time::Duration::from_millis(base.min(8_000))
}

pub fn sha1_file(path: &Path) -> AppResult<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| AppError::io("Не удалось открыть файл", &e))?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| AppError::io("Ошибка чтения файла", &e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn check_cancel(cancel: &AtomicBool) -> AppResult<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(AppError::cancelled())
    } else {
        Ok(())
    }
}

/// Sidecar recording that a file at this path was hashed successfully.
///
/// Hashing every library on every launch is real work — the shared library
/// folder is over a hundred megabytes — and it is repeated work: nothing about
/// the file changes between launches. The note is only trusted while the file's
/// size *and* modification time still match what was hashed, so an edited or
/// replaced file is hashed again.
fn verified_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|s| s.to_os_string()).unwrap_or_default();
    name.push(".verified");
    path.with_file_name(name)
}

fn mtime_nanos(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn read_verified(path: &Path) -> Option<(String, u64, i64)> {
    let text = std::fs::read_to_string(verified_path(path)).ok()?;
    let mut parts = text.split_whitespace();
    let sha1 = parts.next()?.to_string();
    if sha1.len() != 40 {
        return None;
    }
    let size: u64 = parts.next()?.parse().ok()?;
    let mtime: i64 = parts.next()?.parse().ok()?;
    Some((sha1, size, mtime))
}

fn write_verified(path: &Path, sha1: &str) {
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::write(verified_path(path), format!("{sha1} {} {}\n", meta.len(), mtime_nanos(&meta)));
    }
}

fn forget_verified(path: &Path) {
    let _ = std::fs::remove_file(verified_path(path));
}

/// Download (or resume) a single file and verify it.
pub async fn ensure_file(
    http: &reqwest::Client,
    task: &FileTask,
    cancel: &AtomicBool,
    sink: &EventSink,
) -> AppResult<u64> {
    check_cancel(cancel)?;

    // Fast path: already present and valid.
    if task.path.exists() {
        if let Some(expected) = &task.sha1 {
            if let Ok(meta) = std::fs::metadata(&task.path) {
                if let Some((sha1, size, mtime)) = read_verified(&task.path) {
                    if sha1.eq_ignore_ascii_case(expected)
                        && size == meta.len()
                        && mtime == mtime_nanos(&meta)
                    {
                        emit_file(sink, &task.id, meta.len(), meta.len(), true, false);
                        return Ok(meta.len());
                    }
                }
            }
            match sha1_file(&task.path) {
                Ok(actual) if actual.eq_ignore_ascii_case(expected) => {
                    let len = std::fs::metadata(&task.path).map(|m| m.len()).unwrap_or(0);
                    write_verified(&task.path, &actual);
                    emit_file(sink, &task.id, len, len, true, false);
                    return Ok(len);
                }
                Ok(_) => {
                    // Corrupt cache entry: drop it and download again.
                    crate::log_warn!("mc::download", "checksum mismatch, re-downloading {}", task.path.display());
                    let _ = std::fs::remove_file(&task.path);
                    forget_verified(&task.path);
                }
                Err(_) => {}
            }
        } else {
            let len = std::fs::metadata(&task.path).map(|m| m.len()).unwrap_or(0);
            if task.size.map(|s| s == len).unwrap_or(true) {
                emit_file(sink, &task.id, len, len, true, false);
                return Ok(len);
            }
            let _ = std::fs::remove_file(&task.path);
            forget_verified(&task.path);
        }
    }

    if let Some(parent) = task.path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::io("Не удалось создать папку кэша", &e))?;
    }

    let part = task.part_path();
    let mut last_error: Option<AppError> = None;

    for attempt in 0..=MAX_RETRIES {
        check_cancel(cancel)?;
        match download_once(http, task, &part, cancel, sink).await {
            Ok(len) => return Ok(len),
            Err(err) => {
                if err.kind == ErrorKind::Cancelled {
                    return Err(err);
                }
                crate::log_warn!(
                    "mc::download",
                    "attempt {}/{} failed for {}: {}",
                    attempt + 1,
                    MAX_RETRIES + 1,
                    task.id,
                    err
                );
                // A permanent answer (404, 403) does not become a success by
                // being asked five more times. Retrying regardless used to cost
                // five attempts plus backoff *per file*; with a few thousand
                // assets that is hours of waiting for a result that never
                // changes.
                if !err.retryable {
                    emit_file(sink, &task.id, 0, task.size.unwrap_or(0), false, true);
                    return Err(err);
                }
                last_error = Some(err);
                tokio::time::sleep(backoff(attempt)).await;
            }
        }
    }

    emit_file(sink, &task.id, 0, task.size.unwrap_or(0), false, true);
    Err(last_error.unwrap_or_else(|| {
        AppError::new(ErrorKind::Network, format!("Не удалось скачать {}", task.id)).retryable(true)
    }))
}

fn emit_file(sink: &EventSink, id: &str, bytes: u64, total: u64, done: bool, failed: bool) {
    sink(DownloadEvent::File { id: id.to_string(), bytes, total, done, failed });
}

async fn download_once(
    http: &reqwest::Client,
    task: &FileTask,
    part: &Path,
    cancel: &AtomicBool,
    sink: &EventSink,
) -> AppResult<u64> {
    let mut existing = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);

    // A `.part` longer than the expected size can only be garbage.
    if let Some(expected) = task.size {
        if existing > expected {
            let _ = std::fs::remove_file(part);
            existing = 0;
        }
    }

    let mut request = http.get(&task.url);
    if existing > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }

    let response = request.send().await.map_err(|e| {
        AppError::new(ErrorKind::Unreachable, "Сервер недоступен")
            .with_detail(e.to_string())
            .retryable(true)
    })?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::new(ErrorKind::Network, format!("HTTP {}", status.as_u16()))
            .with_detail(task.url.clone())
            .retryable(is_retryable(status)));
    }

    // 200 instead of 206 means the range was ignored: start over.
    if existing > 0 && status != reqwest::StatusCode::PARTIAL_CONTENT {
        let _ = std::fs::remove_file(part);
        existing = 0;
    }

    let total = response
        .content_length()
        .map(|rest| rest + existing)
        .or(task.size)
        .unwrap_or(0);

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(existing > 0)
        .truncate(existing == 0)
        .open(part)
        .await
        .map_err(|e| AppError::io("Не удалось открыть файл загрузки", &e))?;

    let mut written = existing;
    let mut stream = response.bytes_stream();
    let mut last_emit = util_now();

    while let Some(chunk) = stream.next().await {
        check_cancel(cancel)?;
        let chunk = chunk.map_err(|e| {
            AppError::new(ErrorKind::Network, "Ошибка при передаче данных")
                .with_detail(e.to_string())
                .retryable(true)
        })?;
        file.write_all(&chunk).await.map_err(|e| AppError::io("Ошибка записи файла", &e))?;
        written += chunk.len() as u64;
        let now = util_now();
        if now - last_emit > 120 {
            last_emit = now;
            emit_file(sink, &task.id, written, total.max(written), false, false);
        }
    }

    file.flush().await.map_err(|e| AppError::io("Не удалось сбросить файл", &e))?;
    drop(file);

    if let Some(expected) = task.size {
        if expected != written {
            return Err(AppError::new(
                ErrorKind::Checksum,
                format!("Размер файла не совпадает (получено {written}, ожидалось {expected})"),
            )
            .retryable(true));
        }
    }

    if let Some(expected) = &task.sha1 {
        let actual = sha1_file(part)?;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(part);
            return Err(AppError::new(ErrorKind::Checksum, "Контрольная сумма не совпала")
                .with_detail(format!("sha1 {actual} != {expected}"))
                .retryable(true));
        }
    }

    tokio::fs::rename(part, &task.path)
        .await
        .map_err(|e| AppError::io("Не удалось сохранить загруженный файл", &e))?;

    if let Some(expected) = &task.sha1 {
        // The file was just hashed above, so the note is a fact, not a guess.
        write_verified(&task.path, expected);
    }

    emit_file(sink, &task.id, written, written, true, false);
    Ok(written)
}

fn util_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Fetch a small text/JSON document with retries.
pub async fn fetch_text(http: &reqwest::Client, url: &str) -> AppResult<String> {
    let mut last: Option<AppError> = None;
    for attempt in 0..=MAX_RETRIES {
        match http.get(url).send().await {
            Ok(resp) if resp.status().is_success() => {
                return resp.text().await.map_err(|e| {
                    AppError::new(ErrorKind::Network, "Не удалось прочитать ответ")
                        .with_detail(e.to_string())
                });
            }
            Ok(resp) => {
                let status = resp.status();
                last = Some(
                    AppError::new(ErrorKind::Network, format!("HTTP {}", status.as_u16()))
                        .with_detail(url.to_string())
                        .retryable(is_retryable(status)),
                );
            }
            Err(e) => {
                last = Some(
                    AppError::new(ErrorKind::Unreachable, "Сервер недоступен")
                        .with_detail(e.to_string())
                        .retryable(true),
                );
            }
        }
        // Only transient failures are worth asking again.
        if !last.as_ref().is_some_and(|e| e.retryable) {
            break;
        }
        tokio::time::sleep(backoff(attempt)).await;
    }
    Err(last.unwrap_or_else(|| AppError::new(ErrorKind::Network, "Запрос не выполнен")))
}

/// Run many tasks with a bounded number in flight.
pub async fn run_bounded<T, F, Fut>(
    items: Vec<T>,
    limit: usize,
    cancel: Arc<AtomicBool>,
    handler: F,
) -> Vec<AppResult<()>>
where
    T: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + Clone + 'static,
    Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
{
    let semaphore = Arc::new(tokio::sync::Semaphore::new(limit.max(1)));
    let handler = Arc::new(handler);
    let mut handles: Vec<
        std::pin::Pin<Box<dyn std::future::Future<Output = AppResult<()>> + Send>>,
    > = Vec::with_capacity(items.len());

    for item in items {
        let semaphore = semaphore.clone();
        let handler = handler.clone();
        let cancel = cancel.clone();
        handles.push(Box::pin(async move {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::cancelled());
            }
            let _permit = match semaphore.acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => return Err(AppError::cancelled()),
            };
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::cancelled());
            }
            handler(item).await
        }));
    }

    futures_util::future::join_all(handles).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sink() -> EventSink {
        Arc::new(|_| {})
    }

    #[test]
    fn part_path_is_a_sibling_file() {
        let task = FileTask::new("a", "https://x/y", PathBuf::from("libs/dep.jar"));
        let part = task.part_path();
        assert_eq!(part, PathBuf::from("libs/dep.jar.part"));
        assert!(!part.as_os_str().to_string_lossy().ends_with('\\'));
    }

    #[test]
    fn retry_classification_matches_mojang_behaviour() {
        assert!(is_retryable(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable(reqwest::StatusCode::SERVICE_UNAVAILABLE));
        assert!(is_retryable(reqwest::StatusCode::REQUEST_TIMEOUT));
        assert!(!is_retryable(reqwest::StatusCode::NOT_FOUND));
        assert!(!is_retryable(reqwest::StatusCode::FORBIDDEN));
    }

    #[test]
    fn backoff_grows_and_is_capped() {
        assert!(backoff(0) < backoff(3));
        assert!(backoff(10) <= std::time::Duration::from_millis(8_000));
    }

    #[tokio::test]
    async fn cancelled_download_stops_before_any_request() {
        let dir = std::env::temp_dir().join("sifora-dl-test");
        std::fs::create_dir_all(&dir).unwrap();
        let cancel = Arc::new(AtomicBool::new(true));
        let http = reqwest::Client::new();
        let task = FileTask::new("x", "https://example.invalid/x", dir.join("x"));
        let err = ensure_file(&http, &task, &cancel, &sink()).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::Cancelled);
        assert!(!task.path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn bounded_runner_collects_results_in_order() {
        let cancel = Arc::new(AtomicBool::new(false));
        let items = vec![1u32, 2, 3, 4, 5, 6, 7, 8];
        let results = run_bounded(items, 2, cancel, |i| async move {
            if i % 3 == 0 {
                Err(AppError::internal("boom"))
            } else {
                Ok(())
            }
        })
        .await;
        assert_eq!(results.len(), 8);
        assert!(results[0].is_ok(), "item 1");
        assert!(results[2].is_err(), "item 3 must fail");
        assert!(results[5].is_err(), "item 6 must fail");
        assert!(results[7].is_ok(), "item 8");
    }

    #[test]
    fn only_transient_failures_are_retried() {
        // A missing file stays missing: retrying it five times per asset cost
        // hours of pointless waiting on a broken install.
        assert!(!is_retryable(reqwest::StatusCode::NOT_FOUND));
        assert!(!is_retryable(reqwest::StatusCode::FORBIDDEN));
        assert!(!is_retryable(reqwest::StatusCode::UNAUTHORIZED));
        assert!(!is_retryable(reqwest::StatusCode::BAD_REQUEST));
        // Genuinely transient answers are worth another attempt.
        assert!(is_retryable(reqwest::StatusCode::INTERNAL_SERVER_ERROR));
        assert!(is_retryable(reqwest::StatusCode::BAD_GATEWAY));
        assert!(is_retryable(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable(reqwest::StatusCode::REQUEST_TIMEOUT));
    }

    #[tokio::test]
    async fn bounded_runner_reports_cancellation() {
        let cancel = Arc::new(AtomicBool::new(true));
        let results = run_bounded(vec![1u32, 2], 2, cancel, |_| async { Ok(()) }).await;
        assert!(results.iter().all(|r| matches!(r, Err(e) if e.kind == ErrorKind::Cancelled)));
    }

    #[test]
    fn sha1_of_empty_file_is_the_well_known_value() {
        let dir = std::env::temp_dir().join("sifora-sha-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("empty");
        std::fs::write(&file, b"").unwrap();
        assert_eq!(sha1_file(&file).unwrap(), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file that was hashed once is noted, so the next launch can skip the
    /// work — but only while size *and* mtime still match.
    #[tokio::test]
    async fn a_verified_note_is_trusted_only_for_the_same_file() {
        let dir = std::env::temp_dir().join("sifora-verified-note-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("lib.jar");
        std::fs::write(&file, b"payload").unwrap();
        let sha1 = sha1_file(&file).unwrap();

        let cancel = Arc::new(AtomicBool::new(false));
        let http = reqwest::Client::new();
        let task = FileTask::new("x", "https://example.invalid/x", &file).with_sha1(Some(sha1.clone()));

        // First pass: hashes and records.
        assert_eq!(ensure_file(&http, &task, &cancel, &sink()).await.unwrap(), 7);
        assert!(verified_path(&file).exists(), "the note must be written");

        let noted = std::fs::read_to_string(verified_path(&file)).unwrap();
        let mut parts = noted.split_whitespace();
        assert_eq!(parts.next().unwrap(), sha1);

        // Second pass: the note still describes the file, so no request is made
        // (the URL is unreachable — success proves the short path was taken).
        assert_eq!(ensure_file(&http, &task, &cancel, &sink()).await.unwrap(), 7);

        // Same size, different bytes: the note is stale and must not be trusted.
        std::fs::write(&file, b"PAYLOAD").unwrap();
        let err = ensure_file(&http, &task, &cancel, &sink()).await.unwrap_err();
        assert_ne!(err.kind, ErrorKind::Cancelled);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_ledger_throttles_but_flush_always_reports() {
        let seen = Arc::new(parking_lot::Mutex::new(Vec::<u32>::new()));
        let sink_seen = seen.clone();
        let sink: EventSink = Arc::new(move |event| {
            if let DownloadEvent::Aggregate { done_files, .. } = event {
                sink_seen.lock().push(done_files);
            }
        });
        let mut ledger = Ledger::new("job", 0, 0, sink);
        for _ in 0..500 {
            ledger.add_file(1);
            ledger.set_file_bytes(1);
        }
        // 1000 updates in far less than the throttle window collapse into a
        // handful of events...
        assert!(seen.lock().len() < 20, "throttle did not apply: {:?}", seen.lock().len());
        // ...and the final totals still arrive exactly.
        ledger.flush();
        assert_eq!(*seen.lock().last().unwrap(), 500);
    }
}