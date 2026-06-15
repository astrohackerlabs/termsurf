#!/usr/bin/env python3
"""Guard desktop notification runtime parity for Issue 805 CFG-223."""

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
    ghostty_osc9 = read("vendor/ghostty/src/terminal/osc/parsers/osc9.zig")
    ghostty_rxvt = read("vendor/ghostty/src/terminal/osc/parsers/rxvt_extension.zig")
    ghostty_surface_msg = read("vendor/ghostty/src/apprt/surface.zig")
    ghostty_stream_handler = read("vendor/ghostty/src/termio/stream_handler.zig")
    ghostty_surface = read("vendor/ghostty/src/Surface.zig")
    roastty_config = read("roastty/src/config/mod.rs")
    roastty_osc = read("roastty/src/terminal/osc.rs")
    roastty_terminal = read("roastty/src/terminal/terminal.rs")
    roastty_termio = read("roastty/src/termio.rs")
    roastty_lib = read("roastty/src/lib.rs")
    roastty_header = read("roastty/include/roastty.h")
    roastty_swift = read("roastty/macos/Sources/Roastty/Roastty.App.swift")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_config,
        [
            ('@"desktop-notifications": bool = true', "Ghostty desktop-notifications default"),
        ],
    )
    require_all(
        ghostty_osc9,
        [
            ("show_desktop_notification", "Ghostty OSC 9 command"),
            ('test "OSC 9: show desktop notification"', "Ghostty OSC 9 test"),
            ('title = ""', "Ghostty OSC 9 empty title"),
            ("body = data[0 .. data.len - 1 :0]", "Ghostty OSC 9 body"),
        ],
    )
    require_all(
        ghostty_rxvt,
        [
            ("show_desktop_notification", "Ghostty OSC 777 command"),
            ('test "OSC: OSC 777 show desktop notification with title"', "Ghostty OSC 777 test"),
        ],
    )
    require_all(
        ghostty_surface_msg,
        [
            ("desktop_notification: struct", "Ghostty surface message"),
            ("title: [63:0]u8", "Ghostty title buffer"),
            ("body: [255:0]u8", "Ghostty body buffer"),
        ],
    )
    require_all(
        ghostty_stream_handler,
        [
            ("fn showDesktopNotification", "Ghostty stream handler helper"),
            ("@min(title.len, message.desktop_notification.title.len)", "Ghostty title truncation"),
            ("message.desktop_notification.title[title_len] = 0", "Ghostty title nul"),
            ("@min(body.len, message.desktop_notification.body.len)", "Ghostty body truncation"),
            ("message.desktop_notification.body[body_len] = 0", "Ghostty body nul"),
            ("self.surfaceMessageWriter(message)", "Ghostty surface message dispatch"),
        ],
    )
    require_all(
        ghostty_surface,
        [
            ("if (!self.config.desktop_notifications)", "Ghostty desktop notification gate"),
            ("try self.showDesktopNotification(title, body)", "Ghostty app dispatch"),
        ],
    )

    require_all(
        roastty_config,
        [
            ("pub desktop_notifications: bool", "Roastty config field"),
            ("desktop_notifications: true", "Roastty default"),
            ('"desktop-notifications"', "Roastty config key"),
        ],
    )
    require_all(
        roastty_osc,
        [
            ("fn parse_osc9_notification", "Roastty OSC 9 parser"),
            ("fn parse_osc777_notification", "Roastty OSC 777 parser"),
            ("Command::DesktopNotification", "Roastty parser notification command"),
        ],
    )
    require_all(
        roastty_terminal,
        [
            ("pub(crate) struct TerminalDesktopNotification", "Roastty notification type"),
            ("pending_desktop_notifications", "Roastty terminal queue"),
            ("take_pending_desktop_notifications", "Roastty terminal drain"),
            ("terminal_desktop_notification_runtime_captures_osc_events_without_side_effects", "Roastty terminal runtime test"),
        ],
    )
    require_all(
        roastty_termio,
        [
            ("pub(crate) desktop_notifications: Vec<TerminalDesktopNotification>", "Roastty pump field"),
            ("take_pending_desktop_notifications", "Roastty termio drain"),
            ("termio_desktop_notification_runtime_pump_reports_child_osc", "Roastty termio runtime test"),
        ],
    )
    require_all(
        roastty_lib,
        [
            ("const DESKTOP_NOTIFICATION_TITLE_LIMIT: usize = 63", "Roastty title limit"),
            ("const DESKTOP_NOTIFICATION_BODY_LIMIT: usize = 255", "Roastty body limit"),
            ("fn nul_terminated_truncated", "Roastty nul truncation helper"),
            ("desktop_notifications: bool", "Roastty surface config cache"),
            ("self.desktop_notifications = parsed.desktop_notifications", "Roastty live config gate update"),
            ("fn perform_desktop_notification", "Roastty action helper"),
            ("ROASTTY_ACTION_DESKTOP_NOTIFICATION", "Roastty action tag"),
            ("surface_desktop_notification_runtime_dispatches_config_enabled_action", "Roastty enabled surface test"),
            ("surface_desktop_notification_runtime_suppresses_config_disabled_action", "Roastty disabled surface test"),
            ("surface_desktop_notification_runtime_truncates_overlong_payloads", "Roastty truncation test"),
        ],
    )
    require_all(
        roastty_header,
        [
            ("ROASTTY_ACTION_DESKTOP_NOTIFICATION = 31", "Roastty header action tag"),
            ("roastty_action_desktop_notification_s desktop_notification", "Roastty header action union"),
        ],
    )
    require_all(
        roastty_swift,
        [
            ("ROASTTY_ACTION_DESKTOP_NOTIFICATION", "Roastty Swift action switch"),
            ("showDesktopNotification(app, target: target, n: action.action.desktop_notification)", "Roastty Swift action payload"),
            ("String(cString: n.title!", "Roastty Swift title pointer read"),
            ("String(cString: n.body!", "Roastty Swift body pointer read"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-012B2A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "completed row status"),
            ("OSC desktop notification runtime dispatch", "completed behavior"),
            ("terminal_desktop_notification_runtime", "terminal evidence"),
            ("termio_desktop_notification_runtime", "termio evidence"),
            ("surface_desktop_notification_runtime", "surface evidence"),
            ("desktop_notification_runtime_parity.py", "static guard evidence"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-012B2B2B2")
    require_all(
        row_gap,
        [
            ("Gap", "remaining row status"),
            ("actual OS banner/sound delivery", "remaining OS delivery gap"),
            ("Command-finish notifications", "remaining command notification gap"),
            ("link previews", "remaining link preview gap"),
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

    print("desktop_notification_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
