#!/usr/bin/env python3
"""Guard shell startup rewrite helper parity for Issue 805 CFG-223."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ISSUE = ROOT / "issues/0805-roastty-ghostty-parity"


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise AssertionError(f"missing {label}: {needle!r}")


def require_all(text: str, needles: list[tuple[str, str]]) -> None:
    for needle, label in needles:
        require(text, needle, label)


def require_row(markdown: str, row_id: str) -> str:
    for line in markdown.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if cells and cells[0] == row_id:
            return line
    raise AssertionError(f"missing inventory row {row_id}")


def main() -> int:
    ghostty_shell = read("vendor/ghostty/src/termio/shell_integration.zig")
    roastty_shell = read("roastty/src/termio/shell_integration.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_shell,
        [
            ('test "force shell"', "Ghostty forced shell test"),
            ('test "shell integration failure"', "Ghostty setup failure test"),
            ("fn detectShell", "Ghostty shell detector"),
            ('test "bash"', "Ghostty bash rewrite test"),
            ('test "bash: unsupported options"', "Ghostty bash fallback test"),
            ('test "bash: inject flags"', "Ghostty bash inject flag test"),
            ('test "bash: rcfile"', "Ghostty bash rcfile test"),
            ('test "bash: HISTFILE"', "Ghostty bash HISTFILE test"),
            ('test "bash: ENV"', "Ghostty bash ENV preservation test"),
            (
                'test "bash: additional arguments"',
                "Ghostty bash separator argument test",
            ),
            ('test "bash: missing resources"', "Ghostty bash missing resource test"),
            ('test "xdg: empty XDG_DATA_DIRS"', "Ghostty XDG default test"),
            ('test "xdg: existing XDG_DATA_DIRS"', "Ghostty XDG prepend test"),
            ('test "xdg: missing resources"', "Ghostty XDG missing resource test"),
            ('test "nushell"', "Ghostty nushell rewrite test"),
            (
                'test "nushell: unsupported options"',
                "Ghostty nushell fallback test",
            ),
            (
                'test "nushell: missing resources"',
                "Ghostty nushell missing resource test",
            ),
            ('test "zsh"', "Ghostty zsh rewrite test"),
            ('test "zsh: ZDOTDIR"', "Ghostty zsh ZDOTDIR test"),
            ('test "zsh: missing resources"', "Ghostty zsh missing resource test"),
        ],
    )

    require_all(
        roastty_shell,
        [
            (
                "detect_shell_matches_supported_programs",
                "Roastty shell detection test",
            ),
            (
                "force_shell_overrides_detection_for_all_supported_shells",
                "Roastty forced shell test",
            ),
            ("bash_setup_rewrites_args_and_env", "Roastty bash rewrite test"),
            ("bash_unsupported_options_fall_back", "Roastty bash fallback test"),
            (
                "bash_setup_inject_flags_rcfiles_history_env_and_separators",
                "Roastty bash detailed rewrite test",
            ),
            (
                "bash_setup_missing_resources_falls_back_without_env_changes",
                "Roastty bash missing resource test",
            ),
            ("zsh_setup_preserves_zdotdir", "Roastty zsh rewrite test"),
            ("xdg_setup_prepends_data_dirs", "Roastty XDG prepend test"),
            (
                "xdg_setup_uses_freedesktop_default_when_unset",
                "Roastty XDG default test",
            ),
            (
                "xdg_setup_missing_resources_falls_back_without_env_changes",
                "Roastty XDG missing resource test",
            ),
            ("nushell_setup_adds_execute_use", "Roastty nushell rewrite test"),
            (
                "nushell_unsupported_options_keep_xdg_env_without_command_rewrite",
                "Roastty nushell fallback test",
            ),
            (
                "nushell_setup_missing_resources_falls_back_without_env_changes",
                "Roastty nushell missing resource test",
            ),
            ("missing_resources_fall_back", "Roastty zsh missing resource test"),
            ('"ROASTTY_BASH_ENV"', "Roastty bash ENV name"),
            ('"ROASTTY_BASH_INJECT"', "Roastty bash inject name"),
            ('"ROASTTY_BASH_RCFILE"', "Roastty bash rcfile name"),
            ('"ROASTTY_BASH_UNEXPORT_HISTFILE"', "Roastty bash HISTFILE name"),
            ('"ROASTTY_SHELL_INTEGRATION_XDG_DIR"', "Roastty XDG name"),
            ('"ROASTTY_ZSH_ZDOTDIR"', "Roastty zsh ZDOTDIR name"),
            ('"use roastty *"', "Roastty nushell module name"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-009B2B2B3B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-009B2B2B3B1 status"),
            ("shell-specific startup rewrite", "shell rewrite behavior"),
            (
                "shell_startup_rewrite_runtime_parity.py",
                "static parity guard evidence",
            ),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-009B2B2B3B2B2B2B3")
    require_all(
        row_gap,
        [
            ("Oracle complete", "RUNTIME-009B2B2B3B2B2B2B3 status"),
            ("terminal-runtime residual audit", "terminal residual audit"),
        ],
    )
    if "shell-specific startup rewrite" in row_gap:
        raise AssertionError("remaining terminal gap still claims shell startup rewrites")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 status"),
            ("89 rows Oracle complete", "CFG-223 oracle count"),
            ("92 rows closed", "CFG-223 closed count"),
            ("1 rows are incomplete", "CFG-223 incomplete count"),
            ("1 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("shell_startup_rewrite_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
