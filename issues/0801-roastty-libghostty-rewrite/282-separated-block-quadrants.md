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

# Experiment 282: Separated Block Quadrants (U+1CC21–U+1CC2F)

## Description

The Separated Block Quadrants from Symbols for Legacy Computing Supplement — the
15 glyphs that draw a 2×2 grid of quadrant boxes with gaps between them (the
"separated" look). Upstream
`font/sprite/draw/symbols_for_legacy_computing_supplement.zig` `draw1CC21_1CC2F`
draws them with plain `canvas.box` rectangles after a gap-based layout. This is
another self-contained rect-only family for the already-ported `Canvas`.

## Upstream behavior (`draw1CC21_1CC2F`)

- `Quads` (`packed struct(u4)`): `tl, tr, bl, br` (bits 0–3), `@bitCast` of the
  low nibble of `cp - 0x1CC20`. So `0x1CC21 → 1 = tl`, `0x1CC22 → 2 = tr`,
  `0x1CC24 → 4 = bl`, `0x1CC28 → 8 = br`, `0x1CC2F → 15 = all four`.
- Layout from `width`/`height`:
  - `gap = max(1, width/12)`;
  - `mid_gap_x = 2·gap + width%2`, `mid_gap_y = 2·gap + height%2` (the centre
    gap, widened by 1 for odd dimensions so the halves stay symmetric);
  - `w = divExact(width - 2·gap - mid_gap_x, 2)`,
    `h = divExact(height - 2·gap - mid_gap_y, 2)` (the numerators are provably
    even).
- Each set quad is a `w × h` `.on` box:
  - `tl → (gap, gap)`,
  - `tr → (gap + w + mid_gap_x, gap)`,
  - `bl → (gap, gap + h + mid_gap_y)`,
  - `br → (gap + w + mid_gap_x, gap + h + mid_gap_y)` (top-left corners; each
    box spans `+w`/`+h`).

## Rust mapping (`roastty/src/font/sprite/draw.rs`)

Reuses `Canvas` and the test helpers; uses `metrics.cell_width`/`cell_height` as
`width`/`height` (upstream ignores `metrics` and reads the passed dims,
identical values).

- `fn draw_separated_quadrant(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool`:
  returns `false` unless `0x1CC21 <= cp <= 0x1CC2F`; otherwise decodes the
  nibble `q = (cp - 0x1CC20) as u8` (`tl=0x01, tr=0x02, bl=0x04, br=0x08`), runs
  the faithful layout (all `i32`; `divExact` becomes `assert!(num % 2 == 0)`
  then `num / 2`, since the numerators are provably even), and draws each set
  quad's `w × h` box with `Canvas::box`.

## Scope / faithfulness notes

- **Deferred**: the octants (`U+1CD00`–`U+1CDE5`, which need the embedded
  `octants.txt` pattern data), the circle/ellipse pieces (`canvas.line`, the
  `z2d` path API), and the rest of the supplement; the other sprite families and
  the unifying sprite `has_codepoint`/draw entry point.
