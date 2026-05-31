# Experiment 22: Port Page Exact Row Capacity

## Description

Port upstream `Page.exactRowCapacity` to Roastty.

The Page storage and row-copy substrates now cover plain cells, graphemes,
styles, and hyperlinks. Upstream Ghostty uses `exactRowCapacity` to compute the
smallest Page capacity needed to hold a row range before cloning or splitting
rows. Roastty currently has capacity adjustment and row-copy behavior, but it
cannot yet compute exact metadata capacities for a selected row range.

This experiment should add exact row capacity calculation and tests. It should
not add `clonePartialRowFrom`, reflow, screen splitting, terminal parser
behavior, or public ABI.

## Changes

1. Re-read upstream source.
   - Use `vendor/ghostty/src/terminal/page.zig` as the source of truth for:
     - `Page.exactRowCapacity`;
     - the style, grapheme, hyperlink, and string byte counting logic;
     - tests from `Page exactRowCapacity ...`.
   - Use the current Roastty Page layout constants and helpers rather than
     inventing new layout arithmetic.
   - Do not modify `vendor/ghostty/`.

2. Add `Page::exact_row_capacity`.
   - Add an internal method:

     ```rust
     fn exact_row_capacity(&self, y_start: usize, y_end: usize) -> Capacity
     ```

   - Match upstream preconditions:
     - `y_start < y_end`;
     - `y_end <= self.size.rows`.
   - Return a `Capacity` with:
     - `cols = self.size.cols`;
     - `rows = y_end - y_start`;
     - `styles` sized for unique style IDs in the row range;
     - `grapheme_bytes` equal to the sum of allocated grapheme chunk bytes
       required by grapheme cells in the range;
     - `hyperlink_bytes` sized for both unique hyperlinks and hyperlink map cell
       capacity;
     - `string_bytes` equal to the sum of URI and explicit-ID string allocation
       chunks required by unique hyperlinks in the range.

3. Count style IDs.
   - Count unique non-default style IDs in the selected row range.
   - Use a deterministic local set keyed by `style::Id`. A fixed `bool`/bitset
     array over `CellCountInt` is acceptable because upstream uses a static
     bitset and the ID space is `u16`.
   - Convert the unique style count with `style::Set::capacity_for_count`.
   - Do not count styles outside the requested row range.

4. Count grapheme bytes.
   - For every cell with grapheme data in the selected row range:
     - look up the stored grapheme slice;
     - add `GraphemeAlloc::bytes_required::<u32>(slice.len())`.
   - Do not count graphemes outside the requested row range.
   - Empty rows should report zero grapheme bytes.

5. Count hyperlinks and strings.
   - Count hyperlink cells and unique hyperlink IDs separately.
   - For every unique hyperlink ID in the row range:
     - add `StringAlloc::bytes_required::<u8>(entry.uri.len())`;
     - if the ID is explicit, add
       `StringAlloc::bytes_required::<u8>(explicit_id.len())`.
   - Do not double-count strings for the same hyperlink ID used by multiple
     cells.
   - Do not count hyperlinks outside the requested row range.

6. Compute hyperlink bytes.
   - Match upstream's dual constraint:
     - the hyperlink set must hold the unique hyperlink entries;
     - the hyperlink map must hold every linked cell.
   - Compute:

     ```rust
     unique_set_cap = hyperlink::Set::capacity_for_count(unique_count)
     map_min = hyperlink_cells.div_ceil(HYPERLINK_CELL_MULTIPLIER)
     hyperlink_cap = unique_set_cap.max(map_min)
     hyperlink_bytes = hyperlink_cap * HYPERLINK_SET_ITEM_SIZE
     ```

   - This is required because `page_layout()` derives hyperlink map capacity
     from `hyperlink_bytes / HYPERLINK_SET_ITEM_SIZE` multiplied by
     `HYPERLINK_CELL_MULTIPLIER`.

