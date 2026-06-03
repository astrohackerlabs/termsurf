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

# Experiment 357: the run iterator's spacer skip and selection/cursor breaks

## Description

Experiment 356 implemented `RunIterator.next()`'s common path. This experiment
adds the three pieces it deferred — the **spacer skip** (wide-cell padding), the
**selection break**, and the **cursor break** — completing the full faithful
`next()`. With these, `next()` handles wide characters, selection boundaries,
and the cursor split, and the `debug_assert!`s that pinned the common-path scope
are removed.

## Upstream behavior (`shaper/run.zig` `next()`)

```zig
// Selection break (before the spacer skip), only past the run start:
if (self.opts.selection) |bounds| {
    if (j > self.i) {
        if (bounds[0] > 0 and j == bounds[0]) break;
        if (bounds[1] > 0 and j == bounds[1] + 1) break;
    }
}
// Spacer skip: a spacer pads a wide cell / wraps a line; it has no glyph.
switch (cell.wide) { .narrow, .wide => {}, .spacer_head, .spacer_tail => continue }

// …style break, font_style, presentation…

// Cursor break (only for non-grapheme cells): break the run around the cursor.
if (!cell.hasGrapheme()) {
    if (self.opts.cursor_x) |cursor_x| {
        if (self.i == cursor_x and j == self.i + 1) break;   // exactly the cursor
        if (self.i < cursor_x and j == cursor_x) break;      // up to the cursor
        // after the cursor: let the run complete
    }
}
```

The **selection** break splits a run at the selection's start column and just
past its end (so selected text shapes separately). The **spacer** skip drops the
padding cells of wide characters from the run (they carry no glyph but still
advance the cluster index). The **cursor** break isolates the cursor cell: a row
with a cursor has up-to-three runs (before, exactly, after), so a cursor over an
emoji stays intact while joiners around it can re-break.

## Rust mapping (`roastty/src/font/run.rs`)

Insert into `RunIterator::next`'s loop (`start` is `self.i` at the run start):

```rust
// (after computing `cluster`, before the style break)

// Selection break. Compare the loop index (`usize`) to the widened `u16`
// bounds — never narrowing the index.
if let Some(bounds) = self.opts.selection {
    if j > start {
        if bounds[0] > 0 && j == usize::from(bounds[0]) {
            break;
        }
        if bounds[1] > 0 && j == usize::from(bounds[1]) + 1 {
            break;
        }
    }
}

// Spacer skip: padding cells carry no glyph (but still advance the index).
if matches!(cell.wide, Wide::SpacerHead | Wide::SpacerTail) {
    j += 1;
    continue;
}

// …the existing style break, font_style, presentation…

// Cursor break (non-grapheme cells only).
if !cell.has_grapheme() {
    if let Some(cursor_x) = self.opts.cursor_x {
        let cursor = usize::from(cursor_x);
        if start == cursor && j == start + 1 {
            break;
        }
        if start < cursor && j == cursor {
            break;
        }
    }
}
```

