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

# Experiment 281: Sextants (U+1FB00–U+1FB3B)

## Description

The sextant glyphs from Symbols for Legacy Computing — a 2-column × 3-row grid
of cells, each glyph a subset of the six cells. Upstream
`font/sprite/draw/symbols_for_legacy_computing.zig` `draw1FB00_1FB3B` draws them
purely with `fill` and `Fraction` lines, which Experiment 278 already ported.
This is a clean fill-only family, parallel to the block-element quadrants.

## Upstream behavior (`draw1FB00_1FB3B`)

- `Sextants` (`packed struct(u6)`): six cell flags, bit order
  `tl, tr, ml, mr, bl, br` (bits 0–5).
- The 60 codepoints map to patterns by index: `idx = cp - 0x1FB00`;
  `sex = (idx + idx/0x14 + 1) as u6`. The `+ 1` skips the empty pattern (`0`),
  and the `+ idx/0x14` (0x14 = 20) skips the two patterns `0b010101` (left
  column = left half block `▌`) and `0b101010` (right column = right half block
  `▐`) that already have dedicated block-element codepoints. So the produced
  `sex` values run `1..=62` minus `{21, 42}`.
- Each set cell is filled (`fill(metrics, canvas, x0, x1, y0, y1)`):
  - columns: left `zero..half`, right `half..full`;
  - rows: top `zero..one_third`, middle `one_third..two_thirds`, bottom
    `two_thirds..end`.
  - `tl→(zero,half, zero,one_third)`, `tr→(half,full, zero,one_third)`,
    `ml→(zero,half, one_third,two_thirds)`,
    `mr→(half,full, one_third,two_thirds)`, `bl→(zero,half, two_thirds,end)`,
    `br→(half,full, two_thirds,end)`.

## Rust mapping (`roastty/src/font/sprite/draw.rs`)

Reuses the in-module `Fraction`/`fill`, `Canvas`, and test helpers.

- `struct Sextants { tl, tr, ml, mr, bl, br: bool }` with
  `fn from_cp(cp: u32) -> Sextants`:
  `let idx = cp - 0x1FB00; let sex = ((idx + idx / 0x14 + 1) & 0x3F) as u8;`
  then decode bits `0x01..0x20` in the upstream order.
- `fn draw_sextant(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool`:
  returns `false` unless `0x1FB00 <= cp <= 0x1FB3B`; otherwise decodes the
  pattern and `fill`s each set cell with the upstream `Fraction` pairs.

## Scope / faithfulness notes

- **Deferred**: the rest of the Symbols-for-Legacy-Computing file (smooth
  mosaics, separated blocks, etc.) — several of those glyphs use `canvas.line`
  (the `z2d` path API, not yet ported), so only the fill-only sextant subset is
  done here. The `z2d` primitives, the other sprite families, and the unifying
  sprite `has_codepoint`/draw entry point remain deferred.
- `idx/0x14` is `u32` floor division (non-negative); the `& 0x3F` makes the `u6`
  width explicit (the value never exceeds 62).
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/draw.rs`: add `Sextants` (+ `from_cp`) and
   `draw_sextant`; update the module doc to note sextant coverage.
2. Tests (deterministic, the fixture `Metrics` — `cell_width = 9`,
   `cell_height = 18`). The grid cells resolve to columns `x` left `[0,5)` /
   right `[4,9)` and rows `y` top `[0,6)` / middle `[6,12)` / bottom `[12,18)`
   (18 is divisible by 3, so the thirds are clean):
   - `sextant_first` (`0x1FB00`, idx 0 → sex 1 = `tl`): only the top-left cell.
   - `sextant_second` (`0x1FB01`, idx 1 → sex 2 = `tr`): only the top-right
     cell.
   - `sextant_tl_tr` (`0x1FB02`, idx 2 → sex 3 = `tl+tr`): the whole top row.
   - `sextant_index_jump` (`0x1FB13` idx 19 → sex 20 = `ml+bl`; `0x1FB14` idx 20
     → sex 22 = `tr+ml+bl`): proves the `idx/0x14` jump at the boundary (sex
     value `21` is skipped between them).
   - `sextant_last` (`0x1FB3B`, idx 59 → sex 62 = `tr+ml+mr+bl+br`, all but
     `tl`): the top-left cell empty, the rest filled.
   - `draw_sextant_excludes`: `0x1FAFF`, `0x1FB3C`, `'M'` return `false`, draw
     nothing.
   - Cells are checked with a `cells_inked` helper that asserts every pixel
     belongs to exactly the expected set of cell rectangles.
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

- `from_cp` reproduces the `idx + idx/0x14 + 1` pattern formula and the bit→cell
  mapping, and `draw_sextant` fills the right cells, returning `false` outside
  `U+1FB00`–`U+1FB3B`;
- the index-jump test confirms the `idx/0x14` skip at the boundary;
- the rest of the legacy-computing file and the `z2d` primitives stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the pattern formula needs a different integer
shape to match upstream exactly.

The experiment **fails** if the sextant pattern mapping or cell geometry
diverges from upstream or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**. It confirmed the bit order (`tl,tr,ml,mr,bl,br = 0x01..0x20`), that
the `sex = idx + idx/0x14 + 1` formula matches upstream and correctly skips `0`
(blank), `21` (left column), and `42` (right column), that all six `fill` cell
rectangles match upstream, and that the worked examples (`0→1 tl`, `1→2 tr`,
`2→3 tl+tr`, `19→20 ml+bl`, `20→22 tr+ml+bl`, `59→62 all-but-tl`) and the `9×18`
cell geometry (left `x[0,5)`, right `x[4,9)`, top `y[0,6)`, middle `y[6,12)`,
bottom `y[12,18)`) are correct.

Review artifacts:

- Prompt: `logs/codex-review/20260603-010831-620214-prompt.md`
- Result: `logs/codex-review/20260603-010831-620214-last-message.md`
