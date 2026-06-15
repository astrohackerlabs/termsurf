#!/usr/bin/env python3
"""Guard active cursor priority runtime parity for Issue 805 CFG-223."""

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
    ghostty_cursor = read("vendor/ghostty/src/renderer/cursor.zig")
    roastty_cursor = read("roastty/src/renderer/cursor.rs")
    roastty_frame = read("roastty/src/renderer/frame_renderer.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_cursor,
        [
            ("if (state.cursor.viewport == null) return null;", "Ghostty viewport gate"),
            ("if (opts.preedit) return .block;", "Ghostty preedit priority"),
            (
                "if (state.cursor.password_input) return .lock;",
                "Ghostty password priority",
            ),
            ("if (!state.cursor.visible) return null;", "Ghostty hidden cursor gate"),
            ("if (!opts.focused) return .block_hollow;", "Ghostty focus hollowing"),
            (
                "if (state.cursor.blinking and !opts.blink_visible)",
                "Ghostty blink gate",
            ),
            ("return .fromTerminal(state.cursor.visual_style);", "Ghostty fallback style"),
        ],
    )

    require_all(
        roastty_cursor,
        [
            ("pub(crate) fn style(state: &RenderStateScalar", "Roastty shared style helper"),
            ("if state.cursor_viewport.is_none()", "Roastty viewport gate"),
            ("if opts.preedit", "Roastty preedit priority"),
            ("if state.cursor_password_input", "Roastty password priority"),
            ("if !state.cursor_visible", "Roastty hidden cursor gate"),
            ("if !opts.focused", "Roastty focus hollowing"),
            (
                "if state.cursor_blinking && !opts.blink_visible",
                "Roastty blink gate",
            ),
            ("Style::from_terminal", "Roastty fallback style"),
        ],
    )

    require_all(
        roastty_frame,
        [
            ("FrameRenderState::from_terminal_for_frame(terminal, preedit.is_some())", "default active preedit wiring"),
            ("FrameRenderState::from_terminal_for_frame", "factored active frame state"),
            ("FrameCursorOptions::default().with_preedit(preedit)", "factored preedit option"),
            ("cursor_options.with_preedit(preedit.is_some())", "caller option preedit wiring"),
            ("cursor::style(", "active frame calls shared priority helper"),
            ("StyleOptions {", "active frame builds style options"),
            ("password_input: bool", "password input option"),
            (
                "cursor_priority_active_renderer_preedit_overrides_hidden_focus_and_blink",
                "preedit priority test",
            ),
            (
                "cursor_priority_active_renderer_password_overrides_hidden_and_blink",
                "password priority test",
            ),
            (
                "cursor_priority_active_renderer_preedit_beats_password",
                "preedit over password test",
            ),
            (
                "cursor_priority_active_renderer_viewport_absence_suppresses_priority",
                "viewport suppression test",
            ),
            (
                "cursor_priority_active_renderer_render_frame_uses_real_preedit_argument",
                "real render_frame preedit test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-008B2B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-008B2B1 status"),
            ("password/preedit cursor-style priority", "RUNTIME-008B2B1 behavior"),
            ("active frame renderer path", "RUNTIME-008B2B1 active path"),
            ("cursor_priority_active_renderer", "RUNTIME-008B2B1 tests"),
            ("cursor_priority_runtime_parity.py", "RUNTIME-008B2B1 guard"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-008B2B2B2B2B")
    require_all(
        row_gap,
        [
            ("Gap", "RUNTIME-008B2B2B2B2B status"),
            ("screenshot-level padding pixel proof", "RUNTIME-008B2B2B2B2B screenshot padding gap"),
            ("GUI cursor pixels", "RUNTIME-008B2B2B2B2B GUI cursor gap"),
            ("broader GUI/pixel parity", "RUNTIME-008B2B2B2B2B GUI parity gap"),
        ],
    )
    if "RUNTIME-008B2B |" in runtime_inventory:
        raise AssertionError("old broad RUNTIME-008B2B row is still present")

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

    print("cursor_priority_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
