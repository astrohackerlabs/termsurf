#!/usr/bin/env python3
"""Guard scroll-to-bottom output runtime parity for Issue 805 CFG-223."""

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


def forbid_row(markdown: str, row_id: str) -> None:
    for line in markdown.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if cells and cells[0] == row_id:
            raise AssertionError(f"unexpected inventory row {row_id}")


def main() -> int:
    ghostty_config = read("vendor/ghostty/src/config/Config.zig")
    ghostty_renderer = read("vendor/ghostty/src/renderer/generic.zig")
    roastty_config = read("roastty/src/config/mod.rs")
    roastty_surface = read("roastty/src/lib.rs")
    roastty_terminal = read("roastty/src/terminal/terminal.rs")
    roastty_screen = read("roastty/src/terminal/screen.rs")
    roastty_modes = read("roastty/src/terminal/modes.rs")
    inventory_source = read(
        "issues/0805-roastty-ghostty-parity/config_runtime_inventory.py"
    )
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_config,
        [
            ('@"scroll-to-bottom": ScrollToBottom = .default,', "Ghostty config field"),
            ("pub const ScrollToBottom = packed struct", "Ghostty packed struct"),
            ("keystroke: bool = true,", "Ghostty keystroke default"),
            ("output: bool = false,", "Ghostty output default"),
        ],
    )
    require_all(
        ghostty_renderer,
        [
            ("last_bottom_node: ?usize,", "Ghostty last bottom node state"),
            ("last_bottom_y: terminal.size.CellCountInt,", "Ghostty last bottom y state"),
            (
                "scroll_to_bottom_on_output: bool,",
                "Ghostty derived config output bool",
            ),
            (
                '.scroll_to_bottom_on_output = config.@"scroll-to-bottom".output,',
                "Ghostty derived config source",
            ),
            (
                "if (state.terminal.modes.get(.synchronized_output)) {",
                "Ghostty synchronized output skip",
            ),
            (
                "if (self.config.scroll_to_bottom_on_output) scroll:",
                "Ghostty output gate",
            ),
            (
                "state.terminal.screens.active.pages.getBottomRight(.screen)",
                "Ghostty screen bottom marker",
            ),
            ("self.last_bottom_node == @intFromPtr(br.node)", "Ghostty node compare"),
            ("self.last_bottom_y == br.y", "Ghostty y compare"),
            ("self.last_bottom_node = @intFromPtr(br.node);", "Ghostty node store"),
            ("self.last_bottom_y = br.y;", "Ghostty y store"),
            ("state.terminal.scrollViewport(.bottom);", "Ghostty bottom scroll"),
        ],
    )

    require_all(
        roastty_config,
        [
            ("pub scroll_to_bottom: ScrollToBottom,", "Roastty config field"),
            ("pub(crate) struct ScrollToBottom", "Roastty packed struct"),
            ("pub keystroke: bool,", "Roastty keystroke field"),
            ("pub output: bool,", "Roastty output field"),
        ],
    )
    require_all(
        roastty_modes,
        [
            ("SynchronizedOutput,", "Roastty synchronized output enum"),
            (
                'ModeEntry::dec(Mode::SynchronizedOutput, "synchronized_output", 2026, false)',
                "Roastty synchronized output DEC mode",
            ),
        ],
    )
    require_all(
        roastty_terminal,
        [
            ("pub(crate) fn synchronized_output_enabled(&self) -> bool", "Roastty sync accessor"),
            ("self.modes.get(modes::Mode::SynchronizedOutput)", "Roastty sync mode read"),
            (
                "pub(crate) fn active_screen_bottom_right(&self) -> Option<TerminalGridRef>",
                "Roastty bottom marker accessor",
            ),
            (".bottom_right(super::point::Tag::Screen)", "Roastty screen bottom tag"),
        ],
    )
    require_all(
        roastty_screen,
        [
            ("pub(super) fn bottom_right(&self, tag: point::Tag) -> Option<GridRef>", "Screen bottom helper"),
            ("self.pages.get_bottom_right(tag).map(GridRef::from)", "Screen bottom source"),
        ],
    )
    require_all(
        roastty_surface,
        [
            ("last_output_bottom_marker: Option<OutputBottomMarker>", "Roastty marker state"),
            ("struct OutputBottomMarker", "Roastty marker struct"),
            ("impl From<TerminalGridRef> for OutputBottomMarker", "Roastty marker conversion"),
            ("self.scroll_to_bottom_on_output_before_present(&config);", "Roastty present hook"),
            (
                "fn scroll_to_bottom_on_output_before_present(&mut self, config: &config::Config)",
                "Roastty helper",
            ),
            ("if !config.scroll_to_bottom.output", "Roastty config gate"),
            ("if terminal.synchronized_output_enabled()", "Roastty sync gate"),
            (
                "OutputBottomMarker::from(terminal.active_screen_bottom_right()?)",
                "Roastty bottom marker read",
            ),
            ("if Some(marker) == last_marker", "Roastty marker compare"),
            ("terminal.scroll_viewport_to_bottom();", "Roastty bottom scroll"),
            (
                "fn scroll_to_bottom_output_disabled_preserves_history_viewport",
                "disabled test",
            ),
            (
                "fn scroll_to_bottom_output_enabled_scrolls_once_per_bottom_marker",
                "enabled marker test",
            ),
            (
                "fn scroll_to_bottom_output_synchronized_output_skips_scroll_and_marker",
                "synchronized output test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-008B2B2B2B2B4")
    require_all(
        row_complete,
        [
            ("Oracle complete", "complete row status"),
            ("scroll-to-bottom output", "complete row behavior"),
            ("Experiment 183", "complete row experiment"),
            ("synchronized output", "complete row sync evidence"),
            ("node pointer and `y`", "complete row marker evidence"),
            ("scroll_to_bottom_output_runtime_parity.py", "complete row guard"),
        ],
    )
    forbid_row(runtime_inventory, "RUNTIME-008B2B2B2B2B")

    require_all(
        inventory_source,
        [
            ('id="RUNTIME-008B2B2B2B2B4"', "source complete row"),
            ("scroll_to_bottom_output_runtime_parity.py", "source guard"),
            ("scroll_to_bottom_output_*", "source focused test evidence"),
            ("synchronized output skips both scrolling", "source sync evidence"),
        ],
    )
    if 'id="RUNTIME-008B2B2B2B2B"' in inventory_source:
        raise AssertionError("old renderer residual row still exists in inventory source")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 remains open"),
            ("89 rows Oracle complete", "CFG-223 oracle count"),
            ("92 rows closed", "CFG-223 closed count"),
            ("1 rows are incomplete", "CFG-223 incomplete count"),
            ("1 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("scroll_to_bottom_output_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
