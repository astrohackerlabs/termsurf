# Experiment 179: Port Render State Row Iterator C ABI

## Description

Port the render-state row iterator and row getter portion of upstream:

- `vendor/ghostty/src/terminal/c/render.zig`;
- `vendor/ghostty/src/terminal/render.zig`.

Experiment 178 added the scalar render-state handle, update, dirty/color/cursor
getters, and a deliberate `ROASTTY_NO_VALUE` result for the deferred
`ROW_ITERATOR` selector. This experiment makes that selector real and adds row
iteration over the render state's viewport rows.

Scope for this experiment:

- add row snapshot storage to `roastty_render_state_t`;
- implement `roastty_render_state_row_iterator_t` lifecycle and `next`;
- make `ROASTTY_RENDER_STATE_DATA_ROW_ITERATOR` initialize a caller-provided row
  iterator;
- add row getters for dirty flag, raw row, and row selection;
- add row dirty setter.

It deliberately does not add row-cell iterators yet. The row `CELLS` selector is
declared with the upstream numeric value but returns `ROASTTY_NO_VALUE` until
the next slice ports row-cell iteration, graphemes, cell styles, and cell color
resolution.

Public names must use Roastty naming only:

- `roastty_render_state_row_iterator_t`;
- `roastty_render_state_row_cells_t`;
- `roastty_render_state_row_selection_s`;
- `roastty_render_state_row_data_e`;
- `roastty_render_state_row_option_e`;
- `roastty_render_state_row_*`.

Upstream names may appear only in this issue document as source citations.

## Changes

1. Re-read upstream and Roastty source:
   - `vendor/ghostty/src/terminal/c/render.zig`;
   - `vendor/ghostty/src/terminal/render.zig`;
   - `roastty/src/terminal/page.rs`;
   - `roastty/src/terminal/page_list.rs`;
   - `roastty/src/terminal/screen.rs`;
   - `roastty/src/terminal/terminal.rs`;
   - existing row C ABI from Experiment 176;
   - existing render-state scalar C ABI from Experiment 178.

2. Extend the private render-state backing storage.

   Add row snapshot storage to the existing private render-state struct. Each
   row snapshot must contain, at minimum:
   - raw `roastty_row_t`;
   - dirty flag;
   - optional row selection range;
   - enough reserved/internal structure for a future row-cell snapshot without
     changing `roastty_render_state_t`.

   The update path should fill exactly `rows` viewport row snapshots after a
   successful terminal update. If there is no clean internal terminal accessor
   for the active viewport rows, add a narrow internal snapshot method instead
   of reading raw page/node pointers through the C ABI.

   Do not expose raw page pointers, node pointers, `PageList` internals, or live
   row storage to C. Row iterators read the render-state snapshot only.

3. Add public C ABI types in `roastty/include/roastty.h` and mirrored Rust
   constants/types in `roastty/src/lib.rs`.

   `roastty_render_state_row_iterator_t` already exists from Experiment 178 as
   an opaque handle. Add:

   ```c
   typedef void* roastty_render_state_row_cells_t;

   typedef struct roastty_render_state_row_selection_s {
     size_t size;
     uint16_t start_x;
     uint16_t end_x;
   } roastty_render_state_row_selection_s;
   ```

   Preserve the expected macOS C layout:
   - `sizeof(roastty_render_state_row_selection_s) == 16`;
   - `_Alignof(roastty_render_state_row_selection_s) == 8`;
   - `offsetof(roastty_render_state_row_selection_s, size) == 0`;
   - `offsetof(roastty_render_state_row_selection_s, start_x) == 8`;
   - `offsetof(roastty_render_state_row_selection_s, end_x) == 10`.

