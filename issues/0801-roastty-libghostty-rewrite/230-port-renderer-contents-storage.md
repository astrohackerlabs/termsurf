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

# Experiment 230: Port Renderer `Contents` Storage and Lifecycle

## Description

Begin porting the `Contents` cell-render-data builder from upstream
`renderer/cell.zig` — the structure that holds per-cell GPU data for the grid
and supports row-wise dirty tracking. `Contents` is the largest remaining piece
of `cell.zig`, so it is split:

- **This experiment (230):** the `Contents` struct and its storage lifecycle —
  fields, `resize`, `reset`, and the `bg_cell` accessor.
- **Experiment 231:** `set_cursor` / `get_cursor_glyph` (the reserved cursor
  lists).
- **Experiment 232:** `add` / `clear` and the `Key` / `CellType` mapping (row
  mutation).

This fits the risk-based sizing rule: one coherent surface (the storage skeleton
in the existing `renderer::cell` module), predictable tests (allocation,
indexing, reset), one mechanism, localized failure.

### Structure and dependencies

`Contents` holds:

- `size: GridSize` (`renderer::size::GridSize`, Experiment 224), default `0×0`;
- `bg_cells: Vec<CellBg>` — flat background-color array, indexed
  `bg_cells[row * columns + col]` (`CellBg` from `renderer::shader`);
- `fg_rows: Vec<Vec<CellTextVertex>>` — the foreground cells per row
  (`CellTextVertex` from `renderer::shader`, the analog of upstream
  `shaderpkg.CellText`).

Upstream uses an `ArrayListCollection(CellText)` for `fg_rows`; in Rust that is
idiomatically a `Vec<Vec<CellTextVertex>>`, where upstream's
`clearRetainingCapacity` is Rust's `Vec::clear` (which retains capacity). No
separate `ArrayListCollection` port is needed.

The `fg_rows` layout is `rows + 2` lists: index `0` and index `rows + 1` are
**reserved for the cursor** (they must be first and last in the GPU buffer), and
indices `1..=rows` hold the real rows. This layout is established by `resize`
here and consumed by `set_cursor`/`add` in later slices.

### Behavior to port

- `resize(&mut self, size: GridSize)`: set `size`; allocate `bg_cells` of
  `columns * rows` entries all zeroed (`CellBg([0, 0, 0, 0])`); build `fg_rows`
  with `rows + 2` empty lists — the two cursor lists (`0` and `rows + 1`) with
  capacity `1`, the rest with capacity `columns * 3` (room for a glyph plus an
  underline plus a strikethrough per column, avoiding reallocation in the common
  case). Always fully invalidates. Upstream's `errdefer`/swap guards alloc
  failure; in Rust the new buffers are built into locals and only then moved
  into `self` — and `self.size` is assigned **together with**
  `bg_cells`/`fg_rows` after local construction (not before), so there is no
  window of half-updated state. (`Vec` allocation aborts rather than returns,
  matching the realistic path.) All `fg_rows` sizing arithmetic casts to `usize`
  first (`rows as usize + 2`, the cursor index `rows as usize + 1`, and
  `columns as usize * 3`) to avoid accidental `u16` overflow.
- `reset(&mut self)`: zero every `bg_cells` entry and `clear()` every `fg_rows`
  list (retaining capacity), without resizing.
- `bg_cell(&self, row, col) -> &CellBg` and
  `bg_cell_mut(&mut self, row, col) -> &mut CellBg`: index
  `row * columns + col`. Upstream exposes a single mutable `bgCell`; Rust splits
  the shared and mutable borrows. These replace direct indexing to centralize
  the index arithmetic.

### Scope and faithfulness notes

- `size.columns`/`size.rows` are `u16` (`renderer::size::Unit`); index math
  casts to `usize` (`row * columns as usize + col`), matching upstream's `usize`
  cell count.
- Rust `Vec` `Drop` replaces upstream `deinit`; no explicit `deinit` is needed.
- Do **not** port `set_cursor`, `get_cursor_glyph`, `add`, `clear`, `Key`, or
  `CellType` — those are Experiments 231–232.
- `Contents` and its methods are `pub(crate)`; fields stay private to the module
  (accessed via methods and, in tests, within the module).