7. Add focused tests.
   - Empty selected rows:
     - cols and rows are preserved;
     - styles, grapheme bytes, hyperlink bytes, and string bytes are zero.
   - Styles:
     - one unique style uses `style::Set::capacity_for_count(1)`;
     - repeated use of the same style does not increase capacity;
     - distinct styles increase capacity;
     - styles outside the selected range are ignored;
     - cloning into a Page initialized with the exact capacity succeeds and
       produces the same exact capacity.
   - Graphemes:
     - one codepoint rounds up to `GRAPHEME_CHUNK`;
     - multiple grapheme cells sum their chunk requirements;
     - a grapheme larger than one chunk rounds up correctly;
     - graphemes outside the selected range are ignored;
     - cloning into a Page initialized with the exact capacity succeeds.
   - Hyperlinks:
     - no hyperlinks reports zero hyperlink/string capacity;
     - one implicit hyperlink reports one unique set capacity and URI string
       bytes;
     - same hyperlink on multiple cells does not double-count string/set
       capacity;
     - explicit IDs add explicit-ID string bytes;
     - hyperlinks outside the selected range are ignored;
     - many cells sharing one hyperlink allocate enough `hyperlink_bytes` for
       hyperlink-map capacity, not only unique-set capacity;
     - cloning into a Page initialized with the exact capacity succeeds.
   - Mixed rows:
     - a row range containing style, grapheme, and hyperlink data returns a
       capacity that can initialize a destination Page and clone the range.
   - Preconditions:
     - empty range panics;
     - end beyond page size panics.

8. Preserve scope.
   - Do not implement:
     - `clonePartialRowFrom`;
     - terminal reflow or split page logic;
     - parser or screen behavior;
     - public ABI or app-facing APIs.
   - Do not change existing Page layout constants except to expose a helper
     needed by tests.

9. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal::page
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

10. Record the result.
    - Append `## Result` and `## Conclusion` to this file.
    - Include:
      - exact capacity API added;
      - counting strategy for styles, graphemes, hyperlinks, and strings;
      - tests added;
      - any deferred upstream Page methods;
      - verification command output summary.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `Page::exact_row_capacity` matches upstream semantics for row ranges;
- unique style IDs are counted once and converted through style set capacity;
- grapheme byte counts use allocator chunk sizing;
- unique hyperlinks are counted separately from hyperlink cells;
- hyperlink bytes are large enough for both hyperlink set entries and map cell
  capacity;
- URI and explicit-ID string bytes are counted once per unique hyperlink;
- data outside the selected row range is ignored;
- a Page initialized with the exact capacity can clone the selected row range
  for style, grapheme, hyperlink, and mixed-data cases;
- invalid ranges panic;
- existing Page layout, row-copy, style, grapheme, and hyperlink tests do not
  regress;
- `cargo fmt`, targeted Page tests, and full `cargo test -p roastty` pass;
- Codex reviews the completed result and approves it or all real findings are
  fixed.

The experiment is partial if:

- exact capacity works for styles and graphemes, but hyperlink/string capacity
  exposes a storage-layout mismatch that requires a focused follow-up.

The experiment fails if:

- capacity counts are based on total cells instead of unique style/hyperlink IDs
  where upstream deduplicates;
- hyperlink capacity only sizes the set and misses map capacity for many linked
  cells;
- string bytes are double-counted for repeated use of the same hyperlink;
- exact-capacity clone fails for a supported managed-memory type;
- row-copy, storage, or layout tests regress;
- out-of-scope partial-row, reflow, parser, screen, or ABI behavior is added.

## Codex Review

This experiment design must be reviewed by Codex before implementation. Any real
design issues must be fixed before committing the plan or implementing the
slice.

## Result

**Result:** Pass

Implemented `Page::exact_row_capacity(y_start, y_end)`.

The new method matches upstream row-range semantics:

- validates `y_start < y_end` and `y_end <= self.size.rows`;
- preserves current Page width and selected row count;
- counts unique non-default style IDs and converts the unique count through
  `style::Set::capacity_for_count`;
- sums grapheme allocator chunk bytes with
  `GraphemeAlloc::bytes_required::<u32>`;
- counts hyperlink cells separately from unique hyperlink IDs;
- computes hyperlink bytes from the larger of unique hyperlink set capacity and
  hyperlink map cell-capacity needs;
- counts URI and explicit-ID string allocation chunks once per unique hyperlink;
- ignores styles, graphemes, and hyperlinks outside the selected row range.

Tests added cover empty rows, invalid ranges, style deduplication and
out-of-range exclusion, grapheme chunk sizing including a multi-chunk grapheme,
implicit and explicit hyperlink string sizing, repeated hyperlink use,
hyperlink-map capacity for many cells sharing one hyperlink, mixed
managed-memory rows, exact-capacity clone success, and invalid-range panics.

Verification run:

```bash
cargo fmt
cargo test -p roastty terminal::page
cargo test -p roastty
```

Results:

- `cargo test -p roastty terminal::page`: 93 passed.
- `cargo test -p roastty`: 202 unit tests passed, ABI harness passed, doc tests
  passed.

## Conclusion

Roastty Page can now compute the exact capacity required for a selected row
range across styles, graphemes, hyperlinks, and Page-backed strings. This
matches the upstream sizing primitive needed before porting partial-row clone,
page splitting, or reflow-oriented behavior.
