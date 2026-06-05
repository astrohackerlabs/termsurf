//! Bundled resources directory discovery.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

const ENV_RESOURCES_DIR: &str = "ROASTTY_RESOURCES_DIR";
const RESOURCE_SUBDIR: &str = "roastty";
const TERMINFO_SENTINEL: &str = "terminfo/78/xterm-roastty";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ResourcesDir {
    app_path: Option<PathBuf>,
    host_path: Option<PathBuf>,
}

impl ResourcesDir {
    fn app_path(path: PathBuf) -> Self {
        Self {
            app_path: Some(path),
            host_path: None,
        }
    }

    pub(crate) fn app(&self) -> Option<&Path> {
        self.app_path.as_deref()
    }

    pub(crate) fn host(&self) -> Option<&Path> {
        self.host_path.as_deref().or(self.app_path.as_deref())
    }
}

pub(crate) fn resources_dir() -> std::io::Result<ResourcesDir> {
    let env_override = std::env::var_os(ENV_RESOURCES_DIR);
    if !cfg!(debug_assertions) {
        if let Some(dir) = non_empty_env(&env_override) {
            return Ok(ResourcesDir::app_path(dir.into()));
        }
    }

    let exe = std::env::current_exe().ok();
    resolve_resources_dir(exe.as_deref(), env_override, cfg!(debug_assertions))
}

fn resolve_resources_dir(
    exe: Option<&Path>,
    env_override: Option<OsString>,
    debug: bool,
) -> std::io::Result<ResourcesDir> {
    if !debug {
        if let Some(dir) = non_empty_env(&env_override) {
            return Ok(ResourcesDir::app_path(dir.into()));
        }
    }

    let exe_available = exe.is_some();
    if let Some(exe) = exe {
        if let Some(dir) = detect_from_exe(exe) {
            return Ok(ResourcesDir::app_path(dir));
        }
    }

    if debug && exe_available {
        if let Some(dir) = non_empty_env(&env_override) {
            return Ok(ResourcesDir::app_path(dir.into()));
        }
    }

    Ok(ResourcesDir::default())
}

fn non_empty_env(value: &Option<OsString>) -> Option<&Path> {
    value
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(Path::new)
}

fn detect_from_exe(exe: &Path) -> Option<PathBuf> {
    let mut current = exe.parent();
    while let Some(dir) = current {
        if let Some(resources) = maybe_dir(dir, "Contents/Resources", TERMINFO_SENTINEL) {
            return Some(resources.join(RESOURCE_SUBDIR));
        }

        if let Some(resources) = maybe_dir(dir, "share", TERMINFO_SENTINEL) {
            return Some(resources.join(RESOURCE_SUBDIR));
        }

        current = dir.parent();
    }

    None
}

