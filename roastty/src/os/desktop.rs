//! Desktop launch/environment helpers (macOS slice of upstream `os/desktop`).

use std::ffi::OsStr;

const ENV_MAC_LAUNCH_SOURCE: &str = "ROASTTY_MAC_LAUNCH_SOURCE";

/// Desktop environments Roastty distinguishes in its current macOS-only product scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesktopEnvironment {
    Macos,
    Other,
}

/// Detect the current desktop environment.
pub(crate) fn desktop_environment() -> DesktopEnvironment {
    #[cfg(target_os = "macos")]
    {
        DesktopEnvironment::Macos
    }
    #[cfg(not(target_os = "macos"))]
    {
        DesktopEnvironment::Other
    }
}

/// Return whether the process was launched from the macOS desktop (`Finder`/`open`).
pub(crate) fn launched_from_desktop() -> bool {
    #[cfg(target_os = "macos")]
    {
        launched_from_desktop_macos_rule(
            current_parent_pid(),
            std::env::var_os(ENV_MAC_LAUNCH_SOURCE).as_deref(),
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(target_os = "macos")]
fn current_parent_pid() -> libc::pid_t {
    unsafe { libc::getppid() }
}

fn launched_from_desktop_macos_rule(
    parent_pid: libc::pid_t,
    launch_source: Option<&OsStr>,
) -> bool {
    if launch_source == Some(OsStr::new("app")) {
        return true;
    }
    parent_pid == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_environment_matches_platform() {
        #[cfg(target_os = "macos")]
        assert_eq!(desktop_environment(), DesktopEnvironment::Macos);

        #[cfg(not(target_os = "macos"))]
        assert_eq!(desktop_environment(), DesktopEnvironment::Other);
    }

    #[test]
    fn app_launch_source_forces_desktop_launch() {
        assert!(launched_from_desktop_macos_rule(
            42,
            Some(OsStr::new("app"))
        ));
    }

    #[test]
    fn other_launch_sources_fall_back_to_parent_pid() {
        for source in [
            Some(OsStr::new("cli")),
            Some(OsStr::new("zig_run")),
            Some(OsStr::new("")),
            Some(OsStr::new("other")),
            None,
        ] {
            assert!(launched_from_desktop_macos_rule(1, source));
            assert!(!launched_from_desktop_macos_rule(2, source));
        }
    }

    #[test]
    fn parent_pid_one_is_desktop_launch_without_app_source() {
        assert!(launched_from_desktop_macos_rule(1, None));
    }

    #[test]
    fn non_init_parent_pid_is_not_desktop_launch_without_app_source() {
        assert!(!launched_from_desktop_macos_rule(2, None));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn launched_from_desktop_is_false_on_non_macos() {
        assert!(!launched_from_desktop());
    }
}
