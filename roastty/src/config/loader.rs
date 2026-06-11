//! Config-file loading (port of upstream `cli/args` `LineIterator`).
//!
//! Parses config-file lines into `(key, value)` pairs that drive `Config::set`. The
//! multi-line driver and file IO are layered on top of this per-line extraction.
#![allow(dead_code)]

use super::DefaultConfigPaths;
use std::path::PathBuf;

pub(crate) const ROASTTY_BUNDLE_ID: &str = "com.termsurf.roastty";
pub(crate) const ROASTTY_XDG_CONFIG_LEGACY: &str = "roastty/config";
pub(crate) const ROASTTY_XDG_CONFIG_PREFERRED: &str = "roastty/config.roastty";
pub(crate) const ROASTTY_XDG_THEMES: &str = "roastty/themes";
pub(crate) const ROASTTY_APP_CONFIG_LEGACY: &str = "config";
pub(crate) const ROASTTY_APP_CONFIG_PREFERRED: &str = "config.roastty";

/// Read an environment variable, treating an empty value as unset (upstream
/// `getenvNotEmpty`).
fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Resolve the XDG config directory from explicit env values (upstream `xdg.dir`'s
/// core for macOS): `$XDG_CONFIG_HOME` (joined with `subdir`, or used as-is with no
/// subdir) when present; else `$HOME/.config` joined with `subdir`; else `None`
/// (upstream `error.NoHomeDir`). `xdg_config_home` / `home` are the **non-empty** env
/// values (`None` when unset or empty); `home` is the `$HOME` fallback input (not
/// Zig's higher-precedence `opts.home`).
fn resolve_xdg_config(
    xdg_config_home: Option<&str>,
    home: Option<&str>,
    subdir: Option<&str>,
) -> Option<PathBuf> {
    if let Some(xdg) = xdg_config_home {
        let mut p = PathBuf::from(xdg);
        if let Some(s) = subdir {
            p.push(s);
        }
        return Some(p);
    }
    if let Some(home) = home {
        let mut p = PathBuf::from(home);
        p.push(".config");
        if let Some(s) = subdir {
            p.push(s);
        }
        return Some(p);
    }
    None
}

/// The XDG config directory (upstream `internal_os.xdg.config` for macOS): reads
/// `$XDG_CONFIG_HOME` / `$HOME` from the environment and resolves the config path.
pub(crate) fn xdg_config_dir(subdir: Option<&str>) -> Option<PathBuf> {
    resolve_xdg_config(
        env_nonempty("XDG_CONFIG_HOME").as_deref(),
        env_nonempty("HOME").as_deref(),
        subdir,
    )
}

/// The user theme directory (upstream `config.theme.Location.user`): the XDG
/// config directory with Roastty's renamed `roastty/themes` subdir.
pub(crate) fn user_theme_dir() -> Option<PathBuf> {
    xdg_config_dir(Some(ROASTTY_XDG_THEMES))
}

/// Resolve the macOS Application Support config path from the `$HOME` value (upstream
/// `macos.appSupportDir` / `commonDir`): `$HOME/Library/Application
/// Support/<bundle_id>/<sub_path>`, or `None` when `$HOME` is unset.
fn resolve_app_support(home: Option<&str>, bundle_id: &str, sub_path: &str) -> Option<PathBuf> {
    let home = home?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join(bundle_id)
            .join(sub_path),
    )
}

/// The macOS Application Support config path (upstream `macos.appSupportDir`): reads
/// `$HOME` and resolves `$HOME/Library/Application Support/<bundle_id>/<sub_path>`.
pub(crate) fn app_support_dir(bundle_id: &str, sub_path: &str) -> Option<PathBuf> {
    resolve_app_support(env_nonempty("HOME").as_deref(), bundle_id, sub_path)
}

/// Build Roastty's default config file candidates from explicit env values.
pub(crate) fn default_config_paths_from_home(
    xdg_config_home: Option<&str>,
    home: Option<&str>,
) -> DefaultConfigPaths {
    DefaultConfigPaths {
        legacy_xdg: resolve_xdg_config(xdg_config_home, home, Some(ROASTTY_XDG_CONFIG_LEGACY)),
        preferred_xdg: resolve_xdg_config(
            xdg_config_home,
            home,
            Some(ROASTTY_XDG_CONFIG_PREFERRED),
        ),
        legacy_app_support: resolve_app_support(home, ROASTTY_BUNDLE_ID, ROASTTY_APP_CONFIG_LEGACY),
        preferred_app_support: resolve_app_support(
            home,
            ROASTTY_BUNDLE_ID,
            ROASTTY_APP_CONFIG_PREFERRED,
        ),
    }
}