- `divExact` is rendered as an explicit even-numerator `assert!` + `/ 2`,
  matching upstream's exact-division contract (the numerator
  `dim - dim%2 - 4·gap` is always even).
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/draw.rs`: add `draw_separated_quadrant`; update the
   module doc to note separated-quadrant coverage. Add a `rects_inked` test
   helper (asserts every pixel belongs to exactly the union of given
   rectangles).
2. Tests (deterministic, the fixture `Metrics` — `cell_width = 9`,
   `cell_height = 18`; layout resolves to `gap = 1`, `mid_gap_x = 3`,
   `mid_gap_y = 2`, `w = 2`, `h = 7`, giving boxes `tl x[1,3) y[1,8)`,
   `tr x[6,8) y[1,8)`, `bl x[1,3) y[10,17)`, `br x[6,8) y[10,17)`):
   - `sep_quad_tl` (`0x1CC21`): only the `tl` box.
   - `sep_quad_tr` (`0x1CC22`): only the `tr` box.
   - `sep_quad_bl` (`0x1CC24`): only the `bl` box.
   - `sep_quad_br` (`0x1CC28`): only the `br` box.
   - `sep_quad_all` (`0x1CC2F`): all four boxes, with the gaps between them
     empty.
   - `draw_separated_quadrant_excludes`: `0x1CC20`, `0x1CC30`, `'M'` return
     `false`, draw nothing.
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

- `draw_separated_quadrant` reproduces the nibble→quad mapping and the gap-based
  box layout, drawing each set quad at the right position, and returns `false`
  outside `U+1CC21`–`U+1CC2F`;
- the worked-out `9×18` box positions and the gap emptiness confirm
  faithfulness;
- the octants, circle pieces, and other families stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the gap/`divExact` math needs a different
integer shape to match upstream exactly.

The experiment **fails** if the quadrant layout or the nibble mapping diverges
from upstream or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**. It confirmed the nibble mapping (`q = cp - 0x1CC20`, `tl=0x01`,
`tr=0x02`, `bl=0x04`, `br=0x08`), that the layout matches upstream exactly with
the `divExact` numerator `dim - dim%2 - 4·gap` provably even (so
`assert!(num % 2 == 0); num / 2` is sound), that all four box positions match,
and that the `9×18` recomputation (`gap=1`, `mid_gap_x=3`, `mid_gap_y=2`, `w=2`,
`h=7`, with the four rects) and the nibble examples (`0x1CC21 tl`, `0x1CC22 tr`,
`0x1CC24 bl`, `0x1CC28 br`, `0x1CC2F all`) are correct.

Review artifacts:

- Prompt: `logs/codex-review/20260603-011434-227615-prompt.md`
- Result: `logs/codex-review/20260603-011434-227615-last-message.md`

## Result

**Result:** Pass

`roastty/src/font/sprite/draw.rs` gained `draw_separated_quadrant` (the nibble
decode, the `gap`/`mid_gap`/`w`/`h` layout with the even-numerator `assert!` for
`divExact`, and the four `Canvas::box` quad rectangles). The module doc now
notes separated-quadrant coverage, and a `rects_inked` test helper was added.

Tests (deterministic, the fixture; `gap=1`, `mid_gap_x=3`, `mid_gap_y=2`, `w=2`,
`h=7`; the four boxes `tl (1,1,3,8)`, `tr (6,1,8,8)`, `bl (1,10,3,17)`,
`br (6,10,8,17)`). `rects_inked` asserts every pixel matches exactly the union
of the expected boxes:

- `sep_quad_tl/tr/bl/br` (`0x1CC21/22/24/28`) — each single quad.
- `sep_quad_all` (`0x1CC2F`) — all four boxes, the gaps between them empty.
- `draw_separated_quadrant_excludes` — `0x1CC20`, `0x1CC30`, `'M'` return
  `false`, draw nothing.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty sprite` → 71 passed (6 new).
- `cargo test -p roastty` → 2497 passed, 0 failed (no regressions; +6).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The Separated Block Quadrants (`U+1CC21`–`U+1CC2F`) are ported and
pixel-verified — the nibble→quad decode and the gap-based 2×2 box layout both
confirmed. Seven rect/`fill`-based sprite families are now in place. The clear
next big step for the sprite font is the **`z2d` anti-aliased-path port**, which
unblocks the box-drawing arcs/diagonals, the geometric-shape curves, the
circle/ellipse pieces in this supplement, and the remaining legacy-computing
glyphs. A self-contained rect-only family that remains without `z2d` is the
**octants** (`U+1CD00`–`U+1CDE5`), which need the embedded `octants.txt` pattern
table — a data-embedding step worth its own experiment. Wiring the per-family
dispatchers under one sprite `has_codepoint`/draw entry point (which the
resolver's deferred sprite render arm needs) is increasingly worthwhile.
Alongside the sprite font remain the discovery consumer, the UCD
emoji-presentation default, codepoint overrides, the shaper, the Nerd Font
attribute table, and SVG color detection.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**. It confirmed the nibble decode matches upstream (`q = cp - 0x1CC20`,
`tl=0x01`/`tr=0x02`/`bl=0x04`/`br=0x08`), the layout formulas match exactly with
the even-numerator `assert!` a sound stand-in for `@divExact`, all four
`Canvas::box` coordinate sets match upstream, the function returns `false`
outside `0x1CC21..=0x1CC2F`, and the test rects and `rects_inked` helper are
correct for `9×18`. It judged the verification clean.

Review artifacts:

- Prompt: `logs/codex-review/20260603-011647-447625-prompt.md`
- Result: `logs/codex-review/20260603-011647-447625-last-message.md`
