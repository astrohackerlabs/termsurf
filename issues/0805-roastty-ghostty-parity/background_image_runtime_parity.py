#!/usr/bin/env python3
"""Guard background image renderer runtime parity for Issue 805 CFG-223."""

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
    ghostty_renderer = read("vendor/ghostty/src/renderer/generic.zig")
    roastty_image = read("roastty/src/renderer/image.rs")
    roastty_shader = read("roastty/src/renderer/shader.rs")
    roastty_render_pass = read("roastty/src/renderer/metal/render_pass.rs")
    roastty_compositor = read("roastty/src/renderer/metal/compositor.rs")
    roastty_frame_renderer = read("roastty/src/renderer/frame_renderer.rs")
    roastty_lib = read("roastty/src/lib.rs")
    inventory_source = read(
        "issues/0805-roastty-ghostty-parity/config_runtime_inventory.py"
    )
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_config,
        [
            ('@"background-image": ?Path = null,', "Ghostty background-image field"),
            (
                '@"background-image-opacity": f32 = 1.0,',
                "Ghostty background image opacity field",
            ),
            (
                '@"background-image-position": BackgroundImagePosition = .center,',
                "Ghostty background image position field",
            ),
            (
                '@"background-image-fit": BackgroundImageFit = .contain,',
                "Ghostty background image fit field",
            ),
            (
                '@"background-image-repeat": bool = false,',
                "Ghostty background image repeat field",
            ),
        ],
    )
    require_all(
        ghostty_renderer,
        [
            ('if (config.@"background-image") |bg|', "Ghostty derived path copy"),
            (".bg_image = bg_image,", "Ghostty derived image field"),
            (
                '.bg_image_opacity = config.@"background-image-opacity",',
                "Ghostty derived opacity copy",
            ),
            (
                '.bg_image_position = config.@"background-image-position",',
                "Ghostty derived position copy",
            ),
            (
                '.bg_image_fit = config.@"background-image-fit",',
                "Ghostty derived fit copy",
            ),
            (
                '.bg_image_repeat = config.@"background-image-repeat",',
                "Ghostty derived repeat copy",
            ),
            ("fn prepBackgroundImage(self: *Self) !void", "Ghostty image prep"),
            ("FileType.detect(contents)", "Ghostty file type detection"),
            (".png => try wuffs.png.decode", "Ghostty PNG decode"),
            (".jpeg => try wuffs.jpeg.decode", "Ghostty JPEG decode"),
            ("img.markForReplace(self.alloc, image)", "Ghostty replacement"),
            ("img.markForUnload()", "Ghostty unload"),
            ("fn uploadBackgroundImage(self: *Self) !void", "Ghostty upload"),
            ("bg.isUnloading()", "Ghostty unload upload branch"),
            ("bg.isPending()", "Ghostty pending upload branch"),
            ("const bg_image_config_changed =", "Ghostty option change detection"),
            ("const bg_image_changed =", "Ghostty path change detection"),
            ("if (bg_image_changed) try self.prepBackgroundImage();", "Ghostty path reload"),
            ("if (bg_image_config_changed) self.updateBgImageBuffer();", "Ghostty buffer refresh"),
            ("fn updateBgImageBuffer(self: *Self) void", "Ghostty vertex buffer update"),
            (".opacity = self.config.bg_image_opacity,", "Ghostty vertex opacity"),
            (".position = switch (self.config.bg_image_position)", "Ghostty vertex position"),
            (".fit = switch (self.config.bg_image_fit)", "Ghostty vertex fit"),
            (".repeat = self.config.bg_image_repeat,", "Ghostty vertex repeat"),
            (".pipeline = self.shaders.pipelines.bg_image", "Ghostty draw pipeline"),
            (".textures = &.{texture}", "Ghostty draw texture"),
            (".vertex_count = 3", "Ghostty fullscreen triangle"),
        ],
    )

    require_all(
        roastty_image,
        [
            ("pub(crate) struct BackgroundImageConfig", "Roastty image config"),
            ("pub(crate) fn from_config(config: &Config) -> Option<Self>", "Roastty config source"),
            ("let path = config.background_image.as_ref()?;", "Roastty image path source"),
            ("opacity: config.bg_image_opacity", "Roastty opacity source"),
            ("position: config.bg_image_position", "Roastty position source"),
            ("fit: config.bg_image_fit", "Roastty fit source"),
            ("repeat: config.bg_image_repeat", "Roastty repeat source"),
            ("pub(crate) fn vertex(&self) -> BgImageVertex", "Roastty vertex pack"),
            ("BackgroundImagePosition::BottomRight => BgImagePosition::BottomRight", "Roastty position mapping"),
            ("BackgroundImageFit::Cover => BgImageFit::Cover", "Roastty fit mapping"),
            ("self.repeat", "Roastty repeat pack"),
            ("pub(crate) fn update_from_config(&mut self, config: &Config) -> bool", "Roastty state update"),
            ("let path_changed = match", "Roastty path change detection"),
            ("load_background_image(config)", "Roastty image load"),
            ("image.mark_for_replace(pending)", "Roastty replace"),
            ("image.mark_for_unload()", "Roastty unload"),
            ("pub(crate) fn upload<Backend>", "Roastty upload"),
            ("fn load_background_image(", "Roastty loader"),
            ("ImageFormat::Png", "Roastty PNG support"),
            ("ImageFormat::Jpeg", "Roastty JPEG support"),
            ("fn background_image_state_loads_decodes_uploads_reuses_and_replaces", "Roastty load test"),
            ("fn background_image_failed_replacement_preserves_previous_ready_image", "Roastty failed replace test"),
            ("fn background_image_reset_unloads_and_initial_missing_skips", "Roastty reset test"),
            ("fn background_image_initial_unsupported_format_skips_without_image", "Roastty unsupported test"),
            ("fn background_image_config_packs_vertex_from_config", "Roastty vertex test"),
        ],
    )
    require_all(
        roastty_shader,
        [
            ("fn bg_image_vertex_layout_matches_upstream_shader_parameter", "Roastty vertex layout test"),
            ("fn bg_image_position_raw_values_match_upstream", "Roastty position raw test"),
            ("fn bg_image_fit_raw_values_match_upstream", "Roastty fit raw test"),
            ("fn bg_image_info_packs_position_fit_and_repeat", "Roastty info pack test"),
        ],
    )
    require_all(
        roastty_render_pass,
        [
            ("pub(crate) fn draw_background_image(", "Roastty render pass helper"),
            ("pipeline: &pipelines.bg_image", "Roastty render pass pipeline"),
            ("textures: &[Some(texture)]", "Roastty render pass texture"),
            ("primitive_type: MetalPrimitiveType::Triangle", "Roastty render primitive"),
            ("vertex_count: 3", "Roastty render triangle"),
            ("fn bg_image_render_pass_draws_texture_over_background", "Roastty pixel draw test"),
            ("fn bg_image_none_fit_uses_vertex_texture_size_for_placement", "Roastty none fit pixel test"),
            ("fn bg_image_zero_instance_step_does_not_bind_or_draw", "Roastty zero instance test"),
        ],
    )
    require_all(
        roastty_compositor,
        [
            ("if let Some(texture) = background.ready_texture()", "Roastty background ready gate"),
            ("background.upload(&mut upload_backend)", "Roastty compositor upload"),
            ("&[background.vertex()]", "Roastty vertex upload"),
            ("pass.draw_background_image(", "Roastty image draw call"),
            ("pass.draw_background_color(", "Roastty fallback background draw"),
        ],
    )
    require_all(
        roastty_frame_renderer,
        [
            ("background.update_from_config(config)", "Roastty frame config update"),
            ("fn live_background_image_frame_renderer_presents_config_path_and_unloads", "Roastty live frame test"),
            ("expect(\"background image frame should present\")", "Roastty live present assertion"),
            ("background image should add red contribution", "Roastty live pixel assertion"),
            ("expect(\"reset background frame should present\")", "Roastty live reset assertion"),
        ],
    )
    require_all(
        roastty_lib,
        [
            ("background_image: renderer::image::BackgroundImageState", "Roastty live renderer field"),
            ("background_image: renderer::image::BackgroundImageState::default()", "Roastty live init"),
            ("background_image,", "Roastty live present destructure"),
            ("render_and_present_frame_with_images", "Roastty live render route"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-008B2B2B2B2B2")
    require_all(
        row_complete,
        [
            ("Oracle complete", "complete row status"),
            ("background image renderer runtime", "complete row behavior"),
            ("Experiment 181", "complete row experiment"),
            ("background-image-opacity", "complete row opacity evidence"),
            ("background-image-position", "complete row position evidence"),
            ("background-image-fit", "complete row fit evidence"),
            ("background-image-repeat", "complete row repeat evidence"),
            ("background_image_runtime_parity.py", "complete row guard"),
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
    for forbidden in [
        "background-image-opacity",
        "background-image-position",
        "background-image-fit",
        "background-image-repeat",
        "window-colorspace",
        "alpha-blending",
    ]:
        if forbidden in row_gap:
            raise AssertionError(f"{forbidden} still appears in remaining renderer gap")

    require_all(
        inventory_source,
        [
            ('id="RUNTIME-008B2B2B2B2B2"', "source complete row"),
            ("background_image_runtime_parity.py", "source guard"),
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

    print("background_image_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
