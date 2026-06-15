#!/usr/bin/env python3
"""Guard osc-color-report-format runtime parity for Issue 805 CFG-223."""

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
            (
                '@"osc-color-report-format": OSCColorReportFormat = .@"16-bit"',
                "Ghostty config field",
            ),
            (
                "OSC 4, 10, 11, and 12 default color reporting format",
                "Ghostty enum documentation",
            ),
            ("pub const OSCColorReportFormat = enum", "Ghostty enum"),
            ("none,", "Ghostty none tag"),
            ('@"8-bit"', "Ghostty 8-bit tag"),
            ('@"16-bit"', "Ghostty 16-bit tag"),
        ],
    )
    require_all(
        ghostty_termio,
        [
            (
                "osc_color_report_format: configpkg.Config.OSCColorReportFormat",
                "Ghostty derived config field",
            ),
            (
                'config.@"osc-color-report-format"',
                "Ghostty parsed config handoff",
            ),
        ],
    )
    require_all(
        ghostty_stream,
        [
            (
                "osc_color_report_format: configpkg.Config.OSCColorReportFormat",
                "Ghostty stream handler state",
            ),
            ("pub fn changeConfig", "Ghostty runtime config update hook"),
            (
                "self.osc_color_report_format = config.osc_color_report_format",
                "Ghostty runtime update assignment",
            ),
            (
                "if (self.osc_color_report_format == .none) break :report",
                "Ghostty none suppression",
            ),
            ('.@"16-bit" => switch (kind)', "Ghostty 16-bit branch"),
            ('.@"8-bit" => switch (kind)', "Ghostty 8-bit branch"),
            (
                '"\\x1b]4;{d};rgb:{x:0>4}/{x:0>4}/{x:0>4}"',
                "Ghostty 16-bit palette format",
            ),
            (
                '"\\x1b]4;{d};rgb:{x:0>2}/{x:0>2}/{x:0>2}"',
                "Ghostty 8-bit palette format",
            ),
        ],
    )

    require_all(
        roastty_config,
        [
            (
                "pub osc_color_report_format: OscColorReportFormat",
                "Roastty parsed config field",
            ),
            ('"osc-color-report-format"', "Roastty config key"),
            ("pub(crate) enum OscColorReportFormat", "Roastty enum"),
        ],
    )
    require_all(
        roastty_terminal,
        [
            (
                "osc_color_report_format: OscColorReportFormat",
                "Roastty terminal owned format",
            ),
            (
                "pub(crate) fn set_osc_color_report_format",
                "Roastty runtime setter",
            ),
            (
                "fn color_report_format",
                "Roastty report format mapper",
            ),
            (
                'Self::Bits8 => format!("rgb:{:02x}/{:02x}/{:02x}"',
                "Roastty 8-bit format",
            ),
            (
                '"rgb:{:04x}/{:04x}/{:04x}"',
                "Roastty 16-bit format",
            ),
            (
                "terminal_stream_osc_color_report_format_defaults_to_16_bit",
                "Roastty default format test",
            ),
            (
                "terminal_stream_osc_color_report_format_8_bit_and_runtime_update",
                "Roastty 8-bit/update test",
            ),
            (
                "terminal_stream_osc_color_report_format_none_suppresses_queries_only",
                "Roastty none suppression test",
            ),
        ],
    )
    require_all(
        roastty_termio,
        [
            (
                "pub(crate) osc_color_report_format: crate::config::OscColorReportFormat",
                "Roastty spawn option",
            ),
            (
                "osc_color_report_format: options.osc_color_report_format",
                "Roastty Termio init handoff",
            ),
            (
                "termio_osc_color_report_format_reaches_child_pty",
                "Roastty PTY child test",
            ),
        ],
    )
    require_all(
        roastty_lib,
        [
            (
                "terminal.set_osc_color_report_format(parsed.osc_color_report_format)",
                "Roastty live config update",
            ),
            (
                "osc_color_report_format: config.osc_color_report_format",
                "Roastty startup config handoff",
            ),
            (
                "surface_osc_color_report_format_runtime_startup_and_update",
                "Roastty surface config test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-009B2B2B3B2B2A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-009B2B2B3B2B2A status"),
            ("`osc-color-report-format` runtime effects", "OSC format behavior"),
            ("OSC palette and dynamic color query replies", "query evidence"),
            ("startup/update wiring", "config update evidence"),
            (
                "osc_color_report_format_runtime_parity.py",
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
            ("89 rows Oracle complete", "CFG-223 oracle count"),
            ("92 rows closed", "CFG-223 closed count"),
            ("1 rows are incomplete", "CFG-223 incomplete count"),
            ("1 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("osc_color_report_format_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
