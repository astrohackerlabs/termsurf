#!/usr/bin/env python3
"""Guard the title/PWD fallback runtime split for Issue 805 CFG-223."""

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
    ghostty_stream = read("vendor/ghostty/src/termio/stream_handler.zig")
    ghostty_surface = read("vendor/ghostty/src/Surface.zig")
    roastty_terminal = read("roastty/src/terminal/terminal.rs")
    roastty_termio = read("roastty/src/termio.rs")
    roastty_lib = read("roastty/src/lib.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_stream,
        [
            ("seen_title: bool = false", "Ghostty seen_title state"),
            ("if (title.len == 0)", "Ghostty empty-title branch"),
            ("self.seen_title = false;", "Ghostty clears seen_title"),
            ("self.seen_title = true;", "Ghostty marks explicit title"),
            ("if (!self.seen_title)", "Ghostty PWD fallback gate"),
            ("try self.windowTitle(path);", "Ghostty PWD title fallback"),
            ("try self.windowTitle(\"\");", "Ghostty PWD clear blank title"),
            ("self.surfaceMessageWriter(.{ .set_title = buf });", "Ghostty set_title message"),
        ],
    )
    require_all(
        ghostty_surface,
        [
            ("if (self.config.title != null)", "Ghostty static title suppression"),
            (".set_title => |*v|", "Ghostty surface set_title message handler"),
        ],
    )

    require_all(
        roastty_terminal,
        [
            ("seen_explicit: bool", "Roastty explicit title state"),
            ("pending_title_updates: Vec<String>", "Roastty pending title event queue"),
            ("take_pending_title_updates", "Roastty drains pending title events"),
            ("fn window_title(&mut self, title: &str)", "Roastty title state machine"),
            ("fn report_pwd(&mut self, url: &str)", "Roastty PWD state machine"),
            ("set_explicit", "Roastty explicit title setter"),
            ("set_fallback", "Roastty fallback title setter"),
            ("terminal_stream_title_pwd_fallback_state_machine", "terminal fallback test"),
            (
                "terminal_stream_title_pwd_fallback_queues_noop_title_events",
                "terminal no-op title event test",
            ),
            (
                "terminal_stream_title_pwd_fallback_preserves_multiple_events_in_one_slice",
                "terminal ordered title event test",
            ),
        ],
    )
    require_all(
        roastty_termio,
        [
            ("let titles = self.terminal.take_pending_title_updates();", "Termio drains title events"),
            ("|| !pump.titles.is_empty()", "Termio worker emits title pumps"),
            ("TermioWorkerError::TerminalCallbacksInstalled", "callback rejection intact"),
            (
                "termio_title_pwd_fallback_worker_emits_empty_title_pump",
                "Termio empty title test",
            ),
            (
                "termio_title_pwd_fallback_worker_emits_pwd_title_pump",
                "Termio PWD fallback title test",
            ),
            (
                "termio_title_pwd_fallback_worker_preserves_multiple_title_events",
                "Termio ordered title event test",
            ),
        ],
    )
    require_all(
        roastty_lib,
        [
            ("if self.static_title.is_none()", "surface static title suppression"),
            (
                "surface_title_pwd_fallback_empty_title_dispatches",
                "surface empty title dispatch test",
            ),
            (
                "surface_title_pwd_fallback_dispatches_multiple_titles_in_order",
                "surface ordered title dispatch test",
            ),
            (
                "surface_title_pwd_fallback_static_title_suppresses_empty_and_fallback",
                "surface static title suppression test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-009B2B2B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-009B2B2B1 status"),
            ("stored-PWD title fallback", "stored-PWD fallback evidence"),
            ("empty title", "empty title evidence"),
            ("explicit titles suppress", "explicit title suppression evidence"),
            ("blank/same-string empty-title", "no-op title dispatch evidence"),
            ("in order", "ordered title event evidence"),
            ("title_pwd_fallback_runtime_parity.py", "static guard evidence"),
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
            ("73 rows Oracle complete", "CFG-223 oracle count"),
            ("76 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
