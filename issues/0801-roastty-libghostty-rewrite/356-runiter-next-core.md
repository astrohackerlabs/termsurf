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

# Experiment 356: the run iterator's core grouping loop

## Description

This experiment ports the heart of `RunIterator.next()` — the cell-walking loop
that groups a terminal row's cells into runs by font and style, accumulates each
run's `(codepoint, cluster)` stream, and emits a `TextRun`. It builds entirely
on the already-ported `font/run.rs` primitives (`RunOptions`/`RunCell`, the
per-cell helpers, `run_hash`, `TextRun`) and the `CodepointResolver`. The scope
is the **common path**: rows of narrow cells with no selection and no cursor.
The spacer skip, the selection break, and the cursor break (all driven by inputs
absent on the common path) are deferred to Experiment 357.

## Upstream behavior (`shaper/run.zig` `next()`)

```zig
// Trim trailing empty cells → `max`.
const max = …last non-empty index + 1…;
// Skip leading invisible cells.
while (self.i < max and cells[self.i].hasStyling() and styles[self.i].flags.invisible) self.i += 1;
if (self.i >= max) return null;

var hasher = Hasher.init(0);
const style = if (cells[self.i].hasStyling()) styles[self.i] else .{};
var current_font: Collection.Index = .{};

var j = self.i;
while (j < max) : (j += 1) {
    const cluster = j - self.i;                       // run-relative
    const cell = &cells[j];
    // …selection break (Exp 357)…
    // …spacer skip (Exp 357)…
    if (j > self.i) {
        const prev = cells[j - 1];
        // bad-ligature break (fl/fi/st)
        if (prev.content_tag == .codepoint and cell.content_tag == .codepoint) { …break… }
        if (prev.style_id == cell.style_id) break :style;        // fast path
        if (!comparableStyle(style).eql(comparableStyle(styles[j]))) break;
    }
    const font_style = …from style.flags…;                       // bold/italic
    const presentation = if (cell.hasGrapheme()) …graphemes[j][0]… else null;
    // …cursor break (Exp 357)…
    const font_info = indexForCell(cell, graphemes[j], font_style, presentation)
        orelse getIndex(0xFFFD, …) orelse getIndex(' ', …);      // fallback chain
    if (j == self.i) current_font = font_info.idx;
    if (font_info.idx != current_font) break;                    // font break
    if (font_info.fallback) |cp| { addCodepoint(cp, cluster); continue; }
    addCodepoint(if (cell.codepoint() == 0) ' ' else cell.codepoint(), cluster);
    if (cell.hasGrapheme()) for (graphemes[j]) |cp| {
        if (cp == 0xFE0E or cp == 0xFE0F) continue;               // skip VS
        addCodepoint(cp, cluster);
    };
}
autoHash(&hasher, j - self.i); autoHash(&hasher, current_font);
defer self.i = j;
return .{ .hash = hasher.final(), .offset = self.i, .cells = j - self.i,
          .font_index = current_font, … };
```

A run starts at the first non-empty/non-invisible cell, accumulates cells while
they resolve to the **same font index** and a **comparable style** (background
differences allowed; same `style_id` is a fast-path equal), and stops at a font
change, a style change, or a bad ligature. Each kept cell contributes its
primary codepoint (`0` → space) and its non-`VS` grapheme components, all under
the same run-relative cluster.

## Rust mapping (`roastty/src/font/run.rs`)

A `RunIterator` over a `RunOptions`, advancing through the row:

