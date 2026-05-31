# Experiment 45: Port PageList Compact

## Description

Port upstream PageList `compact`.

Experiment 44 added the replacement-node mechanics needed to grow managed
memory. The next smaller source-order PageList operation is compaction: replace
an oversized page with a smaller exact-capacity clone when doing so actually
saves memory. This builds directly on earlier Page exact-row-capacity work and
on the node replacement/pin remapping patterns from Experiment 44.

This experiment should add compaction only. It must not implement split,
erase/eraseRow, resize/reflow, scrollClear, row/cell/prompt iterators, parser
retry loops, renderer dirty-region delivery, or public ABI.

## Changes

1. Re-read upstream source.
   - Use `vendor/ghostty/src/terminal/PageList.zig` as the source of truth for:
     - `PageList.compact`;
     - `PageList compact std_size page returns null`;
     - `PageList compact oversized page`;
     - `PageList compact insufficient savings returns null`;
     - compaction references in later split/resize tests only as future context.
   - Do not modify `vendor/ghostty/`.

2. Add `PageList::compact`.
   - Input should identify the target node.
   - If the target page is standard-size or smaller, return `Ok(None)`.
   - Compute the exact row capacity for the whole live page with
     `page.exact_row_capacity(0, page.size_rows())`.
   - Compute the replacement layout size with `page_layout(required_capacity)`.
   - If the replacement size is greater than or equal to the current backing
     length, return `Ok(None)`.
   - Allocate a replacement page with the required capacity.
   - Preserve page row/column size, page-level dirty state, row-level dirty
     state, text cells, styles, graphemes, hyperlinks, and string-backed data.
   - Update tracked pins whose node points at the old page to point at the
     replacement node.
   - Preserve viewport state. If `viewport_pin` points at the old node, it must
     point at the replacement node after compaction.
   - Preserve page order and PageList accounting.
   - Verify integrity after successful compaction.
   - If allocation fails, return an allocation error and leave the PageList
     unchanged.
   - If clone/copy unexpectedly fails after allocation but before installation,
     roll back temporary accounting and return `Ok(None)`, matching upstream's
     graceful "pretend compaction was not needed" fallback.

3. Add or reuse narrow helpers.
   - Reuse Experiment 44's replacement-node patterns where they fit.
   - Do not create a broader generic page-replacement abstraction unless it
     removes real duplication without obscuring rollback behavior.
   - Do not widen Page/PageList visibility beyond what compaction needs.

4. Add tests.
   - Standard-size page:
     - compact a fresh page;
     - verify it returns `None`;
     - verify the node, `page_size`, serials, and integrity are unchanged.
   - Oversized page:
     - make a page oversized via `increase_capacity`;
     - write visible cell data across live rows;
     - track a pin on the compacted page;
     - set page-level dirty state;
     - set at least two row-level dirty bits, leaving neighboring rows clean;
     - compact the page;
     - verify a replacement node is returned;
     - verify replacement backing is smaller than the oversized backing;
     - verify size, data, page-level dirty state, row-level dirty state, tracked
       pin, page order, `page_size`, and integrity.
   - Managed-memory exactness:
     - include style, grapheme, and hyperlink data that must survive compaction;
     - verify copied managed data survives and trimmed unused capacity is not
       retained beyond what exact row capacity requires.
   - Viewport pin:
     - compact a page containing `viewport_pin`;
     - verify `viewport_pin` points at the replacement node.
   - Insufficient savings:
     - make a page slightly oversized if possible;
     - compact it;
     - accept either `None` if exact capacity is not smaller, or a smaller
       replacement if the exact capacity does save memory;
     - in both cases verify integrity and no dangling pins.

5. Preserve scope.
   - Do not implement:
     - split;
     - erase/eraseRow;
     - resize/reflow;
     - scrollClear;
     - row/cell/prompt iterators;
     - parser retry loops;
     - renderer or app integration;
     - public C ABI additions.
   - Do not add `ghostty` names except when citing upstream paths or test
     provenance.

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
     - compaction behavior implemented;
     - standard-size no-op behavior;
     - oversized replacement behavior;
     - pin and viewport remapping behavior;
     - dirty and managed-memory preservation;
     - verification command output summary;
     - independent result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- standard-size pages are not compacted;
- oversized pages compact to smaller exact-capacity replacement nodes when exact
  capacity saves memory;
- page data, managed memory, page-level dirty state, and row-level dirty state
  survive compaction;
- tracked pins and the viewport pin are remapped from the old node to the
  replacement node;
- page order, `page_size`, serial accounting, total rows, and integrity remain
  valid;
- allocation or copy failure before installation leaves the PageList unchanged
  or gracefully returns `None` without dangling pointers;
- no split, erase, resize/reflow, scrollClear, iterator, parser, renderer, app,
  or ABI work is introduced;
- `cargo fmt`, targeted PageList tests, and full `cargo test -p roastty` pass;
- an independent agent reviews the experiment design and completed result and
  approves them, or all real findings are fixed.

The experiment is partial if:

- standard-size/no-op behavior and basic compaction work, but managed-memory
  exactness exposes a Page-level capacity bug that needs a follow-up.

The experiment fails if:

- compaction loses row data, dirty state, or managed memory;
- compaction leaves tracked pins or viewport pins pointing at the dropped old
  node;
- compaction corrupts page order, `page_size`, serials, or total row accounting;
- no-op compaction mutates the PageList;
- the implementation expands into unrelated PageList operations;
- tests or formatting fail.

## Result

**Result:** Pass

Implemented `PageList::compact` in `roastty/src/terminal/page_list.rs`.
Compaction now:

- returns `Ok(None)` for standard-size pages and for oversized pages where exact
  capacity does not save memory;
- computes exact live-row capacity with `Page::exact_row_capacity`;
- allocates a replacement page, preserves rows, columns, cells, styles,
  graphemes, hyperlinks, page-level dirty state, and row-level dirty state;
- replaces the old node in place when compaction succeeds;
- remaps tracked pins and `viewport_pin` from the old node to the replacement
  node;
- preserves page order and `page_size` accounting; and
- verifies PageList integrity after successful replacement.

The tests added coverage for standard no-op compaction, oversized compaction,
managed-memory exactness, viewport remapping, insufficient-savings safety, and
multi-page page-order preservation.

Verification:

```bash
cargo fmt && cargo test -p roastty terminal::page_list
```

Result: 129 PageList tests passed.

```bash
cargo test -p roastty
```

Result: 410 unit tests passed, plus the ABI harness passed.

Independent result review: Codex reviewer approved recording Experiment 45 as
Pass with no findings. The reviewer specifically confirmed that the earlier
multi-page page-order test gap is covered by
`page_list_compact_multi_page_preserves_order`.

## Conclusion

PageList compaction is now ported for the currently implemented PageList
surface. The implementation follows the same replacement-node pattern as
capacity growth while shrinking oversized backing storage to exact live-row
capacity and preserving externally tracked pointers through remapping.

The next experiment should continue with the next upstream PageList operation in
source order, keeping the same design-review, implementation, verification, and
result-review cadence.
