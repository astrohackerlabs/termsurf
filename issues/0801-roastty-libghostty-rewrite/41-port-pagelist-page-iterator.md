# Experiment 41: Port PageList Page Iterator

## Description

Port upstream PageList `pageIterator` and its row-chunk semantics.

Experiment 40 finished `reset`. The next upstream PageList method is `clone`,
but `clone` depends immediately on `pageIterator`: it walks a tagged top-to-
bottom region as page-local row chunks, counts chunks first, then clones each
chunk. Roastty does not yet have PageList row/page iterators, so implementing
`clone` directly would combine the iterator contract, clone allocation,
partial-row cloning, tracked-pin remapping, and active-area padding in one
oversized experiment.

This experiment should port the PageList page-iterator foundation only. It
should produce the same chunk boundaries that upstream uses, while staying
read-only and allocation-free. The next experiment can then use this iterator to
implement PageList `clone`.

This remains PageList-local traversal work. It must not implement clone, row or
cell iterators, prompt iterators, erase, resize/reflow, dirty tracking,
screen/parser integration, renderer/app integration, or public ABI.

## Changes

1. Re-read upstream source.
   - Use `vendor/ghostty/src/terminal/PageList.zig` as the source of truth for:
     - `PageIterator`;
     - `PageIterator.next`;
     - `PageIterator.nextDown`;
     - `PageIterator.nextUp`;
     - `PageIterator.Chunk`;
     - `PageIterator.Chunk.fullPage`;
     - `PageIterator.Chunk.overlaps`;
     - `pageIterator`.
   - Use upstream clone tests only as context for why page chunks matter. Do not
     implement `clone` in this experiment.
   - Do not modify `vendor/ghostty/`.

2. Add iterator support types.
   - Add a private `Direction` enum with upstream's two directions: `RightDown`
     and `LeftUp`.
   - Add a private `PageChunk` struct containing:
     - `node: NonNull<Node>`;
     - `start: CellCountInt` inclusive;
     - `end: CellCountInt` exclusive.
   - Add helper methods on `PageChunk`:
     - `full_page(&self, list: &PageList) -> bool`;
     - `overlaps(&self, other: &PageChunk) -> bool`.
   - Do not expose these through the public C ABI.

3. Add `PageIterator`.
   - Add a private iterator struct that stores:
     - a reference to the `PageList`;
     - the current row pin, if any;
     - a row-limit pin, if any;
     - the direction.
   - Rust does not need upstream's `count` limit mode yet unless a direct test
     proves it is required. The public `pageIterator` path used by clone only
     needs no-limit and row-limit behavior.
   - The iterator should be read-only and allocate no memory.

4. Implement right-down iteration.
   - Starting at `tl_pt`, yield chunks moving downward through pages.
   - If no bottom point is supplied, stop at `get_bottom_right(tl_pt)`.
   - If the bottom point is on a later page, yield the current page from `start`
     to page size and continue at the next page.
   - If the bottom point is on the same page, yield from `start` through
     `limit.y` inclusive by returning exclusive `end = limit.y + 1`, then stop.
   - If `tl_pt` or `bot_pt` cannot be pinned, return an empty iterator.

5. Implement left-up iteration.
   - Starting from the bottom point, yield chunks moving upward through pages.
   - If no bottom point is supplied, start at `get_bottom_right(tl_pt)`.
   - If the top point is on an earlier page, yield the current page from `0` to
     `current.y + 1` and continue at the previous page.
   - If the top point is on the same page, yield from `limit.y` through
     `current.y` inclusive by returning exclusive `end = current.y + 1`, then
     stop.
   - Preserve upstream's chunk shape: left-up chunks still report `start <= end`
     row ranges.

6. Add `PageList::page_iterator`.
   - Signature should stay private to PageList tests for now.
   - Inputs:
     - direction;
     - top-left point;
     - optional bottom-left point.
   - It should:
     - convert points to pins using existing `pin`;
     - default the bottom pin with `get_bottom_right(tl_pt)` when no bottom
       point is supplied;
     - return an empty iterator if either endpoint is invalid;
     - for `RightDown`, start at the top pin and limit at the bottom pin;
     - for `LeftUp`, start at the bottom pin and limit at the top pin.
   - Do not add slow-runtime-safety assertions yet. If ordering validation is
     useful, add tests first and keep behavior deterministic in normal builds.

