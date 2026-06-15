#!/usr/bin/env python3
"""Guard clipboard-write device-attributes runtime parity for Issue 805 CFG-223."""

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
    ghostty_config = read("vendor/ghostty/src/config/Config.zig")
    ghostty_termio = read("vendor/ghostty/src/termio/Termio.zig")
    ghostty_stream = read("vendor/ghostty/src/termio/stream_handler.zig")
    roastty_config = read("roastty/src/config/mod.rs")
    roastty_device_attributes = read("roastty/src/terminal/device_attributes.rs")
    roastty_terminal = read("roastty/src/terminal/terminal.rs")
    roastty_termio = read("roastty/src/termio.rs")
    roastty_lib = read("roastty/src/lib.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_config,
        [
            ('@"clipboard-write": ClipboardAccess = .allow', "Ghostty config field"),
            (
                "Whether to allow programs running in the terminal to read/write to the",
                "Ghostty clipboard-write documentation",
            ),
        ],
    )
    require_all(
        ghostty_termio,
        [
            ("clipboard_write: configpkg.ClipboardAccess", "Ghostty derived config field"),
            ('config.@"clipboard-write"', "Ghostty parsed config handoff"),
        ],
    )
    require_all(
        ghostty_stream,
        [
            ("clipboard_write: configpkg.ClipboardAccess", "Ghostty stream state"),
            ("pub fn changeConfig", "Ghostty runtime config update hook"),
            (
                "self.clipboard_write = config.clipboard_write",
                "Ghostty runtime update assignment",
            ),
            (
                "if (self.clipboard_write != .deny)",
                "Ghostty clipboard-write DA branch",
            ),
            (r'"\x1B[?62;22;52c"', "Ghostty DA with clipboard feature"),
            (r'"\x1B[?62;22c"', "Ghostty DA without clipboard feature"),
        ],
    )

    require_all(
        roastty_config,
        [
            ("pub clipboard_write: ClipboardAccess", "Roastty parsed config field"),
            ('"clipboard-write"', "Roastty config key"),
        ],
    )
    require_all(
        roastty_device_attributes,
        [
            ("fn with_clipboard_write", "Roastty DA clipboard helper"),
            ("attrs.primary.features.push(52)", "Roastty DA feature 52"),
        ],
    )
    require_all(
        roastty_terminal,
        [
            ("clipboard_write: ClipboardAccess", "Roastty terminal owned config"),
            ("pub(crate) fn set_clipboard_write", "Roastty runtime setter"),
            (
                "Attributes::with_clipboard_write",
                "Roastty terminal DA helper use",
            ),
            (
                "!self.clipboard_write.denied()",
                "Roastty deny-only DA clipboard predicate",
            ),
            (
                "terminal_stream_device_attributes_clipboard_write_config_and_runtime_update",
                "Roastty terminal startup/update test",
            ),
            (
                "terminal_stream_device_attributes_clipboard_write_callback_precedence",
                "Roastty callback precedence test",
            ),
        ],
    )
    require_all(
        roastty_termio,
        [
            (
                "pub(crate) clipboard_write: crate::config::ClipboardAccess",
                "Roastty Termio spawn option",
            ),
            (
                "clipboard_write: options.clipboard_write",
                "Roastty Termio init handoff",
            ),
            (
                "termio_device_attributes_clipboard_write_reaches_child_pty",
                "Roastty PTY child DA test",
            ),
        ],
    )
    require_all(
        roastty_lib,
        [
            (
                "terminal.set_clipboard_write(parsed.clipboard_write)",
                "Roastty live config update",
            ),
            (
                "clipboard_write: config.clipboard_write",
                "Roastty startup config handoff",
            ),
            (
                "surface_device_attributes_clipboard_write_runtime_startup_and_update",
                "Roastty surface config DA test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-009B2B2B3B2B2B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-009B2B2B3B2B2B1 status"),
            (
                "`clipboard-write` primary device-attributes clipboard capability advertisement",
                "clipboard-write DA behavior",
            ),
            (
                "terminal_stream_device_attributes_clipboard_write_config_and_runtime_update",
                "terminal update evidence",
            ),
            (
                "termio_device_attributes_clipboard_write_reaches_child_pty",
                "PTY evidence",
            ),
            (
                "surface_device_attributes_clipboard_write_runtime_startup_and_update",
                "config update evidence",
            ),
            (
                "clipboard_device_attributes_runtime_parity.py",
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

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 status"),
            ("69 rows Oracle complete", "CFG-223 oracle count"),
            ("72 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("clipboard_device_attributes_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