4. Add row data and option selector enums matching upstream numeric order with
   Roastty names.

   Row data selectors:

   | Name                                      | Value | Output type                                        |
   | ----------------------------------------- | ----- | -------------------------------------------------- |
   | `ROASTTY_RENDER_STATE_ROW_DATA_INVALID`   | 0     | none; always invalid                               |
   | `ROASTTY_RENDER_STATE_ROW_DATA_DIRTY`     | 1     | `bool*`                                            |
   | `ROASTTY_RENDER_STATE_ROW_DATA_RAW`       | 2     | `roastty_row_t*`                                   |
   | `ROASTTY_RENDER_STATE_ROW_DATA_CELLS`     | 3     | deferred; returns `ROASTTY_NO_VALUE` in this slice |
   | `ROASTTY_RENDER_STATE_ROW_DATA_SELECTION` | 4     | `roastty_render_state_row_selection_s*`            |

   Row option selectors:

   | Name                                    | Value | Input type |
   | --------------------------------------- | ----- | ---------- |
   | `ROASTTY_RENDER_STATE_ROW_OPTION_DIRTY` | 0     | `bool*`    |

5. Add public row iterator functions:

   ```c
   ROASTTY_API roastty_result_e
   roastty_render_state_row_iterator_new(
       roastty_render_state_row_iterator_t*);

   ROASTTY_API void
   roastty_render_state_row_iterator_free(
       roastty_render_state_row_iterator_t);

   ROASTTY_API bool
   roastty_render_state_row_iterator_next(
       roastty_render_state_row_iterator_t);
   ```

   Behavior:
   - `new(NULL)` returns `ROASTTY_INVALID_VALUE`;
   - `new(out)` allocates an iterator in an unbound state;
   - `free(NULL)` is a no-op;
   - `next(NULL)` returns `false`;
   - `next(iterator)` returns `false` while the iterator is unbound;
   - after binding through `ROASTTY_RENDER_STATE_DATA_ROW_ITERATOR`, the first
     `next` selects row `0`, each subsequent `next` advances by one row, and
     `next` returns `false` after the last row.

6. Change `roastty_render_state_get(... ROW_ITERATOR ...)`.

   Experiment 178 returned `ROASTTY_NO_VALUE` for the row iterator selector.
   This experiment replaces that deferred result with real behavior:
   - output type is `roastty_render_state_row_iterator_t*`;
   - null output still returns `ROASTTY_INVALID_VALUE`;
   - the pointed-to iterator handle must already be non-null;
   - if the pointed-to iterator handle is null, return `ROASTTY_INVALID_VALUE`;
   - on success, bind the iterator to the render state's current row snapshot
     and reset it to the before-first-row state.

   Binding clones the render state's current row snapshot into the iterator. The
   iterator owns that cloned snapshot until the next successful binding or until
   the iterator is freed. This is the C ABI lifetime contract:
   - terminal mutation after binding does not affect the iterator;
   - `roastty_render_state_update(state, terminal)` after binding does not
     affect the iterator;
   - `roastty_render_state_free(state)` after binding does not invalidate the
     iterator;
   - rebinding the iterator to a render state replaces the iterator's owned
     snapshot and resets it to the before-first-row state.

   The iterator must never reference terminal page storage directly or borrow
   the render state's row vector.

7. Add public row getter/setter functions:

   ```c
   ROASTTY_API roastty_result_e
   roastty_render_state_row_get(roastty_render_state_row_iterator_t,
                                roastty_render_state_row_data_e,
                                void*);

   ROASTTY_API roastty_result_e
   roastty_render_state_row_get_multi(
       roastty_render_state_row_iterator_t,
       size_t,
       const roastty_render_state_row_data_e*,
       void**,
       size_t*);

   ROASTTY_API roastty_result_e
   roastty_render_state_row_set(roastty_render_state_row_iterator_t,
                                roastty_render_state_row_option_e,
                                const void*);
   ```

   Behavior:
   - raw enum values are accepted as `int`/`c_int` and validated before
     matching;
   - null iterator returns `ROASTTY_INVALID_VALUE`;
   - invalid selector returns `ROASTTY_INVALID_VALUE`;
   - null output/input pointers return `ROASTTY_INVALID_VALUE`;
   - getters/setters before the iterator has selected a row return
     `ROASTTY_INVALID_VALUE`;
   - `DIRTY` reads the iterator row's dirty flag;
   - `RAW` reads the iterator row's raw `roastty_row_t`;
   - `CELLS` returns `ROASTTY_NO_VALUE` until row-cell iteration is ported;
   - `SELECTION` treats `out->size` as caller-provided input capacity and does
     not rewrite it;
   - `SELECTION` validates
     `out->size >= sizeof(roastty_render_state_row_selection_s)` before writing
     any field;
   - `SELECTION` returns `ROASTTY_NO_VALUE` when the row has no selection;
   - `SELECTION` writes `start_x` and `end_x` when selection exists;
   - `row_set(DIRTY, bool*)` mutates the snapshot row dirty flag only, not the
     terminal;
   - `row_get_multi` null keys or values returns `ROASTTY_INVALID_VALUE`;
   - `row_get_multi` with `count == 0` succeeds and writes `0` to `out_written`
     if provided;
   - `row_get_multi` accepts null `out_written`;
   - `row_get_multi` writes the number of completed entries on success or first
     failure.

