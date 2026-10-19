//! Small shared helpers with no external dependencies.

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch. Used for progress smoothing, retries and
/// monotonic ordering.
pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Seconds since the Unix epoch.
pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Minimal RFC 3339 / ISO 8601 UTC timestamp (`2026-01-02T03:04:05Z`).
///
/// Implemented by hand so the binary does not pull in a full date-time crate
/// for a handful of timestamps.
pub fn now_rfc3339() -> String {
    let secs = now_secs();
    let (y, mo, d, h, mi, s) = civil_from_unix(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Civil date from a Unix timestamp (Howard Hinnant's algorithm).
pub fn civil_from_unix(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, (rem / 3600) as u32, ((rem % 3600) / 60) as u32, (rem % 60) as u32)
}

/// Write a file atomically: temp file in the same directory, fsync, rename.
///
/// Keeps settings and themes readable even if the app is killed mid-write.
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    if let Ok(file) = std::fs::File::open(&tmp) {
        let _ = file.sync_all();
    }
    std::fs::rename(&tmp, path)
}

/// Escape a string for safe inclusion inside a Windows command-line argument.
///
/// Windows `CommandLineToArgvW` treats backslashes as escape characters when
/// they precede a quote. Doubling trailing backslashes (and escaping embedded
/// quotes) prevents argument-splitting surprises.
pub fn windows_arg_escape(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len() + 8);
    let mut backslashes = 0usize;
    for ch in arg.chars() {
        match ch {
            '\\' => {
                backslashes += 1;
            }
            '"' => {
                out.extend(std::iter::repeat('\\').take(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            c => {
                out.extend(std::iter::repeat('\\').take(backslashes));
                backslashes = 0;
                out.push(c);
            }
        }
    }
    out.extend(std::iter::repeat('\\').take(backslashes * 2));
    out
}

/// Normalise a path for display without leaking the user's home directory in
/// the UI.
pub fn display_path(path: &std::path::Path, home: Option<&std::path::Path>) -> String {
    if let Some(home) = home {
        if let Ok(rest) = path.strip_prefix(home) {
            return format!("~{}", rest.display());
        }
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates_are_correct() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil_from_unix(1_767_225_845), (2026, 1, 1, 0, 4, 5));
        assert_eq!(civil_from_unix(951_782_400), (2000, 2, 29, 0, 0, 0));
    }

    #[test]
    fn timestamp_shape_is_iso8601() {
        let s = now_rfc3339();
        assert_eq!(s.len(), 20, "{s}");
        assert!(s.ends_with('Z'));
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[10..11], "T");
    }

    #[test]
    fn windows_arg_escaping_handles_trailing_backslashes_and_quotes() {
        assert_eq!(windows_arg_escape(r"C:\games\mc\"), r"C:\games\mc\\");
        assert_eq!(windows_arg_escape(r#"a"b"#), r#"a\"b"#);
        assert_eq!(windows_arg_escape("C:\\a\\\""), "C:\\a\\\\\\\"");
        assert_eq!(windows_arg_escape("plain"), "plain");
    }

    #[test]
    fn atomic_write_replaces_content() {
        let dir = std::env::temp_dir().join("sifora-util-test");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("x.json");
        write_atomic(&file, b"one").unwrap();
        write_atomic(&file, b"two").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "two");
        let _ = std::fs::remove_dir_all(&dir);
    }
}