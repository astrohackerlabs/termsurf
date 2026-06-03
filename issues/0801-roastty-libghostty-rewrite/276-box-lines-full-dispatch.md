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

# Experiment 276: Complete the lines_char dispatch (all 109 line glyphs)

## Description

Experiment 275 ported the `lines_char` primitive and a 13-codepoint
representative dispatch. This experiment completes the dispatch: every codepoint
in upstream's `draw2500_257F` switch that routes through `linesChar` — **109**
glyphs spanning `U+2500`–`U+257F` (all straight lines, every light/heavy corner
and T-junction permutation, the light/heavy crosses, the full double-line block
`U+2550`–`U+256C`, and the half-line stubs `U+2574`–`U+257F`). The primitive is
already proven pixel-exact; this experiment is the faithful transcription of the
dispatch table plus coverage tests. The non-`linesChar` primitives (dashes,
arcs, diagonals) remain deferred.

## Upstream behavior (`font/sprite/draw/box.zig`, `draw2500_257F`)

The switch maps each line codepoint to `linesChar(metrics, canvas, .{ … })` with
a per-direction `Lines`. The 109 `linesChar` codepoints are:

- `0x2500`–`0x2503` straight lines (already dispatched).
- `0x250C`–`0x251B` corners (light/heavy permutations).
- `0x251C`–`0x254B` T-junctions and crosses (light/heavy permutations).
- `0x2550`–`0x256C` double-line lines, corners, T-junctions, crosses (mixed
  light/double).
- `0x2574`–`0x257F` half-line stubs and light/heavy transitions.

The interleaved codepoints that route to **other** primitives stay deferred:
dashes `0x2504`–`0x250B` **and `0x254C`–`0x254F`** (the double/triple dashes),
and `0x256D`–`0x2573` (rounded corners and diagonals, which call
`arc`/`lightDiagonal*`, not `linesChar`).

## Rust mapping (`roastty/src/font/sprite/draw.rs`)

To make the 109-entry transcription **testable for exactness** (not just
non-empty), the dispatch is an audited `cp -> Lines` table rather than an opaque
`match`:

- `const BOX_LINES: &[(u32, Lines)]` — the audited table, one entry per upstream
  `linesChar` codepoint, each pairing the codepoint with the exact `Lines`
  (field-for-field from upstream).
- `fn box_lines_styles(cp: u32) -> Option<Lines>`: linear lookup in `BOX_LINES`
  (the table is small and built once; faithfulness, not speed, is the goal).
- `draw_box_lines` becomes:
  `match box_lines_styles(cp) { Some(l) => { lines_char(metrics, canvas, l); true } None => false }`.
- No new types or primitive changes — `lines_char`, `Lines`, `LineStyle`, and
  `Thickness` are unchanged from Experiment 275.

## Scope / faithfulness notes

- **Deferred**: the dashes (`0x2504`–`0x250B`, `0x254C`–`0x254F`), the rounded
  corners/diagonals (`0x256D`–`0x2573`), the rest of `draw2500_257F`
  (blocks/shades, a different switch range), the sprite `hasCodepoint`
  inventory, and the other sprite categories. None use `linesChar`.
- The `(cp, Lines)` table is an equally faithful, more testable representation
  of the upstream switch's `linesChar` arms.