8. Define snapshot semantics.

   On update:
   - row count equals the render state's `rows`;
   - each snapshot row corresponds to the active viewport row at the same `y`;
   - raw row data matches the existing `roastty_row_t` C ABI representation;
   - row dirty state is copied from the terminal/page dirty state at update
     time;
   - row selection is copied from the terminal's active selection if the row is
     selected;
   - later terminal mutation does not mutate the render state's row snapshot
     until `roastty_render_state_update` is called again.

   A row iterator may be bound before the first successful render-state update.
   In that case, binding succeeds with the render state's current empty snapshot
   and `roastty_render_state_row_iterator_next(iterator)` returns `false`.

   Selection ranges use inclusive row-local coordinates, matching upstream row
   cell selection checks (`x >= start_x && x <= end_x`):
   - `start_x` and `end_x` are both inclusive;
   - ranges are clipped to `0..cols - 1`;
   - single-row regular selections normalize reversed endpoints before producing
     the row range;
   - multi-row regular selections produce:
     - first selected row: selected start x through the last column;
     - fully covered middle rows: `0` through the last column;
     - last selected row: `0` through selected end x;
   - reversed multi-row selections normalize the top/bottom row order before
     applying the same rules;
   - rectangular selections produce the same normalized inclusive x range on
     every selected row between the selected top and bottom rows.

   If row selection cannot be derived cleanly yet, this experiment may return
   `ROASTTY_NO_VALUE` for all row selections only if the result records that as
   Partial and designs the next experiment around selection derivation. A Pass
   requires at least unselected rows to return `ROASTTY_NO_VALUE` and selected
   rows to return the correct range.

9. Keep scope narrow:
   - Do not implement `roastty_render_state_row_cells_t` lifecycle or row-cell
     iteration.
   - Do not add cell getters under render-state row cells.
   - Do not add grapheme extraction, style lookup, cell foreground/background
     resolution, Kitty graphics, highlights, hyperlinks, or formatter objects.
   - Do not expose raw page pointers or node pointers.
   - Do not add Metal renderer, Swift integration, app runtime integration, or
     browser overlay behavior.
   - Do not add `ghostty_*` symbols or compatibility aliases.

## Verification

Run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/mod.rs roastty/src/terminal/page_list.rs roastty/src/terminal/screen.rs roastty/src/terminal/terminal.rs
cargo test -p roastty render_state_row_c_abi
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

Pass criteria:

- row iterator handle lifecycle works and frees cleanly;
- `ROASTTY_RENDER_STATE_DATA_ROW_ITERATOR` binds an existing iterator and resets
  it before the first row;
- iteration visits exactly the render state's viewport row count in
  top-to-bottom order;
- row data and option selector numeric values match upstream order with Roastty
  names;
- `roastty_render_state_row_selection_s` size, alignment, and offsets match the
  specified macOS C layout;
- row `DIRTY`, `RAW`, and `SELECTION` getters behave as specified;
- row selection size-capacity validation reads `size` before writing any
  selection fields and never rewrites `size`;
- row dirty set mutates only the snapshot row and is visible through the row
  getter;
- bound iterators remain valid and stable after terminal mutation,
  `roastty_render_state_update`, and `roastty_render_state_free`;
- pre-update binding succeeds with an empty snapshot whose `next` returns
  `false`;
- row `CELLS` returns `ROASTTY_NO_VALUE` without allocating or faking a
  row-cells object;
- `row_get_multi` reports partial progress correctly;
- C header, Rust ABI tests, C harness, full `roastty` tests, no-Ghostty check,
  and `git diff --check` all pass;
