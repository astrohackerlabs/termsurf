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

# Experiment 280: Braille Patterns (U+2800–U+28FF)

## Description

The Braille Patterns block — all 256 glyphs, each a subset of an 8-dot (2
columns × 4 rows) grid selected by the codepoint's low byte. Upstream
`font/sprite/draw/braille.zig` draws them with plain `canvas.box` dot rectangles
after an iterative layout pass that sizes the dots, spacings, and margins to the
cell. This is a clean rect-only family for the already-ported `Canvas`.

## Upstream behavior (`font/sprite/draw/braille.zig`)

- `Pattern` (`packed struct(u8)`): the 8 dot flags in bit order
  `tl, ul, ll, tr, ur, lr, bl, br` (bits 0–7).
  `from(cp) = @bitCast(low byte of cp)` — so dot `tl` is bit `0x01`, `ul`
  `0x02`, `ll` `0x04`, `tr` `0x08`, `ur` `0x10`, `lr` `0x20`, `bl` `0x40`, `br`
  `0x80`.
- `draw2800_28FF(cp, canvas, width, height, metrics)` (ignores `metrics`, uses
  `width`/`height`): computes the dot width `w = min(width/4, height/8)`, the
  `x_spacing = width/4`, `y_spacing = height/8`, the margins
  `x_margin = floor(x_spacing/2)`, `y_margin = floor(y_spacing/2)`, and the
  leftover budgets `x_px_left = width - 2·x_margin - x_spacing - 2·w`,
  `y_px_left = height - 2·y_margin - 3·y_spacing - 4·w`. It then runs a fixed
  refinement sequence that spends the leftover budget, in priority order: (1)
  force a non-zero dot width, (2) prefer non-zero margins, (3) increase spacing,
  (4) increase margins, (5) increase dot width — each guarded by how much budget
  remains. Finally it asserts the layout fits, builds the two column x-positions
  `x = [x_margin, x_margin + w + x_spacing]` and four row y-positions
  `y[0]=y_margin`, `y[i+1]=y[i] + w + y_spacing`, and draws a `w × w` `.on` box
  at each set dot's `(column, row)`.

## Rust mapping (`roastty/src/font/sprite/draw.rs`)

The braille family joins `draw.rs` to reuse `Canvas` and the test helpers; it
uses `metrics.cell_width`/`cell_height` as the `width`/`height` (the cell dims —
equal to upstream's passed args).

- `struct BraillePattern { tl, ul, ll, tr, ur, lr, bl, br: bool }` with
  `fn from_cp(cp: u32) -> BraillePattern` decoding `(cp & 0xFF) as u8`
  bit-by-bit in the upstream order.
- `fn draw_braille(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool`:
  returns `false` unless `0x2800 <= cp <= 0x28FF`; otherwise runs the faithful
  layout port (all `i32`, `div_euclid` for `@divFloor`, the same five refinement
  steps and the same `assert!`s), then draws the set dots with `Canvas::box`.

## Scope / faithfulness notes

- **Deferred**: the `z2d` anti-aliased primitives (arcs/diagonals) and the
  remaining sprite families (legacy-computing sextants/octants, geometric
  shapes, powerline). `draw_braille` is another sibling dispatch; the unifying
  sprite `has_codepoint`/draw entry point is a later step.
- Upstream reads `width`/`height` and ignores `metrics`; roastty passes the cell
  dims via `metrics`, identical values.
- The leftover budgets can legitimately go negative during the refinement; they
  stay `i32` (signed) exactly as upstream.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/draw.rs`: add `BraillePattern` (+ `from_cp`) and
   `draw_braille`; update the module doc to note braille coverage.
2. Tests (deterministic, the fixture `Metrics` — `cell_width = 9`,
   `cell_height = 18`). For `9×18` the layout resolves to `w = 2`, `x = [1, 6]`,
   `y = [2, 6, 10, 14]` (worked out from the refinement):
   - `braille_layout_blank` (`0x2800`): nothing drawn.
   - `braille_dot_tl` (`0x2801`): only the top-left dot — `box x[1,3) y[2,4)`;
     no other pixels.
   - `braille_dot_br` (`0x2880`): only the bottom-right dot — `x[6,8) y[14,16)`.
   - `braille_bit_mapping` (`0x284D` = bits `tl, ll, tr, bl`): dots at
     `(x0,y0)`, `(x0,y2)`, `(x1,y0)`, `(x0,y3)` inked; `ul`/`ur`/`lr`/`br`
     positions empty — proving the bit→dot order.
   - `braille_all` (`0x28FF`): all 8 dots inked (each dot's top-left pixel set),
     with the inter-dot gaps empty.
   - `draw_braille_excludes`: `0x27FF`, `0x2900`, `'M'` return `false`, draw
     nothing.
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

- `draw_braille` reproduces the upstream layout refinement and the bit→dot
  mapping, drawing each set dot as a `w × w` box at the right grid position, and
  returns `false` outside `U+2800`–`U+28FF`;
- the worked-out `9×18` dot positions and the bit-mapping test confirm
  faithfulness;
- the `z2d` primitives and other sprite families stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the layout refinement needs a different
integer-arithmetic shape to match upstream exactly.

The experiment **fails** if the dot layout or the bit mapping diverges from
upstream or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**. It confirmed the `Pattern` bit→dot mapping matches upstream exactly
(`tl..br` = bits `0x01..0x80` from `cp & 0xFF`), that the layout algorithm is
captured in the right order (initial `w`/spacings/margins, the leftover budgets,
the five refinement steps, the fit asserts, the x/y grid construction, and the
`w × w` dot boxes), that `div_euclid` is appropriate for the non-negative
division sites while the signed leftover budgets stay signed, and that the
`9×18` recomputation (`w=2`, `x_spacing 2→3`, `y_margin 1→2`, `x=[1,6]`,
`y=[2,6,10,14]`) and the `0x2801`/`0x2880`/`0x284D` test expectations are
correct.

Review artifacts:

- Prompt: `logs/codex-review/20260603-010321-884609-prompt.md`
- Result: `logs/codex-review/20260603-010321-884609-last-message.md`
