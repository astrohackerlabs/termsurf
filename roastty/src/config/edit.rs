//! Selecting which config file to open for editing (port of upstream `config/edit`'s
//! `configPath`).

use std::io;
use std::path::{Path, PathBuf};

/// Choose the config path to open for editing from `candidates` (upstream `configPath`).
///
/// Precedence: the first **non-empty** candidate, else the first **existing** (empty) candidate,
/// else the first candidate. A candidate that does not exist (or whose path is malformed) is
/// skipped; any other IO error propagates. `candidates` must be non-empty.
pub(crate) fn config_path(candidates: &[PathBuf]) -> io::Result<&Path> {
    assert!(
        !candidates.is_empty(),
        "config_path requires at least one candidate"
    );

    let mut exists: Option<&Path> = None;
    for path in candidates {
        // Open first (upstream `openFileAbsolute`), then stat — so an unreadable file surfaces as
        // an open error rather than a successful `metadata` probe.
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            // Doesn't exist / malformed path → skip (upstream skips FileNotFound / BadPathName).
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::InvalidInput
                ) =>
            {
                continue
            }
            // Any other IO error propagates (upstream `else => return err`).
            Err(err) => return Err(err),
        };

        let meta = file.metadata()?; // upstream `try f.stat()` — propagates errors.
                                     // First non-empty file wins immediately.
        if meta.len() > 0 {
            return Ok(path);
        }
        // Otherwise remember the first existing (empty) file.
        if exists.is_none() {
            exists = Some(path);
        }
    }

    // No non-empty file → the first existing one; nothing exists → the first candidate.
    Ok(exists.unwrap_or_else(|| candidates[0].as_path()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    /// A unique temp directory for one test, removed on drop.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> TempDir {
            let path =
                std::env::temp_dir().join(format!("roastty-edit-{}-{}", std::process::id(), tag));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TempDir { path }
        }

        /// An absolute candidate path under this dir (the file is not created).
        fn candidate(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }

        fn write(&self, name: &str, contents: &[u8]) -> PathBuf {
            let p = self.candidate(name);
            let mut f = fs::File::create(&p).unwrap();
            f.write_all(contents).unwrap();
            p
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn first_non_empty_wins() {
        let dir = TempDir::new("first-non-empty");
        let empty = dir.write("empty", b"");
        let a = dir.write("a", b"x = 1");
        let b = dir.write("b", b"y = 2");
        let candidates = vec![empty, a.clone(), b];
        assert_eq!(config_path(&candidates).unwrap(), a.as_path());
    }

    #[test]
    fn first_existing_empty_is_the_fallback() {
        let dir = TempDir::new("existing-fallback");
        let missing = dir.candidate("missing");
        let empty_a = dir.write("empty_a", b"");
        let empty_b = dir.write("empty_b", b"");
        let candidates = vec![missing, empty_a.clone(), empty_b];
        assert_eq!(config_path(&candidates).unwrap(), empty_a.as_path());
    }

    #[test]
    fn nothing_exists_returns_first_candidate() {
        let dir = TempDir::new("nothing-exists");
        let m1 = dir.candidate("missing_1");
        let m2 = dir.candidate("missing_2");
        let candidates = vec![m1.clone(), m2];
        assert_eq!(config_path(&candidates).unwrap(), m1.as_path());
    }

    #[test]
    fn earlier_non_empty_beats_later_non_empty() {
        let dir = TempDir::new("ordering");
        let a = dir.write("a", b"first");
        let b = dir.write("b", b"second");
        let candidates = vec![a.clone(), b];
        assert_eq!(config_path(&candidates).unwrap(), a.as_path());
    }
}
