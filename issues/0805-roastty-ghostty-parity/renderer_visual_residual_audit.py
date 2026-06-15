#!/usr/bin/env python3
"""Audit the residual renderer-visible CFG-223 row for Issue 805."""

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


def require_row_complete(markdown: str, row_id: str, needles: list[str]) -> None:
    row = require_row(markdown, row_id)
    require(row, "Oracle complete", f"{row_id} complete status")
    for needle in needles:
        require(row, needle, f"{row_id} evidence {needle}")


def main() -> int:
    ghostty_config = read("vendor/ghostty/src/config/Config.zig")
    ghostty_renderer = read("vendor/ghostty/src/renderer/generic.zig")
    ghostty_surface = read("vendor/ghostty/src/Surface.zig")
    ghostty_size = read("vendor/ghostty/src/renderer/size.zig")
    ghostty_cell = read("vendor/ghostty/src/renderer/cell.zig")
    ghostty_thread = read("vendor/ghostty/src/renderer/Thread.zig")
    ghostty_shaders = read("vendor/ghostty/src/renderer/shaders/shaders.metal")
    ghostty_metal = read("vendor/ghostty/src/renderer/Metal.zig")
    ghostty_metal_shaders = read("vendor/ghostty/src/renderer/metal/shaders.zig")
    ghostty_container = read(
        "vendor/ghostty/macos/Sources/Features/Terminal/TerminalViewContainer.swift"
    )
    ghostty_window = read(
        "vendor/ghostty/macos/Sources/Features/Terminal/Window Styles/TerminalWindow.swift"
    )
    ghostty_titlebar = read(
        "vendor/ghostty/macos/Sources/Features/Terminal/Window Styles/TransparentTitlebarTerminalWindow.swift"
    )
    ghostty_quick = read(
        "vendor/ghostty/macos/Sources/Features/QuickTerminal/QuickTerminalController.swift"
    )
    inventory_source = read(
        "issues/0805-roastty-ghostty-parity/config_runtime_inventory.py"
    )
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_config,
        [
            ('@"window-vsync": bool = true,', "window-vsync config"),
            ('@"font-thicken": bool = false,', "font-thicken config"),
            ('@"font-thicken-strength": u8 = 255,', "font-thicken strength config"),
            ('@"cursor-color": ?TerminalColor = null,', "cursor-color config"),
            ('@"cursor-text": ?TerminalColor = null,', "cursor-text config"),
            ('@"background-opacity": f64 = 1.0,', "background-opacity config"),
            (
                '@"background-opacity-cells": bool = false,',
                "background-opacity-cells config",
            ),
            ('@"background-image": ?Path = null,', "background-image config"),
            (
                '@"background-image-opacity": f32 = 1.0,',
                "background-image opacity config",
            ),
            (
                '@"background-image-position": BackgroundImagePosition = .center,',
                "background-image position config",
            ),
            (
                '@"background-image-fit": BackgroundImageFit = .contain,',
                "background-image fit config",
            ),
            ('@"background-image-repeat": bool = false,', "background-image repeat config"),
            ('@"background-blur": BackgroundBlur = .false,', "background-blur config"),
            ('@"alpha-blending": AlphaBlending =', "alpha-blending config"),
            (
                '@"scroll-to-bottom": ScrollToBottom = .default,',
                "scroll-to-bottom config",
            ),
            ('@"window-padding-x": WindowPadding', "window-padding-x config"),
            ('@"window-padding-y": WindowPadding', "window-padding-y config"),
            (
                '@"window-padding-balance": WindowPaddingBalance = .false,',
                "window-padding-balance config",
            ),
            (
                '@"window-padding-color": WindowPaddingColor = .background,',
                "window-padding-color config",
            ),
            ('@"custom-shader": RepeatablePath = .{},', "custom-shader config"),
            (
                '@"custom-shader-animation": CustomShaderAnimation = .true,',
                "custom-shader-animation config",
            ),
            ('@"window-colorspace": WindowColorspace = .srgb,', "window-colorspace config"),
            ('.@"macos-glass-regular", .@"macos-glass-clear" => true', "macOS glass variants"),
        ],
    )

    require_all(
        ghostty_renderer,
        [
            ('.vsync = config.@"window-vsync",', "vsync renderer option"),
            ('const custom_shaders = try config.@"custom-shader".clone(alloc);', "custom shader clone"),
            (
                '.background_opacity = @max(0, @min(1, config.@"background-opacity")),',
                "background opacity clamp",
            ),
            (
                '.background_opacity_cells = config.@"background-opacity-cells",',
                "background opacity cells",
            ),
            ('if (config.@"background-image") |bg|', "background image load"),
            (".bg_image = bg_image,", "background image renderer option"),
            (
                '.bg_image_opacity = config.@"background-image-opacity",',
                "background image opacity option",
            ),
            (
                '.bg_image_position = config.@"background-image-position",',
                "background image position option",
            ),
            (
                '.bg_image_fit = config.@"background-image-fit",',
                "background image fit option",
            ),
            (
                '.bg_image_repeat = config.@"background-image-repeat",',
                "background image repeat option",
            ),
            ('.colorspace = config.@"window-colorspace",', "window colorspace option"),
            ('.blending = config.@"alpha-blending",', "alpha blending option"),
            (
                '.scroll_to_bottom_on_output = config.@"scroll-to-bottom".output,',
                "scroll-to-bottom output option",
            ),
            ('.font_thicken = config.@"font-thicken",', "font-thicken option"),
            (
                '.font_thicken_strength = config.@"font-thicken-strength",',
                "font-thicken strength option",
            ),
            ('.cursor_color = config.@"cursor-color",', "cursor color option"),
            ('.cursor_text = config.@"cursor-text",', "cursor text option"),
            ('.padding_color = config.@"window-padding-color",', "padding color option"),
            ('.@"macos-glass-regular"', "regular glass renderer marker"),
            ('.@"macos-glass-clear"', "clear glass renderer marker"),
            ("if (self.has_custom_shaders)", "custom shader draw gate"),
            ("try self.updateCustomShaderUniformsForFrame();", "custom uniform update"),
            ("for (self.shaders.post_pipelines, 0..) |pipeline, i|", "post pipeline loop"),
            ("defer state.swap();", "custom shader ping-pong"),
            ("// If cursor-text is set, then compute the correct color.", "cursor text resolution"),
        ],
    )
    require_all(
        ghostty_surface,
        [
            (
                ".window_padding_top = config.@\"window-padding-y\".top_left,",
                "surface top padding",
            ),
            (
                ".window_padding_bottom = config.@\"window-padding-y\".bottom_right,",
                "surface bottom padding",
            ),
            (
                ".window_padding_left = config.@\"window-padding-x\".top_left,",
                "surface left padding",
            ),
            (
                ".window_padding_right = config.@\"window-padding-x\".bottom_right,",
                "surface right padding",
            ),
            (
                ".window_padding_balance = config.@\"window-padding-balance\",",
                "surface padding balance",
            ),
            ("self.balancePaddingIfNeeded();", "surface resize balance"),
            ("self.size.padding = self.config.scaledPadding(x_dpi, y_dpi);", "content-scale padding"),
        ],
    )
    require_all(
        ghostty_size,
        [
            ("pub fn grid(self: Size) GridSize", "renderer grid from padded size"),
            ("return self.screen.subPadding(self.padding);", "terminal size subtracts padding"),
            ("pub fn balancePadding(", "padding balance helper"),
        ],
    )
    require_all(
        ghostty_cell,
        [
            ("make window-padding-color=extend work better", "padding-color extend rationale"),
        ],
    )
    require_all(
        ghostty_thread,
        [
            ('.custom_shader_animation = config.@"custom-shader-animation",', "custom shader animation thread option"),
        ],
    )
    require_all(
        ghostty_shaders,
        [
            ("cursor_pos", "Metal cursor position uniform"),
            ("cursor_wide", "Metal cursor wide uniform"),
            ("uniforms.cursor_color", "Metal cursor color uniform"),
            ("IS_CURSOR_GLYPH", "Metal cursor glyph marker"),
        ],
    )
    require_all(
        ghostty_metal,
        [
            ("pub const custom_shader_target: shadertoy.Target = .msl;", "Metal custom shader target"),
            ("pub const custom_shader_y_is_down = true;", "Metal custom shader Y convention"),
        ],
    )
    require_all(
        ghostty_metal_shaders,
        [
            ("post_pipelines: []const Pipeline", "Metal post pipelines"),
            ("initPostPipelines(", "Metal post pipeline init"),
        ],
    )
    require_all(
        ghostty_container,
        [
            ("NSGlassEffectView", "macOS glass view"),
            ("glassEffectView.tintColor = backgroundColor.withAlphaComponent(backgroundOpacity)", "glass opacity tint"),
            ("effectView.updateTopInset(-themeFrameView.safeAreaInsets.top)", "glass safe-area inset"),
        ],
    )
    require_all(
        ghostty_window,
        [
            ("surfaceConfig.backgroundOpacity < 1", "window opacity threshold"),
            ("surfaceConfig.backgroundBlur.isGlassStyle", "window glass gate"),
            ("backgroundColor = .white.withAlphaComponent(0.001)", "window opacity workaround"),
            ("ghostty_set_window_background_blur", "non-glass blur ABI"),
            ("surface.derivedConfig.backgroundOpacity.clamped(to: 0.001...1)", "window alpha clamp"),
        ],
    )
    require_all(
        ghostty_titlebar,
        [
            ("derivedConfig.backgroundBlur.isGlassStyle", "titlebar glass decision"),
            (": preferredBackgroundColor?.cgColor", "titlebar preferred non-glass color"),
        ],
    )
    require_all(
        ghostty_quick,
        [
            ("self.derivedConfig.backgroundOpacity < 1", "quick opacity threshold"),
            ("derivedConfig.backgroundBlur.isGlassStyle", "quick glass gate"),
            ("ghostty_set_window_background_blur", "quick non-glass blur ABI"),
        ],
    )

    require_row_complete(
        runtime_inventory,
        "RUNTIME-008A",
        ["window-vsync", "cursor blink", "focus", "occlusion", "live renderer rebuild"],
    )
    require_row_complete(
        runtime_inventory,
        "RUNTIME-008B1",
        [
            "background-opacity",
            "background-opacity-cells",
            "window-padding-color",
            "font-thicken",
        ],
    )
    require_row_complete(
        runtime_inventory,
        "RUNTIME-008B2A",
        ["cursor", "cursor color/text color", "wide cursor", "lock"],
    )
    require_row_complete(
        runtime_inventory,
        "RUNTIME-008B2B1",
        ["password", "preedit", "active frame renderer"],
    )
    require_row_complete(
        runtime_inventory,
        "RUNTIME-008B2B2A",
        ["window-padding-x", "window-padding-y", "window-padding-balance", "padded rows/columns"],
    )
    require_row_complete(
        runtime_inventory,
        "RUNTIME-008B2B2B1",
        ["macOS glass", "NSGlassEffectView", "backgroundOpacity"],
    )
    require_row_complete(
        runtime_inventory,
        "RUNTIME-008B2B2B2A",
        ["non-glass", "backgroundOpacity", "backgroundBlur.isGlassStyle"],
    )
    require_row_complete(
        runtime_inventory,
        "RUNTIME-008B2B2B2B1",
        ["Metal custom shader output", "offscreen frame", "ping-pong", "readback"],
    )
    require_row_complete(
        runtime_inventory,
        "RUNTIME-008B2B2B2B2A",
        ["Metal text shader cursor pixel", "cursor", "readback"],
    )
    require_row_complete(
        runtime_inventory,
        "RUNTIME-008B2B2B2B2C",
        ["window-padding pixel", "screencapture", "background-dominant"],
    )
    require_row_complete(
        runtime_inventory,
        "RUNTIME-008B2B2B2B2D",
        ["block cursor pixel", "cursor-color", "magenta-dominant"],
    )
    require_row_complete(
        runtime_inventory,
        "RUNTIME-008B2B2B2B2B1",
        ["custom-shader-animation", "focus/always/false", "draw-timer"],
    )
    require_row_complete(
        runtime_inventory,
        "RUNTIME-008B2B2B2B2B2",
        [
            "background image renderer runtime",
            "background-image-opacity",
            "background-image-position",
            "background-image-fit",
            "background-image-repeat",
        ],
    )
    require_row_complete(
        runtime_inventory,
        "RUNTIME-008B2B2B2B2B3",
        [
            "colorspace and alpha-blending",
            "use_display_p3",
            "use_linear_blending",
            "use_linear_correction",
        ],
    )
    require_row_complete(
        runtime_inventory,
        "RUNTIME-008B2B2B2B2B4",
        [
            "scroll-to-bottom output",
            "synchronized output",
            "node pointer and `y`",
            "scroll_to_bottom_output_runtime_parity.py",
        ],
    )
    for line in runtime_inventory.splitlines():
        if line.startswith("| RUNTIME-008B2B2B2B2B "):
            raise AssertionError("old renderer residual row still exists")

    scroll_row = require_row(runtime_inventory, "RUNTIME-008B2B2B2B2B4")
    for forbidden in [
        "background-image-opacity",
        "background-image-position",
        "background-image-fit",
        "background-image-repeat",
        "window-colorspace",
        "alpha-blending",
    ]:
        if forbidden in scroll_row:
            raise AssertionError(f"{forbidden} still appears in scroll-to-bottom row")

    font_row = require_row(runtime_inventory, "RUNTIME-007B2B2B2B2")
    require_all(
        font_row,
        [
            ("Oracle complete", "font residual row status"),
            ("font renderer residual output effects", "font residual behavior"),
            ("font_renderer_residual_parity.py", "font residual guard"),
        ],
    )
    macos_row = require_row(runtime_inventory, "RUNTIME-011B2B")
    require(macos_row, "Oracle complete", "completed macOS walkthrough residual")
    require(
        macos_row,
        "macos_walkthrough_residual_parity.py",
        "completed macOS walkthrough guard",
    )
    notification_row = require_row(runtime_inventory, "RUNTIME-012B2B2B2B2B3C")
    require(notification_row, "Gap", "notification/link/bell gap")

    require_all(
        inventory_source,
        [
            ('id="RUNTIME-008B2B2B2B2B1"', "source custom shader animation row"),
            ('id="RUNTIME-008B2B2B2B2B2"', "source background image row"),
            ('id="RUNTIME-008B2B2B2B2B3"', "source color uniform row"),
            ('id="RUNTIME-008B2B2B2B2B4"', "source scroll-to-bottom row"),
            ("background_image_runtime_parity.py", "source background image guard"),
            ("color_uniform_runtime_parity.py", "source color uniform guard"),
            (
                "scroll_to_bottom_output_runtime_parity.py",
                "source scroll-to-bottom guard",
            ),
            ('id="RUNTIME-007B2B2B2B2"', "font residual row remains tracked"),
            ("font_renderer_residual_parity.py", "font residual guard tracked"),
            ('id="RUNTIME-011B2B"', "macOS walkthrough gap remains tracked"),
            ('id="RUNTIME-012B2B2B2B2B3C"', "notification gap remains tracked"),
        ],
    )
    if 'id="RUNTIME-008B2B2B2B2B"' in inventory_source:
        raise AssertionError("old renderer residual row still exists in source")

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

    print("renderer_visual_residual_audit=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