- No C ABI, header, or ABI inventory changes; no new dependencies.

## Changes

1. Extend `roastty/src/renderer/cell.rs`:
   - `use super::shader::{CellBg, CellTextVertex};` and
     `use super::size::GridSize;`.
   - Add
     `pub(crate) struct Contents { size: GridSize, bg_cells: Vec<CellBg>, fg_rows: Vec<Vec<CellTextVertex>> }`
     with a `Default` (size `0×0`, empty vecs).
   - Implement `resize`, `reset`, `bg_cell`, and `bg_cell_mut` as above.

2. Tests in `renderer/cell.rs`:
   - `contents_resize_allocates`: resize to `3×2` → `bg_cells.len() == 6`, all
     `CellBg([0,0,0,0])`; `fg_rows.len() == 4` (`rows + 2`).
   - `contents_resize_capacity_layout`: after resize to `3×2`, real rows
     (indices `1..=rows`) have `capacity() >= columns * 3`, and the two cursor
     lists (`0` and `rows + 1`) have a smaller capacity than a real row (they
     are the cursor-sized lists), proving the upstream capacity layout.
   - `contents_bg_cell_indexing`: write through `bg_cell_mut(1, 2)`, read back
     through `bg_cell(1, 2)`; confirm it maps to flat index `1 * 3 + 2 = 5`.
   - `contents_reset_zeroes_bg`: write a non-zero bg cell, `reset`, all entries
     zero again; `fg_rows` length unchanged.
   - `contents_reset_clears_fg_rows`: directly push a dummy `CellTextVertex`
     into a real row (`fg_rows[1]`) and a cursor-reserved row (`fg_rows[0]`),
     call `reset`, and assert every `fg_rows` list is empty. (Tests within the
     module can manipulate the private fields directly; this does not need
     `add`.) This proves `reset` actually clears the foreground lists, not just
     the backgrounds.
   - `contents_resize_zero_sized`: resize to `0×0` → `bg_cells` empty,
     `fg_rows.len() == 2` (the two cursor lists), no panic.
   - `contents_resize_reinvalidates`: resize, write a bg cell, resize again to
     the same size, the written cell is zero again.

3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo test -p roastty renderer::cell
cargo test -p roastty renderer
cargo test -p roastty
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/renderer/cell.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Contents` stores `size`, `bg_cells`, and `fg_rows` with the upstream
  `rows + 2` cursor-reserved `fg_rows` layout;
- `resize` allocates and zeroes correctly and fully invalidates; `reset` zeroes
  bg and clears fg lists; `bg_cell`/`bg_cell_mut` index `row * columns + col`;
- the storage/indexing/reset tests pass, including the zero-sized case;
- `set_cursor`/`get_cursor_glyph`/`add`/`clear`/`Key` are not pulled in;
- no C ABI, header, or ABI inventory changes;
- `cargo fmt` accepted and `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the `fg_rows` capacities or cursor-list layout
turn out to need adjustment once `set_cursor`/`add` are designed.

The experiment **fails** if the `fg_rows` layout omits the two reserved cursor
lists, if `bg_cell` index math diverges from upstream, if `resize` leaves
half-updated state on the failure path, or if any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation.

Review artifacts:

- Prompt: `logs/codex-review/20260602-075339-636772-prompt.md`
- Result: `logs/codex-review/20260602-075339-636772-last-message.md`

Codex confirmed the storage shape is faithful (flat zeroed `bg_cells`, `fg_rows`
of `rows + 2` with cursor-reserved lists at `0` and `rows + 1`, real rows at
`1..=rows`), that `Vec<Vec<CellTextVertex>>` and `Vec::clear` are the right
analogs of `ArrayListCollection` / `clearRetainingCapacity`, that building
locals before replacing `self` is the right swap analog, and that splitting
`bg_cell`/`bg_cell_mut` preserves the upstream index formula.

Findings fixed in the design above before this commit:

1. **(Medium)** the `reset` tests only checked `fg_rows` length — added
   `contents_reset_clears_fg_rows`, which pushes dummy vertices into a real and
   a cursor row and asserts every list is empty after `reset`.
2. **(Low)** the capacity layout was untested — added
   `contents_resize_capacity_layout` (real rows `>= columns * 3`, cursor lists
   smaller).
