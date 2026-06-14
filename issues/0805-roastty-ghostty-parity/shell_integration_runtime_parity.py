#!/usr/bin/env python3
"""Check shell-integration and terminal identity runtime wiring for Issue 805."""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]

GHOSTTY_EXEC = ROOT / "vendor/ghostty/src/termio/Exec.zig"
GHOSTTY_SHELL_INTEGRATION = ROOT / "vendor/ghostty/src/termio/shell_integration.zig"
GHOSTTY_SURFACE = ROOT / "vendor/ghostty/src/Surface.zig"
GHOSTTY_CONFIG = ROOT / "vendor/ghostty/src/config/Config.zig"
ROASTTY_TERMIO = ROOT / "roastty/src/termio.rs"
ROASTTY_SHELL_INTEGRATION = ROOT / "roastty/src/termio/shell_integration.rs"
ROASTTY_LIB = ROOT / "roastty/src/lib.rs"
ROASTTY_CONFIG = ROOT / "roastty/src/config/mod.rs"


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def require_all(source: str, needles: list[str], message: str) -> None:
    missing = [needle for needle in needles if needle not in source]
    if missing:
        raise AssertionError(f"{message}: missing {missing!r}")


def main() -> int:
    ghostty_exec = read(GHOSTTY_EXEC)
    ghostty_shell = read(GHOSTTY_SHELL_INTEGRATION)
    ghostty_surface = read(GHOSTTY_SURFACE)
    ghostty_config = read(GHOSTTY_CONFIG)
    roastty_termio = read(ROASTTY_TERMIO)
    roastty_shell = read(ROASTTY_SHELL_INTEGRATION)
    roastty_lib = read(ROASTTY_LIB)
    roastty_config = read(ROASTTY_CONFIG)

    require_all(
        ghostty_config,
        [
            '@"shell-integration": ShellIntegration = .detect',
            '@"shell-integration-features": ShellIntegrationFeatures = .{}',
            'term: []const u8 = "xterm-ghostty"',
        ],
        "Pinned Ghostty shell integration config fields changed",
    )
    require_all(
        ghostty_surface,
        [
            '.shell_integration = config.@"shell-integration"',
            '.shell_integration_features = config.@"shell-integration-features"',
            ".term = config.term",
        ],
        "Pinned Ghostty Surface no longer passes shell integration options to termio Exec",
    )
    require_all(
        ghostty_exec,
        [
            'try env.put("GHOSTTY_RESOURCES_DIR", dir)',
            'try env.put("TERM", cfg.term)',
            'try env.put("COLORTERM", "truecolor")',
            'try env.put("TERMINFO", dir)',
            'try env.put("TERM", "xterm-256color")',
        ],
        "Pinned Ghostty terminal identity env setup changed",
    )
    require_all(
        ghostty_shell,
        [
            '"GHOSTTY_SHELL_FEATURES"',
            '"cursor:blink"',
            '"cursor:steady"',
            '"ssh-env"',
            '"ssh-terminfo"',
            "setupXdgDataDirs",
            '"GHOSTTY_SHELL_INTEGRATION_XDG_DIR"',
            '"XDG_DATA_DIRS"',
            "fn setupZsh(",
            '"GHOSTTY_ZSH_ZDOTDIR"',
            '"{s}/shell-integration/zsh"',
        ],
        "Pinned Ghostty shell integration env/setup markers changed",
    )

    require_all(
        roastty_config,
        [
            "pub shell_integration: ShellIntegration",
            "pub shell_integration_features: ShellIntegrationFeatures",
            'term: "xterm-roastty".to_string()',
            '"shell-integration" =>',
            '"shell-integration-features" =>',
        ],
        "Roastty config shell integration fields/parser wiring is missing",
    )
    require_all(
        roastty_lib,
        [
            "shell_integration: config.shell_integration",
            "shell_integration_features: config.shell_integration_features",
            "term: config.term.clone()",
        ],
        "Roastty surface startup does not pass shell integration options to TermioSpawnOptions",
    )
    require_all(
        roastty_termio,
        [
            "pub(crate) shell_integration: crate::config::ShellIntegration",
            "pub(crate) shell_integration_features: crate::config::ShellIntegrationFeatures",
            'pub(crate) term: String',
            "setup_terminal_identity(&mut env, options.resource_dir.as_deref(), &options.term)",
            "shell_integration::setup_features(",
            "shell_integration::setup(command, resource_dir, options.shell_integration)",
            "apply_env_overrides(&mut env, env_override)",
            'put_env(env, "TERM", term.to_string())',
            'put_env(env, "COLORTERM", "truecolor".to_string())',
            'put_env(env, "TERM", "xterm-256color".to_string())',
            "spawn_with_options_sets_fallback_terminal_identity_without_resources",
            "termio_env_explicit_overrides_win_after_terminal_identity",
            "spawn_with_options_sets_resource_terminal_identity",
            "spawn_with_options_resource_identity_overwrites_inherited_env",
            "termio_env_spawn_with_options_explicit_env_overrides_shell_integration_env",
            "spawn_with_options_sets_shell_feature_env_even_when_integration_is_none",
            "zsh_integration_spawn_with_options_reaches_child_env",
            "zsh_integration_spawn_with_options_sources_inherited_zdotdir",
        ],
        "Roastty Termio shell integration runtime wiring or tests are missing",
    )
    require(
        re.search(r'put_env\(\s*env,\s*"ROASTTY_RESOURCES_DIR"', roastty_termio)
        is not None
        and re.search(r'put_env\(\s*env,\s*"TERMINFO"', roastty_termio) is not None,
        "Roastty terminal resource identity env setup is missing",
    )
    require(
        re.search(
            r"setup_terminal_identity\(.*?\);\s*shell_integration::setup_features\(.*?\);\s*if options\.shell_integration\.enabled\(\).*?apply_env_overrides",
            roastty_termio,
            flags=re.DOTALL,
        )
        is not None,
        "Roastty Termio setup order no longer lets explicit env override identity/shell env",
    )
    require_all(
        roastty_shell,
        [
            "pub(crate) fn setup_features",
            '"ROASTTY_SHELL_FEATURES"',
            '"cursor:blink"',
            '"cursor:steady"',
            '"ssh-env"',
            '"ssh-terminfo"',
            "fn setup_xdg_data_dirs",
            '"ROASTTY_SHELL_INTEGRATION_XDG_DIR"',
            '"XDG_DATA_DIRS"',
            "fn setup_zsh",
            '"ROASTTY_ZSH_ZDOTDIR"',
            "xdg_setup_prepends_data_dirs",
            "xdg_setup_uses_freedesktop_default_when_unset",
            "nushell_setup_adds_execute_use",
        ],
        "Roastty shell integration helpers or guards are missing",
    )

    print("shell_integration_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
