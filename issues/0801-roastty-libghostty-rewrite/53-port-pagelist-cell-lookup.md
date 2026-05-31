# Experiment 53: Port PageList Cell Lookup

## Description

Port the upstream PageList cell-read helper surface around `getCell`, plus the
small `totalPages` test/debug helper.

Roastty already has point-to-pin conversion and dirty helpers, but it does not
yet expose the upstream cell lookup object that ties a point to its page, row,
cell, row index, and column index. This experiment should add that read-only
PageList cell view and use it to align dirty/style/screen-point test helpers
with upstream.

This is still PageList-only work. It must not implement diagrams, semantic
highlighting, prompt iteration, row/cell iterators, parser integration, renderer
delivery, app behavior, or public ABI.

## Changes

1. Re-read upstream source.
   - Use `vendor/ghostty/src/terminal/PageList.zig` as the source of truth for:
     - `PageList.getCell`;
     - `PageList.Cell`;
     - `PageList.totalPages`;
     - `PageList.isDirty` and `markDirty` as consumers of cell/pin lookup.
   - Do not modify `vendor/ghostty/`.

2. Add a Rust `PageListCell<'a>` view.
   - Store:
     - a safe borrowed owner (`&'a Node`, or `&'a Page` plus a node pointer if
       identity is still needed);
     - a borrowed `Row`;
     - a borrowed `Cell`;
     - `row_idx`;
     - `col_idx`.
   - If node identity is needed for comparison or screen-point lookup, store
     `node_ptr: NonNull<Node>` in addition to the safe owner borrow.
   - Keep the type private/internal for now.
   - The view must be read-only. Do not expose mutable row/cell references.
   - Do not extend the lifetime beyond the immutable `PageList` borrow.
   - Do not require unsafe raw-node dereference for `style`, `is_dirty`, or
     `screen_point`; use the safe owner borrow for those operations.

3. Add `PageList::get_cell`.
   - Input: `point::Point`.
   - Output: `Option<PageListCell<'_>>`.
   - Return `None` for out-of-bounds coordinates or points that cannot be
     pinned.
   - Resolve the point through the existing `pin` path.
   - Retrieve the row and cell from the resolved page using existing `Page`
     accessors.
   - Preserve upstream's slow/debug-helper semantics; this must not become a
     hot-path abstraction or public ABI.

4. Add helper methods on `PageListCell`.
   - `is_dirty(&self) -> bool`: true when the owning page is dirty or the row is
     dirty, matching upstream.
   - `style(&self) -> style::Style`: default style when the cell has the default
     style id, otherwise lookup through the owning page.
   - `screen_point(&self, list: &PageList) -> point::Point`: compute the screen
     coordinate by summing rows from earlier pages, matching upstream's
     expensive/debug-only behavior.
   - If Rust borrowing makes `style` or `screen_point` cleaner as `PageList`
     methods that take `PageListCell`, that is acceptable, but preserve the
     upstream behavior and keep the API internal.

5. Add `PageList::total_pages`.
   - Return `self.pages.len()`.
   - Keep it internal and test/debug oriented.
   - Prefer this helper in new tests where page count is part of upstream
     behavior.

6. Update existing dirty helpers only if it improves alignment.
   - `is_dirty` may be reimplemented through `get_cell`.
   - `mark_dirty` may continue using `pin` directly because it mutates.
   - Do not change dirty behavior beyond the upstream-aligned lookup path.

7. Add tests.
   - `get_cell` returns correct cell contents and row/column indexes for:
     - active points;
     - screen points;
     - history points when history exists;
     - points across page boundaries.
   - `get_cell` returns `None` for:
     - `x >= cols`;
     - a point beyond the available rows in the requested coordinate space.
   - `PageListCell::is_dirty` is true when the page is dirty or the row is dirty
     and false when both are clean.
   - `PageListCell::style` returns default style for default cells and resolves
     stored styles for styled cells.
   - `PageListCell::screen_point` returns the correct screen coordinate across
     multiple pages and partial active starts.
   - `total_pages` returns the number of pages before and after growth/pruning
     cases already supported by PageList.

8. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

9. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - cell view shape;
     - lookup behavior;
     - dirty/style/screen-point behavior;
     - page-count behavior;
     - verification command output summary;
     - independent result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `get_cell` resolves active, screen, and history points correctly;
- `get_cell` returns `None` for out-of-bounds or unpinnable points;
- the cell view exposes the correct node, row, cell, row index, and column index
  without mutable access;
- dirty, style, and screen-point helpers match upstream semantics;
- `total_pages` reports the current page count;
- no diagram, semantic highlighting, prompt iterator, row/cell iterator, parser,
  renderer, app, public ABI, resize/reflow, selection, or search work is
  introduced;
- `cargo fmt`, targeted PageList tests, and full `cargo test -p roastty` pass;
- an independent agent reviews the experiment design and completed result and
  approves them, or all real findings are fixed.

The experiment is partial if:

- basic lookup works, but the style or screen-point helper exposes a borrowing
  issue that should be split into a narrower follow-up.

The experiment fails if:

- `get_cell` panics instead of returning `None` for invalid points;
- the cell view can outlive the immutable PageList borrow;
- the cell view exposes mutable row or cell access;
- dirty/style/screen-point semantics diverge from upstream;
- the implementation expands into diagrams, semantic highlighting, iterators,
  parser, renderer, app, ABI, resize/reflow, selection, or search work;
- tests or formatting fail.

## Result

**Result:** Pass

Added a private read-only `PageListCell<'a>` view with a safe borrowed owner,
node identity pointer, borrowed row, borrowed cell, and row/column indexes. The
view provides upstream-aligned helpers for dirty state, style lookup, and
screen-point conversion without unsafe raw-node dereference.

`PageList::get_cell` now resolves points through the existing `pin` path and
returns `None` for unpinnable or out-of-bounds points. It supports active,
screen, and history coordinates, including existing upstream-preserving
no-history behavior. `PageList::is_dirty` now routes through `get_cell` while
preserving the Rust helper behavior of returning false for invalid points.

`PageList::total_pages` was added as a narrow internal debug/test helper that
returns `self.pages.len()`.

Tests cover:

- active, screen, and history lookup;
- lookup across page boundaries;
- invalid x and out-of-range row handling;
- dirty state from page/row dirtiness;
- default and stored style lookup;
- screen-point conversion across pages and partial active starts;
- page count before and after growth.

Verification:

- `cargo fmt -- roastty/src/terminal/page_list.rs`
- `cargo test -p roastty terminal::page_list` — 197 PageList tests passed, ABI
  harness filtered out
- `cargo test -p roastty` — 478 unit tests passed, ABI harness passed, doc-tests
  passed

Independent result review approved the experiment as a Pass with no required
findings. The review accepted the safe owner-borrow shape, lookup behavior,
dirty/style/screen-point semantics, `total_pages` scope, `is_dirty` routing, and
scope boundaries. It noted that `Pin::is_dirty` appears unused after the
`is_dirty` routing change, but this was non-blocking.

## Conclusion

Experiment 53 completed the PageList cell lookup/debug surface. Roastty now has
the read-only cell view needed by later debugging, iterator, and semantic
inspection work while keeping the API internal and borrow-safe.

The next PageList experiments can build above this by porting row/cell/prompt
iterators, diagram/debug output, or semantic highlighting in separate scoped
steps.
