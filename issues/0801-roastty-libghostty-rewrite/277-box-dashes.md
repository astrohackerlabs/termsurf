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

# Experiment 277: Box-drawing dash primitives

## Description

The next deferred box-drawing primitive after `lines_char`: the **dashes**.
Upstream's `draw2500_257F` routes 12 codepoints through `dashHorizontal` /
`dashVertical` (`font/sprite/draw/box.zig`) — `U+2504`–`U+250B` (the triple- and
quadruple-dash light/heavy lines) and `U+254C`–`U+254F` (the double-dash
light/heavy lines). This experiment ports those two functions, their `hline` /
`vline` / `hlineMiddle` / `vlineMiddle` helpers (`font/sprite/draw/common.zig`),
and a `draw_box_dashes` dispatch for the 12 codepoints.

## Upstream behavior

- `hline(canvas, x1, x2, y, thick)` → `canvas.box(x1, y, x2, y + thick, .on)`;
  `vline(canvas, y1, y2, x, thick)` → `canvas.box(x, y1, x + thick, y2, .on)`.
- `hlineMiddle(metrics, canvas, thickness)` / `vlineMiddle(…)`: a centered solid
  line of the given thickness across the full cell width/height.
- `dashHorizontal(metrics, canvas, count, thick_px, desired_gap)`
  (`count ∈ [2, 4]`): draws `count` evenly-tiled horizontal dashes centered
  vertically, with half-gaps on each side so the pattern tiles seamlessly. If
  the cell is too narrow (`cell_width < count + gap_count`,
  `gap_count = count`), it falls back to a solid light `hlineMiddle`. Otherwise:
  `gap_width = min(desired_gap, cell_width / (2*count))`,
  `total_gap = gap_count * gap_width`, `total_dash = cell_width - total_gap`,
  `dash_width = floor(total_dash/count)`, `remaining = total_dash mod count`.
  The dashes start at `x = gap_width/2`; the `remaining` extra pixels are
  distributed one-per-dash into the **dash** widths (not the gaps);
  `y = (cell_height -| thick_px) / 2`.
- `dashVertical(…)`: the vertical analogue — a single full extra gap at the
  bottom (`gap_count = count`), dashes start at `y = 0`, centered horizontally
  at `x = (cell_width -| thick_px) / 2`, falls back to a solid light
  `vlineMiddle` when `cell_height < count + gap_count`.
- Both `assert(count >= 2 and count <= 4)` and assert the
  `dash*count + gap*gap_count + remaining == cell_extent` invariant.

The 12 dispatched codepoints (`count`, `thick`, `desired_gap`):

| cp       | fn   | count | thick | desired_gap   |
| -------- | ---- | ----- | ----- | ------------- |
| `0x2504` | hdsh | 3     | light | max(4, light) |
| `0x2505` | hdsh | 3     | heavy | max(4, light) |
| `0x2506` | vdsh | 3     | light | max(4, light) |
| `0x2507` | vdsh | 3     | heavy | max(4, light) |
| `0x2508` | hdsh | 4     | light | max(4, light) |
| `0x2509` | hdsh | 4     | heavy | max(4, light) |
| `0x250A` | vdsh | 4     | light | max(4, light) |
| `0x250B` | vdsh | 4     | heavy | max(4, light) |
| `0x254C` | hdsh | 2     | light | light         |
| `0x254D` | hdsh | 2     | heavy | heavy         |
| `0x254E` | vdsh | 2     | light | heavy         |
| `0x254F` | vdsh | 2     | heavy | heavy         |

## Rust mapping (`roastty/src/font/sprite/draw.rs`)

- `fn hline(canvas, x1: i32, x2: i32, y: i32, thick: u32)` and
  `fn vline(canvas, y1: i32, y2: i32, x: i32, thick: u32)` — thin `Canvas::box`
  wrappers (`Color::ON`).
- `fn hline_middle(metrics, canvas, thickness: Thickness)` /
  `fn vline_middle(…)` — centered full-extent solid line.
