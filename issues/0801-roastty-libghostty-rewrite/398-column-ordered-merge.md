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

# Experiment 398: the column-ordered decoration merge

## Description

Upstream emits a row's foreground in a **single column loop**: for each cell, in
column order, it adds the underline, the overline, the glyph(s), then the
strikethrough. roastty's `rebuild_row` (Experiment 378) instead uses **three
separate passes** — all underlines/overlines, then all glyphs, then all
strikethroughs. That preserves the _within-cell_ layering (decorations under the
glyph, strikethrough over it) but differs _cross-column_: a column's underline
is emitted before an earlier column's glyph, so a glyph that overhangs into a
neighbor (an italic, a descender) layers differently than upstream. This
experiment merges the three passes into one **column-interleaved** loop — per
column: underline → overline → glyph(s) → strikethrough — making the foreground
emission order byte-exact to upstream and removing the cross-column overhang
caveat. The per-run `add_run` helper is folded into the loop (its glyph emission
becomes a per-column walk over the shaped runs).

## Upstream behavior

In `rebuildCells` (`renderer/generic.zig`), one loop iterates the cells in
column order; for each cell `x` it emits, in this order:

```zig
// 1. underline (with the link override), if != .none — "drawn first so that they
//    layer underneath text"
if (underline != .none) self.addUnderline(x, y, underline, …);
// 2. overline
if (style.flags.overline) self.addOverline(x, y, …);
// 3. the glyph(s) at this column, from the shaper run iterator: while the next
//    shaped cell is at this column (run.offset + cell.x == x), addGlyph and advance
while (… run.offset + shaped_cells[i].x == x) : (i += 1) self.addGlyph(x, y, …);
// 4. "Finally, draw a strikethrough if necessary."
if (style.flags.strikethrough) self.addStrikethrough(x, y, …);
```

The glyph step walks a **shaper run cursor** that advances monotonically with
`x`: when the current run's cells are exhausted it moves to the next run, and it
emits every shaped cell whose absolute column (`run.offset + cell.x`) equals the
current `x`. So the per-row foreground list is
`[col0: ul, ol, glyph, st], [col1: …], …` — strictly column-ordered.

## Rust mapping (`roastty/src/renderer/cell.rs`)

`rebuild_row`'s three passes become one column loop with an inline run/glyph
cursor; `add_run` is removed (its body is the glyph step):

```rust
let grid_metrics = grid.metrics;
let mut run_i = 0usize;   // current shaped run
let mut glyph_i = 0usize; // current glyph within run_i

for (col, cell) in row_cells.iter().enumerate() {
    let x = u16::try_from(col).expect("viewport column fits u16");
    let grid_pos = [x, y];
    let rgba = fg_colors[col];
    let fg = [rgba[0], rgba[1], rgba[2]];
    let flags = cell.style.flags;

    // 1. Underline (its own color, else the foreground) — underneath.
    if flags.underline != Underline::None {
        let underline_color = cell
            .style
            .resolve_underline_color(palette)
            .map(|rgb| [rgb.r, rgb.g, rgb.b])
            .unwrap_or(fg);
        add_underline(contents, grid, grid_pos, flags.underline, underline_color, rgba[3])?;
    }
    // 2. Overline — underneath.
    if flags.overline {
        add_overline(contents, grid, grid_pos, fg, rgba[3])?;
    }

    // 3. The glyph(s) at this column, walking the shaped runs in column order
    //    (the run cursor advances monotonically with `col`).
    while run_i < row_runs.len() && glyph_i >= row_runs[run_i].glyphs.len() {
        run_i += 1;
        glyph_i = 0;
    }
    if run_i < row_runs.len() {
        let run = &row_runs[run_i];
        // The cursor never falls behind `col` (monotonic, like upstream's assert).
        debug_assert!(
            glyph_i >= run.glyphs.len()
                || usize::from(run.run.offset) + usize::from(run.glyphs[glyph_i].x) >= col
        );
        let opts = render_options(grid_metrics, &infos, col, cols, thicken, thicken_strength);
        let cp = infos[col].codepoint;
        while glyph_i < run.glyphs.len()
            && usize::from(run.run.offset) + usize::from(run.glyphs[glyph_i].x) == col
        {
            add_glyph(
                contents, grid, grid_pos, run.run.font_index, &run.glyphs[glyph_i],
                fg, rgba[3], no_min_contrast(cp), &opts,
            )?;
            glyph_i += 1;
        }
    }

    // 4. Strikethrough — on top.
    if flags.strikethrough {
        add_strikethrough(contents, grid, grid_pos, fg, rgba[3])?;
    }
}
Ok(())
```

