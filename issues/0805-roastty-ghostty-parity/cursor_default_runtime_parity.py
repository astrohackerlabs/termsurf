#!/usr/bin/env python3
"""Guard cursor default runtime parity for Issue 805 CFG-223."""

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
    roastty_terminal = read("roastty/src/terminal/terminal.rs")
    roastty_termio = read("roastty/src/termio.rs")
    roastty_lib = read("roastty/src/lib.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_config,
        [
            ('@"cursor-style": terminal.CursorStyle = .block', "Ghostty cursor-style field"),
            ('@"cursor-style-blink": ?bool = null', "Ghostty cursor-style-blink field"),
        ],
    )
    require_all(
        ghostty_termio,
        [
            ("cursor_style: terminalpkg.CursorStyle", "Ghostty derived cursor style"),
            ("cursor_blink: ?bool", "Ghostty derived cursor blink"),
            ('config.@"cursor-style"', "Ghostty cursor-style handoff"),
            ('config.@"cursor-style-blink"', "Ghostty cursor blink handoff"),
            ("term.screens.active.cursor.cursor_style = opts.config.cursor_style", "Ghostty startup cursor style"),
            (".default_cursor_style = opts.config.cursor_style", "Ghostty stream default style init"),
            (".default_cursor_blink = opts.config.cursor_blink", "Ghostty stream default blink init"),
        ],
    )
    require_all(
        ghostty_stream,
        [
            ("default_cursor: bool = true", "Ghostty default cursor state"),
            ("default_cursor_style: terminal.CursorStyle", "Ghostty default cursor style state"),
            ("default_cursor_blink: ?bool", "Ghostty default cursor blink state"),
            ("self.default_cursor_style = config.cursor_style", "Ghostty live style update"),
            ("self.default_cursor_blink = config.cursor_blink", "Ghostty live blink update"),
            ("if (self.default_cursor) self.setCursorStyle(.default)", "Ghostty default-only immediate update"),
            ("pub fn setCursorStyle", "Ghostty cursor style handler"),
            ("self.default_cursor = false", "Ghostty non-default cursor marker"),
            ("self.default_cursor = true", "Ghostty default cursor marker"),
            ("self.default_cursor_blink orelse true", "Ghostty blink fallback"),
            ("mode == .cursor_blinking", "Ghostty DEC mode 12 gate"),
            ("self.default_cursor_blink != null", "Ghostty explicit blink gate"),
            ("pub fn fullReset", "Ghostty full reset handler"),
            ("self.terminal.fullReset();", "Ghostty terminal full reset delegation"),
        ],
    )

    require_all(
        roastty_config,
        [
            ("pub cursor_style: CursorStyle", "Roastty parsed cursor-style field"),
            ("pub cursor_style_blink: Option<bool>", "Roastty parsed cursor-style-blink field"),
            ('"cursor-style"', "Roastty cursor-style key"),
            ('"cursor-style-blink"', "Roastty cursor-style-blink key"),
            ("pub(crate) fn to_terminal", "Roastty config-to-terminal cursor map"),
        ],
    )
    require_all(
        roastty_terminal,
        [
            ("default_cursor: bool", "Roastty default cursor state"),
            ("default_cursor_visual_style: cursor::VisualStyle", "Roastty default cursor style state"),
            ("default_cursor_blink: Option<bool>", "Roastty default cursor blink state"),
            ("pub(crate) fn set_cursor_defaults", "Roastty live cursor default setter"),
            ("if self.default_cursor", "Roastty default-only immediate update"),
            ("*self.default_cursor = false", "Roastty non-default cursor marker"),
            ("*self.default_cursor = true", "Roastty default cursor marker"),
            ("self.default_cursor_blink.unwrap_or(true)", "Roastty blink fallback"),
            ("mode == modes::Mode::CursorBlinking", "Roastty DEC mode 12 gate"),
            ("self.default_cursor_blink.is_some()", "Roastty explicit blink gate"),
            (
                "terminal_cursor_default_runtime_update_applies_when_default",
                "Roastty default update test",
            ),
            (
                "terminal_cursor_default_runtime_update_preserves_program_cursor_until_reset",
                "Roastty program cursor preservation test",
            ),
            (
                "terminal_cursor_default_runtime_blink_update_controls_dec_mode_12_gate",
                "Roastty blink gate update test",
            ),
            (
                "terminal_cursor_default_runtime_direct_reset_does_not_apply_configured_default",
                "Roastty direct reset parity test",
            ),
            (
                "terminal_cursor_default_runtime_ris_preserves_program_cursor_state_until_reset",
                "Roastty RIS cursor-state parity test",
            ),
        ],
    )
    require_all(
        roastty_termio,
        [
            ("pub(crate) cursor_visual_style: cursor::VisualStyle", "Roastty Termio style option"),
            ("pub(crate) cursor_blink: Option<bool>", "Roastty Termio blink option"),
            ("cursor_visual_style: options.cursor_visual_style", "Roastty Termio style handoff"),
            ("cursor_blink: options.cursor_blink", "Roastty Termio blink handoff"),
            (
                "termio_cursor_default_runtime_spawn_options_reach_terminal",
                "Roastty Termio cursor runtime test",
            ),
        ],
    )
    require_all(
        roastty_lib,
        [
            (
                "terminal.set_cursor_defaults(",
                "Roastty live config cursor update",
            ),
            (
                "parsed.cursor_style.to_terminal()",
                "Roastty parsed cursor-style surface handoff",
            ),
            (
                "parsed.cursor_style_blink",
                "Roastty parsed cursor blink surface handoff",
            ),
            (
                "cursor_visual_style: config.cursor_style.to_terminal()",
                "Roastty startup cursor-style handoff",
            ),
            (
                "cursor_blink: config.cursor_style_blink",
                "Roastty startup cursor blink handoff",
            ),
            (
                "surface_cursor_default_runtime_startup_and_update",
                "Roastty surface cursor runtime test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-009B2B2B3B2B2B2A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-009B2B2B3B2B2B2A status"),
            (
                "`cursor-style` and `cursor-style-blink` default cursor runtime effects",
                "cursor default behavior",
            ),
            (
                "terminal_cursor_default_runtime_update_applies_when_default",
                "terminal update evidence",
            ),
            (
                "terminal_cursor_default_runtime_update_preserves_program_cursor_until_reset",
                "program cursor preservation evidence",
            ),
            (
                "terminal_cursor_default_runtime_ris_preserves_program_cursor_state_until_reset",
                "RIS reset evidence",
            ),
            (
                "termio_cursor_default_runtime_spawn_options_reach_terminal",
                "PTY evidence",
            ),
            (
                "surface_cursor_default_runtime_startup_and_update",
                "surface update evidence",
            ),
            (
                "cursor_default_runtime_parity.py",
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
            ("75 rows Oracle complete", "CFG-223 oracle count"),
            ("78 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("cursor_default_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
