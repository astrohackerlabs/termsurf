#!/usr/bin/env python3
"""Guard link hover preview dispatch parity for Issue 805 CFG-223."""

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
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_surface,
        [
            ("fn mouseRefreshLinks", "Ghostty mouseRefreshLinks helper"),
            ("const over_link = self.mouse.over_link", "Ghostty previous hover state"),
            ("self.mouse.over_link = false", "Ghostty hover state reset before refresh"),
            ("self.mouse.link_point == null", "Ghostty missing link point refresh"),
            ("!self.mouse.link_point.?.eql(pos_vp)", "Ghostty changed-cell refresh"),
            ("self.io.terminal.flags.mouse_event == .none", "Ghostty mouse-reporting gate"),
            ("self.mouse.mods.shift and !self.mouseShiftCapture(false)", "Ghostty shift override gate"),
            ("if (self.mouse.click_state[left_idx] == .press)", "Ghostty left press suppression"),
            ("mouse moved while left click held, ignoring link hover", "Ghostty drag suppression log"),
            (".mouse_shape,\n            .pointer", "Ghostty pointer dispatch"),
            (".mouse_over_link,\n                link", "Ghostty hover URL dispatch"),
            (".mouse_over_link,\n            .{ .url = \"\" }", "Ghostty hover clear dispatch"),
            ("self.config.link_previews == .true", "Ghostty regular preview gate"),
            ("self.config.link_previews != .false", "Ghostty OSC8 preview gate"),
            ("if (mouse_mods.equal(input.ctrlOrSuper(.{})))", "Ghostty OSC8 ctrl/super gate"),
        ],
    )

    require_all(
        roastty_lib,
        [
            ("const ROASTTY_ACTION_MOUSE_SHAPE: c_int = 36", "Roastty mouse shape action tag"),
            ("const ROASTTY_ACTION_MOUSE_OVER_LINK: c_int = 38", "Roastty mouse over link action tag"),
            ("pub struct RoasttyActionMouseOverLink", "Roastty typed hover payload"),
            ("mouse_shape: c_int", "Roastty typed mouse shape union field"),
            ("mouse_over_link: RoasttyActionMouseOverLink", "Roastty typed hover union field"),
            ("fn perform_mouse_shape(&self, shape: c_int) -> bool", "Roastty mouse shape dispatch helper"),
            ("fn perform_mouse_over_link(&self, url: &CStr) -> bool", "Roastty hover dispatch helper"),
            ("fn refresh_link_hover(&mut self)", "Roastty hover refresh helper"),
            ("let shift_override = reporting && self.mouse.mods.shift && !self.mouse_shift_capture()", "Roastty shift override gate"),
            ("if reporting && !shift_override", "Roastty mouse-reporting suppression"),
            ("fn link_hover_url_at_viewport_cell", "Roastty link hover URL lookup"),
            ("self.link_previews.previews_regular_link()", "Roastty regular preview gate"),
            ("self.link_previews.previews_osc8_link()", "Roastty OSC8 preview gate"),
            ("ROASTTY_MOUSE_SHAPE_POINTER", "Roastty pointer dispatch"),
            ("fn mouse_shape_to_abi(shape: mouse::MouseShape) -> c_int", "Roastty terminal shape conversion"),
            ("fn terminal_mouse_shape(&self) -> c_int", "Roastty current terminal mouse shape lookup"),
            ("self.perform_mouse_over_link(&empty)", "Roastty empty URL clear dispatch"),
            ("left_press_cell", "Roastty left-press cell tracking"),
            ("link_hover_preview_dispatch_regular_link_gates_preview_and_shape", "Roastty regular hover test"),
            ("link_hover_preview_dispatch_osc8_link_gates_preview_with_ctrl_or_super", "Roastty OSC8 hover test"),
            ("link_hover_preview_dispatch_repeats_while_over_link_and_clears_on_leave", "Roastty repeat/clear hover test"),
            ("link_hover_preview_dispatch_respects_mouse_reporting_and_shift_override", "Roastty reporting/shift test"),
            ("link_hover_preview_dispatch_suppresses_left_drag_hover", "Roastty drag suppression test"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-012B2B2B2B2B2")
    require_all(
        row_complete,
        [
            ("Oracle complete", "complete row status"),
            ("link hover preview surface action dispatch", "complete row behavior"),
            ("Experiment 161", "complete row experiment"),
            ("mouse-reporting/shift-override gate", "complete row reporting gate"),
            ("left-click drag suppression", "complete row drag suppression"),
            ("link_hover_preview_dispatch", "complete row cargo guard"),
            ("link_hover_preview_dispatch_parity.py", "complete row Python guard"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-012B2B2B2B2B3")
    require_all(
        row_gap,
        [
            ("Gap", "remaining row status"),
            ("Actual OS banner/sound delivery", "remaining notification gap"),
            ("actual audio/dock/border/title GUI effects", "remaining bell gap"),
            ("real app link hover/cursor UI", "remaining hover UI gap"),
            ("native link preview display", "remaining preview UI gap"),
            ("native context/menu display", "remaining menu UI gap"),
            ("OS URL-opening flows", "remaining OS URL gap"),
        ],
    )
    if "runtime `mouse_over_link` preview dispatch" in row_gap:
        raise AssertionError("runtime hover dispatch still listed in remaining gap row")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("69 rows Oracle complete", "CFG-223 oracle count"),
            ("72 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("link_hover_preview_dispatch_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
