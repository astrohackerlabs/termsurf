#!/usr/bin/env python3
"""Guard command-finished runtime parity for Issue 805 CFG-223."""

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
            (".end_input_start_output", "Ghostty semantic prompt start action"),
            ("self.surfaceMessageWriter(.start_command)", "Ghostty start command surface message"),
            (".end_command", "Ghostty semantic prompt stop action"),
            ("const raw: i32 = cmd.readOption(.exit_code) orelse 0", "Ghostty absent/malformed exit default"),
            ("std.math.cast(u8, raw) orelse 1", "Ghostty out-of-range exit fallback"),
            (".{ .stop_command = code }", "Ghostty stop command surface message"),
        ],
    )
    require_all(
        ghostty_surface,
        [
            ("command_timer: ?std.time.Instant = null", "Ghostty command timer state"),
            (".start_command => {", "Ghostty start timer branch"),
            ("self.command_timer = try .now()", "Ghostty start timer assignment"),
            (".stop_command => |v| timer: {", "Ghostty stop timer branch"),
            ("const start = self.command_timer orelse break :timer", "Ghostty stop without start noop"),
            ("self.command_timer = null", "Ghostty clears command timer"),
            (".command_finished", "Ghostty command finished action"),
            (".exit_code = v", "Ghostty exit code payload"),
            (".duration = duration", "Ghostty duration payload"),
        ],
    )

    require_all(
        roastty_terminal,
        [
            ("pub(crate) enum TerminalCommandEvent", "Roastty command event enum"),
            ("pending_command_events: Vec<TerminalCommandEvent>", "Roastty pending event queue"),
            ("take_pending_command_events", "Roastty event drain"),
            ("Action::EndInputStartOutput", "Roastty OSC 133 start handling"),
            ("TerminalCommandEvent::Start", "Roastty start event"),
            ("Action::EndCommand", "Roastty OSC 133 stop handling"),
            ("Some(code @ 0..=255) => code as u8", "Roastty valid exit mapping"),
            ("Some(_) => 1", "Roastty out-of-range exit mapping"),
            ("None => 0", "Roastty absent/malformed exit mapping"),
            ("TerminalCommandEvent::Stop { exit_code }", "Roastty stop event"),
            (
                "terminal_command_event_runtime_captures_osc133_without_display_side_effects",
                "Roastty terminal command event side-effect test",
            ),
            (
                "terminal_command_event_runtime_maps_osc133_exit_codes_like_ghostty",
                "Roastty terminal exit-code mapping test",
            ),
        ],
    )
    require_all(
        roastty_termio,
        [
            ("command_events: Vec<TerminalCommandEvent>", "Roastty pump command events"),
            ("let command_events = self.terminal.take_pending_command_events()", "Roastty pump drains events"),
            ("|| !pump.command_events.is_empty()", "Roastty worker emits command-only pumps"),
            (
                "termio_command_event_runtime_pump_reports_child_osc133",
                "Roastty termio child PTY command-event test",
            ),
        ],
    )
    require_all(
        roastty_lib,
        [
            ("ROASTTY_ACTION_COMMAND_FINISHED", "Roastty command-finished action tag"),
            ("command_finished: RoasttyActionCommandFinished", "Roastty action union member"),
            ("fn command_started_at", "Roastty explicit-time command start helper"),
            ("self.command_timer = Some(now)", "Roastty records command timer"),
            ("fn command_stopped_at", "Roastty command stop helper"),
            ("let Some(start) = self.command_timer.take()", "Roastty stop without start noop and clear"),
            ("now.duration_since(start).as_nanos()", "Roastty nanosecond duration"),
            ("fn perform_command_finished", "Roastty command-finished dispatch helper"),
            ("storage[0] = exit_code as usize", "Roastty exit-code storage"),
            ("storage[1] = duration as usize", "Roastty duration storage"),
            ("u.command_finished = RoasttyActionCommandFinished", "Roastty storage-to-union conversion"),
            ("ROASTTY_ACTION_COMMAND_FINISHED => {", "Roastty union-to-storage test conversion"),
            ("match event {\n                        terminal::terminal::TerminalCommandEvent::Start", "Roastty surface start event handling"),
            ("let _ = self.command_stopped_at(*exit_code, now)", "Roastty surface stop event handling"),
            (
                "surface_command_finished_runtime_dispatches_after_start_stop",
                "Roastty surface dispatch test",
            ),
            (
                "surface_command_finished_runtime_stop_without_start_does_not_dispatch",
                "Roastty stop-without-start test",
            ),
            (
                "surface_command_finished_runtime_repeated_start_resets_timer",
                "Roastty repeated-start reset test",
            ),
            (
                "surface_command_finished_runtime_pump_events_dispatch_and_mark_dirty",
                "Roastty pump dispatch/dirty test",
            ),
            (
                "surface_command_finished_runtime_child_exited_dispatch_remains",
                "Roastty child-exit coexistence test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-012B2B2B2A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-012B2B2B2A status"),
            ("command-finished notification runtime dispatch", "RUNTIME-012B2B2B2A behavior"),
            ("OSC 133", "RUNTIME-012B2B2B2A OSC 133 evidence"),
            ("exit-code mapping", "RUNTIME-012B2B2B2A exit mapping evidence"),
            ("ROASTTY_ACTION_COMMAND_FINISHED", "RUNTIME-012B2B2B2A action evidence"),
            ("command_finished_runtime_parity.py", "RUNTIME-012B2B2B2A guard evidence"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-012B2B2B2B2B3C")
    require_all(
        row_gap,
        [
            ("Gap", "RUNTIME-012B2B2B2B2B3C status"),
            ("actual OS notification delivery/banner/sound", "remaining OS delivery gap"),
            ("audible bell output", "remaining bell GUI gap"),
            ("real OS cursor pixels", "remaining hover cursor gap"),
            ("native link preview display", "remaining link preview gap"),
            ("external Launch Services handler delivery", "remaining external URL-handler gap"),
        ],
    )
    if "Command-finish notifications" in row_gap:
        raise AssertionError("remaining notification/link/bell gap still lists command-finish notifications")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("89 rows Oracle complete", "CFG-223 oracle count"),
            ("92 rows closed", "CFG-223 closed count"),
            ("1 rows are incomplete", "CFG-223 incomplete count"),
            ("1 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("command_finished_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