```rust
/// One run's shaped input: the [`TextRun`] descriptor plus the accumulated
/// `(codepoint, cluster)` stream to hand to [`Face::shape_run`].
pub(crate) struct RunOutput {
    pub run: TextRun,
    pub codepoints: Vec<Codepoint>,
}

pub(crate) struct RunIterator<'a> {
    opts: &'a RunOptions,
    resolver: &'a mut CodepointResolver,
    i: usize,
    max: usize,                 // trailing-empty trim, computed once
}

impl<'a> RunIterator<'a> {
    pub(crate) fn new(opts: &'a RunOptions, resolver: &'a mut CodepointResolver) -> Self {
        let max = trailing_trim(&opts.cells);
        Self { opts, resolver, i: 0, max }
    }

    pub(crate) fn next(&mut self) -> Option<RunOutput> {
        let cells = &self.opts.cells;
        // Skip leading invisible cells.
        while self.i < self.max && cells[self.i].style.flags.invisible {
            self.i += 1;
        }
        if self.i >= self.max {
            return None;
        }
        // This slice handles the common path only; Exp 357 adds these.
        debug_assert!(self.opts.selection.is_none() && self.opts.cursor_x.is_none());
        let start = self.i;
        let style = cells[start].style;
        let mut codepoints: Vec<Codepoint> = Vec::new();
        let mut current_font: Option<Index> = None;

        let mut j = start;
        while j < self.max {
            let cell = &cells[j];
            // Spacers are out of scope for this slice (Exp 357).
            debug_assert!(matches!(cell.wide, Wide::Narrow | Wide::Wide));
            // A run-relative cluster (column count fits `u16`, so `u32` is safe).
            let cluster = u32::try_from(j - start).expect("a run cluster fits u32");
            // (spacer/selection/cursor breaks: Exp 357)
            if j > start {
                let prev = &cells[j - 1];
                if prev.is_codepoint
                    && cell.is_codepoint
                    && is_bad_ligature_break(prev.codepoint, cell.codepoint)
                {
                    break;
                }
                if prev.style_id != cell.style_id
                    && comparable_style(style) != comparable_style(cell.style)
                {
                    break;
                }
            }
            let fstyle = font_style(style.flags.bold, style.flags.italic);
            let presentation = if cell.has_grapheme() {
                presentation_for_grapheme(cell.graphemes[0])
            } else {
                None
            };
            let (idx, fallback) = self.resolve_font(cell, fstyle, presentation);
            if j == start {
                current_font = Some(idx);
            }
            if Some(idx) != current_font {
                break; // font change → run ends (cell j starts the next run)
            }
            if let Some(cp) = fallback {
                codepoints.push(Codepoint { codepoint: cp, cluster });
                j += 1;
                continue;
            }
            let primary = if cell.codepoint == 0 { ' ' as u32 } else { cell.codepoint };
            codepoints.push(Codepoint { codepoint: primary, cluster });
            for &cp in &cell.graphemes {
                if cp == 0xFE0E || cp == 0xFE0F {
                    continue;
                }
                codepoints.push(Codepoint { codepoint: cp, cluster });
            }
            j += 1;
        }

        let font_index = current_font.expect("a non-empty run resolves a font");
        let cell_count = u16::try_from(j - start).expect("a run's cell count fits u16");
        let offset = u16::try_from(start).expect("a run's column offset fits u16");
        self.i = j;
        Some(RunOutput {
            run: TextRun {
                hash: run_hash(&codepoints, cell_count, font_index),
                offset,
                cells: cell_count,
                font_index,
            },
            codepoints,
        })
    }

    /// Resolve a cell's font index, with upstream's fallback chain: the grapheme's
    /// own font, else U+FFFD, else space. Returns the index and, for a fallback,
    /// the substituted codepoint.
    fn resolve_font(&mut self, cell: &RunCell, fstyle: Style, p: Option<Presentation>)
        -> (Index, Option<u32>)
    {
        if let Some(idx) = self.resolver.index_for_grapheme(cell.codepoint, &cell.graphemes, fstyle, p) {
            return (idx, None);
        }
        if let Some(idx) = self.resolver.get_index(0xFFFD, fstyle, p) {
            return (idx, Some(0xFFFD));
        }
        let idx = self.resolver.get_index(' ' as u32, fstyle, p).expect("a font renders space");
        (idx, Some(' ' as u32))
    }
}

/// The exclusive upper bound after trimming trailing empty cells.
fn trailing_trim(cells: &[RunCell]) -> usize {
    for k in 0..cells.len() {
        let rev = cells.len() - 1 - k;
        if !cells[rev].is_empty {
            return rev + 1;
        }
    }
    0
}
```