fn maybe_dir(base: &Path, sub: &str, sentinel: &str) -> Option<PathBuf> {
    let dir = base.join(sub);
    let sentinel = dir.join(sentinel);
    if sentinel.try_exists().unwrap_or(false) {
        Some(dir)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "roastty-exp621-{}-{counter}-{label}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn exe(&self, parts: &[&str]) -> PathBuf {
            parts
                .iter()
                .fold(self.path.clone(), |path, part| path.join(part))
        }

        fn create_sentinel(&self, base_parts: &[&str], sub: &str) -> PathBuf {
            let base = base_parts
                .iter()
                .fold(self.path.clone(), |path, part| path.join(part));
            let sentinel = base.join(sub).join(TERMINFO_SENTINEL);
            fs::create_dir_all(sentinel.parent().expect("sentinel parent"))
                .expect("create sentinel parent");
            fs::write(&sentinel, b"").expect("write sentinel");
            sentinel
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn host_falls_back_to_app() {
        let dir = ResourcesDir::app_path(PathBuf::from("/tmp/roastty"));

        assert_eq!(dir.app(), Some(Path::new("/tmp/roastty")));
        assert_eq!(dir.host(), Some(Path::new("/tmp/roastty")));
    }

    #[test]
    fn release_prefers_non_empty_env_override() {
        let temp = TempDir::new("release-env");
        temp.create_sentinel(&["Bundle.app"], "Contents/Resources");
        let exe = temp.exe(&["Bundle.app", "Contents", "MacOS", "roastty"]);
        let env = OsString::from("/override/resources");

        let dir = resolve_resources_dir(Some(&exe), Some(env), false).expect("resolve");

        assert_eq!(dir.app(), Some(Path::new("/override/resources")));
    }

    #[test]
    fn release_env_override_survives_missing_exe_path() {
        let env = OsString::from("/override/resources");

        let dir = resolve_resources_dir(None, Some(env), false).expect("resolve");

        assert_eq!(dir.app(), Some(Path::new("/override/resources")));
    }

    #[test]
    fn debug_prefers_detected_resources_over_env_override() {
        let temp = TempDir::new("debug-detect");
        temp.create_sentinel(&["Bundle.app"], "Contents/Resources");
        let exe = temp.exe(&["Bundle.app", "Contents", "MacOS", "roastty"]);
        let env = OsString::from("/override/resources");

        let dir = resolve_resources_dir(Some(&exe), Some(env), true).expect("resolve");

        assert_eq!(
            dir.app(),
            Some(
                temp.path()
                    .join("Bundle.app/Contents/Resources/roastty")
                    .as_path()
            )
        );
    }

    #[test]
    fn debug_falls_back_to_env_override_when_detection_misses() {
        let temp = TempDir::new("debug-env");
        let exe = temp.exe(&["bin", "roastty"]);
        let env = OsString::from("/override/resources");

        let dir = resolve_resources_dir(Some(&exe), Some(env), true).expect("resolve");

        assert_eq!(dir.app(), Some(Path::new("/override/resources")));
    }

    #[test]
    fn debug_missing_exe_path_returns_empty_even_with_env_override() {
        let env = OsString::from("/override/resources");

        let dir = resolve_resources_dir(None, Some(env), true).expect("resolve");

        assert_eq!(dir, ResourcesDir::default());
    }

    #[test]
    fn empty_env_override_is_ignored() {
        let dir = resolve_resources_dir(None, Some(OsString::new()), false).expect("resolve");

        assert_eq!(dir.app(), None);
        assert_eq!(dir.host(), None);
    }

    #[test]
    fn app_bundle_sentinel_resolves_contents_resources_subdir() {
        let temp = TempDir::new("app-bundle");
        temp.create_sentinel(&["Roastty.app"], "Contents/Resources");
        let exe = temp.exe(&["Roastty.app", "Contents", "MacOS", "roastty"]);

        let dir = resolve_resources_dir(Some(&exe), None, true).expect("resolve");

        assert_eq!(
            dir.app(),
            Some(
                temp.path()
                    .join("Roastty.app/Contents/Resources/roastty")
                    .as_path()
            )
        );
    }

    #[test]
    fn share_sentinel_resolves_share_subdir() {
        let temp = TempDir::new("share");
        temp.create_sentinel(&["prefix"], "share");
        let exe = temp.exe(&["prefix", "bin", "roastty"]);

        let dir = resolve_resources_dir(Some(&exe), None, true).expect("resolve");

        assert_eq!(
            dir.app(),
            Some(temp.path().join("prefix/share/roastty").as_path())
        );
    }

    #[test]
    fn app_bundle_wins_over_share_under_same_ancestor() {
        let temp = TempDir::new("precedence");
        temp.create_sentinel(&["Roastty.app"], "Contents/Resources");
        temp.create_sentinel(&["Roastty.app"], "share");
        let exe = temp.exe(&["Roastty.app", "Contents", "MacOS", "roastty"]);

        let dir = resolve_resources_dir(Some(&exe), None, true).expect("resolve");

        assert_eq!(
            dir.app(),
            Some(
                temp.path()
                    .join("Roastty.app/Contents/Resources/roastty")
                    .as_path()
            )
        );
    }

    #[test]
    fn missing_sentinels_return_empty_resources_dir() {
        let temp = TempDir::new("missing");
        let exe = temp.exe(&["bin", "roastty"]);

        let dir = resolve_resources_dir(Some(&exe), None, true).expect("resolve");

        assert_eq!(dir, ResourcesDir::default());
    }

    #[test]
    fn maybe_dir_ignores_missing_sentinel() {
        let temp = TempDir::new("maybe-missing");

        assert_eq!(maybe_dir(temp.path(), "share", TERMINFO_SENTINEL), None);
    }
}
