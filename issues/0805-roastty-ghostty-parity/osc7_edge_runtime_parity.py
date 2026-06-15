#!/usr/bin/env python3
"""Guard OSC 7 edge runtime parity for Issue 805 CFG-223."""

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
    ghostty_uri = read("vendor/ghostty/src/os/uri.zig")
    roastty_terminal = read("roastty/src/terminal/terminal.rs")
    roastty_termio = read("roastty/src/termio.rs")
    roastty_lib = read("roastty/src/lib.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_stream,
        [
            ("fn reportPwd", "Ghostty reportPwd handler"),
            (".raw_path = std.mem.startsWith(u8, url, \"kitty-shell-cwd://\")", "Ghostty kitty raw-path parse option"),
            ("const path = try uri.path.toRawMaybeAlloc", "Ghostty raw path extraction"),
            ("try self.terminal.setPwd(path)", "Ghostty terminal PWD update"),
            ("self.surfaceMessageWriter(.{ .pwd_change = req });", "Ghostty PWD surface dispatch"),
            ("try self.windowTitle(path);", "Ghostty title fallback path"),
        ],
    )
    require_all(
        ghostty_uri,
        [
            ("When the raw_path option is active", "Ghostty raw-path documentation"),
            ("including any query and fragment values", "Ghostty raw suffix documentation"),
            ("try testing.expectEqualStrings(\"/path??#fragment\"", "Ghostty raw-path suffix test"),
        ],
    )

    require_all(
        roastty_terminal,
        [
            ("fn normalize_report_pwd_url", "Roastty OSC 7 normalizer"),
            ("\"file\" =>", "Roastty file branch"),
            ("percent_decode_path(path)", "Roastty file percent decode"),
            ("\"kitty-shell-cwd\" =>", "Roastty kitty branch"),
            ("&rest[host_end..]", "Roastty raw kitty suffix path"),
            (
                "terminal_stream_osc7_pwd_edge_file_paths_trim_and_decode",
                "terminal file edge test",
            ),
            (
                "terminal_stream_osc7_pwd_edge_kitty_raw_path_keeps_suffixes",
                "terminal kitty raw suffix test",
            ),
            (
                "terminal_stream_osc7_pwd_edge_no_slash_dispatches_empty_path",
                "terminal empty path test",
            ),
        ],
    )
    require_all(
        roastty_termio,
        [
            (
                "termio_osc7_pwd_edge_worker_emits_raw_kitty_pwd_pump",
                "Termio OSC 7 edge pump test",
            ),
            ("/termio%2Fraw?x#y", "Termio raw kitty edge path"),
        ],
    )
    require_all(
        roastty_lib,
        [
            (
                "surface_osc7_pwd_edge_dispatches_raw_kitty_path",
                "surface OSC 7 edge dispatch test",
            ),
            ("/surface%2Fraw?x#y", "surface raw kitty edge path"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-009B2B2B3B2A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-009B2B2B3B2A status"),
            ("OSC 7 query/fragment", "query/fragment behavior"),
            ("UTF-8 percent-decoding", "UTF-8 decode behavior"),
            ("raw kitty path", "raw kitty behavior"),
            ("osc7_edge_runtime_parity.py", "static parity guard evidence"),
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
    if "Add runtime proof or fixes for unproven exotic OSC 7" in row_gap:
        raise AssertionError("remaining terminal gap still lists OSC 7 edge missing evidence")

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

    print("osc7_edge_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
