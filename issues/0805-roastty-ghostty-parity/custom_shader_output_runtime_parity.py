#!/usr/bin/env python3
"""Guard Metal custom shader output parity for Issue 805 CFG-223."""

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
    ghostty_renderer = read("vendor/ghostty/src/renderer/generic.zig")
    ghostty_metal = read("vendor/ghostty/src/renderer/Metal.zig")
    ghostty_metal_api = read("vendor/ghostty/src/renderer/metal/api.zig")
    ghostty_metal_shaders = read("vendor/ghostty/src/renderer/metal/shaders.zig")
    roastty_compositor = read("roastty/src/renderer/metal/compositor.rs")
    roastty_render_pass = read("roastty/src/renderer/metal/render_pass.rs")
    roastty_shaders = read("roastty/src/renderer/metal/shaders.rs")
    roastty_texture = read("roastty/src/renderer/metal/texture.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_renderer,
        [
            ("if (self.has_custom_shaders)", "Ghostty custom shader state creation gate"),
            ("frame.custom_shader_state = try .init(self.api)", "Ghostty custom shader state init"),
            ("frame.custom_shader_state.?.resize(", "Ghostty custom shader state resize"),
            ("try self.updateCustomShaderUniformsForFrame();", "Ghostty per-frame custom uniforms"),
            (".target = if (frame.custom_shader_state) |state|", "Ghostty offscreen render target"),
            (".{ .texture = state.back_texture }", "Ghostty back texture normal frame target"),
            ("if (frame.custom_shader_state) |*state|", "Ghostty postprocess state branch"),
            ("try state.uniforms.sync(&.{self.custom_shader_uniforms});", "Ghostty custom uniforms sync"),
            ("for (self.shaders.post_pipelines, 0..) |pipeline, i|", "Ghostty ordered post pipelines"),
            (".target = if (i < self.shaders.post_pipelines.len - 1)", "Ghostty intermediate target branch"),
            (".{ .texture = state.front_texture }", "Ghostty front texture intermediate target"),
            (".{ .target = frame.target }", "Ghostty final target branch"),
            (".textures = &.{state.back_texture}", "Ghostty samples prior pass texture"),
            ("defer state.swap();", "Ghostty ping-pong swap"),
        ],
    )
    require_all(
        ghostty_metal,
        [
            ("pub const custom_shader_target: shadertoy.Target = .msl;", "Ghostty Metal shader target"),
            ("pub const custom_shader_y_is_down = true;", "Ghostty Metal Y-down convention"),
            ("// textureOptions is currently only used for custom shaders", "Ghostty texture usage rationale"),
            (".shader_read = true,", "Ghostty custom shader texture shader-read usage"),
            (".render_target = true,", "Ghostty custom shader texture render-target usage"),
        ],
    )
    require_all(
        ghostty_metal_api,
        [
            ("shader_read: bool = false", "Ghostty shader-read texture usage flag"),
            ("render_target: bool = false", "Ghostty render-target texture usage flag"),
        ],
    )
    require_all(
        ghostty_metal_shaders,
        [
            ("post_pipelines: []const Pipeline", "Ghostty post pipeline storage"),
            ("post_shaders: []const [:0]const u8", "Ghostty post shader source input"),
            ("const post_pipelines: []const Pipeline = initPostPipelines(", "Ghostty post pipeline init"),
            ("log.warn(\"error initializing postprocess shaders err={}\", .{err});", "Ghostty post shader failure fallback"),
        ],
    )

    require_all(
        roastty_compositor,
        [
            ("fn custom_shader_output_requires_metal_device", "Roastty non-skipping Metal proof test"),
            ("pub(crate) struct MetalCustomShaderInput", "Roastty custom shader input"),
            ("pub(crate) fn draw_frame_with_images_and_custom_shaders", "Roastty image-aware custom draw entry"),
            ("fn draw_frame_with_custom_shaders_immediate", "Roastty test custom draw helper"),
            ("fn draw_frame_with_images_and_custom_shaders_immediate", "Roastty test image custom draw helper"),
            ("let custom_active = custom.is_some_and(|input| !input.pipelines.is_empty());", "Roastty custom active gate"),
            ("self.ensure_custom_shader_state(input.width, input.height, custom_active)?;", "Roastty custom state ensure"),
            ("state.back_texture", "Roastty back texture normal frame target"),
            ("state.uniforms.sync(", "Roastty custom uniform sync"),
            ("for (index, pipeline) in custom.pipelines.iter().enumerate()", "Roastty ordered custom pipelines"),
            ("let final_pass = index + 1 == custom.pipelines.len();", "Roastty final pass detection"),
            ("target.texture()", "Roastty final target texture"),
            ("state.front_texture.texture()", "Roastty intermediate texture"),
            ("pass.draw_custom_shader(", "Roastty custom shader draw call"),
            ("state.swap();", "Roastty ping-pong swap"),
            ("post_process_texture_options(options.pixel_format, options.storage_mode)", "Roastty post texture options"),
            ("fn custom_shader_sampler_descriptor()", "Roastty custom sampler descriptor"),
            ("compositor_custom_shader_samples_offscreen_frame_into_final_target", "Roastty single pass pixel test"),
            ("assert_eq!(compositor.target_bytes(), vec![0, 255, 0, 255]);", "Roastty single pass pixel readback"),
            ("compositor_custom_shader_ping_pongs_multiple_passes", "Roastty multi-pass pixel test"),
            ("assert_eq!(compositor.target_bytes(), vec![255, 0, 0, 255]);", "Roastty multi-pass pixel readback"),
            ("compositor_custom_shader_resizes_intermediate_textures", "Roastty resize test"),
            ("compositor_custom_shader_uses_shadertoy_sampler_options", "Roastty sampler test"),
            ("compositor_image_aware_frame_can_be_custom_shader_source", "Roastty image-aware custom source test"),
        ],
    )
    require_all(
        roastty_render_pass,
        [
            ("pub(crate) fn draw_custom_shader", "Roastty render pass custom draw helper"),
            ("textures: &[Some(source)]", "Roastty source texture binding"),
            ("samplers: &[Some(sampler)]", "Roastty source sampler binding"),
            ("uniforms: Some(uniforms.buffer())", "Roastty custom uniform binding"),
        ],
    )
    require_all(
        roastty_shaders,
        [
            ("pub(crate) fn build_post_process_pipelines", "Roastty post pipeline builder"),
            ("for (index, source) in sources.iter().enumerate()", "Roastty ordered post source build"),
            ("post_process_pipeline_build_values(\"main0\", pixel_format)", "Roastty post pipeline build values"),
            ("MetalPostProcessPipelineError", "Roastty post pipeline error type"),
        ],
    )
    require_all(
        roastty_texture,
        [
            ("pub(crate) fn post_process_texture_options", "Roastty post texture options helper"),
            ("shader_read: true", "Roastty post texture shader-read usage"),
            ("render_target: true", "Roastty post texture render-target usage"),
            ("fn post_process_texture_options_match_custom_shader_intent", "Roastty post texture options test"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-008B2B2B2B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "complete row status"),
            ("Metal custom shader output", "complete row behavior"),
            ("Experiment 163", "complete row experiment"),
            ("offscreen frame", "complete row offscreen evidence"),
            ("post-process pipeline", "complete row pipeline evidence"),
            ("ping-pong", "complete row ping-pong evidence"),
            ("readback", "complete row readback evidence"),
            ("custom_shader_output_runtime_parity.py", "complete row guard"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-008B2B2B2B2B")
    require_all(
        row_gap,
        [
            ("Gap", "renderer residual row status"),
            ("scroll-to-bottom.output", "concrete renderer gap evidence"),
        ],
    )
    if "scroll-to-bottom.output" not in row_gap:
        raise AssertionError("renderer residual row still has unexpected missing-evidence wording")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 status"),
            ("80 rows Oracle complete", "CFG-223 oracle count"),
            ("83 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("custom_shader_output_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
