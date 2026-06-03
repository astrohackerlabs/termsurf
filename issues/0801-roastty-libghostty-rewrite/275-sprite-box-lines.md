+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"
+++

# Experiment 275: Sprite box-drawing — the lines_char primitive

## Description

The sprite font draws box-drawing/block/braille glyphs procedurally into the
atlas via the (already-ported) `Canvas`. Its foundational primitive is
`linesChar` (`font/sprite/draw/box.zig`), which draws the box-drawing **line**
glyphs (`U+2500`–`U+254B`: straight lines, corners, T-junctions, crosses) from a
per-direction line style. This experiment ports `Thickness`, the line style and
`Lines`, `lines_char`, and a dispatch for a representative set of line
characters. The other box-drawing primitives (dashes, arcs, diagonals) and the
other sprite categories (block, braille, powerline, legacy) are later
experiments.

## Upstream behavior (`font/sprite/draw/box.zig`, `common.zig`)

- `Thickness { super_light, light, heavy }`; `height(base)` →
  `super_light: max(base/2, 1)`, `light: base`, `heavy: base*2`.
- `Lines { up, right, down, left: Style }`,
  `Style { none, light, heavy, double }`.
- `linesChar(metrics, canvas, lines)`: from
  `metrics.box_thickness`/`cell_width`/ `cell_height` it computes the
  light/heavy/double stroke edges and the meeting points (`up_bottom`,
  `down_top`, `left_right`, `right_left`) where perpendicular strokes join, then
  draws a `canvas.box(..., .on)` rectangle for each non-`none` direction (with
  the `double` style drawing two parallel strokes). All arithmetic is
  **saturating** (`-|`, `+|`).
- The `draw2500_257F` switch maps each line codepoint to a `linesChar(...)` call
  with the appropriate `Lines` (e.g. `0x2500` → `{ left: light, right: light }`,
  `0x253C` → all four `light`).

## Rust mapping (`roastty/src/font/sprite/draw.rs`, new)

- `enum Thickness { SuperLight, Light, Heavy }` with
  `fn height(self, base: u32) -> u32` (`super_light → (base/2).max(1)`,
  `light → base`, `heavy → base*2`).
- `enum LineStyle { None, Light, Heavy, Double }` (`PartialEq` for the meeting
  logic).
- `struct Lines { up: LineStyle, right: LineStyle, down: LineStyle, left: LineStyle }`
  (`Default` = all `None`).
- `fn lines_char(metrics: &Metrics, canvas: &mut Canvas, lines: Lines)`: the
  faithful port — `saturating_sub`/`saturating_add` for `-|`/`+|`, `u32`→`i32`
  casts for `Canvas::box`, `Color::ON` for `.on`, and the four `match` arms
  (`up`/`right`/`down`/`left`) including the `Double` two-stroke geometry.
- `fn draw_box_lines(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool`:
  dispatches the representative line chars to `lines_char` and returns `true` if
  drawn — `0x2500 ─`/`0x2501 ━` (h light/heavy), `0x2502 │`/`0x2503 ┃` (v),
  `0x253C ┼`/`0x254B ╋` (cross light/heavy), the four light corners
  `0x250C ┌`/`0x2510 ┐`/`0x2514 └`/`0x2518 ┘`, and the **double** chars
  `0x2550 ═` (`left/right = double`), `0x2551 ║` (`up/down = double`), and
  `0x256C ╬` (all four `double`) so the two-stroke `Double` geometry is
  exercised. (The full `U+2500`–`U+257F` switch, incl. dashes/arcs/diagonals, is
  deferred.)
- `roastty/src/font/sprite/mod.rs`: `pub(crate) mod draw;`.

## Scope / faithfulness notes

