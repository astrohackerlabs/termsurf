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

# Experiment 315: the powerline flame separators (E0D2/E0D4)

## Description

The "flame" powerline separators `E0D2` and `E0D4` are two filled quadrilaterals
(a top piece and a bottom piece) that taper toward a thin gap at the cell
center. Upstream `powerline.zig`'s `drawE0D2` fills the two quads; `drawE0D4`
draws the same then flips horizontally. This experiment ports
`draw_powerline_flame` over the already-wired `Canvas::fill_path` and
`Canvas::flip_horizontal` — completing the entire powerline block (`E0B0`–`E0BF`
plus `E0D2`/`E0D4`).

## Upstream behavior (`powerline.zig`)

With `w`/`h` the glyph dimensions and `t = box_thickness`, `drawE0D2` fills two
closed quads:

- **Top piece**: `move(0, 0)`, `line(w, 0)`, `line(w/2, h/2 − t/2)`,
  `line(0, h/2 − t/2)`, `close`, `fillPath(.on)`.
- **Bottom piece**: `move(0, h)`, `line(w, h)`, `line(w/2, h/2 + t/2)`,
  `line(0, h/2 + t/2)`, `close`, `fillPath(.on)`.

`drawE0D4`: `drawE0D2(…)` then `flipHorizontal`.

## Rust mapping (`roastty/src/font/sprite/draw.rs`)

`pub(crate) fn draw_powerline_flame(cp: u32, width: u32, height: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool`
— map `0xE0D2 → (flip = false)`, `0xE0D4 → (flip = true)`, `_ => false`. With
`w = width as f64`, `h = height as f64`, `t = metrics.box_thickness as f64`:

- build and `canvas.fill_path` the top quad (`MoveTo(0,0)`, `LineTo(w,0)`,
  `LineTo(w/2, h/2 − t/2)`, `LineTo(0, h/2 − t/2)`, `ClosePath`);
- build and `canvas.fill_path` the bottom quad (`MoveTo(0,h)`, `LineTo(w,h)`,
  `LineTo(w/2, h/2 + t/2)`, `LineTo(0, h/2 + t/2)`, `ClosePath`);
- if `flip`: `canvas.flip_horizontal()`.

Update the module doc.

## Scope / faithfulness notes

- **Ported**: the two flame powerline separators — completing the powerline
  block.
- **Deferred**: the unifying sprite dispatch.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/draw.rs`: add `draw_powerline_flame`; update the
   module doc.
2. Tests (deterministic — the fixture `9×18` cell, `box_thickness 2` → `t = 2`,
   so the gap is at `y ∈ (8, 10)` and the pieces fill `y ≤ 8` / `y ≥ 10` near
   the left edge):
   - `powerline_e0d2_flame`: the top piece inks the upper-left (`(0, 2)` inked),
     the bottom piece inks the lower-left (`(0, 16)` inked), and the center gap
     at the left (`(0, 9)`) is empty.
   - `powerline_e0d4_flipped`: `E0D4` is `E0D2` mirrored — the pieces' wide side
     is now on the **right** (`(8, 2)` and `(8, 16)` inked, the right-center gap
     `(8, 9)` empty); the left side (`(0, 2)`) reflects the tapered point.
   - `draw_powerline_flame_excludes`: `0x2500`, `0xE0B0`, `'M'` return `false`
     and draw nothing.
   - (The exact pixels are confirmed against the render during implementation.)
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

- `draw_powerline_flame` reproduces z2d's two-quad flame (the top/bottom piece
  vertices, the `t/2` gap) and the `E0D4` flip, returning `false` otherwise;
- the flame, flipped, and exclusion tests confirm the rendering;
- the sprite dispatch stays deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the flame needs a fill nuance the closed-path
fill does not capture.

The experiment **fails** if the flame geometry diverges from z2d, or any public
C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and raised one **Required**
finding: the upstream-behavior section's Bottom-piece fourth point read
`line(0, h/2 − t/2)` (a prettier line-wrap artifact) but upstream is
`line(0, h/2 + t/2)`; the Rust mapping already had the correct `+ t/2`. Fixed:
both the top and bottom piece vertex lists now match upstream exactly (top uses
`− t/2`, bottom `+ t/2`). Codex confirmed the rest is sound: the two-quad
geometry, `width`/`height` plus `metrics.box_thickness`, the two separate opaque
`fill_path` calls (which do not overlap — top is `y < h/2`, bottom `y > h/2` —
so no double-composite issue), and `E0D4` as a post-draw `flip_horizontal` all
match upstream; and the fixture tests are reasonable for `h = 18`, `t = 2` with
the center gap at `y = 9`.

Review artifacts:

- Prompt: `logs/codex-review/20260603-091654-545587-prompt.md`
- Result: `logs/codex-review/20260603-091654-545587-last-message.md`

## Result

**Result:** Pass

`roastty/src/font/sprite/draw.rs` gained
`draw_powerline_flame(cp, width, height, metrics, canvas)`: it fills the top
quad (`MoveTo(0,0)`, `LineTo(w,0)`, `LineTo(w/2, h/2 − t/2)`,
`LineTo(0, h/2 − t/2)`, `ClosePath`) and the bottom quad (`MoveTo(0,h)`,
`LineTo(w,h)`, `LineTo(w/2, h/2 + t/2)`, `LineTo(0, h/2 + t/2)`, `ClosePath`)
via `fill_path`, with `t = box_thickness`; `E0D4` then `flip_horizontal`;
`_ => false`.

Tests (the fixture `9×18` cell, `t = 2`, gap at `y = 8–9`), confirmed against
the render:

- `powerline_e0d2_flame` — top piece `(0,2)` inked, bottom piece `(0,16)` inked,
  center gap `(0,9)` empty.
- `powerline_e0d4_flipped` — the wide side on the right (`(8,2)`/`(8,16)` inked,
  `(8,9)` empty).
- `draw_powerline_flame_excludes` — `0x2500`, `0xE0B0`, `'M'` return `false` and
  draw nothing.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2659 passed, 0 failed (+3, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The flame separators render faithfully — completing the **entire powerline
block** (`E0B0`–`E0BF` and `E0D2`/`E0D4`). The sprite font's `z2d`-backed and
geometric coverage is now comprehensive: the box diagonals/arcs, the geometric
corner triangles, the full underline/special-sprite family, the cursors, and the
whole powerline family.

With all the rendering primitives (stroke, fill, inner-stroke, arc, triangle)
and essentially all the glyph families ported, the major remaining sprite-font
work is the unifying sprite `has_codepoint`/draw and **sprite-kind dispatch**
(mapping the codepoint tables — box, braille, sextant, octant, quadrant, block,
diagonals, arcs, geometric shapes, powerline — and a `Sprite` enum to all the
standalone `draw_*` functions, filling the resolver's deferred
`SpriteUnavailable` arm). After the sprite font: the discovery consumer, the UCD
emoji-presentation default, codepoint overrides, the shaper, the Nerd Font
attribute table, and SVG color detection.

## Completion Review

Codex reviewed the completed implementation and result and found **no Required
changes**. It confirmed `draw_powerline_flame` matches upstream: both quads use
the correct `w`/`h` dimensions and the `box_thickness` half-gap, each is filled
as a closed path, and `E0D4` is the same render followed by `flip_horizontal`;
and that the two separate fills introduce no compositing concern. No Optional
findings.

Review artifacts:

- Result review: `logs/codex-review/20260603-091912-595869-last-message.md`