To make the spacer-first edge faithful, `current_font` is no longer an
`Option<Index>` (Experiment 356) but an `Index` initialized to the **default**
(`Index::new(Style::Regular, 0)`, matching upstream's `Collection.Index = .{}`),
overwritten at `j == start`:

```rust
let mut current_font = Index::new(Style::Regular, 0);  // upstream's `.{}`
// …in the loop, after resolving (idx, fallback):
if j == start {
    current_font = idx;
}
if idx != current_font {
    break;
}
// …at emit: `font_index: current_font` (no `unwrap`).
```

This reproduces upstream exactly: a run that begins on a skipped spacer leaves
`current_font` at the default, and the following cell is kept in the **same**
run iff it resolves to that default font (otherwise the run breaks before it) —
rather than always breaking. The common-path `debug_assert!`s (no
selection/cursor; every cell narrow/wide) are removed, since all three are now
handled. The stored `offset`/`cells`/`cluster` keep their checked `try_from`
conversions.

## Scope / faithfulness notes

- **Ported**: the selection break (`bounds[0]`/`bounds[1] + 1` at `j > start`),
  the spacer skip (`SpacerHead`/`SpacerTail` → skip), and the cursor break
  (exactly/before the cursor, non-grapheme only) — completing `next()`.
- **Faithful**: the break order (selection → spacer → style → … → cursor)
  matches upstream; spacers advance the index but emit no codepoint (the cluster
  gap is preserved); the cursor break only applies to non-grapheme cells;
  `current_font` is the default `Index` until set at `j == start` (upstream's
  `.{}`), so a leading spacer followed by a default-font cell stays in one run;
  the selection/cursor comparisons widen the bounds to `usize` (never narrowing
  the loop index).
- **Deferred** (unchanged): the renderer code that builds `RunCell`s from
  terminal cells and supplies the real `selection`/`cursor_x`. (Consumed by
  tests now.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/font/run.rs`: add the selection break, the spacer skip, and the
   cursor break to `RunIterator::next`; change `current_font` from
   `Option<Index>` to an `Index` initialized to the default (set at
   `j == start`); remove the common-path `debug_assert!`s.
2. Tests (in `run.rs`):
   - `next_skips_spacer`: a row `['W'(Wide), spacer(SpacerTail), 'A'(Narrow)]`
     yields one 3-cell run whose codepoints are `[(W, 0), (A, 2)]` — the spacer
     at index 1 is skipped but its cluster gap remains.
   - `next_breaks_on_selection`: `"ABCD"` with `selection = [1, 2]` yields runs
     `[A]`, `[B, C]`, `[D]` (breaks at `j == bounds[0]` and
     `j == bounds[1] + 1`).
   - `next_breaks_on_cursor_exact`: `"AB"` with `cursor_x = 0` yields `[A]` then
     `[B]` (the cursor cell is its own run).
   - `next_breaks_on_cursor_before`: `"AB"` with `cursor_x = 1` yields `[A]`
     then `[B]` (the run breaks reaching the cursor).
   - `next_leading_spacer_default_font`: a row
     `[spacer(SpacerTail), 'A'(Narrow)]` (both resolving to the default Menlo
     regular face) yields **one** run whose codepoints are `[(A, 1)]` — the
     leading spacer is skipped but does not break the run (the following
     default-font cell joins it), proving the default-index `current_font`
     behavior.
   - The existing `next_*` tests (no selection/cursor, narrow cells) still pass.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty next_
cargo test -p roastty run
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `next()` applies the selection break, the spacer skip, and the cursor break in
  upstream's order, faithful to `next()`; spacers advance the index without
  emitting; the cursor break is non-grapheme-only; the spacer-only edge emits
  the default index;
- the spacer, selection, and cursor tests pass, and the existing `next_*`/`run`
  tests still pass;
- the renderer's `RunCell` extraction stays deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if any break diverges from upstream (wrong boundary,
wrong order, spacer emitting a codepoint, cursor break on a grapheme), or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **two Required
findings**, both now fixed:

- **Required (fixed):** the draft cast `j`/`start` to `u16` for the
  selection/cursor comparisons (`j as u16`, `bounds[1] + 1`), which can wrap or
  truncate at boundary values. Changed to widen the bounds to `usize`
  (`j == usize::from(bounds[0])`, `j == usize::from(bounds[1]) + 1`,
  `cursor = usize::from(cursor_x)`) — the loop index is never narrowed.
- **Required (fixed):** the spacer-first edge used `Option<Index>`
  (`Some(idx) != None` breaks unconditionally at the first non-spacer),
  diverging from upstream, which initializes `current_font` to the default
  `Collection.Index = .{}` and so **keeps** a leading-spacer run's following
  cell when it resolves to the default font. Changed `current_font` to an
  `Index` initialized to `Index::new(Style::Regular, 0)` (the default), set at
  `j == start`, compared directly — reproducing upstream exactly. A
  `next_leading_spacer_default_font` test (a `SpacerTail` then a default-font
  `'A'` → one run `[(A, 1)]`) was added.

Codex confirmed the rest is correct: the break order (selection before spacer,
spacer before the style break, cursor after presentation and before resolution,
cursor only for non-grapheme cells); the `[1, 2] → [A], [B,C], [D]` selection
trace; and the spacer cluster-gap preservation.

Review artifacts:

- Prompt: `logs/codex-review/20260603-163751-955802-prompt.md` (design)
- Result: `logs/codex-review/20260603-163751-955802-last-message.md` (design)

## Result

**Result:** Pass

`RunIterator.next()` is now a complete faithful port.

- `roastty/src/font/run.rs`: added the **selection break** (`bounds[0]` /
  `bounds[1] + 1` at `j > start`, widened `usize` comparisons), the **spacer
  skip** (`SpacerHead`/`SpacerTail` → advance the index, emit nothing), and the
  **cursor break** (exactly/before the cursor, non-grapheme only, after
  presentation); changed `current_font` to an `Index` initialized to
  `Index::new(Style::Regular, 0)` (upstream's `.{}`), set at `j == start`; and —
  per the completion review — added the **kitty placeholder** handling
  (`resolve_font` resolves a placeholder as space; accumulation emits a space
  and continues). Removed the common-path `debug_assert!`s.

Tests: `next_skips_spacer` (`[W, spacer, A]` → one 3-cell run, `[(W,0),(A,2)]`),
`next_breaks_on_selection` (`"ABCD"` sel `[1,2]` → `[A]`/`[B,C]`/`[D]`),
`next_breaks_on_cursor_exact` / `next_breaks_on_cursor_before` (`"AB"` → two
runs), `next_leading_spacer_default_font` (`[spacer, A]` → one run `[(A,1)]`),
`next_placeholder_is_space` (`[A, placeholder, B]` → `[(A,0),(space,1),(B,2)]`).
All pass; the existing `next_*`/`run` tests unchanged.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2803 passed, 0 failed (+6, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

`RunIterator.next()` is **complete** relative to upstream over the modeled
`RunCell` input — every break (selection, spacer, style, ligature, cursor,
font), the fallback chain, the kitty placeholder, the default-`current_font`
behavior, the trailing-empty trim, the invisible skip, the codepoint
accumulation, and the run hash. The libghostty **shaping subsystem is
functionally complete**: a row of `RunCell`s shapes to `TextRun`s + glyph
streams end to end.

The remaining work is the renderer-side integration: building
`RunCell`s/`RunOptions` from roastty's `terminal/page.rs` cells (extracting each
cell's codepoint / graphemes / style / wide-kind across the `pub(super)`
boundary) so the pipeline runs live against real terminal rows, and wiring the
resulting `TextRun`s + glyphs into the renderer's draw path.

## Completion Review

Codex reviewed the completed implementation and result. The **first** result
review found **one Required finding** — the kitty placeholder branch was missing
(upstream treats the placeholder as space in both resolution and accumulation),
so `next()` was not yet a complete port. **Fixed:** imported
`crate::terminal::kitty::graphics_unicode::PLACEHOLDER`; `resolve_font` maps a
placeholder to `0` so it resolves through the space path; and the accumulation
emits a space and `continue`s (after the fallback branch, before the primary
push) — with a `next_placeholder_is_space` test added.

The **re-review** found **no Required findings**: it confirmed both placeholder
branches match upstream (resolve as space; shape as blank), and that the
selection, spacer, cursor, default-`current_font`, and common-path behavior
remain faithful — `RunIterator::next` is now complete for the modeled `RunCell`
input. It ran `cargo test -p roastty next_` (all 32 matching tests passed,
including `next_placeholder_is_space`).

Review artifacts:

- Result review (placeholder finding):
  `logs/codex-review/20260603-164245-555750-last-message.md`
- Completion re-review:
  `logs/codex-review/20260603-164613-879148-last-message.md`
