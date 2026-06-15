#!/usr/bin/env python3
"""Guard key/modifier-driven link hover refresh parity for Issue 805 CFG-223."""

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
    roastty_lib = read("roastty/src/lib.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()

    require_all(
        ghostty_surface,
        [
            ("fn modsChanged(self: *Surface, mods: input.Mods) void", "Ghostty modsChanged helper"),
            ("self.mouse.mods = mods.binding()", "Ghostty binding modifier storage"),
            ("pub fn keyCallback(", "Ghostty key callback"),
            ("if (!self.mouse.mods.equal(event.mods)) mouse_mods:", "Ghostty key modifier comparison"),
            ("self.modsChanged(event.mods)", "Ghostty key modifier update"),
            ("self.io.terminal.flags.mouse_event == .none or", "Ghostty no-reporting refresh branch"),
            ("(self.mouse.mods.shift and !self.mouseShiftCapture(false))", "Ghostty shift override refresh branch"),
            ("self.mouseRefreshLinks(", "Ghostty key-driven hover refresh"),
            ("else if (self.io.terminal.flags.mouse_event != .none and !self.mouse.mods.shift)", "Ghostty reporting clear branch"),
            (".mouse_shape,\n                self.io.terminal.mouse_shape", "Ghostty current terminal shape clear"),
            (".mouse_over_link,\n                .{ .url = \"\" }", "Ghostty empty URL clear"),
        ],
    )

    require_all(
        roastty_lib,
        [
            ("fn refresh_link_hover_for_key_mods(&mut self, mods: key_mods::Mods)", "Roastty key modifier hover helper"),
            ("let mods = mods.binding();", "Roastty binding modifier storage"),
            ("if self.mouse.mods == mods", "Roastty modifier comparison"),
            ("self.mouse.mods = mods;", "Roastty mouse modifier update"),
            ("let reporting = self.mouse_report_context().is_some();", "Roastty reporting check"),
            ("let shift_override = reporting && self.mouse.mods.shift && !self.mouse_shift_capture();", "Roastty shift override check"),
            ("if !reporting || shift_override", "Roastty refresh branch"),
            ("self.mouse.link_point = None;", "Roastty same-cell cache invalidation"),
            ("self.refresh_link_hover();", "Roastty key-driven hover refresh"),
            ("} else if !self.mouse.mods.shift", "Roastty reporting clear branch"),
            ("self.dispatch_link_hover_clear(self.terminal_mouse_shape());", "Roastty current terminal shape clear"),
            ("self.refresh_link_hover_for_key_mods(event.mods);", "Roastty key callback hook"),
            ("link_hover_modifier_refresh_super_enables_regular_link_without_mouse_move", "Roastty regular stationary test"),
            ("link_hover_modifier_refresh_super_release_clears_regular_link", "Roastty release clear test"),
            ("link_hover_modifier_refresh_super_enables_osc8_without_mouse_move", "Roastty OSC8 stationary test"),
            ("link_hover_modifier_refresh_respects_reporting_and_shift_override", "Roastty reporting/shift test"),
            ("link_hover_modifier_refresh_reporting_captured_shift_noops", "Roastty captured shift test"),
        ],
    )

    row = require_row(runtime_inventory, "RUNTIME-012B2B2B2B2B2")
    require_all(
        row,
        [
            ("Oracle complete", "inventory status"),
            ("Experiment 162", "inventory experiment"),
            ("key/modifier-driven hover refresh", "inventory key modifier wording"),
            ("stationary super press", "inventory stationary super proof"),
            ("same-cell no-link cache", "inventory cache proof"),
            ("captured shift under mouse reporting", "inventory captured shift proof"),
            ("link_hover_modifier_refresh", "inventory cargo guard"),
            ("link_hover_modifier_refresh_parity.py", "inventory Python guard"),
        ],
    )

    print("link_hover_modifier_refresh_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
