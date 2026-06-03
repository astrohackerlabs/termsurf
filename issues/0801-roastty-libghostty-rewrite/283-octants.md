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

# Experiment 283: Octants (U+1CD00–U+1CDE5)

## Description

The octant glyphs from Symbols for Legacy Computing Supplement — 230 glyphs,
each a subset of a 2-column × 4-row (quarter-height) cell grid. Unlike the
sextants, the octant codepoint→pattern mapping has no formula; upstream embeds a
data file (`octants.txt`) and parses it at comptime into a lookup table. The
drawing itself is pure `fill`. This experiment vendors the data, parses it into
a `const` lookup table, and dispatches the octants.

## Upstream behavior (`draw1CD00_1CDE5`, `octants.txt`)

- `Octant` (`packed struct(u8)`): cell flags `1..8` in bit order (cell `1` is
  bit 0 … cell `8` is bit 7).
- `octants.txt`: a comment header (`#` lines) then 230 lines
  `BLOCK OCTANT-<digits>`, where the digits after the `-` are the set cells. The
  file is `@embedFile`'d and parsed at comptime: each non-comment line, in
  order, fills `octants[i]` (`i = 0..229`) by setting bit `(digit-1)` for each
  digit after the `-`. The Nth line maps to codepoint `0x1CD00 + N`.
- `draw1CD00_1CDE5(cp, …)`: `oct = octants[cp - 0x1CD00]`; fills each set cell —
  cell `1→(zero,half, zero,one_quarter)`, `2→(half,full, zero,one_quarter)`,
  `3→(zero,half, one_quarter,two_quarters)`,
  `4→(half,full, one_quarter,two_quarters)`,
  `5→(zero,half, two_quarters,three_quarters)`,
  `6→(half,full, two_quarters,three_quarters)`,
  `7→(zero,half, three_quarters,end)`, `8→(half,full, three_quarters,end)`.

## Rust mapping (`roastty/src/font/sprite/draw.rs`)

- Vendor the data: copy upstream `octants.txt` to
  `roastty/src/font/sprite/octants.txt` (it is free of `ghostty` references, so
  the no-name gate stays clean).
- `const OCTANTS: [u8; 230] = parse_octants(include_str!("octants.txt"));` where
  `const fn parse_octants(data: &str) -> [u8; 230]` walks the bytes line by line
  (trimming a trailing `\r`, skipping `#`/blank lines), and for each data line
  sets `oct |= 1 << (digit - b'1')` for each digit after the `-`. It
  `assert!(i == 230)` at the end (a compile-time check mirroring upstream's
  comptime `assert`). This reproduces the comptime table generation as a Rust
  `const fn`. (If a `const fn` limitation bites, the fallback is a
  `OnceLock`-cached runtime parse of the same embedded string; the design keeps
  the same table and dispatch either way.)
- `fn draw_octant(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool`:
  returns `false` unless `0x1CD00 <= cp <= 0x1CDE5`; otherwise looks up
  `OCTANTS[(cp - 0x1CD00) as usize]` and `fill`s each set cell with the upstream
  `Fraction` pairs.

## Scope / faithfulness notes

- **Deferred**: the circle/ellipse pieces (`canvas.line`, `z2d`), the rest of
  the supplement, the geometric shapes, and the `z2d` path port itself; the
  other sprite families and the unifying sprite `has_codepoint`/draw entry
  point.
- The `const fn` parser is the faithful equivalent of upstream's comptime
  `@embedFile` + parse; the embedded `octants.txt` is the single source of truth
  (vendored data, not a re-derived table).
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/octants.txt` (new): vendored copy of the upstream
   data file.
2. `roastty/src/font/sprite/draw.rs`: add `parse_octants` (`const fn`), the
   `OCTANTS` table, and `draw_octant`; update the module doc to note octant
   coverage.
3. Tests. Octant rows are quarter-height, so the tests use an `8×16` fixture
   (`fixture_8x16`) where the halves/quarters divide cleanly — columns left
   `[0,4)` / right `[4,8)`, rows `[0,4)`, `[4,8)`, `[8,12)`, `[12,16)` — giving
   the eight cell rects `1..8`:
   - `octant_table_first_entries`: `OCTANTS[0] == 0b0000_0100` (cell 3),
     `OCTANTS[1] == 0b0000_0110` (cells 2,3), `OCTANTS[15] == 0b0001_0111`
     (cells 1,2,3,5), `OCTANTS[229] == 0b1111_1110` (cells 2–8) — validates the
     parser directly against known `octants.txt` lines.
   - `octant_first` (`0x1CD00`): only cell 3.
   - `octant_second` (`0x1CD01`): cells 2 and 3.
   - `octant_multi` (`0x1CD0F`): cells 1,2,3,5.
   - `octant_last` (`0x1CDE5`): all cells but cell 1 (cell 1 rect empty, rest
     filled — exercises cell 8).
   - `draw_octant_excludes`: `0x1CCFF`, `0x1CDE6`, `'M'` return `false`, draw
     nothing.
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

- `parse_octants` builds the 230-entry table from the vendored `octants.txt`
  with the correct bit mapping, `draw_octant` fills the right cells, and it
  returns `false` outside `U+1CD00`–`U+1CDE5`;
- the table-entry and per-glyph tests confirm the parse and the cell geometry;
- the circle pieces, `z2d` primitives, and other families stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates (including the new `octants.txt`) and
  `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if a `const fn` limitation forces the
`OnceLock`-cached runtime parse fallback (same table/behavior).

The experiment **fails** if the parsed table or the cell geometry diverges from
upstream or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**. It confirmed the bit mapping (digit `d` → bit `d-1`, cells `1..8` →
bits `0..7`), that all eight `fill` fraction pairs match upstream, that
non-comment line `N` maps to `0x1CD00 + N`, that the `const fn` parse design is
sound (trim `\r`, skip `#`/empty, parse digits after `-`, `assert!(i == 230)` a
valid compile-time size check), the verified `octants.txt` entries
(`OCTANTS[0]=0x04`, `[1]=0x06`, `[15]=0x17`, `[229]=0xFE`), and that the `8×16`
fixture divides halves/quarters exactly.

Review artifacts:

- Prompt: `logs/codex-review/20260603-012023-931694-prompt.md`
- Result: `logs/codex-review/20260603-012023-931694-last-message.md`