- `fn dash_horizontal(metrics, canvas, count: u32, thick_px: u32, desired_gap: u32)`
  and `fn dash_vertical(…)`: faithful ports.
  `assert!(2 <= count && count <= 4)`; the `< count + gap_count` solid-line
  fallback; `i32` arithmetic with `div_euclid`/`rem_euclid` for
  `@divFloor`/`@mod` (operands are non-negative here, so this matches floor/mod
  exactly); the invariant `assert!`; the one-per-dash `extra` distribution;
  `(cell_extent.saturating_sub(thick_px))/2` for the centered offset.
- `fn draw_box_dashes(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool`:
  a `match` over the 12 codepoints computing `(count, thick_px, desired_gap)`
  from `Thickness::{Light,Heavy}.height(metrics.box_thickness)` and calling the
  right primitive; `_ => false`.

## Scope / faithfulness notes

- **Deferred**: the rounded corners and diagonals (`0x256D`–`0x2573`,
  `arc`/`lightDiagonal*`), and the other sprite categories
  (block/braille/powerline/legacy). `draw_box_dashes` is a sibling dispatch to
  `draw_box_lines`; wiring all the box dispatchers under one
  `has_codepoint`/draw entry point is a later step.
- `count` is a runtime `u32` (upstream `dashVertical` takes it `comptime`, but
  the value range and behavior are identical).
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/draw.rs`: add `hline`, `vline`, `hline_middle`,
   `vline_middle`, `dash_horizontal`, `dash_vertical`, and `draw_box_dashes`.
2. Tests (deterministic, the Experiment 275 fixture `Metrics` —
   `cell_width = 9`, `cell_height = 18`, `box_thickness = 2`; `light = 2`,
   `heavy = 4`):
   - `dash_horizontal_3` (`0x2504`): the exact computed pattern — segments at
     `x ∈ [0,2) ∪ [3,5) ∪ [6,8)` on rows 8–9, gaps at `x = 2, 5, 8`.
   - `dash_vertical_3` (`0x2506`): segments at `y ∈ [0,3) ∪ [6,9) ∪ [12,15)` on
     cols 3–4, gaps between.
   - `dash_count_4` (`0x2508`): four segments (the first 2px wide from the
     distributed `remaining`, the rest 1px):
     `x ∈ [0,2) ∪ [3,4) ∪ [5,6) ∪ [7,8)`.
   - `dash_double_2` (`0x254C`): two segments `x ∈ [1,4) ∪ [6,8)` on rows 8–9.
   - `dash_heavy_thickness` (`0x2505`): a dash band is 4px tall (rows 7–10), vs
     the light 2px.
   - `dash_fallback_solid`: a narrow `Metrics` (`cell_width = 5`) with `0x2504`
     (`5 < 3 + 3`) → a **solid** continuous light line (rows 8–9 inked across
     the full width, no gaps).
   - `draw_box_dashes_excludes`: a line char (`0x2500`) and `'M'` return `false`
     and draw nothing.
3. Format and test (`cargo fmt`, accept output).

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

- `dash_horizontal`/`dash_vertical` reproduce the upstream tiling math (gap
  clamping, `remaining` distribution, centering) and the solid-line fallback;
- `draw_box_dashes` dispatches all 12 codepoints with the correct
  `(count, thick, gap)` and returns `false` otherwise;
- the rounded corners/diagonals and other sprite categories stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the integer-division/centering math needs a
different shape than `div_euclid`/`rem_euclid` to match upstream exactly.

The experiment **fails** if the dash geometry diverges from upstream or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**. It confirmed the 12-codepoint dispatch table (count/thickness/gap)
matches the upstream switch arms, that `div_euclid`/`rem_euclid` are safe and
agree with Zig `@divFloor`/`@mod` for the non-negative operands here, that the
solid-line fallback condition / `gap_count = count` / the horizontal half-gap
start / the vertical `y = 0` start / the one-per-dash `remaining` distribution
all match upstream, and that the worked-out test pixel patterns (`0x2504`,
`0x2506`, `0x2508`, `0x254C`, and the `cell_width = 5` solid fallback) are
arithmetically correct for the `9×18`, `box_thickness = 2` fixture.

(The first design-gate invocation hit a transient Codex backend `429`; the retry
above succeeded.)

Review artifacts:

- Prompt: `logs/codex-review/20260602-233919-805836-prompt.md`
- Result: `logs/codex-review/20260602-233919-805836-last-message.md`
