# Experiment 54: Port PageList Row Iterator

## Description

Port the upstream PageList row-iteration layer.

Upstream `PageList.zig` builds several higher-level traversal helpers on
`RowIterator`: `CellIterator` advances through cells by first stepping rows, and
prompt traversal relies on pins and directional movement over rows. Roastty
already has `PageIterator`, point-to-pin conversion, page chunks, and cell
lookup. This experiment should add the missing row iterator above
`PageIterator`, without also porting cell iteration, prompt iteration, diagrams,
semantic highlighting, selection, renderer delivery, or parser behavior.

This keeps the rewrite moving toward Ghostty parity while preserving the
one-layer-at-a-time PageList porting cadence.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/PageList.zig` for:
     - `RowIterator`;
     - `PageList.rowIterator`;
     - `Pin.rowIterator`;
     - the interaction between `RowIterator` and `PageIterator.Chunk`.
   - Do not modify `vendor/ghostty/`.

2. Add an internal Rust `RowIterator<'a>`.
   - Store:
     - the underlying `PageIterator<'a>`;
     - the current `Option<PageChunk>`;
     - the current row offset within that chunk.
   - Implement `Iterator<Item = Pin>`.
   - Return row pins with `x = 0`, matching upstream row pins.
   - Keep the type private/internal for now.
   - Do not expose mutable row or page access through the iterator.

3. Preserve upstream direction semantics.
   - For `Direction::RightDown`:
     - start at the first chunk's `start`;
     - yield each row from `start` up to but not including `end`;
     - advance to the next chunk when the current chunk is exhausted.
   - For `Direction::LeftUp`:
     - start at the first chunk's `end - 1`;
     - yield each row down to `start`;
     - advance to the prior chunk when the current chunk is exhausted.
   - Use `PageIterator` as the source of page/chunk boundaries rather than
     duplicating cross-page traversal logic.

4. Add row-iterator constructors.
   - Add a private helper equivalent to upstream `Pin.rowIterator`, shaped as a
     `PageList` helper if that is cleaner in Rust borrowing terms.
   - Add `PageList::row_iterator(direction, top_left, bottom_left)`.
   - Match the existing `PageList::page_iterator` point handling:
     - resolve `top_left` with `pin`;
     - resolve explicit `bottom_left` with `pin`;
     - use `get_bottom_right(top_left.tag())` when no bottom-left point is
       supplied;
     - return an empty iterator if either endpoint cannot be pinned.
   - For `RightDown`, iterate from top-left toward bottom-left.
   - For `LeftUp`, iterate from bottom-left toward top-left.

5. Add tests.
   - `row_iterator` yields active rows from top to bottom for a single page.
   - `row_iterator` yields active rows from bottom to top for a single page.
   - `row_iterator` crosses page boundaries in both directions.
   - `row_iterator` honors a partial active top-left when the active screen
     starts below the first stored row.
   - `row_iterator` honors implicit history bounds in both directions:
     - `History` right-down iteration includes history rows and stops before
       active rows;
     - `History` left-up iteration includes history rows and stops before active
       rows.
   - `row_iterator` honors explicit bottom-left limits in both directions.
   - `LeftUp` explicit-limit coverage must include a final/top chunk whose
     `start > 0`, proving the iterator yields the boundary row once and then
     stops instead of underflowing, skipping it, or continuing past the limit.
   - `row_iterator` returns no rows for invalid/unpinnable endpoints.
   - yielded row pins have `x = 0`.
   - collected row pins convert back to the expected points with
     `point_from_pin` where appropriate.

6. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

7. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - iterator shape;
     - direction behavior;
     - boundary/limit behavior;
     - tests added;
     - verification command output summary;
     - independent result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `RowIterator` yields rows in upstream order for both directions;
- row iteration crosses pages using the existing `PageIterator` chunk logic;
- explicit and implicit bottom-left bounds match upstream behavior;
- history row iteration stops before active rows in both directions;
- `LeftUp` explicit-limit iteration includes a nonzero-start boundary row once
  and does not continue past it;
- invalid endpoints produce an empty iterator instead of panics;
- yielded row pins have `x = 0`;
- no cell iterator, prompt iterator, diagram, semantic highlighting, parser,
  renderer, app, public ABI, resize/reflow, selection, or search work is
  introduced;
- `cargo fmt`, targeted PageList tests, and full `cargo test -p roastty` pass;
- an independent agent reviews the experiment design and completed result and
  approves them, or all real findings are fixed.

The experiment is partial if:

- basic row iteration works, but a corner case around implicit bottom bounds or
  history/active coordinate conversion needs a follow-up experiment.

The experiment fails if:

- row iteration duplicates page traversal logic instead of building on
  `PageIterator`;
- row iteration yields rows in the wrong direction or skips/duplicates boundary
  rows;
- invalid points panic;
- row pins preserve caller `x` instead of normalizing to `x = 0`;
- the implementation expands into cell iteration, prompt iteration, diagram
  output, semantic highlighting, parser, renderer, app, ABI, resize/reflow,
  selection, or search work;
- tests or formatting fail.

## Result

**Result:** Pass

Added a private `RowIterator<'a>` that stores the underlying `PageIterator<'a>`,
current `PageChunk`, and current row offset. The iterator returns `Pin` values
with `x = 0`, matching upstream row-pin behavior, and keeps the traversal
surface internal to PageList.

`RowIterator` now preserves upstream direction behavior:

- `RightDown` starts at the current chunk's `start`, yields rows up to `end`,
  and advances through the next `PageIterator` chunk.
- `LeftUp` starts at the current chunk's `end - 1`, yields down to `start`, and
  preserves upstream's distinction between `start == 0` page-boundary movement
  and nonzero-start explicit-limit termination.

Added PageList constructors equivalent to upstream `Pin.rowIterator` and
`PageList.rowIterator`, shaped for Rust borrowing:

- `row_iterator_from_pin`;
- `empty_row_iterator`;
- `row_iterator`.

The public-facing behavior remains internal and builds on existing
`PageIterator` chunk logic instead of duplicating page traversal.

Tests cover:

- active single-page iteration in both directions;
- cross-page iteration in both directions;
- partial active top-left starts across page boundaries;
- implicit history bounds in both directions, stopping before active rows;
- explicit right/down limits;
- explicit left/up limits with a nonzero-start boundary row;
- invalid endpoint handling;
- `x = 0` row-pin normalization;
- row-pin conversion back to points via `point_from_pin`.

Verification:

- `cargo fmt`
- `cargo test -p roastty terminal::page_list` — 209 PageList tests passed, ABI
  harness filtered out
- `cargo test -p roastty` — 490 unit tests passed, ABI harness passed, doc-tests
  passed

Independent result review approved the experiment as a Pass with no required
code or test findings. The review specifically confirmed that the Rust
`RowIterator` state machine matches upstream, including the `LeftUp`
nonzero-start boundary behavior, and that the scope stayed PageList-local.

## Conclusion

Experiment 54 completed the PageList row-iteration layer. Roastty now has the
row traversal primitive that upstream uses as the base for cell iteration and
other higher-level PageList traversal features.

The next PageList experiment can build on this by porting `CellIterator` as a
separate focused layer, while continuing to defer prompt iteration, semantic
highlighting, diagrams, parser, renderer, app, and ABI work.