- Pure dispatch expansion; no C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/draw.rs`: add the `BOX_LINES` table (all 109
   `linesChar` codepoints → exact `Lines`), `box_lines_styles`, and rewrite
   `draw_box_lines` to look up the table.
2. Tests (deterministic, the Experiment 275 fixture `Metrics`):
   - `table_codepoint_set`: the **exact** set guard — `BOX_LINES`'s codepoints,
     collected and sorted, equal an independently-written `EXPECTED_CPS`
     (`[0x2500..=0x2503, 0x250C..=0x254B, 0x2550..=0x256C, 0x2574..=0x257F]`
     expanded), and the table has no duplicate codepoints. This catches a
     missing, extra, or wrong codepoint. Length is 4 + 64 + 29 + 12 = **109**.
   - `table_exact_mappings`: exact `Lines` equality for a representative from
     **every block**, transcribed independently from upstream — e.g.
     `0x2501 → {left:Heavy,right:Heavy}`, `0x250D → {down:Light,right:Heavy}`,
     `0x251C → {up:Light,down:Light,right:Light}`,
     `0x2540 → {up:Heavy,down:Light,left:Light,right:Light}`,
     `0x254B → all Heavy`, `0x2552 → {down:Light,right:Double}`,
     `0x256B → {up:Double,down:Double,left:Light,right:Light}`,
     `0x257C → {left:Light,right:Heavy}`, `0x257F → {up:Heavy,down:Light}`. This
     catches wrong-direction / wrong-style transcription in the sampled entries.
   - `dispatch_covers_all_line_chars`: a sweep over `EXPECTED_CPS` — each
     `draw_box_lines(cp, …)` returns `true` **and** leaves ≥1 inked pixel (no
     silently-empty arm).
   - `dispatch_excludes_non_line_chars`: the deferred codepoints
     (`0x2504`–`0x250B`, `0x254C`–`0x254F`, `0x256D`–`0x2573`, and `'M'`) all
     return `false` and draw nothing.
   - `tee_right_light` (`0x251C ├`): the vertical band spans the full height and
     the right-half horizontal band is present, while the **left** half center
     row is empty (no left stub).
   - `tee_down_light` (`0x252C ┬`): the horizontal band spans the full width and
     the down-half vertical band is present, while the **up** half center column
     is empty.
   - `stub_left_light` (`0x2574 ╴`): only the left half of the center row is
     inked (`x` in `[0, v_light_right)`), the right half empty.
   - `stub_up_light` (`0x2575 ╵`): only the top half of the center column is
     inked, the bottom half empty.
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

- `draw_box_lines` dispatches all 109 `linesChar` codepoints to the correct
  `Lines`, each drawing non-empty ink, and returns `false` for the deferred
  primitives;
- the targeted T-junction and stub geometry checks confirm the half-line
  directions land on the correct side;
- the dashes/arcs/diagonals and the other sprite categories stay cleanly
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if some codepoints need a primitive other than
`lines_char` (i.e. a mis-scoped entry) and must be deferred.

The experiment **fails** if any dispatched `Lines` diverges from upstream or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and raised two **Medium**
findings: (1) the deferred non-`linesChar` set omitted the dashes
`0x254C`–`0x254F`; (2) the original non-empty sweep would not catch
wrong-direction/wrong-style transcription errors across 109 hand-written `Lines`
structs. Both fixed: `0x254C`–`0x254F` added to the deferred set and the
exclusion test, and the dispatch restructured around an audited
`const BOX_LINES: &[(u32, Lines)]` table with a `table_codepoint_set` exact-set
guard (vs. an independently-written `EXPECTED_CPS`, no duplicates, count 109)
and a `table_exact_mappings` test checking exact `Lines` for an independently
transcribed representative from every block. Codex confirmed both findings are
resolved, that the four `EXPECTED_CPS` ranges (`0x2500..=0x2503`,
`0x250C..=0x254B`, `0x2550..=0x256C`, `0x2574..=0x257F` = 109) are exactly the
upstream `linesChar` set with no non-`linesChar` codepoint inside and none
outside, and that the test plan is sound — with no remaining required changes.

Review artifacts:

- Prompt: `logs/codex-review/20260602-232741-784124-prompt.md`
- Result: `logs/codex-review/20260602-232741-784124-last-message.md`
- Follow-up: `logs/codex-review/20260602-232939-616874-last-message.md`

## Result

**Result:** Pass

`roastty/src/font/sprite/draw.rs` now carries the audited `BOX_LINES` table —
all **109** `linesChar` codepoints paired with their exact `Lines`, written via
a `const fn lines(up, right, down, left)` constructor and `N`/`L`/`H`/`D` style
aliases for a compact, auditable layout (`#[rustfmt::skip]`). `box_lines_styles`
does the table lookup; `draw_box_lines` dispatches through it. `lines_char`,
`Lines`, `LineStyle`, and `Thickness` are unchanged from Experiment 275.

Tests (deterministic, the Experiment 275 fixture `Metrics`):

- `table_codepoint_set` — the table's codepoints (no duplicates, count 109)
  sorted equal the independently-written `expected_cps()` (the four contiguous
  ranges).
- `table_exact_mappings` — exact `Lines` equality for an independently
  transcribed representative from every block (`0x2501`, `0x250D`, `0x251C`,
  `0x2540`, `0x254B`, `0x2552`, `0x256B`, `0x257C`, `0x257F`).
- `dispatch_covers_all_line_chars` — every one of the 109 returns `true` and
  draws ≥1 inked pixel.
- `dispatch_excludes_non_line_chars` — the dashes (`0x2504`–`0x250B`,
  `0x254C`–`0x254F`), arcs/diagonals (`0x256D`–`0x2573`), and `'M'` return
  `false` and draw nothing.
- `tee_right_light` / `tee_down_light` — the `├`/`┬` arms land on the correct
  sides (full band + the third arm present, the opposite stub absent).
- `stub_left_light` / `stub_up_light` — the `╴`/`╵` half-stubs ink only their
  half of the center row/column.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty sprite` → 30 passed (8 new).
- `cargo test -p roastty` → 2456 passed, 0 failed (no regressions; +8).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The full box-drawing line-glyph dispatch is complete and exactness-verified: all
109 `linesChar` codepoints map to the upstream `Lines`, guarded by an exact
codepoint-set check and independent representative mappings. The sprite
subsystem can now draw every straight line, corner, T-junction, cross,
double-line, and half-stub box glyph. The next sprite work is the remaining
box-drawing primitives the dispatch defers to — dashes
(`dashHorizontal`/`dashVertical`), rounded corners and diagonals
(`arc`/`lightDiagonal*`) — and then the other sprite categories (block, braille,
powerline, legacy). Together those complete the `draw2500_257F`+ inventory that
the sprite `hasCodepoint` is derived from, which in turn unblocks the resolver's
deferred sprite render arm (`SpriteUnavailable`). Alongside the sprite font
remain the discovery consumer, the UCD emoji-presentation default, codepoint
overrides, the shaper, the Nerd Font attribute table, and SVG color detection.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**. It compared the full `BOX_LINES` table arm-by-arm against upstream
for all four ranges (`0x2500..=0x2503`, `0x250C..=0x254B`, `0x2550..=0x256C`,
`0x2574..=0x257F`) and confirmed every positional `lines(up, right, down, left)`
entry matches the upstream `Lines` fields exactly — no wrong-direction,
wrong-style, swapped-field, or missing-style mismatches. It verified the
deferred codepoints (`0x2504`–`0x250B`, `0x254C`–`0x254F`, `0x256D`–`0x2573`)
are absent, the count is 109 with no duplicates, and the codepoint-set,
exact-mapping, sweep, exclusion, and tee/stub geometry tests are appropriate
coverage. With the clean test/build/fmt/name/diff gates, Codex judged the result
sound.

Review artifacts:

- Prompt: `logs/codex-review/20260602-233325-947354-prompt.md`
- Result: `logs/codex-review/20260602-233325-947354-last-message.md`