- **Deferred**: the remaining box-drawing primitives (dashes `0x2504`–`250B`,
  rounded corners `0x256D`–`2570`, diagonals `0x2571`–`2573`), the full
  `draw2500_257F` dispatch, the sprite `hasCodepoint` inventory (which the full
  dispatch enables), and the other sprite categories (block/braille/powerline/
  legacy). `lines_char` is the foundational primitive they all build on.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/draw.rs` (new): `Thickness`, `LineStyle`, `Lines`,
   `lines_char`, `draw_box_lines`.
2. `roastty/src/font/sprite/mod.rs`: declare `draw`.
3. Tests (deterministic, fixture `Metrics`):
   - a `fixture_metrics()` with `cell_width = 9`, `cell_height = 18`,
     `box_thickness = 2` (and the other fields filled).
   - `thickness_heights`: `Light.height(2) == 2`, `Heavy.height(2) == 4`,
     `SuperLight.height(2) == 1`, `SuperLight.height(1) == 1`.
   - `box_light_horizontal`: draw `0x2500` into a `Canvas`; the inked pixels
     form a horizontal band centered vertically (`y` in `[(h-2)/2, (h-2)/2+2)`),
     spanning the full width, with no ink in the top/bottom rows.
   - `box_light_vertical`: `0x2502` → a vertical band centered horizontally,
     spanning the full height, with empty left/right columns.
   - `box_light_cross`: `0x253C` → both bands present (ink in the center row
     span across the width _and_ the center column span down the height).
   - `box_heavy_horizontal`: `0x2501` → the band is twice as tall as `0x2500`'s.
   - `box_double_horizontal`: `0x2550` (`left/right = double`) → **two**
     separate horizontal bands (the upper at `h_double_top..h_light_top`, the
     lower at `h_light_bottom..h_double_bottom`) with the light-stroke-height
     gap between them, verifying the `Double` two-stroke split.
   - `box_double_vertical`: `0x2551` (`up/down = double`) → two separate
     vertical bands with a gap, mirrored on the vertical axis.
   - `box_double_cross`: `0x256C` (all four `double`) → the four corner-notched
     two-stroke arms — checks that the double-vs-double meeting points
     (`left_top`/`right_top`/`top_right`/`bottom_right` conditioned on the
     perpendicular `== Double`) leave the center hole, i.e. the cell center
     pixel is **off**.
   - `draw_box_lines_unknown`: `draw_box_lines('M' as u32, …)` returns `false`
     and draws nothing.
   - Pixel inspection: add a
     `#[cfg(test)] pub(crate) fn get(&self, x: i32, y: i32) -> u8` to `Canvas`
     (the padding-offset read mirroring `pixel`), so the `draw` tests in a
     sibling module can read back ink without touching the private `buf`.
     (Test-only; no change to the real Canvas API.)
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty sprite
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `lines_char` faithfully ports the stroke-edge/meeting geometry and the four
  direction arms (incl. `double`), with saturating arithmetic;
- `draw_box_lines` draws the representative line chars and returns `false` for a
  non-box codepoint;
- the rendered light horizontal/vertical/cross and heavy horizontal have the
  expected centered bands and widths;
- the remaining box primitives, the full dispatch, and `hasCodepoint` are
  cleanly deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if `lines_char`'s geometry needs a different
casting/saturation shape than expected.

The experiment **fails** if the line geometry diverges from upstream or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation. It raised one **Medium**
finding: `LineStyle::Double` is the highest-risk geometry but the original
dispatch (`U+2500`–`U+254B`) contained no double-line glyphs and no test
exercised it. Fixed by dispatching the double chars `0x2550 ═`
(`left/right = double`), `0x2551 ║` (`up/down = double`), and `0x256C ╬` (all
four `double`), and adding `box_double_horizontal`, `box_double_vertical`, and
`box_double_cross` (the all-double center-hole) tests. Codex confirmed the fix
resolves the finding — the mappings and two-stroke geometry match upstream
`linesChar` — with no remaining required design changes.

Review artifacts:

