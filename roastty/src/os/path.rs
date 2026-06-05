//! Filesystem path helpers (port of upstream `os/path`).

use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

/// An error from `expand` (upstream's `error.PathTooLong` / `error.AccessDenied` plus
/// propagated I/O errors).
#[derive(Debug)]
pub(crate) enum ExpandError {
    /// The combined directory + command path exceeded `PATH_MAX`.
    PathTooLong,
    /// No match was found, but a candidate was access-denied.
    AccessDenied,
    /// Another I/O error from opening/stat-ing a candidate.
    Io(std::io::Error),
}

/// Search for `cmd` in `PATH` and return the absolute path of the matching executable, or
/// `None` if not found (upstream `os.path.expand`). A `cmd` containing `/` is returned
/// as-is (assumed absolute/relative).
pub(crate) fn expand(cmd: &str) -> Result<Option<PathBuf>, ExpandError> {
    // If the command already contains a slash, return it as-is because it is assumed to be
    // absolute or relative.
    if cmd.contains('/') {
        return Ok(Some(PathBuf::from(cmd)));
    }

    match std::env::var_os("PATH") {
        Some(path_var) => expand_in(cmd, &path_var),
        None => Ok(None),
    }
}

/// The PATH-searching core, parameterized over the `PATH` value for testability. `cmd` is
/// assumed not to contain `/` (the caller handles that case).
fn expand_in(cmd: &str, path_var: &OsStr) -> Result<Option<PathBuf>, ExpandError> {
    // PATH_MAX is 1024 on macOS, the same bound as upstream's `std.fs.max_path_bytes`.
    const MAX_PATH_BYTES: usize = libc::PATH_MAX as usize;

    let mut seen_eacces = false;
    for dir in std::env::split_paths(path_var) {
        // Upstream's tokenizeScalar skips empty PATH components; split_paths does not.
        if dir.as_os_str().is_empty() {
            continue;
        }

        // dir + '/' + cmd must fit, mirroring upstream's fixed-buffer guard.
        if dir.as_os_str().len() + cmd.len() + 1 > MAX_PATH_BYTES {
            return Err(ExpandError::PathTooLong);
        }

        // Build `dir + "/" + cmd` by raw byte concatenation (upstream emits one '/' even
        // when dir already ends with '/', so the result bytes match exactly).
        let mut full_os = OsString::with_capacity(dir.as_os_str().len() + 1 + cmd.len());
        full_os.push(dir.as_os_str());
        full_os.push("/");
        full_os.push(cmd);
        let full = PathBuf::from(full_os);

        let file = match std::fs::File::open(&full) {
            Ok(file) => file,
            Err(err) => match err.kind() {
                std::io::ErrorKind::NotFound => continue,
                std::io::ErrorKind::PermissionDenied => {
                    // Accumulate this and return it later so we can try other paths that we
                    // have access to.
                    seen_eacces = true;
                    continue;
                }
                _ => return Err(ExpandError::Io(err)),
            },
        };

        let metadata = file.metadata().map_err(ExpandError::Io)?;
        if !metadata.is_dir() && is_executable(metadata.permissions().mode()) {
            return Ok(Some(full));
        }
    }

    if seen_eacces {
        return Err(ExpandError::AccessDenied);
    }

    Ok(None)
}

fn is_executable(mode: u32) -> bool {
    mode & 0o111 != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;

    /// A unique temp directory that removes itself on drop.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!("roastty-exp542-{}-{}", std::process::id(), tag));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        /// Create an executable file in the temp dir and return its name.
        fn add_executable(&self, name: &str) {
            let file_path = self.path.join(name);
            let mut file = std::fs::File::create(&file_path).expect("create exec file");
            file.write_all(b"#!/bin/sh\n").expect("write exec file");
            let mut perms = file.metadata().expect("metadata").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&file_path, perms).expect("chmod");
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn expand_finds_real_executable() {
        // `uname` is the *nix equivalent of `hostname.exe` on Windows.
        let path = expand("uname").expect("expand ok").expect("uname found");
        assert!(path.as_os_str().len() > "uname".len());
    }

    #[test]
    fn expand_missing_returns_none() {
        let path = expand("thisreallyprobablydoesntexist123").expect("expand ok");
        assert_eq!(path, None);
    }

    #[test]
    fn expand_slash_passthrough() {
        let path = expand("foo/env").expect("expand ok").expect("passthrough");
        assert_eq!(path, PathBuf::from("foo/env"));
        assert_eq!(path.as_os_str().len(), 7);
    }

    #[test]
    fn expand_in_skips_empty_components() {
        let dir = TempDir::new("empty");
        dir.add_executable("tool");

        // Leading and trailing empty components (`:{tmp}:`) must be skipped, not treated
        // as the current directory.
        let mut path_var = OsString::from(":");
        path_var.push(dir.path.as_os_str());
        path_var.push(":");

        let found = expand_in("tool", &path_var)
            .expect("expand ok")
            .expect("tool found");
        assert_eq!(found, dir.path.join("tool"));
    }

    #[test]
    fn expand_in_preserves_trailing_slash() {
        let dir = TempDir::new("slash");
        dir.add_executable("tool");

        // A PATH entry ending in '/' yields a raw `dir + "/" + cmd` with a `//`.
        let mut path_var = OsString::from(dir.path.as_os_str());
        path_var.push("/");

        let found = expand_in("tool", &path_var)
            .expect("expand ok")
            .expect("tool found");
        assert!(
            found.as_os_str().as_bytes().windows(2).any(|w| w == b"//"),
            "expected a doubled slash in {found:?}",
        );
    }
}
