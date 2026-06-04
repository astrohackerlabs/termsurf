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

# Experiment 407: the Contents upload accessors

## Description

The frame draw uploads the assembled cells to the GPU by reading two views off
`Contents`: the flat background slice and the per-row foreground lists. Upstream
`drawFrame` does exactly this:

```zig
try frame.cells_bg.sync(self.cells.bg_cells);
const fg_count = try frame.cells.syncFromArrayLists(self.cells.fg_rows.lists);
```

roastty's `Contents` keeps `bg_cells` and `fg_rows` **private** with no read
view, so neither upload primitive (`sync`, `sync_from_array_lists` —
Experiment 406) can be fed from it. This experiment adds the two read accessors
— `bg_cells()` and `fg_rows()` — returning the **complete** buffers, so the next
experiment can wire the frame-cell sync. The key faithfulness point is that
`fg_rows()` returns **all** the lists, including the two reserved cursor lists
(index `0` and the last), because upstream uploads `fg_rows.lists` whole — the
cursor glyph lives in the reserved lists and must be uploaded too.

## Upstream behavior

`Contents` (`renderer/cell.zig`) stores `bg_cells` (a flat row-major slice) and
`fg_rows`, an `ArrayListCollection` whose `.lists` field is the array of per-row
vertex lists. The collection reserves the first list and the last list for the
cursor (the cursor glyph is added to `fg_rows.lists[0]` or the last list, never
a real row). `drawFrame` reads both directly:

- `self.cells.bg_cells` → the whole flat background slice, synced 1:1 into the
  background buffer;
- `self.cells.fg_rows.lists` → the **whole** list array (reserved cursor lists
  included), concatenated into the cell-text buffer by `syncFromArrayLists`.

So the upload sees every list, in order, including the reserved cursor lists.

## Rust mapping (`roastty/src/renderer/cell.rs`)

roastty's `Contents` already stores the matching fields —
`bg_cells: Vec<CellBg>` (flat, `bg_cells[row * columns + col]`) and
`fg_rows: Vec<Vec<CellTextVertex>>` (index `0` and the last reserved for the
cursor; real rows are `1..=rows`). The accessors return borrowed slices of the
whole buffers:

```rust
/// The flat background cells, row-major (`bg_cells[row * columns + col]`). The
/// upload view consumed by the background buffer's `sync` (upstream
/// `self.cells.bg_cells`).
pub(crate) fn bg_cells(&self) -> &[CellBg] {
    &self.bg_cells
}

/// All foreground row lists, **including** the two reserved cursor lists (index
/// `0` and the last); real rows are `1..=rows`. The upload view consumed by the
/// cell-text buffer's `sync_from_array_lists` (upstream
/// `self.cells.fg_rows.lists`) — the whole array, so the cursor glyph in the
/// reserved lists is uploaded too.
pub(crate) fn fg_rows(&self) -> &[Vec<CellTextVertex>] {
    &self.fg_rows
}
```

`fg_rows()` returns the entire `fg_rows` vector (length `rows + 2`), not the
real rows `1..=rows`, matching upstream's `fg_rows.lists`. Both are plain
borrows; no copying.

## Scope / faithfulness notes

- **Ported (bridged)**: the two `Contents` upload read views — `bg_cells()` (the
  flat background slice) and `fg_rows()` (all foreground lists, reserved cursor
  lists included). These are the exact views upstream's `drawFrame` reads
  (`self.cells.bg_cells`, `self.cells.fg_rows.lists`).
- **Faithful**: `bg_cells()` returns the whole flat slice; `fg_rows()` returns
  the whole list array (length `rows + 2`), so the reserved cursor lists (index
  `0` and the last) are included — exactly what upstream concatenates. The order
  is the storage order (row-major for `bg_cells`; list index order for
  `fg_rows`), so a later `sync` / `sync_from_array_lists` reproduces upstream's
  upload layout.
- **Faithful adaptation**: roastty's `fg_rows` is a `Vec<Vec<CellTextVertex>>`
  (upstream's `ArrayListCollection.lists`); returning `&[Vec<CellTextVertex>]`
  matches `sync_from_array_lists`'s `&[Vec<T>]` parameter, so the accessor feeds
  the upload primitive directly.
- **Deferred**: the frame-cell sync that consumes these (a frame-owned
  background buffer + cell-text buffer, syncing from a `Contents` and returning
  the foreground count — upstream's `drawFrame` lines) is the next experiment;
  the full draw wiring and the remaining Metal upload stay deferred.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/cell.rs`:
   - add `Contents::bg_cells(&self) -> &[CellBg]` and
     `Contents::fg_rows(&self) -> &[Vec<CellTextVertex>]` returning borrows of
     the whole buffers.
2. Tests (in `cell.rs`):
   - assemble a small `Contents` (`resize` to a 2×1 grid, set both background
     cells via `bg_cell_mut`, add a foreground vertex to the real row via `add`,
     and a cursor glyph to the reserved list via `set_cursor`): `bg_cells()`
     returns the two background cells in row-major order; `fg_rows()` has length
     `rows + 2` (`3`), with the cursor glyph in the reserved list `0`, the added
     vertex in real row `1`, and the last (reserved) list present — proving the
     accessor exposes **all** lists including the reserved cursor lists, in
     order.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty contents_upload
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `bg_cells()` returns the whole flat background slice and `fg_rows()` returns
  the whole list array (reserved cursor lists included, length `rows + 2`), in
  storage order — faithful to upstream's `self.cells.bg_cells` /
  `self.cells.fg_rows.lists` upload views;
- the test passes (the row-major background cells; the foreground lists with the
  cursor glyph in the reserved list and the added vertex in the real row, all
  lists present), and the existing tests still pass;
- the frame-cell sync and the rest of the draw wiring stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if `fg_rows()` omits the reserved cursor lists (returns
only the real rows), the order is wrong, the accessors copy instead of borrow,
or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed that returning the **full** `fg_rows` slice is the
faithful choice: upstream uploads `self.cells.fg_rows.lists` whole, and
roastty's layout deliberately mirrors that with the cursor-reserved lists at
index `0` and the last index — exposing only the real rows `1..=rows` would be
wrong because it would drop the cursor vertices from the eventual
`sync_from_array_lists` upload. It confirmed `bg_cells()` as the whole flat
row-major slice is the correct counterpart to upstream `self.cells.bg_cells`,
that both accessors being plain borrows is the right scoped shape, and that
`&[Vec<CellTextVertex>]` feeds the Experiment 406 primitive directly with no
copy or reshaping. It judged the test plan sufficient (row-major background
exposure, `fg_rows` length `rows + 2`, the block cursor in reserved list `0`, a
normal foreground vertex in real row `1`, and the presence of the final reserved
list) and agreed that keeping this experiment limited to the accessors is
reasonable, with the frame-cell sync composition as a clean follow-up.

Review artifacts:

- Prompt: `logs/codex-review/20260604-071238-d407-prompt.md` (design)
- Result: `logs/codex-review/20260604-071238-d407-last-message.md` (design)
