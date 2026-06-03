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

# Experiment 368: placing a shaped run's glyphs

## Description

Experiments 366–367 port the two halves of `addGlyph` (`render_options` builds a
cell's `RenderOptions`; `add_glyph` renders and emits one glyph). This
experiment composes them over a whole **`ShapedRun`**: `add_run` walks the run's
shaped cells, computes each one's absolute column (`run.offset + cell.x`),
derives its `RenderOptions` and `no_min_contrast`, and calls `add_glyph`. This
is the inner glyph-placement loop of upstream `rebuildCells` — the per-run step.
The outer loop (iterating the viewport's runs/rows and deriving the
`CellInfo`/colors from the terminal page) is the next experiment.

## Upstream behavior

`rebuildCells` (`renderer/generic.zig`) walks each row's columns; for the column
matching a shaped cell it calls
`addGlyph(x, y, …, shaper_cell, shaper_run, color, alpha)`. The shaper emits
cells at run-relative `x`; the absolute column is `run.offset + shaper_cell.x`.
`addGlyph` then builds the per-cell `RenderOptions` (now roastty's
`render_options`) and emits the cell (now `add_glyph`), with the codepoint's
`no_min_contrast`. This experiment ports that per-run walk: for each shaped cell
of one run, place its glyph at its absolute column.

## Rust mapping (`roastty/src/renderer/cell.rs`)

```rust
use crate::font::run::ShapedRun;

/// Place every glyph of one [`ShapedRun`] into `contents` on row `y`. For each
/// shaped cell, the absolute column is `run.offset + cell.x`; its `RenderOptions`
/// come from [`render_options`] over `row_cells`, its color/alpha from
/// `fg_colors[col]`, and its `no_min_contrast` from the cell's codepoint. The
/// per-run inner loop of upstream `rebuildCells`.
pub(crate) fn add_run(
    contents: &mut Contents,
    grid: &mut SharedGrid,
    y: u16,
    run: &ShapedRun,
    row_cells: &[CellInfo],
    fg_colors: &[[u8; 4]],
    cols: usize,
    thicken: bool,
    thicken_strength: u8,
) -> Result<(), ResolverRenderError> {
    let grid_metrics = grid.metrics;
    for cell in &run.glyphs {
        let col = usize::from(run.run.offset) + usize::from(cell.x);
        debug_assert!(col < cols && cols <= row_cells.len() && cols <= fg_colors.len());
        // Checked, like upstream's `@intCast` (and the bearings in `add_glyph`).
        let grid_x = u16::try_from(col).expect("glyph column fits u16");
        let opts = render_options(grid_metrics, row_cells, col, cols, thicken, thicken_strength);
        let cp = row_cells[col].codepoint;
        let rgba = fg_colors[col];
        add_glyph(
            contents,
            grid,
            [grid_x, y],
            run.run.font_index,
            cell,
            [rgba[0], rgba[1], rgba[2]],
            rgba[3],
            no_min_contrast(cp),
            &opts,
        )?;
    }
    Ok(())
}
```

## Scope / faithfulness notes

- **Ported (bridged)**: the per-run glyph-placement loop of upstream
  `rebuildCells` — for each shaped cell of a run, place its glyph at its
  absolute column (`run.offset + cell.x`) via `render_options` + `add_glyph`,
  with the cell's color/alpha and `no_min_contrast`.
- **Faithful**: the absolute column is `run.offset + cell.x` (upstream's run
  offset plus the shaper cell's run-relative `x`); the per-cell `RenderOptions`,
  color, and `no_min_contrast(cp)` are derived exactly as upstream's `addGlyph`
  call site; the run's `font_index` is used for every glyph in the run (a run
  shares a font); invisible (0-size) glyphs are skipped inside `add_glyph`.
- **Faithful adaptation**: `add_run` takes the row's `CellInfo` slice and a
  per-column `fg_colors` (RGBA) — the renderer-derived inputs the future
  `rebuildCells` computes from the terminal page (color from the cell style,
  `CellInfo` from the codepoint + grid width). It splits each `[u8; 4]` into the
  `[u8; 3]` color + `u8` alpha `add_glyph` expects.
- **Deferred**: the outer `rebuildCells` loop (iterate the viewport's
  `ShapedRun`s per row, build the `CellInfo` slice and `fg_colors` from the
  terminal page, and call `add_run` per run), plus the background/decoration/
  cursor cells and the Metal upload. (Consumed by tests now.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/cell.rs`: add the `add_run` function; import
   `font::run::ShapedRun`.
2. Tests (in `cell.rs`): with a Menlo `SharedGrid` and a `Contents`, construct a
   `ShapedRun` with a **nonzero** `run.offset = 2` and two glyphs (`'A'`/`'B'`)
   at run-relative `x = 0`/`1` (so the column math is exercised, not hidden by a
   zero offset); build a `CellInfo` row of width `cols = 4` with `'A'`/`'B'` at
   columns `2`/`3` and a matching `fg_colors`, and call `add_run` on row
   `y = 1`:
   - assert two cells land in `fg_rows[2]`, at `grid_pos [2, 1]` and `[3, 1]`
     (the run offset `2` plus the shaped cells' `x` `0`/`1`) — a regression that
     dropped `run.offset` would put them at `[0, 1]`/`[1, 1]`;
   - assert each cell's `color` is the matching `fg_colors` column entry and
     `atlas == Grayscale`.
   - (a run whose glyphs include a 0-size space adds no cell for that column —
     covered by `add_glyph`'s skip, noted.)
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty add_run
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `add_run` places each shaped cell of a run at its absolute column
  (`run.offset + cell.x`) via `render_options` + `add_glyph`, with the per-cell
  color and `no_min_contrast` — faithful to upstream `rebuildCells`'s per-run
  walk;
- the test passes (a shaped `"AB"` run adds two correctly-placed cells), and the
  existing tests still pass;
- the outer `rebuildCells` loop, decorations, cursor, and Metal upload stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a glyph lands at the wrong column (offset math), the
wrong color/font is used, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with two
**Required** findings, both now addressed:

- **Required (addressed):** the `grid_pos` column used `col as u16`, which can
  silently truncate; upstream's `@intCast` is checked. Now uses
  `u16::try_from(col).expect("glyph column fits u16")`, matching the bearings
  pattern in `add_glyph`. A
  `debug_assert!(col < cols && cols <= row_cells.len() && cols <= fg_colors.len())`
  was also added.
- **Required (addressed):** the test used a run with `offset == 0`, so a
  regression dropping `run.offset` would still pass. The test now constructs a
  `ShapedRun` with `run.offset = 2` and glyphs at `x = 0/1` and asserts grid
  positions `[2, 1]`/`[3, 1]`, exercising the column math.

Codex confirmed the rest is sound: `run.run.offset + cell.x` is the faithful
absolute-column calculation; `render_options` + `add_glyph` is the right
composition; reading `grid.metrics` once (it is `Copy`) before mutably borrowing
`grid` in the loop is correct; `run.run.font_index` is the correct font for
every glyph in the run; and panicking on a malformed `row_cells`/`fg_colors`
slice is acceptable for this internal helper (matching the `constraint_width`
style).

Review artifacts:

- Prompt: `logs/codex-review/20260603-180745-101114-prompt.md` (design)
- Result: `logs/codex-review/20260603-180745-101114-last-message.md` (design)
