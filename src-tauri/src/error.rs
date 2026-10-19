//! Typed error surface shared by every command.
//!
//! Commands return `Result<T, AppError>`; `AppError` serialises to a stable
//! `{ kind, message, ... }` object so the frontend can react without parsing
//! free-form strings. Messages are deliberately non-sensitive: they are shown
//! to the user and written to the local log.

use std::fmt;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorKind {
    /// Bad input coming from the renderer.
    InvalidInput,
    /// A file/asset was requested but is not present in any cache.
    NotFound,
    /// Remote side answered with an error status.
    Network,
    /// Remote side is unreachable / TLS failed.
    Unreachable,
    /// Downloaded bytes did not match the expected digest.
    Checksum,
    /// Local IO failure.
    Io,
    /// Configuration could not be parsed or is out of range.
    Config,
    /// The requested Minecraft version cannot be launched by this build.
    Unsupported,
    /// Theme / design-token validation rejected the value.
    Theme,
    /// Java runtime discovery or provisioning failed.
    Java,
    /// Game process failed to spawn.
    Spawn,
    /// Operation was cancelled by the user.
    Cancelled,
    /// Something went wrong that has no better classification.
    Internal,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppError {
    pub kind: ErrorKind,
    /// Human readable, safe to display.
    pub message: String,
    /// Extra machine-readable context (paths, status codes, ...).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Coarse retry hint for the UI.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub retryable: bool,
}

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), detail: None, retryable: false }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn retryable(mut self, value: bool) -> Self {
        self.retryable = value;
        self
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidInput, msg)
    }
    pub fn config(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Config, msg)
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, msg)
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, msg)
    }
    pub fn cancelled() -> Self {
        Self::new(ErrorKind::Cancelled, "Операция отменена")
    }
    pub fn unsupported(msg: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unsupported, msg)
    }

    /// Keep internal paths out of the user-facing message but still reachable
    /// for debugging through the log bus.
    pub fn io(context: &str, err: &std::io::Error) -> Self {
        Self {
            kind: ErrorKind::Io,
            message: context.to_string(),
            detail: Some(err.to_string()),
            retryable: true,
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.detail {
            Some(d) => write!(f, "{} ({})", self.message, d),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError {
            kind: ErrorKind::Io,
            message: "Ошибка файловой системы".into(),
            detail: Some(e.to_string()),
            retryable: true,
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::new(ErrorKind::Config, "Не удалось разобрать конфигурацию")
            .with_detail(e.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        let kind = if e.is_timeout() {
            ErrorKind::Network
        } else if e.is_connect() || e.is_request() || e.is_builder() {
            ErrorKind::Unreachable
        } else {
            ErrorKind::Network
        };
        AppError { kind, message: "Сетевая ошибка".into(), detail: Some(e.to_string()), retryable: true }
    }
}

pub type AppCommandResult<T> = Result<T, AppError>;