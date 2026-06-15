#!/usr/bin/env python3
"""Guard live font-grid update runtime parity for Issue 805 CFG-223."""

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
            ("pub fn updateConfig", "Ghostty config reload"),
            ("try self.setFontSize(font_size:", "Ghostty config reload setFontSize"),
            ("if (self.font_size_adjusted)", "Ghostty adjusted font-size preserve branch"),
            ("pub fn setFontSize", "Ghostty setFontSize"),
            ("app.font_grid_set.ref(", "Ghostty font grid ref"),
            (".font_grid = .{", "Ghostty renderer font-grid message"),
            ("self.font_size_adjusted = true", "Ghostty manual adjusted flag"),
            ("self.font_size_adjusted = false", "Ghostty reset adjusted flag"),
        ],
    )

    require_all(
        roastty_lib,
        [
            ("fn invalidate_live_font_grid(&mut self)", "Roastty invalidation helper"),
            ("if self.has_live_view() {", "Roastty live-view invalidation gate"),
            ("self.renderer = None;", "Roastty live renderer drop"),
            ("self.invalidate_live_font_grid();", "Roastty font-size invalidation call"),
            ("fn set_font_size_points(&mut self, points: f32)", "Roastty font-size setter"),
            ("fn increase_font_size(&mut self, delta: f32)", "Roastty increase action"),
            ("fn decrease_font_size(&mut self, delta: f32)", "Roastty decrease action"),
            ("fn reset_font_size(&mut self) -> bool", "Roastty reset action"),
            ("fn set_font_size(&mut self, points: f32) -> bool", "Roastty set-size action"),
            ("fn build_live_renderer", "Roastty live renderer builder"),
            (
                "font::shared_grid_set::build_grid_from_config(config, (font_size * scale as f32).max(1.0))",
                "Roastty live grid construction",
            ),
            (
                "font_live_grid_update_manual_size_changes_dirty_and_wake_live_view",
                "manual live font-grid update test",
            ),
            (
                "font_live_grid_update_same_size_is_idempotent",
                "same-size idempotence test",
            ),
            (
                "font_live_grid_update_config_reload_preserves_state_and_rebuild_trigger",
                "config reload rebuild test",
            ),
            (
                "surface_reload_font_size_updates_unadjusted_and_preserves_manual",
                "surface-state font-size test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-007B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "completed row status"),
            ("live renderer font-grid rebuild/update triggers", "completed behavior"),
            ("font_live_grid_update_manual_size_changes_dirty_and_wake_live_view", "manual evidence"),
            ("font_live_grid_update_same_size_is_idempotent", "idempotent evidence"),
            ("font_live_grid_update_config_reload_preserves_state_and_rebuild_trigger", "reload evidence"),
            ("font_live_grid_update_runtime_parity.py", "static guard evidence"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-007B2B2B2B2")
    require_all(
        row_gap,
        [
            ("Gap", "remaining row status"),
            ("remaining font renderer output effects", "remaining behavior"),
            ("bitmap/color font thickening edge cases", "remaining thicken edge gap"),
            ("renderer-visible font pixel parity", "remaining pixel gap"),
        ],
    )
    if "RUNTIME-007B |" in runtime_inventory:
        raise AssertionError("old RUNTIME-007B row is still present")

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

    print("font_live_grid_update_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
