#!/usr/bin/env python3
"""Guard font fallback runtime parity for Issue 805 CFG-223."""

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
    ghostty_run = read("vendor/ghostty/src/font/shaper/run.zig")
    ghostty_resolver = read("vendor/ghostty/src/font/CodepointResolver.zig")
    ghostty_grid = read("vendor/ghostty/src/font/SharedGrid.zig")
    roastty_run = read("roastty/src/font/run.rs")
    roastty_face = read("roastty/src/font/face/coretext.rs")
    roastty_grid = read("roastty/src/font/shared_grid.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_run,
        [
            ("if (try self.indexForCell(", "Ghostty cell/grapheme font lookup"),
            ("0xFFFD, // replacement char", "Ghostty replacement fallback"),
            (".{ .idx = idx, .fallback = 0xFFFD }", "Ghostty replacement substitution"),
            ("' ',", "Ghostty space fallback lookup"),
            (".{ .idx = idx, .fallback = ' ' }", "Ghostty space substitution"),
            ("if (font_info.fallback) |cp|", "Ghostty fallback-codepoint branch"),
            ("try self.addCodepoint(&hasher, cp, @intCast(cluster));", "Ghostty fallback adds single codepoint"),
        ],
    )
    require_all(
        ghostty_resolver,
        [
            ("if (style == .regular and font.Discover != void)", "Ghostty regular-style fallback discovery"),
            ("disco.discoverFallback", "Ghostty discovery iterator"),
            (".fallback = true", "Ghostty fallback face marker"),
            ("font.default_fallback_adjustment", "Ghostty fallback size adjustment"),
            ("face.hasCodepoint(cp, p_mode)", "Ghostty presentation recheck"),
            ("addDeferred", "Ghostty deferred fallback add"),
        ],
    )
    require_all(
        ghostty_grid,
        [
            ("pub fn renderCodepoint", "Ghostty render-codepoint entry"),
            ("const index = try self.getIndex", "Ghostty render-codepoint index lookup"),
            ("face.glyphIndex(cp)", "Ghostty render-codepoint glyph lookup"),
            ("return try self.renderGlyph", "Ghostty render-codepoint render call"),
        ],
    )

    require_all(
        roastty_run,
        [
            ("fn resolve_font", "Roastty fallback resolver helper"),
            ("index_for_grapheme(primary_cp", "Roastty primary grapheme lookup"),
            ("self.resolver.get_index(0xFFFD", "Roastty replacement lookup"),
            ("return (idx, Some(0xFFFD));", "Roastty replacement substitution"),
            ("get_index(' ' as u32", "Roastty space lookup"),
            ("(idx, Some(' ' as u32))", "Roastty space substitution"),
            ("fn next_missing_codepoint_substitutes_replacement", "Roastty missing-codepoint test"),
            ("vec![('A' as u32, 0), (0xFFFD, 1), ('B' as u32, 2)]", "Roastty replacement-codepoint assertion"),
            ("fn next_placeholder_is_space", "Roastty placeholder-space test"),
        ],
    )
    require_all(
        roastty_face,
        [
            ("pub(crate) fn font_for_codepoint", "Roastty CoreText fallback entry"),
            ("self.font.for_string", "Roastty CoreText for-string lookup"),
            ("post_script_name() }.to_string() == \"LastResort\"", "Roastty LastResort rejection"),
            ("fn font_for_codepoint_cjk", "Roastty CJK fallback test"),
            ("fn font_for_codepoint_supplementary", "Roastty emoji fallback test"),
            ("fn font_for_codepoint_none", "Roastty LastResort-none test"),
        ],
    )
    require_all(
        roastty_grid,
        [
            ("fn get_index_discovery_fallback_caches_without_duplicates", "Roastty fallback cache test"),
            ("expect(\"emoji fallback resolves\")", "Roastty fallback resolve assertion"),
            ("face_count", "Roastty fallback duplicate guard"),
            ("fn get_index_deferred_discovery_preloads_face_before_caching", "Roastty deferred preload test"),
            ("expect(\"fallback entry exists\")", "Roastty fallback entry assertion"),
            ("is_deferred()", "Roastty deferred state assertion"),
            ("pub(crate) fn render_codepoint", "Roastty render-codepoint entry"),
            ("let Some(glyph_index) = self.resolver.glyph_index", "Roastty glyph lookup"),
            ("Ok(Some(self.render_glyph", "Roastty render-codepoint render call"),
            ("fn render_codepoint_renders_a_present_glyph", "Roastty present render test"),
            ("fn render_codepoint_missing_codepoint_is_none", "Roastty missing render test"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-007B2B2B2B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "complete row status"),
            ("font fallback resolution", "complete row behavior"),
            ("Experiment 165", "complete row experiment"),
            ("U+FFFD", "replacement fallback evidence"),
            ("LastResort", "LastResort evidence"),
            ("discovery caches", "fallback cache evidence"),
            ("render-codepoint", "render-codepoint evidence"),
            ("font_fallback_runtime_parity.py", "complete row guard"),
        ],
    )

    row_font_residual = require_row(runtime_inventory, "RUNTIME-007B2B2B2B2")
    require_all(
        row_font_residual,
        [
            ("Oracle complete", "font residual row status"),
            ("font renderer residual output effects", "font residual behavior"),
            ("CoreText fallback discovery", "font residual fallback evidence"),
            ("shaping clusters", "font residual shaping evidence"),
            ("font_renderer_residual_parity.py", "font residual guard"),
        ],
    )
    if "RUNTIME-007B2B2B2B |" in runtime_inventory:
        raise AssertionError("old broad RUNTIME-007B2B2B2B row is still present")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 status"),
            ("89 rows Oracle complete", "CFG-223 oracle count"),
            ("92 rows closed", "CFG-223 closed count"),
            ("1 rows are incomplete", "CFG-223 incomplete count"),
            ("1 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("font_fallback_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