`add_glyph` (the per-glyph emitter), `add_underline`/`add_overline`/
`add_strikethrough`, `render_options`, `cell_infos`, and `no_min_contrast` are
unchanged. The `fg_colors` builder above the loop is unchanged. `add_run` (the
per-run wrapper) is removed — its only caller was the old Pass 2, and its logic
(compute `col`/opts/`cp`, `add_glyph`) is now the column loop's glyph step. The
`render_options`/`cp` are computed once per column (only used when a glyph is
emitted there).

## Scope / faithfulness notes

- **Ported (bridged)**: the single column-ordered foreground emission — per
  column: underline → overline → glyph(s) → strikethrough — replacing roastty's
  three passes, so the foreground cell order is byte-exact to upstream.
- **Faithful**: each cell still draws its underline (with the explicit-color
  fallback) and overline underneath, its glyph(s), then its strikethrough on top
  — the same per-cell layering, now in upstream's column-interleaved order. The
  glyph step walks the shaped runs with a monotonic cursor (the
  `run.offset + cell.x == col` test and the next-run advance), exactly as
  upstream's shaper-cell cursor; `add_glyph` receives identical inputs (the
  `render_options`, `cp`, `fg`/alpha). This removes the cross-column overhang
  difference noted in Experiment 378.
- **Faithful adaptation**: roastty walks the pre-shaped `row_runs` with
  `(run_i, glyph_i)` indices (upstream lazily pulls runs from a `run_iter`); the
  result is identical — every shaped cell at column `col` is emitted there, in
  run/glyph order. The `debug_assert` mirrors upstream's monotonic `assert`.
  `render_options` and `cp` are hoisted to once-per-column (they only matter
  when a glyph is emitted, and are constant across the same column's glyphs).
- **Deferred**: the link-underline wiring (Experiment 397's `link_underline` is
  not yet wired — the underline is still `flags.underline`); the hovered-link
  set; the Metal upload. (Consumed by tests now.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/cell.rs`:
   - `rebuild_row`: replace the three passes (underline/overline loop, run loop,
     strikethrough loop) with one column loop that emits underline → overline →
     glyph(s) (via an inline run/glyph cursor) → strikethrough per column.
     Update its doc comment (column-ordered emission).
   - remove `add_run` (folded into the loop).
2. Tests (in `cell.rs`):
   - migrate `add_run_places_glyphs_at_absolute_columns` to a `rebuild_row`
     test: a 4-column row whose shaped run is at **offset 2** with two glyphs →
     the two glyph vertices land at absolute columns 2 and 3 with the right
     colors (the run-cursor offset/column mapping, now via `rebuild_row`);
   - add a **column-order** test: a 2-column row where **both** cells have an
     underline **and** a glyph → the foreground list is
     `[col0 underline, col0 glyph, col1 underline, col1 glyph]` (interleaved by
     column), not `[col0 ul, col1 ul, col0 glyph, col1 glyph]` (the old
     three-pass order);
   - the existing single-column `rebuild_row` tests (faint, selection fg, the
     explicit-underline-color, the search fg) are unchanged (a single column has
     the same order in both schemes).
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

- `rebuild_row` emits the foreground in one column-ordered loop (per column:
  underline → overline → glyph(s) → strikethrough), the glyph step walking the
  shaped runs with a monotonic cursor — byte-exact to upstream's order;
- the tests pass (the offset-2 run lands glyphs at columns 2/3; the two-column
  interleave order is column-ordered), and the existing single-column tests
  still pass (unchanged order);
- the link-underline wiring and the Metal upload stay deferred; `add_glyph` and
  the decoration writers are unchanged;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the order is wrong (decorations not
column-interleaved with glyphs, the within-cell layering broken), a glyph is
dropped or mis-positioned (the run cursor wrong), or any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**, scrutinizing the run-cursor logic specifically. It confirmed the
cursor walk is sound for `shape_row`'s invariants — shaped runs are returned in
row order and glyph cells are monotonic by `run.offset + glyph.x` (with non-LTR
output sorted by `x`) — so advancing exhausted runs before each column and
emitting all glyphs whose absolute column equals `col` preserves the upstream
shaper-cell cursor behavior without dropped or duplicated glyphs. It confirmed a
backwards glyph would be an invariant violation and the `debug_assert` is an
acceptable guard given the shaper ordering; that hoisting `opts`/`cp` per column
is correct (they are column properties, shared by every glyph at that cell); and
that the merged order preserves the within-cell layering while fixing the
cross-column ordering (underline → overline → glyph(s) → strikethrough per
column). It judged removing `add_run` reasonable provided its offset behavior is
covered by the migrated `rebuild_row` offset test, and the two-column interleave
test plus the existing single-column decoration tests sufficient.

Review artifacts:

- Prompt: `logs/codex-review/20260604-055335-385074-prompt.md` (design)
- Result: `logs/codex-review/20260604-055335-385074-last-message.md` (design)

## Result

**Result:** Pass

`rebuild_row`'s foreground is now emitted in one column-ordered pass.

- `roastty/src/renderer/cell.rs`:
  - `rebuild_row`: the three passes (underline/overline loop, run loop,
    strikethrough loop) are replaced by **one column loop** — per cell, in
    column order: the underline (with the explicit-color fallback) and overline
    (underneath), then the glyph(s) at that column (walking the shaped runs with
    a `(run_i, glyph_i)` cursor: advance past exhausted runs, then emit every
    glyph whose `run.offset + glyph.x == col`, `opts`/`cp` hoisted once per
    column), then the strikethrough (on top). A `debug_assert` guards the
    monotonic cursor. The doc comment now describes the column-ordered emission.
  - `add_run` is removed — its glyph step is now the column loop's glyph
    emission; `add_glyph`, the decoration writers, `render_options`,
    `no_min_contrast`, and the `fg_colors` builder are unchanged.

Tests (in `cell.rs`):

- `rebuild_row_places_glyphs_at_absolute_columns` (migrated from the removed
  `add_run` test) — an offset-2 shaped run's two glyphs land at absolute columns
  2/3 with the right colors, via `rebuild_row`.
- `rebuild_row_emits_foreground_column_ordered` — two cells each with an
  underline and a glyph → the foreground grid-pos column sequence is
  `[0, 0, 1, 1]` (column-interleaved), proving the new order vs the old
  three-pass `[0, 1, 0, 1]`.
- The existing single-column `rebuild_row` tests (faint, the selection
  foreground, the explicit-underline-color) are unchanged — they cover the
  within-cell layering (a single column has the same order in both schemes).

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2857 passed, 0 failed (net +1: +2 new, −1 removed
  `add_run` test; no regressions — the single-column layering tests still pass).
- `cargo build -p roastty` → no warnings (the `ci` helper is still used
  elsewhere).
- No-`ghostty`-name gates (font + renderer) clean; `git diff --check` clean.

## Conclusion

The row foreground is now emitted byte-exact to upstream: one column-ordered
loop (underline → overline → glyph(s) → strikethrough per column), the glyph
step walking the shaped runs with a monotonic cursor. This removes the
cross-column overhang caveat from Experiment 378 — a glyph that overhangs into a
neighbor now layers exactly as upstream. With the cursor/cursor-uniform colors,
the selection and search recolor, the lock cursor, and now the decoration merge
all live, the CPU-side renderer bridge (`renderer/cell.rs`'s `Contents`
assembly) is faithful to upstream's `rebuildCells`.

The remaining renderer-bridge work: the link-underline wiring (the
`link_underline` override, Experiment 397, into the underline pass) and the
hovered-link set; and the **Metal upload** of `Contents` (the GPU buffer/uniform
upload — the largest remaining piece, which carries the min-contrast and cursor
uniforms).

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed the implementation matches the approved design:
`rebuild_row` now emits foreground cells in one column-ordered pass — underline
and overline first, all glyphs for the current column via the `(run_i, glyph_i)`
cursor, strikethrough last — with the glyph cursor advancing exhausted runs,
emitting every glyph whose `run.offset + glyph.x == col`, and keeping the same
`add_glyph` inputs as the old `add_run` path (`grid_pos`, font index,
foreground/alpha, `render_options`, `no_min_contrast(cp)`). It confirmed
`add_glyph`, the decoration writers, `render_options`, and the `fg_colors`
builder are unchanged, that removing `add_run` is sound (its behavior is covered
by the migrated offset-2 test), that the new interleave test proves the
cross-column order changed to upstream's column order while the existing
single-column tests cover the within-cell layering, and that there is no public
C ABI/header impact. Nothing needed to change before the result commit.

Review artifacts:

- Result review: `logs/codex-review/20260604-055757-002055-last-message.md`
