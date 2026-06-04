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

# Experiment 403: the under-preedit foreground skip

## Description

The cells **under** the IME preedit are not drawn by the normal cell loop — the
preedit draws its own cells over them (Experiments 400–401). Upstream skips a
cell when it falls within the preedit range (`continue`), so neither its
background nor its foreground is drawn. This experiment ports the **foreground**
half: `rebuild_row` takes the row's preedit column range and skips the
foreground (decorations + glyph) for cells inside it, advancing the glyph cursor
as for a concealed cell (Experiment 402). The **background** half (transparent
under the preedit) is a follow-up. The preedit range is a per-row input,
threaded through `rebuild_viewport`; computing it from the cursor (and the
`rebuild_viewport` cursor/preedit assembly) is deferred.

## Upstream behavior

In `rebuildCells` (`renderer/generic.zig`), early in the per-cell loop (before
the background and foreground are written):

```zig
// If this cell falls within our preedit range then we skip this because
// preedits are setup separately.
if (preedit_range) |range| {
    if (range.y != y) break :preedit;        // not the preedit row
    if (x < range.x[0]) break :preedit;       // before the preedit
    if (x <= range.x[1]) continue;            // in the preedit range — skip the cell
    // (after the range: catch the shaper run cursor up for the missed cells)
}
```

So a cell on the preedit row with `range.x[0] <= x <= range.x[1]` is skipped
entirely. The `continue` precedes both the background and the foreground, and
the "catch up" branch advances the shaper cursor past the skipped cells' glyphs.
This experiment ports the foreground skip + cursor catch-up; the background skip
is separate (Experiment 404).

## Rust mapping (`roastty/src/renderer/cell.rs`)

`rebuild_row` gains a `preedit_range: Option<[u16; 2]>` (the row's inclusive
preedit columns, or `None`). In the column loop, a cell is foreground-skipped
when **concealed** (Experiment 402) **or** under the preedit; the glyph cursor
advances regardless (as for a concealed cell):

```rust
let conceal = flags.invisible;
// A cell under the preedit is foreground-skipped (the preedit draws its own
// cells over it). The range uses the raw column (no `x_compare`), like links.
let under_preedit = preedit_range
    .is_some_and(|[start, end]| grid_pos[0] >= start && grid_pos[0] <= end);
let skip_fg = conceal || under_preedit;

// underline (+ link override) + overline — `if !skip_fg`
// glyph step — always advance the cursor; `add_glyph` guarded by `if !skip_fg`
// strikethrough — `if !skip_fg && flags.strikethrough`
```

`rebuild_viewport` gains a `preedit_skip: Option<PreeditSkip>` parameter
(`PreeditSkip { row, range }`); per row it passes the range to `rebuild_row`
only when the row matches
(`preedit_skip.filter(|p| p.row == y).map(|p| p.range)`), else `None`.
`rebuild_bg_row` is unchanged this experiment (the background skip is Experiment
404).

## Scope / faithfulness notes

- **Ported (bridged)**: the under-preedit **foreground** skip — a cell within
  the row's preedit range draws no foreground (the preedit draws over it),
  reusing the concealed-skip mechanism (Experiment 402).
- **Faithful**: the skip is the raw-column inclusive range test (upstream's
  `range.x[0] <= x <= range.x[1]` on `range.y`, raw `x`, like links); a skipped
  cell draws no underline (link override included), overline, glyph, or
  strikethrough, and the glyph cursor advances to consume the skipped cells'
  shaped glyphs (upstream's "catch up") — so later cells stay aligned. A row
  with no preedit (`None`) is unchanged.
- **Faithful adaptation**: the skip is folded into the existing `skip_fg`
  (concealed ∨ under-preedit); the per-row range is a parameter (upstream's
  `preedit_range`), with a small `PreeditSkip { row, range }` carrying the row
  and the inclusive columns through `rebuild_viewport`. The cursor advances
  explicitly (roastty's shaper shapes the under-preedit cells), matching
  upstream's catch-up.
- **Deferred**: the under-preedit **background** skip (transparent under the
  preedit, in `rebuild_bg_row`) — Experiment 404; computing the preedit range
  from the cursor viewport and the `rebuild_viewport` cursor/preedit assembly;
  the Metal upload. (Consumed by tests now.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/cell.rs`:
   - add a `PreeditSkip { row: u16, range: [u16; 2] }` struct;
   - `rebuild_row`: add a `preedit_range: Option<[u16; 2]>` param (last);
     compute `under_preedit` and fold it into
     `skip_fg = conceal || under_preedit`. Update its doc comment.
   - `rebuild_viewport`: add a `preedit_skip: Option<PreeditSkip>` param (last);
     pass each row's range to `rebuild_row` (the row that matches, else `None`).
   - Update the existing `rebuild_row`/`rebuild_viewport` test call sites
     (`None`).
2. Tests (in `cell.rs`):
   - `rebuild_row` with `preedit_range = Some([0, 0])` over a 2-cell row (cell 0
     has a glyph + decorations; cell 1 is a plain visible cell with a glyph) →
     cell 0 draws **no** foreground, and cell 1's glyph is emitted at column 1
     (the cursor advanced past the skipped glyph) — mirroring the concealed-skip
     test;
   - a row with `preedit_range = None` draws normally (existing tests);
   - the **raw-column** check (no spacer-tail adjustment): a `SpacerTail` cell
     at **column 1** with `preedit_range = Some([0, 0])` is **not** skipped (raw
     column 1 ∉ `[0, 0]`) — an incorrect `x_compare` (column 0) would wrongly
     skip it, protecting the upstream distinction (raw column, like links).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty rebuild_row
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `rebuild_row` skips the foreground of cells in the row's preedit range (raw
  column, inclusive) while advancing the glyph cursor, and `rebuild_viewport`
  threads the per-row range — faithful to upstream's under-preedit `continue`
  (foreground half);
- the tests pass (an under-preedit cell draws no foreground; a later visible
  cell's glyph lands at the right column), and the existing tests still pass
  (updated for the new signatures, passing `None`);
- the background skip, the preedit-range origin, and the Metal upload stay
  deferred; `rebuild_bg_row` is unchanged;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if an under-preedit cell still draws foreground, the
glyph cursor misaligns, a non-preedit cell changes, or any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with one
**Low** finding (no Required), now addressed:

- **Low (addressed):** the test plan should include an explicit **raw-column**
  case (like Experiment 399's link test) — a normal narrow-cell range would not
  catch an accidental `x_compare` adjustment. The test list now includes a
  `SpacerTail` at column 1 with `preedit_range = Some([0, 0])`: the raw-column
  logic must **not** skip column 1, while an incorrect spacer-tail backstep
  would, protecting the upstream distinction.

Codex confirmed the rest is sound: `skip_fg = conceal || under_preedit` (the
foreground-only skip with glyph-cursor advancement), `rebuild_bg_row` deferred
unchanged for Experiment 404, and `PreeditSkip { row, range }` is the right
threading shape.

Review artifacts:

- Prompt: `logs/codex-review/20260604-063756-319821-prompt.md` (design)
- Result: `logs/codex-review/20260604-063756-319821-last-message.md` (design)