- Codex reviews and approves both the design and completed result.

Partial criteria:

- iterator lifecycle, binding, `next`, raw row, and dirty row behavior pass, but
  row selection cannot yet be derived from terminal selection without a focused
  follow-up.

Failure criteria:

- the API exposes raw page/node pointers or live row storage;
- row iterators reference terminal page storage directly instead of render-state
  snapshot storage;
- raw C enum values are represented as Rust enums before validation;
- `CELLS` is silently implemented as a fake row-cell iterator instead of a clear
  deferred result;
- row dirty set mutates the terminal rather than the render-state snapshot;
- the implementation expands into row cells, formatter objects, renderer backend
  code, Swift integration, or browser overlay behavior;
- the API exposes `ghostty_*` symbols or compatibility aliases.

## Review

This design must be reviewed with the Codex review skill before implementation.
All real findings must be fixed, and the design must be re-reviewed until Codex
approves it.

After implementation and result recording, the completed result must also be
reviewed with Codex and approved before the result commit.

## Result

**Result:** Pass

Implemented the render-state row iterator C ABI.

Code changes:

- Added `roastty_render_state_row_cells_t`,
  `roastty_render_state_row_selection_s`, `roastty_render_state_row_data_e`, and
  `roastty_render_state_row_option_e` to `roastty/include/roastty.h`.
- Added exported row iterator lifecycle functions:
  `roastty_render_state_row_iterator_new`,
  `roastty_render_state_row_iterator_free`, and
  `roastty_render_state_row_iterator_next`.
- Changed `ROASTTY_RENDER_STATE_DATA_ROW_ITERATOR` from the Experiment 178
  deferred `ROASTTY_NO_VALUE` result to real binding of an existing iterator.
- Added `roastty_render_state_row_get`, `roastty_render_state_row_get_multi`,
  and `roastty_render_state_row_set`.
- Added an internal active-viewport row snapshot path through
  `PageList::render_rows_snapshot`, `Screen::render_rows_snapshot`, and
  `Terminal::render_rows_snapshot`.
- Stored row snapshots inside the render-state backing struct and cloned them
  into iterators on bind, so bound iterators remain valid after terminal
  mutation, render-state update, and render-state free.
- Implemented row `DIRTY`, `RAW`, and `SELECTION` getters.
- Kept row `CELLS` as a clear `ROASTTY_NO_VALUE` deferred result.
- Added Rust ABI tests and C harness coverage for layout, lifecycle, binding,
  iteration, raw/dirty consistency, selection ranges, snapshot lifetime,
  validation, zero-count `row_get_multi`, partial progress, deferred cells, and
  dirty set.

One implementation-time test bug was found and fixed: row raw outputs in Rust
tests must be typed as `RoasttyRow`/`u64`, because the C ABI writes
`roastty_row_t`. Leaving them as inferred integer locals caused stack corruption
during early testing.

Verification run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/page_list.rs roastty/src/terminal/screen.rs roastty/src/terminal/terminal.rs
cargo test -p roastty render_state_row_c_abi
cargo test -p roastty render_state_c_abi
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty
git diff | rg -n "ghostty|Ghostty|GHOSTTY" || true
git diff --check
```

Verification results:

- `cargo test -p roastty render_state_row_c_abi`: 4 passed.
- `cargo test -p roastty render_state_c_abi`: 3 passed.
- `cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib`:
  passed.
- `cargo test -p roastty`: 1863 Rust tests passed, C harness passed, doc tests
  passed.
- Diff no-Ghostty check: no new `ghostty`/`Ghostty`/`GHOSTTY` references in the
  experiment diff.
- `git diff --check`: passed.

## Conclusion

Experiment 179 completes the render-state row iterator slice. The render-state
ABI can now snapshot viewport rows, bind iterators to stable owned row
snapshots, iterate rows in top-to-bottom order, expose raw row and dirty state,
expose row-local selection ranges, and mutate iterator snapshot dirty state
without mutating the terminal.

The next coherent slice is row-cell iteration: implement
`roastty_render_state_row_cells_t` lifecycle and cell iteration/getters against
the same snapshot model, including grapheme/style/hyperlink-aware cell data only
to the extent needed for the next reviewed experiment.
