#!/usr/bin/env python3
"""Guard the surface-title runtime split for Issue 805 CFG-223."""

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
    ghostty_surface = read("vendor/ghostty/src/Surface.zig")
    ghostty_stream = read("vendor/ghostty/src/termio/stream_handler.zig")
    roastty_lib = read("roastty/src/lib.rs")
    roastty_termio = read("roastty/src/termio.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_surface,
        [
            ("if (config.title) |title|", "Ghostty configured title startup branch"),
            (".set_title,", "Ghostty set_title action"),
            (".direct => |cmd_str| if (cmd_str.len != 0)", "Ghostty direct command title branch"),
            ("if (self.config.title != null)", "Ghostty static title suppresses terminal title"),
            ("if (config.title) |title| _ = try self.rt_app.performAction", "Ghostty update title branch"),
        ],
    )
    require_all(
        ghostty_stream,
        [
            ("self.surfaceMessageWriter(.{ .set_title = buf });", "Ghostty stream title message"),
            ("if (title.len == 0)", "Ghostty empty-title special case remains tracked"),
            ("If we haven't seen a title, use our pwd as the title.", "Ghostty PWD fallback remains tracked"),
        ],
    )

    require_all(
        roastty_termio,
        [
            ("pub(crate) titles: Vec<String>", "Roastty TermioPump title event field"),
            ("let titles = self.terminal.take_pending_title_updates();", "Roastty title pump events"),
            ("|| !pump.titles.is_empty()", "Roastty worker emits title pump"),
            ("TermioWorkerError::TerminalCallbacksInstalled", "Roastty callback rejection intact"),
            ("termio_title_worker_emits_non_empty_osc_title_pump", "Roastty Termio title test"),
        ],
    )
    require_all(
        roastty_lib,
        [
            ("static_title: Option<String>", "Roastty surface static title state"),
            ("self.static_title = parsed.title.clone();", "Roastty config update stores static title"),
            ("if let Some(title) = config.title.as_ref()", "Roastty configured title startup"),
            ("config::Command::Direct(args)", "Roastty direct command title branch"),
            ("if self.static_title.is_none()", "Roastty terminal title static-title gate"),
            ("surface_title_runtime_configured_title_dispatches_on_startup_and_update", "configured title test"),
            ("surface_title_runtime_direct_command_title_and_shell_noop", "direct/shell title test"),
            ("surface_title_runtime_non_empty_osc_title_dispatch_and_static_suppression", "OSC title gate test"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-009B2B2A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-009B2B2A status"),
            ("configured title", "configured title evidence"),
            ("direct command argv[0]", "direct command title evidence"),
            ("non-empty OSC title", "non-empty OSC title evidence"),
            ("static configured titles suppress", "static title evidence"),
            ("surface_title_runtime_parity.py", "static guard evidence"),
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
            ("58 rows Oracle complete", "CFG-223 oracle count"),
            ("60 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