- Prompt: `logs/codex-review/20260602-232006-065664-prompt.md`
- Result: `logs/codex-review/20260602-232006-065664-last-message.md`
- Follow-up: `logs/codex-review/20260602-232137-381555-last-message.md`

## Result

**Result:** Pass

`roastty/src/font/sprite/draw.rs` (new) ports `Thickness` (`height` via
`super_light = (base/2).max(1)`, `light = base`, `heavy = base*2`), `LineStyle`
(`None`/`Light`/`Heavy`/`Double`), `Lines`, `lines_char` (the faithful
`linesChar` port — light/heavy/double stroke edges, the four meeting points, and
the four direction arms incl. the `Double` two-stroke split, all with
`saturating_sub`/`saturating_add` and `u32 → i32` casts to `Canvas::box`), and
`draw_box_lines` (dispatch for the representative line chars + the three double
chars). `sprite/mod.rs` declares `draw`; `Canvas` gained a `#[cfg(test)]`
`get(x, y)` read-back helper.

Tests (deterministic, fixture `Metrics` with `cell_width = 9`,
`cell_height = 18`, `box_thickness = 2`):

- `thickness_heights` — `Light(2)=2`, `Heavy(2)=4`, `SuperLight(2)=1`,
  `SuperLight(1)=1`.
- `box_light_horizontal` — `0x2500` → a 2px band at rows 8–9 across the full
  width, nothing above/below or at the top/bottom edges.
- `box_light_vertical` — `0x2502` → a 2px band at cols 3–4 down the full height,
  empty either side.
- `box_light_cross` — `0x253C` → both center bands present and overlapping at
  the center.
- `box_heavy_horizontal` — `0x2501` → a 4px band (twice the light height) at
  rows 7–10.
- `box_double_horizontal` — `0x2550` → two bands (rows 6–7 and 10–11) with a 2px
  gap at rows 8–9.
- `box_double_vertical` — `0x2551` → two bands (cols 1–2 and 5–6) with a gap at
  cols 3–4.
- `box_double_cross` — `0x256C` → the center light-stroke rectangle (cols 3–4 ×
  rows 8–9) is the unfilled hole, with the double arms still inked at the edges.
- `draw_box_lines_unknown` — `'M'` → `false`, canvas untouched.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty sprite` → 22 passed (9 new).
- `cargo test -p roastty` → 2448 passed, 0 failed (no regressions; +9).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The sprite font's foundational drawing primitive, `lines_char`, is ported and
verified against deterministic ink geometry — including the highest-risk
`Double` two-stroke arms and the all-double center hole. The sprite subsystem
can now draw box-drawing line glyphs into a `Canvas`. The next sprite work
builds outward from here: the rest of the `draw2500_257F` dispatch (the
remaining straight/junction line glyphs), the other box-drawing primitives
(dashes, arcs, diagonals), and then the other sprite categories (block, braille,
powerline, legacy) — which together enable the sprite `hasCodepoint` inventory
and, in turn, the resolver's deferred sprite render arm (`SpriteUnavailable`)
and sprite-presentation path. Alongside the sprite font remain the discovery
consumer, the UCD emoji-presentation default, codepoint overrides, the shaper,
the Nerd Font attribute table, and SVG color detection.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**. It verified line-by-line that `Thickness::height`, every
light/heavy/double stroke-edge calculation, the saturating `-|`/`+|`
equivalents, the four nested meeting-point expressions, and every direction arm
(including the `Double` two-stroke conditionals) match the Zig `linesChar`
source; that the dispatched codepoints map to the same `Lines` as upstream; and
that the tests meaningfully cover the geometry (centered light/heavy strokes,
full-span behavior, the double horizontal/vertical splits, and the all-double
center hole). With the full `cargo test`, build, fmt, name-gate, and diff-check
results, Codex judged the completion sound.

Review artifacts:

- Prompt: `logs/codex-review/20260602-232517-158682-prompt.md`
- Result: `logs/codex-review/20260602-232517-158682-last-message.md`