7. Add tests.
   - Full active region on one page:
     - iterate `RightDown` from active top with no bottom;
     - get one full-page chunk covering all active rows.
   - Trimmed right/down on one page:
     - use an explicit bottom point;
     - chunk ends at `bottom.y + 1`.
   - Trimmed left/top on one page:
     - start at `screen y=10` or equivalent after adding history;
     - first chunk starts at that row.
   - Both sides trimmed:
     - explicit top and bottom on the same page;
     - chunk covers exactly the inclusive range.
   - Cross-page right-down:
     - choose dimensions/history that produce multiple pages;
     - verify chunk sequence covers page-local ranges in order.
   - Cross-page left-up:
     - same setup as right-down;
     - verify chunk sequence moves from later page to earlier page while each
       chunk reports ascending row bounds.
   - History right-down default boundary:
     - create history that spans multiple pages;
     - iterate `RightDown` over `History` with no explicit bottom;
     - verify chunks include only history rows and stop before the active area.
   - History left-up default boundary:
     - use the same multi-page history setup;
     - iterate `LeftUp` over `History` with no explicit bottom;
     - verify chunks include only history rows in reverse page order and still
       stop before the active area.
   - Empty iterator:
     - invalid top or bottom point returns no chunks.
   - `PageChunk::full_page`:
     - true only when chunk covers `0..page.size_rows()`.
   - `PageChunk::overlaps`:
     - true for overlapping ranges on the same node;
     - false for disjoint ranges;
     - false for different nodes.

8. Preserve scope.
   - Do not implement:
     - PageList clone;
     - row/cell/prompt iterators;
     - erase/compact/split;
     - resize/reflow;
     - dirty tracking;
     - screen/parser integration;
     - renderer or app integration;
     - public C ABI additions.
   - Do not add `ghostty` names except when citing upstream paths or test
     provenance.

9. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

10. Record the result.
    - Append `## Result` and `## Conclusion` to this file.
    - Include:
      - iterator behavior implemented;
      - chunk boundary behavior;
      - right-down and left-up behavior;
      - invalid endpoint behavior;
      - tests added;
      - verification command output summary.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `PageList::page_iterator` yields upstream-shaped page-local row chunks for
  right-down and left-up traversal;
- omitted bottom points default to the bottom-right of the top point's tagged
  region;
- explicit bottom points are inclusive and represented with exclusive chunk
  ends;
- cross-page traversal yields page-local chunks in the correct direction;
- invalid endpoints produce an empty iterator without panicking;
- `PageChunk::full_page` and `PageChunk::overlaps` behave like upstream;
- no clone, erase, resize/reflow, dirty tracking, prompt scrolling,
  screen/parser, renderer/app, or ABI work is introduced;
- `cargo fmt`, targeted PageList tests, and full `cargo test -p roastty` pass;
- an independent agent reviews the experiment design and completed result and
  approves them, or all real findings are fixed.

The experiment is partial if:

- right-down traversal works, but left-up traversal reveals that the current
  Rust `Pin` navigation needs a separate preparatory experiment.

The experiment fails if:

- iterator chunks skip or duplicate rows;
- chunk bounds are not page-local;
- left-up chunks return inverted row ranges;
- invalid points panic in normal test paths;
- the implementation expands into PageList clone or other non-iterator work;
- tests or formatting fail.

## Result

**Result:** Pass

Experiment 41 implemented PageList page-iterator traversal as a private
PageList-local facility for the upcoming clone work.

The implementation adds:

- `Direction` with `RightDown` and `LeftUp`;
- `PageChunk` with page-local `start..end` row bounds;
- `PageChunk::full_page`;
- `PageChunk::overlaps`;
- a private allocation-free `PageIterator`;
- `PageList::page_iterator` with tagged point conversion and default bottom
  endpoints.

Right-down traversal yields page-local chunks from the top pin toward the bottom
pin. Left-up traversal starts from the bottom pin and moves toward the top pin
while preserving upstream's ascending row-bound shape inside each chunk.
Explicit bottom points are inclusive and represented with exclusive chunk ends.
Missing or invalid endpoints produce an empty iterator.

Tests now cover:

- full active-region iteration on one page;
- trimmed bottom bounds on one page;
- trimmed top bounds on one page;
- both-side trimming on one page;
- cross-page right-down traversal;
- cross-page left-up traversal;
- active-region cross-page partial-boundary traversal in both directions;
- history right-down default boundary stopping before active rows;
- history left-up default boundary stopping before active rows;
- invalid endpoint behavior;
- `PageChunk::full_page`;
- `PageChunk::overlaps`.

Verification passed:

```bash
cargo fmt -- roastty/src/terminal/page_list.rs
cargo test -p roastty terminal::page_list
cargo test -p roastty
```

The focused PageList suite passed with 96 tests. The full `roastty` package
passed with 377 unit tests plus the ABI harness test.

## Conclusion

PageList now has the row-chunk traversal primitive that upstream `clone` uses to
count and copy page-local row ranges. The iterator is private, read-only, and
allocation-free, and it preserves the important history-boundary behavior where
history iteration stops before the active area.

The next experiment can implement PageList `clone` on top of this iterator,
without also having to design the page-chunk traversal contract.