## Scope / faithfulness notes

- **Ported**: `next()`'s core — the trailing-empty trim, the leading-invisible
  skip, the font+style+ligature run grouping, the fallback chain (grapheme font
  → `U+FFFD` → space), the codepoint accumulation (primary `0`→space, grapheme
  components minus `VS`), the run hash, and `TextRun` emission. `RunIterator`
  yields each run's `TextRun` plus its `(codepoint, cluster)` stream (roastty's
  `Face::shape_run` consumes the stream directly, where upstream uses a hook).
- **Faithful**: clusters are run-relative (`j - start`); the style break uses
  the `style_id` fast path then `comparable_style`; the bad-ligature guard
  requires both cells be plain codepoints; the font break ends the run at the
  first differing index; `VS` selectors are dropped from the grapheme stream.
- **Faithful simplification (common path)**: this slice handles **narrow cells
  with no selection and no cursor** — exactly the case where upstream's spacer
  skip and selection/cursor breaks do nothing. So `current_font` is always set
  at `j == start` (the first cell is never a skipped spacer). Experiment 357
  adds the spacer skip and the selection/cursor breaks; until then `next()`
  asserts `selection`/`cursor_x` are `None` and treats every cell as narrow.
- **Deferred** (unchanged): the renderer code that builds `RunCell`s from
  terminal cells; the kitty placeholder substitution (its codepoint is otherwise
  carried through). (Consumed by tests now; `#![allow(dead_code)]` covers the
  path.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/font/run.rs`: add `RunOutput`, `RunIterator` (`new`/`next`/
   `resolve_font`), and `trailing_trim`; import `CodepointResolver`.
2. Tests (in `run.rs`, building a Menlo `CodepointResolver`):
   - `next_groups_one_run`: a row of narrow cells `"AB"` (same style) yields one
     run — `offset 0`, `cells 2`, codepoints `[(A,0), (B,1)]`, and a single
     `next()` thereafter returns `None`.
   - `next_trims_trailing_empties`: `"AB"` followed by empty cells yields a
     two-cell run (the trailing empties are trimmed).
   - `next_breaks_on_bad_ligature`: `"fl"` yields two runs (`"f"` then `"l"`) —
     the bad-ligature break.
   - `next_empty_cell_is_space`: a leading empty cell (codepoint `0`, not
     invisible) contributes a space codepoint.
   - `next_all_empty_is_none`: an all-empty (or empty) row yields `None`.
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

- `RunIterator::next` groups narrow cells into runs by font+comparable-style,
  trims trailing empties, skips leading invisibles, applies the bad-ligature
  break, accumulates the codepoint stream (with the fallback chain and `VS`
  skip), and emits a `TextRun` with `run_hash`/`offset`/`cells`/`font_index` —
  faithful to upstream's common path;
- the grouping, trim, ligature, empty-as-space, and all-empty tests pass, and
  the existing tests still pass;
- the spacer skip, the selection/cursor breaks, and the renderer extraction stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the grouping, break order, accumulation, or hash
diverges from upstream's common path, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **one Required
finding**, now fixed:

- **Required (fixed):** the draft used narrowing casts (`(j - start) as u32`,
  `as u16` for `offset`/`cells`) where upstream uses checked `@intCast`; silent
  truncation would diverge. Changed to checked conversions
  (`u32::try_from(j - start).expect(...)` for the cluster,
  `u16::try_from(...).expect(...)` for `offset`/`cell_count`) — the same class
  of fix as the earlier cluster-conversion gate.

Codex confirmed the rest of the common-path loop is faithful: the break/continue
semantics are right (a `break` leaves `j` unchanged so the cell starts the next
run; the fallback/normal paths advance `j`); `current_font` set at `j == start`
is sound for the narrow/no-spacer scope (`j == start` cannot hit a pre-font
break, and `resolve_font` always returns an index via the space fallback); the
break ordering matches upstream (bad ligature first, then the `style_id` fast
path / `comparable_style` break, all under `j > start`); the fallback
accumulation is correct (a fallback emits only the replacement/space, a
non-fallback emits the primary plus grapheme components skipping only
`FE0E`/`FE0F`); `trailing_trim` matches last-non-empty + 1 (all-empty → `0`);
and there is no infinite-loop risk in scope (every emitted run consumes at least
the start cell, and breaks only occur at `j > start`). Per its non-blocking
suggestion, `debug_assert!`s were added for the excluded inputs (no
selection/cursor; every cell narrow/wide, not a spacer).

Review artifacts:

- Prompt: `logs/codex-review/20260603-152420-716700-prompt.md` (design)
- Result: `logs/codex-review/20260603-152420-716700-last-message.md` (design)

## Result

**Result:** Pass

The run iterator's core grouping loop is implemented — `RunIterator.next()`
runs.

- `roastty/src/font/run.rs`: `RunOutput` (the `TextRun` + accumulated
  `(codepoint, cluster)` stream), `RunIterator` (`new`/`next`/`resolve_font`),
  and `trailing_trim`. `next()` trims trailing empties, skips leading
  invisibles, and walks the row's cells — grouping by font index and comparable
  style (with the `style_id` fast path and the bad-ligature break), resolving
  each cell's font via the fallback chain (`index_for_grapheme` → `U+FFFD` →
  space), accumulating the codepoint stream (primary `0`→space, grapheme
  components minus `FE0E`/`FE0F`), and emitting a `TextRun` with
  `run_hash`/`offset`/`cells`/`font_index`. Checked conversions
  (`u32`/`u16::try_from`) are used for the cluster/offset/cell-count;
  `debug_assert!`s pin the common-path scope (no selection/cursor; no spacers).

Tests (over a Menlo `CodepointResolver`): `next_groups_one_run` (`"AB"` → one
run, `[(A,0),(B,1)]`), `next_trims_trailing_empties`,
`next_breaks_on_bad_ligature` (`"fl"` → two runs), `next_empty_cell_is_space`,
`next_all_empty_is_none`. All pass.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2797 passed, 0 failed (+5, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The shaping pipeline is now **functionally complete for the common path**: a row
of `RunCell`s → `RunIterator::next()` → a `TextRun` plus its
`(codepoint, cluster)` stream → `Face::shape_run` → positioned glyphs.
`RunIterator.next()` — the orchestration function that applies every ported
decision helper, resolves fonts, accumulates the stream, computes the hash, and
emits runs — is implemented and gated.

The remaining work: Experiment 357 adds the **spacer skip** and the
**selection/cursor breaks** (the inputs absent on the common path), and a final
renderer-side experiment wires roastty's `terminal/page.rs` cells into
`RunCell`s (and `RunOptions`) so the path runs live against real terminal rows.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no Required findings**. It verified the common-path control flow matches
upstream: breaks leave `j` unchanged for the next run, the fallback and normal
paths advance `j`, and `current_font` is set before the font comparison at
`j == start` (so the font break cannot fire on the first cell); the checked
conversions resolve the design-gate finding; the accumulation is faithful (a
fallback emits only the substituted codepoint, a non-fallback emits the primary
plus grapheme components skipping `FE0E`/`FE0F`, and `0` maps to space); and
`trailing_trim`, the leading invisible skip, `None` termination, and
`self.i = j` are all safe for the scoped common path (no infinite loop —
`self.i` advances each `next()`). It ran `cargo test -p roastty next_` — all 5
new `next_*` tests passed.

Review artifacts:

- Result review: `logs/codex-review/20260603-152816-734273-last-message.md`
