#!/usr/bin/env python3
"""Guard enquiry-response runtime parity for Issue 805 CFG-223."""

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
    roastty_stream = read("roastty/src/terminal/stream.rs")
    roastty_termio = read("roastty/src/termio.rs")
    roastty_lib = read("roastty/src/lib.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_config,
        [
            ('@"enquiry-response": []const u8 = ""', "Ghostty config field"),
            (
                "String to send when we receive `ENQ` (`0x05`)",
                "Ghostty config documentation",
            ),
        ],
    )
    require_all(
        ghostty_termio,
        [
            ("enquiry_response: []const u8", "Ghostty derived config field"),
            (
                'config.@"enquiry-response"',
                "Ghostty parsed config handoff",
            ),
        ],
    )
    require_all(
        ghostty_stream,
        [
            ("enquiry_response: []const u8", "Ghostty stream handler state"),
            ("pub fn changeConfig", "Ghostty runtime config update hook"),
            (
                "self.enquiry_response = config.enquiry_response",
                "Ghostty enquiry-response runtime update",
            ),
            (".enquiry => try self.enquiry()", "Ghostty ENQ action dispatch"),
            ("pub fn enquiry", "Ghostty enquiry handler"),
            (
                "termio.Message.writeReq(self.alloc, self.enquiry_response)",
                "Ghostty ENQ write request",
            ),
        ],
    )

    require_all(
        roastty_config,
        [
            ("pub enquiry_response: String", "Roastty parsed config field"),
            ('"enquiry-response"', "Roastty config key"),
            ('"enquiry-response = hello"', "Roastty formatter assertion"),
            ('"bad\\0response"', "Roastty string parser assertion"),
        ],
    )
    require_all(
        roastty_stream,
        [
            ("0x05 => handler.vt(Action::Enquiry)?", "Roastty ENQ parse"),
        ],
    )
    require_all(
        roastty_terminal,
        [
            ("enquiry_response: Vec<u8>", "Roastty terminal owned response"),
            ("pub(crate) fn set_enquiry_response", "Roastty runtime setter"),
            (
                "self.write_pty_response_bytes(self.enquiry_response)",
                "Roastty configured ENQ write",
            ),
            (
                "terminal_stream_enquiry_response_configured_and_runtime_update",
                "Roastty terminal ENQ response test",
            ),
            (
                "terminal_stream_enquiry_response_callback_precedence_is_preserved",
                "Roastty callback preservation test",
            ),
        ],
    )
    require_all(
        roastty_termio,
        [
            ("pub(crate) enquiry_response: Vec<u8>", "Roastty spawn option"),
            (
                "enquiry_response: options.enquiry_response",
                "Roastty Termio init handoff",
            ),
            (
                "termio_enquiry_response_reaches_child_pty",
                "Roastty PTY child ENQ test",
            ),
        ],
    )
    require_all(
        roastty_lib,
        [
            (
                "terminal.set_enquiry_response(parsed.enquiry_response.as_bytes().to_vec())",
                "Roastty live config update",
            ),
            (
                "enquiry_response: config.enquiry_response.as_bytes().to_vec()",
                "Roastty startup config handoff",
            ),
            (
                "surface_enquiry_response_runtime_startup_and_update",
                "Roastty surface config ENQ test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-009B2B2B3B2B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-009B2B2B3B2B1 status"),
            ("config-driven `enquiry-response` ENQ replies", "ENQ behavior"),
            ("terminal core", "terminal-core evidence"),
            ("PTY-backed runtime", "PTY evidence"),
            ("startup/update wiring", "config update evidence"),
            (
                "enquiry_response_runtime_parity.py",
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
            ("53 rows Oracle complete", "CFG-223 oracle count"),
            ("55 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("enquiry_response_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