/// Build Roastty's default config file candidates from the process environment.
pub(crate) fn default_config_paths() -> DefaultConfigPaths {
    let xdg_config_home = env_nonempty("XDG_CONFIG_HOME");
    let home = env_nonempty("HOME");
    let mut paths = default_config_paths_from_home(xdg_config_home.as_deref(), home.as_deref());
    if !cfg!(target_os = "macos") {
        paths.legacy_app_support = None;
        paths.preferred_app_support = None;
    }
    paths
}

/// Parse one config-file line into a `(key, value)` pair (upstream
/// `cli.args.LineIterator.next`'s per-line logic). Returns `None` for a blank line or
/// a `#` comment. A line with `=` yields `(key, Some(value))` with the key and value
/// `" \t"`-trimmed and the value's surrounding double quotes stripped (not decoded —
/// the per-field parsers decode any inner escapes); a line with no `=` yields
/// `(key, None)` (a bare key).
///
/// The line must already be split on `\n` (no trailing newline); the surrounding
/// trim removes `" \t\r"` (so a CRLF line's `\r` is handled).
pub(crate) fn parse_config_line(line: &str) -> Option<(&str, Option<&str>)> {
    let edge = |c: char| c == ' ' || c == '\t' || c == '\r';
    let ws = |c: char| c == ' ' || c == '\t';

    let trimmed = line.trim_matches(edge);
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    match trimmed.find('=') {
        Some(idx) => {
            let key = trimmed[..idx].trim_matches(ws);
            let mut value = trimmed[idx + 1..].trim_matches(ws);
            if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
                value = &value[1..value.len() - 1];
            }
            Some((key, Some(value)))
        }
        None => Some((trimmed, None)),
    }
}