3. **(Low)** specified that `fg_rows` sizing casts to `usize` first (`rows + 2`,
   `rows + 1`, `columns * 3`), and that `self.size` is assigned with the buffers
   after local construction.

## Result

**Result:** Pass

Extended `roastty/src/renderer/cell.rs` with the `Contents` struct
(`size: GridSize`, `bg_cells: Vec<CellBg>`, `fg_rows: Vec<Vec<CellTextVertex>>`,
`#[derive(Default)]`) and `resize`, `reset`, `bg_cell`, `bg_cell_mut`. Added
`use super::shader::{CellBg, CellTextVertex};` and `use super::size::GridSize;`,
and updated the module doc comment to cover the cell-data builder.

`resize` casts `columns`/`rows` to `usize`, allocates `bg_cells` of
`columns * rows` zeroed entries, builds `rows + 2` `fg_rows` lists (cursor lists
`0` and `rows + 1` with capacity 1, real rows with capacity `columns * 3`), and
assigns `size`/`bg_cells`/`fg_rows` together after local construction. `reset`
zeroes the backgrounds and `clear()`s each foreground list. `bg_cell`/
`bg_cell_mut` index `row * columns + col`.

Tests added (7): `contents_resize_allocates`, `contents_resize_capacity_layout`
(real rows `>= columns * 3`, cursor lists smaller), `contents_bg_cell_indexing`,
`contents_reset_zeroes_bg`, `contents_reset_clears_fg_rows` (pushes dummy
vertices into a real and a cursor row, asserts all empty after reset),
`contents_resize_zero_sized`, and `contents_resize_reinvalidates`.

### Verification

```bash
cargo fmt -p roastty
cargo test -p roastty renderer::cell
cargo test -p roastty renderer
cargo test -p roastty
```

Observed:

- `renderer::cell`: 27 passed (20 prior + 7 new).
- Full `roastty`: 2263 unit tests passed (2256 prior + 7 new), plus the C ABI
  harness passed.
- `cargo fmt -p roastty -- --check`: clean.
- `cargo build -p roastty`: no warnings.
- No-`ghostty`-name gates passed for `roastty/src/renderer/cell.rs` and for
  `roastty/src/lib.rs`, `roastty/include/roastty.h`,
  `roastty/tests/abi_harness.c`.
- `git diff --check`: clean.

No C ABI, header, or ABI inventory changes; `set_cursor`/`get_cursor_glyph`/
`add`/`clear`/`Key` not pulled in.

### Completion Review

Codex reviewed the completed implementation and found **no issues** ("nothing
should change before the result commit").

Review artifacts:

- Prompt: `logs/codex-review/20260602-075742-705921-prompt.md`
- Result: `logs/codex-review/20260602-075742-705921-last-message.md`

Codex confirmed `resize` matches upstream storage behavior (zeroed `bg_cells`,
`rows + 2` `fg_rows` with cursor lists at `0`/`rows + 1` capacity 1 and real
rows `columns * 3`, `usize` casts with no overflow path), that `reset` zeroes
backgrounds and clears each foreground list retaining capacity, that
`bg_cell`/`bg_cell_mut` use the upstream index formula, that the 7 tests are
sufficient, and that deriving `Default` (0×0, empty vecs) is the right
pre-resize analog.

## Conclusion

Experiment 230 succeeds. The `Contents` storage skeleton — `size`, `bg_cells`,
`fg_rows` with the `rows + 2` cursor-reserved layout, plus `resize`, `reset`,
and the `bg_cell` accessors — is now in `renderer::cell`. Both Codex gates
passed (three design findings fixed; zero result findings).

The remaining `Contents` slices build on this:

- **Experiment 231:** `set_cursor` / `get_cursor_glyph`, which use the reserved
  cursor lists (`fg_rows[0]` for block cursors drawn first, `fg_rows[rows + 1]`
  for the others drawn last) and depend on the cursor `Style` (Experiment 223).
- **Experiment 232:** `add` / `clear` and the `Key` / `CellType` mapping (which
  routes a cell to the `fg_rows[y + 1]` real-row list, the `+ 1` skipping the
  reserved cursor list).
