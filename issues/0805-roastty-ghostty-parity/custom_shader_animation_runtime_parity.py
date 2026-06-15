#!/usr/bin/env python3
"""Guard custom-shader-animation runtime parity for Issue 805 CFG-223."""

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
    ghostty_thread = read("vendor/ghostty/src/renderer/Thread.zig")
    ghostty_config = read("vendor/ghostty/src/config/Config.zig")
    roastty_config = read("roastty/src/config/mod.rs")
    roastty_lib = read("roastty/src/lib.rs")
    inventory_source = read("issues/0805-roastty-ghostty-parity/config_runtime_inventory.py")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_config,
        [
            ('@"custom-shader-animation": CustomShaderAnimation = .true,', "Ghostty config default"),
            ("pub const CustomShaderAnimation = enum", "Ghostty enum"),
            ("    false,", "Ghostty false enum value"),
            ("    true,", "Ghostty true enum value"),
            ("    always,", "Ghostty always enum value"),
        ],
    )
    require_all(
        ghostty_thread,
        [
            ('.custom_shader_animation = config.@"custom-shader-animation"', "Ghostty thread config copy"),
            ("fn syncDrawTimer(self: *Thread) void", "Ghostty draw timer sync"),
            ("self.renderer.hasAnimations()", "Ghostty animation capability gate"),
            ("switch (self.config.custom_shader_animation)", "Ghostty policy switch"),
            (".always => break :skip", "Ghostty always animates"),
            (".true => if (self.flags.focused) break :skip", "Ghostty true focused gate"),
            (".false => {}", "Ghostty false never animates"),
            ("self.draw_active = false", "Ghostty draw timer inactive"),
            ("self.draw_active = true", "Ghostty draw timer active"),
            ("self.syncDrawTimer();", "Ghostty resync on focus/config"),
        ],
    )
    require_all(
        roastty_config,
        [
            ("pub(crate) enum CustomShaderAnimation", "Roastty enum"),
            ("pub(crate) fn should_animate(self, focused: bool) -> bool", "Roastty config helper"),
            ("CustomShaderAnimation::Always => true", "Roastty always policy"),
            ("CustomShaderAnimation::True => focused", "Roastty true policy"),
            ("CustomShaderAnimation::False => false", "Roastty false policy"),
            ("fn custom_shader_animation_should_animate_truth_table", "Roastty config truth table"),
        ],
    )
    require_all(
        roastty_lib,
        [
            ("fn custom_shader_animation_tick_enabled(", "Roastty tick helper"),
            ("pipelines_active && policy.should_animate(focused)", "Roastty helper policy"),
            ("fn should_animate_custom_shader_frame(&self) -> bool", "Roastty surface method"),
            ("!live.custom_shader.pipelines.is_empty()", "Roastty pipeline gate"),
            ("self.active_config().custom_shader_animation", "Roastty runtime config read"),
            ("let animate_custom_shader = surface.should_animate_custom_shader_frame();", "Roastty tick computes animation"),
            ("should_present_on_tick(surface.dirty, should_present_live, animate_custom_shader)", "Roastty tick integration"),
            ("fn custom_shader_animation_tick_policy_matches_focus", "Roastty policy test"),
            ("fn custom_shader_animation_tick_requires_pipeline", "Roastty pipeline test"),
            ("fn custom_shader_animation_tick_present_decision_preserves_dirty_gate", "Roastty dirty gate test"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-008B2B2B2B2B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "complete row status"),
            ("custom-shader-animation", "complete row behavior"),
            ("focus/always/false", "policy evidence"),
            ("draw-timer", "draw timer evidence"),
            ("custom_shader_animation_runtime_parity.py", "complete row guard"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-008B2B2B2B2B")
    require_all(
        row_gap,
        [
            ("Gap", "remaining renderer gap status"),
            ("scroll-to-bottom.output", "scroll-to-bottom gap remains"),
        ],
    )
    if "custom-shader-animation" in row_gap:
        raise AssertionError("custom-shader-animation still appears in remaining renderer gap")
    for forbidden in ["window-colorspace", "alpha-blending"]:
        if forbidden in row_gap:
            raise AssertionError(f"{forbidden} still appears in remaining renderer gap")

    require_all(
        inventory_source,
        [
            ('id="RUNTIME-008B2B2B2B2B1"', "source complete row"),
            ("custom_shader_animation_runtime_parity.py", "source guard"),
            ('id="RUNTIME-008B2B2B2B2B"', "source remaining row"),
            ("scroll-to-bottom.output", "source remaining scroll gap"),
        ],
    )

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 remains open"),
            ("80 rows Oracle complete", "CFG-223 oracle count"),
            ("83 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("custom_shader_animation_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
