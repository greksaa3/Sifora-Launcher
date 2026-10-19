//! Small in-memory ring-buffer logger.
//!
//! We deliberately avoid a logging framework: the release build must stay tiny
//! and must not spawn file-handling background threads. Recent records are kept
//! in a fixed-size ring and can be read from the renderer for diagnostics.

use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::OnceLock;

const CAPACITY: usize = 400;

#[derive(Debug, Clone, serde::Serialize)]
pub struct LogRecord {
    pub seq: u64,
    pub level: &'static str,
    pub target: String,
    pub message: String,
}

struct Inner {
    seq: u64,
    buf: VecDeque<LogRecord>,
    mirror_to_stderr: bool,
}

fn inner() -> &'static Mutex<Inner> {
    static CELL: OnceLock<Mutex<Inner>> = OnceLock::new();
    CELL.get_or_init(|| {
        Mutex::new(Inner {
            seq: 0,
            buf: VecDeque::with_capacity(CAPACITY),
            mirror_to_stderr: std::env::var_os("SIFORA_DEBUG").is_some(),
        })
    })
}

fn push(level: &'static str, target: &str, message: String) {
    let mut guard = inner().lock();
    guard.seq += 1;
    let record = LogRecord {
        seq: guard.seq,
        level,
        target: target.to_string(),
        message,
    };
    if guard.mirror_to_stderr {
        eprintln!("[{:>5} {}] {}", record.seq, level, record.message);
    }
    if guard.buf.len() == CAPACITY {
        guard.buf.pop_front();
    }
    guard.buf.push_back(record);
}

pub fn info(target: &str, message: impl Into<String>) {
    push("info", target, message.into());
}
pub fn warn(target: &str, message: impl Into<String>) {
    push("warn", target, message.into());
}
pub fn error(target: &str, message: impl Into<String>) {
    push("error", target, message.into());
}

/// Snapshot the tail of the log (newest last), capped at `limit` entries.
pub fn tail(limit: usize) -> Vec<LogRecord> {
    let guard = inner().lock();
    let skip = guard.buf.len().saturating_sub(limit);
    guard.buf.iter().skip(skip).cloned().collect()
}

#[macro_export]
macro_rules! log_info {
    ($target:expr, $($arg:tt)*) => { $crate::logbus::info($target, format!($($arg)*)) };
}
#[macro_export]
macro_rules! log_warn {
    ($target:expr, $($arg:tt)*) => { $crate::logbus::warn($target, format!($($arg)*)) };
}
#[macro_export]
macro_rules! log_error {
    ($target:expr, $($arg:tt)*) => { $crate::logbus::error($target, format!($($arg)*)) };
}