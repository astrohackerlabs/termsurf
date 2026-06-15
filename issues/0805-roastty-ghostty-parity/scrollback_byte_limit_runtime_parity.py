#!/usr/bin/env python3
"""Guard nonzero scrollback-limit byte-quota runtime parity for Issue 805."""

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
    ghostty_screen = read("vendor/ghostty/src/terminal/Screen.zig")
    ghostty_termio = read("vendor/ghostty/src/termio/Termio.zig")
    roastty_lib = read("roastty/src/lib.rs")
    roastty_termio = read("roastty/src/termio.rs")
    roastty_terminal = read("roastty/src/terminal/terminal.rs")
    roastty_screen = read("roastty/src/terminal/screen.rs")
    roastty_page_list = read("roastty/src/terminal/page_list.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require(
        ghostty_config,
        '@"scrollback-limit": usize = 10_000_000',
        "Ghostty default scrollback byte limit",
    )
    require_all(
        ghostty_screen,
        [
            ("max_scrollback: usize", "Ghostty Screen max_scrollback option"),
            (
                "The maximum size of scrollback in bytes.",
                "Ghostty byte-limit semantics comment",
            ),
            (
                "If max scrollback is 0, then no scrollback is kept at all.",
                "Ghostty zero/no-scrollback init comment",
            ),
            ("opts.max_scrollback == 0", "Ghostty zero disables scrollback"),
            (
                "opts.max_scrollback,",
                "Ghostty passes byte limit into PageList",
            ),
        ],
    )
    require(
        ghostty_termio,
        'opts.full_config.@"scrollback-limit"',
        "Ghostty termio passes parsed scrollback-limit",
    )

    require_all(
        roastty_lib,
        [
            ("scrollback_limit_to_bytes", "Roastty config bridge helper"),
            ("Some(limit)", "Roastty preserves parsed nonzero byte limit"),
            (
                "config_scrollback_limit_runtime_nonzero_byte_limit_bounds_history",
                "Roastty app runtime byte-limit test",
            ),
        ],
    )
    require_all(
        roastty_termio,
        [
            ("max_scrollback_bytes: Option<usize>", "Roastty Termio option name"),
            ("options.max_scrollback_bytes", "Roastty Termio startup forwarding"),
        ],
    )
    require_all(
        roastty_terminal,
        [
            (
                "max_scrollback_bytes: Option<usize>",
                "Roastty Terminal byte-limit parameter",
            ),
            (
                "terminal_stream_scrollback_byte_limit_bounds_history",
                "Roastty terminal runtime byte-limit test",
            ),
        ],
    )
    require(
        roastty_screen,
        "PageList::init(cols, rows, max_scrollback_bytes)",
        "Roastty Screen passes byte limit into PageList",
    )
    require_all(
        roastty_page_list,
        [
            ("explicit_max_size", "Roastty PageList explicit byte limit storage"),
            ("min_max_size", "Roastty PageList active-area minimum clamp"),
            (
                "page_size + standard_page_size() > self.max_size()",
                "Roastty PageList byte-size pruning threshold",
            ),
            (
                "page_list_scrollback_byte_limit_prunes_by_page_size",
                "Roastty PageList byte-limit pruning test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-009B2B2B3A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-009B2B2B3A status"),
            ("nonzero scrollback byte quota", "scrollback byte-limit behavior"),
            (
                "scrollback_byte_limit_runtime_parity.py",
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
    if "exact nonzero scrollback byte quota" in row_gap:
        raise AssertionError("remaining terminal gap still claims exact byte quota is missing")

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

    print("scrollback_byte_limit_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