/// Parse one CLI argument into a `(key, value)` config pair (upstream
/// `cli.args.parse`'s per-arg logic). A `--key=value` argument yields
/// `(key, Some(value))` and a `--key` argument yields `(key, None)`; the first `=`
/// splits the key from the value. A non-`--` argument is not a config flag and yields
/// `None` (upstream records an "invalid field" diagnostic). roastty does not support
/// positional arguments or space-separated values.
pub(crate) fn parse_cli_arg(arg: &str) -> Option<(&str, Option<&str>)> {
    let key = arg.strip_prefix("--")?;
    match key.find('=') {
        Some(idx) => Some((&key[..idx], Some(&key[idx + 1..]))),
        None => Some((key, None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config_line_extracts_key_value() {
        // A `key = value` line; whitespace around the key and value is trimmed.
        assert_eq!(
            parse_config_line("key = value"),
            Some(("key", Some("value")))
        );
        assert_eq!(
            parse_config_line("  key  =  value  "),
            Some(("key", Some("value")))
        );
        // A surrounding double-quoted value is unwrapped (not decoded).
        assert_eq!(
            parse_config_line("key = \"a b\""),
            Some(("key", Some("a b")))
        );
        // An empty value after the `=`.
        assert_eq!(parse_config_line("key ="), Some(("key", Some(""))));
        // A bare key with no `=` carries no value.
        assert_eq!(parse_config_line("flag"), Some(("flag", None)));
        // A CRLF line: the trailing `\r` is trimmed.
        assert_eq!(
            parse_config_line("key=value\r"),
            Some(("key", Some("value")))
        );
    }

    #[test]
    fn parse_config_line_skips_blank_and_comments() {
        assert_eq!(parse_config_line(""), None);
        assert_eq!(parse_config_line("   "), None);
        assert_eq!(parse_config_line("# a comment"), None);
        // A comment is detected after the surrounding trim (leading spaces).
        assert_eq!(parse_config_line("   # x = y"), None);
    }

    #[test]
    fn resolve_xdg_config_precedence_and_fallback() {
        let p = |s: &str| PathBuf::from(s);

        // `$XDG_CONFIG_HOME` takes precedence over `$HOME`, joined with the subdir.
        assert_eq!(
            resolve_xdg_config(Some("/x"), Some("/h"), Some("roastty/config")),
            Some(p("/x/roastty/config"))
        );
        // `$XDG_CONFIG_HOME` with no subdir is used as-is.
        assert_eq!(resolve_xdg_config(Some("/x"), None, None), Some(p("/x")));
        // Only `$HOME`: `$HOME/.config` joined with the subdir.
        assert_eq!(
            resolve_xdg_config(None, Some("/h"), Some("roastty/config")),
            Some(p("/h/.config/roastty/config"))
        );
        // `$HOME` with no subdir is `$HOME/.config`.
        assert_eq!(
            resolve_xdg_config(None, Some("/h"), None),
            Some(p("/h/.config"))
        );
        // Neither set is `None` (upstream `NoHomeDir`).
        assert_eq!(resolve_xdg_config(None, None, Some("roastty/config")), None);
    }

    #[test]
    fn user_theme_dir_uses_roastty_themes_subdir() {
        assert_eq!(
            resolve_xdg_config(Some("/x"), Some("/h"), Some(ROASTTY_XDG_THEMES)),
            Some(PathBuf::from("/x/roastty/themes"))
        );
        assert_eq!(
            resolve_xdg_config(None, Some("/h"), Some(ROASTTY_XDG_THEMES)),
            Some(PathBuf::from("/h/.config/roastty/themes"))
        );
    }

    #[test]
    fn parse_cli_arg_extracts_flag_key_value() {
        // `--key=value` and `--key` forms.
        assert_eq!(
            parse_cli_arg("--fullscreen=non-native"),
            Some(("fullscreen", Some("non-native")))
        );
        assert_eq!(
            parse_cli_arg("--background-image-repeat"),
            Some(("background-image-repeat", None))
        );
        // The first `=` splits the key from the value.
        assert_eq!(parse_cli_arg("--key=a=b"), Some(("key", Some("a=b"))));
        // An empty value after the `=`.
        assert_eq!(parse_cli_arg("--key="), Some(("key", Some(""))));
        // `--` alone is an empty key with no value.
        assert_eq!(parse_cli_arg("--"), Some(("", None)));
        // A non-`--` argument is not a config flag.
        assert_eq!(parse_cli_arg("key=value"), None);
        assert_eq!(parse_cli_arg("-h"), None);
    }

    #[test]
    fn resolve_app_support_builds_path() {
        // `$HOME/Library/Application Support/<bundle_id>/<sub_path>`.
        assert_eq!(
            resolve_app_support(Some("/h"), "com.termsurf.roastty", "config"),
            Some(PathBuf::from(
                "/h/Library/Application Support/com.termsurf.roastty/config"
            ))
        );
        // `$HOME` unset is `None`.
        assert_eq!(
            resolve_app_support(None, "com.termsurf.roastty", "config"),
            None
        );
    }

    #[test]
    fn default_config_paths_from_home_builds_roastty_candidates() {
        let paths = default_config_paths_from_home(Some("/xdg"), Some("/home/tester"));
        assert_eq!(paths.legacy_xdg, Some(PathBuf::from("/xdg/roastty/config")));
        assert_eq!(
            paths.preferred_xdg,
            Some(PathBuf::from("/xdg/roastty/config.roastty"))
        );
        assert_eq!(
            paths.legacy_app_support,
            Some(PathBuf::from(
                "/home/tester/Library/Application Support/com.termsurf.roastty/config"
            ))
        );
        assert_eq!(
            paths.preferred_app_support,
            Some(PathBuf::from(
                "/home/tester/Library/Application Support/com.termsurf.roastty/config.roastty"
            ))
        );

        let paths = default_config_paths_from_home(None, Some("/home/tester"));
        assert_eq!(
            paths.legacy_xdg,
            Some(PathBuf::from("/home/tester/.config/roastty/config"))
        );
        assert_eq!(
            paths.preferred_xdg,
            Some(PathBuf::from("/home/tester/.config/roastty/config.roastty"))
        );

        let paths = default_config_paths_from_home(None, None);
        assert_eq!(paths.legacy_xdg, None);
        assert_eq!(paths.preferred_xdg, None);
        assert_eq!(paths.legacy_app_support, None);
        assert_eq!(paths.preferred_app_support, None);
    }
}
