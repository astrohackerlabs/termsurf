#!/usr/bin/env python3
"""Guard grapheme-width-method runtime parity for Issue 805 CFG-223."""

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
    ghostty_terminal = read("vendor/ghostty/src/terminal/Terminal.zig")
    roastty_config = read("roastty/src/config/mod.rs")
    roastty_modes = read("roastty/src/terminal/modes.rs")
    roastty_terminal = read("roastty/src/terminal/terminal.rs")
    roastty_termio = read("roastty/src/termio.rs")
    roastty_lib = read("roastty/src/lib.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_config,
        [
            ('@"grapheme-width-method": GraphemeWidthMethod = .unicode', "Ghostty default"),
            ("pub const GraphemeWidthMethod = enum", "Ghostty enum"),
            (".legacy", "Ghostty legacy tag"),
            (".unicode", "Ghostty unicode tag"),
        ],
    )
    require_all(
        ghostty_termio,
        [
            ('opts.full_config.@"grapheme-width-method"', "Ghostty full-config source"),
            (".unicode => modes.grapheme_cluster = true", "Ghostty unicode switch"),
            (".legacy => {},", "Ghostty legacy switch"),
            (".default_modes = default_modes", "Ghostty terminal default modes handoff"),
        ],
    )
    require_all(
        ghostty_terminal,
        [
            ("default_modes: modespkg.ModePacked = .{}", "Ghostty init option"),
            (".values = opts.default_modes", "Ghostty current mode init"),
            (".default = opts.default_modes", "Ghostty reset mode init"),
            ("self.modes.reset()", "Ghostty reset path"),
            (".default_modes = .{ .grapheme_cluster = true }", "Ghostty reset regression test"),
        ],
    )

    require_all(
        roastty_config,
        [
            ("pub grapheme_width_method: GraphemeWidthMethod", "Roastty config field"),
            ("grapheme_width_method: GraphemeWidthMethod::Unicode", "Roastty default"),
            ('"grapheme-width-method"', "Roastty config key"),
            ("pub(crate) fn grapheme_cluster(self) -> bool", "Roastty config-to-mode helper"),
        ],
    )
    require_all(
        roastty_modes,
        [
            ('ModeEntry::dec(Mode::GraphemeCluster, "grapheme_cluster", 2027, false)', "Roastty DEC 2027 mode"),
            ("pub(super) fn set_default(&mut self, mode: Mode, value: bool)", "Roastty default mode setter"),
            ("self.default[mode.index()] = value", "Roastty reset default update"),
            ("self.values[mode.index()] = value", "Roastty current mode update"),
            ("self.values = self.default", "Roastty reset restores defaults"),
        ],
    )
    require_all(
        roastty_terminal,
        [
            ("pub(crate) grapheme_cluster: bool", "Roastty terminal init option"),
            ("modes.set_default(modes::Mode::GraphemeCluster, options.grapheme_cluster)", "Roastty default mode handoff"),
            ("pub(crate) fn grapheme_cluster_enabled(&self) -> bool", "Roastty terminal getter"),
            ("grapheme_width_method_runtime_initializes_mode_and_reset_default", "Roastty terminal runtime test"),
            ("b\"\\x1b[?2027$p\"", "Roastty DEC 2027 mode report test"),
            ("b\"\\x1bc\"", "Roastty RIS reset test"),
        ],
    )
    require_all(
        roastty_termio,
        [
            ("pub(crate) grapheme_cluster: bool", "Roastty Termio spawn option"),
            ("GraphemeWidthMethod::Unicode.grapheme_cluster()", "Roastty Termio default"),
            ("grapheme_cluster: options.grapheme_cluster", "Roastty Termio terminal handoff"),
            ("grapheme_width_method_runtime_spawn_options_reach_terminal", "Roastty Termio test"),
        ],
    )
    require_all(
        roastty_lib,
        [
            ("grapheme_cluster: config.grapheme_width_method.grapheme_cluster()", "Roastty surface startup handoff"),
            ("surface_grapheme_width_method_runtime_startup_config", "Roastty surface runtime test"),
            ('"grapheme-width-method = unicode\\ncommand = sleep 5\\n"', "Roastty unicode config test"),
            ('"grapheme-width-method = legacy\\ncommand = sleep 5\\n"', "Roastty legacy config test"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-009B2B2B3B2B2B2B2")
    require_all(
        row_complete,
        [
            ("Oracle complete", "completed row status"),
            ("`grapheme-width-method` terminal default mode startup and reset effects", "completed behavior"),
            ("grapheme_width_method_runtime_initializes_mode_and_reset_default", "terminal evidence"),
            ("grapheme_width_method_runtime_spawn_options_reach_terminal", "termio evidence"),
            ("surface_grapheme_width_method_runtime_startup_config", "surface evidence"),
            ("grapheme_width_method_runtime_parity.py", "static guard evidence"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-009B2B2B3B2B2B2B3")
    require_all(
        row_gap,
        [
            ("Oracle complete", "remaining row status"),
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

    print("grapheme_width_method_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
