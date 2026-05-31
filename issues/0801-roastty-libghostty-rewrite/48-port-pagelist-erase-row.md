# Experiment 48: Port PageList Erase Row

## Description

Port upstream PageList `eraseRow`.

Experiment 47 added `fixup_viewport`, the viewport adjustment helper used by
upstream erase paths. The next source-order mutation is the fast path
`eraseRow`: physically remove exactly one row, shift all following rows up by
one across page boundaries, leave page row counts unchanged, clear the final
rotated row, update tracked pins, and fix up viewport state.

This experiment should add `eraseRow` only. It must not implement
`eraseRowBounded`, `eraseHistory`, `eraseActive`, `eraseRows`, page deletion,
active regrowth, resize/reflow, scrollClear, row/cell/prompt iterators, parser
retry loops, renderer dirty-region delivery, or public ABI.

## Changes

1. Re-read upstream source.
   - Use `vendor/ghostty/src/terminal/PageList.zig` as the source of truth for:
     - `PageList.eraseRow`;
     - erase-row tests, especially
       `PageList eraseRow invalidates viewport offset cache`;
     - tracked-pin erase tests only as future context, since most of them go
       through `eraseHistory` or `eraseActive`.
   - Do not modify `vendor/ghostty/`.

2. Add narrow Page helpers if needed.
   - Reuse Page's existing managed-memory-aware copy and cleanup paths.
   - If PageList needs row-level primitives, add only narrowly scoped
     `pub(super)` helpers, such as:
     - rotating row metadata left within a row range;
     - cloning one source row from another page into a specific destination row;
     - clearing one row through the existing `Page::clear_cells` path.
   - These helpers must preserve styles, graphemes, hyperlinks, string-backed
     data, row flags, and dirty state correctly.
   - Do not expose raw row/cell storage more broadly than `eraseRow` requires.

3. Add Rust erase-row error behavior.
   - Add a narrow error type if existing PageList errors do not fit.
   - Cross-page cloning can fail if the destination page lacks managed-memory
     capacity. Match upstream by returning an error from `erase_row`; do not
     pretend the erase succeeded.
   - Upstream does not roll back earlier row rotations or pin updates after a
     cross-page clone failure. Rust may either match that behavior exactly or
     provide a stronger rollback, but the chosen behavior must be explicit in
     the implementation comments and result.
   - In either case, the error path must not leave dangling style, grapheme,
     hyperlink, string, or pin references. If the error path is practically
     testable, add a test; if not, document why the existing capacity model
     makes the path unreachable in current tests.

4. Add `PageList::erase_row`.
   - Input should be a `point::Point`.
   - Resolve the point through existing `PageList::pin`; if the point does not
     resolve, return an error or no-op consistently with current Rust PageList
     internal-method style.
   - Rotate rows in the target page from the erased row through the end of that
     page so later rows move up by one.
   - Update tracked pins in the target page:
     - pins below the erased row move up one row;
     - pins at or above the erased row remain unchanged, matching upstream.
   - Call `fixup_viewport(1)` after the first-page pin update, matching upstream
     ordering.
   - Mark every page touched by row shifting as page-level dirty.
   - For each following page:
     - clone that page's top row into the bottom row of the previous page;
     - rotate that following page's rows left by one;
     - update tracked pins in that following page: row-0 pins move to the
       previous page's final row, other pins move up one row.
   - Clear the final row of the final touched page through Page's existing
     managed-memory cleanup path.
   - Preserve page row counts, page order, total row count, page_size, and page
     serials.
   - Verify PageList integrity after mutation.

5. Add tests.
   - Single-page erase:
     - fill all rows with distinct visible data;
     - erase a middle row;
     - verify following rows shifted up, final row is blank, page row count and
       total rows are unchanged, and the page is dirty.
   - Multi-page erase:
     - create at least two pages with distinct row data;
     - erase a row from the first page;
     - verify rows shift across the page boundary and the final row of the last
       page is blank.
   - Tracked pins:
     - pin below the erased row in the target page shifts up by one;
     - pin above the erased row in the target page is unchanged;
     - pin in row 0 of a following page moves to the previous page's final row;
     - pin below row 0 in a following page shifts up by one.
   - Viewport cache:
     - reproduce the upstream `eraseRow invalidates viewport offset cache`
       behavior using existing scroll helpers;
     - erase a row before the pinned viewport;
     - verify `scrollbar()` reports the cached offset decremented by one.
   - Managed memory:
     - include style, grapheme, hyperlink, and string-backed data in the erased
       row itself, then verify the erased row's data is released from the page
       after shifting;
     - include style, grapheme, and hyperlink data in rows that move across the
       erased row;
     - include managed-memory data in a boundary row that is cloned from the
       next page into the previous page;
     - verify moved data survives at its new row;
     - verify the final blank row does not retain managed-memory references.
   - Accounting:
     - verify page row counts, total rows, page_size, page serials, and page
       order are unchanged.

6. Preserve scope.
   - Do not implement:
     - `eraseRowBounded`;
     - `eraseHistory`, `eraseActive`, or `eraseRows`;
     - page deletion or active regrowth;
     - resize/reflow;
     - scrollClear;
     - row/cell/prompt iterators;
     - parser retry loops;
     - renderer or app integration;
     - public C ABI additions.
   - Do not add `ghostty` names except when citing upstream paths or test
     provenance.

7. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

8. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - erase-row behavior implemented;
     - row-shift and cross-page shift behavior;
     - tracked-pin behavior;
     - viewport fixup/cache behavior;
     - managed-memory behavior;
     - verification command output summary;
     - independent result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- erasing one row physically shifts following rows up by one;
- cross-page shifting copies the next page's top row into the previous page's
  bottom row;
- the final row of the final touched page is cleared and releases managed
  memory;
- the erased row's own style, grapheme, hyperlink, and string-backed data is
  released rather than leaked by rotation;
- cross-page clone failure behavior is explicit, returns an error, and does not
  leave dangling managed-memory or pin references;
- page row counts, page order, total rows, page_size, and page serials are
  unchanged;
- touched pages are marked dirty;
- tracked pins update exactly like upstream for target-page and following-page
  cases;
- pinned viewport cached offset behavior matches upstream after erasing a row
  before the viewport;
- style, grapheme, hyperlink, and string-backed data survive movement and are
  released from cleared rows;
- no bounded erase, history/active erase, eraseRows, page deletion, active
  regrowth, resize/reflow, scrollClear, iterator, parser, renderer, app, or ABI
  work is introduced;
- `cargo fmt`, targeted PageList tests, and full `cargo test -p roastty` pass;
- an independent agent reviews the experiment design and completed result and
  approves them, or all real findings are fixed.

The experiment is partial if:

- basic single-page erasure works, but cross-page managed-memory movement
  exposes a missing Page primitive that should be designed as a separate
  Page-level experiment.

The experiment fails if:

- erased rows are only cleared rather than physically removed;
- row data, dirty state, or managed memory is lost during shifting;
- tracked pins or viewport state diverge from upstream semantics;
- row counts, total rows, page_size, serials, or page order change;
- the implementation expands into bounded/history/active erase or page deletion;
- tests or formatting fail.

## Result

**Result:** Pass

Implemented `PageList::erase_row` for one-row physical removal semantics. The
new path resolves the target point to a pin, rotates target-page row metadata
left from the erased row, updates tracked pins, calls `fixup_viewport(1)` in the
upstream order, shifts following pages across page boundaries by cloning each
next page's first row into the previous page's final row, rotates following-page
rows, clears the final row of the last touched page, marks touched pages dirty,
and verifies PageList integrity after mutation.

Narrow Page helpers were added for this operation:

- `Page::clone_row_from` is now `pub(super)` so PageList can copy a whole row
  through Page's existing managed-memory-aware clone path.
- `Page::rotate_rows_left` rotates row metadata inside one page without exposing
  raw storage more broadly.
- `Page::cell_offset_at` now derives the cell offset from the logical row's
  `cells()` offset instead of assuming `y * cols`. This is required once row
  metadata can rotate, because logical row `y` may legitimately point at a
  different physical cell range.

The implementation returns `EraseRowError::InvalidPoint` when the input point
does not resolve and `EraseRowError::CloneFrom` if a cross-page clone fails. It
does not add rollback after earlier rotations or pin updates, matching upstream
behavior. Existing Page clone failure tests cover the managed-memory rollback
inside failed clone operations, and `erase_row` verifies PageList integrity on
successful mutation.

Tests added:

- single-page erase shifts following rows up, clears the final row, preserves
  accounting, and marks the page dirty;
- cross-page erase pulls the next page's top row into the previous page's final
  row, rotates the next page, preserves page order/accounting, and marks both
  pages dirty;
- tracked pins above, at, and below the erased row follow upstream semantics,
  including following-page row-0 pins moving to the previous page's final row;
- pinned viewport cached offset decrements after erasing a row before the
  viewport;
- erased-row style, grapheme, hyperlink, and string-backed data is released;
- boundary style, grapheme, and hyperlink data survives the cross-page clone and
  is released from the source page after rotation/clear;
- row-level dirty flags survive target-page shifts, cross-page boundary clone,
  and following-page rotation.

Verification:

- `cargo fmt -- roastty/src/terminal/page_list.rs`
- `cargo test -p roastty terminal::page_list` — 159 PageList tests passed.
- `cargo test -p roastty` — 440 unit tests passed; ABI harness passed.

Independent result review initially found two real test gaps: a missing tracked
pin exactly at the erased-row boundary, and missing row-level dirty preservation
coverage. Both were fixed. The follow-up review approved recording Experiment 48
as Pass with no concrete blockers remaining.

## Conclusion

PageList now has the upstream-style one-row erase primitive needed by later
bounded/history/active erase operations. The most important implementation
lesson is that row metadata rotation changes the invariant for logical row cell
addressing: helpers must follow each row's stored cell offset rather than derive
physical offsets from row indexes. The next experiment can build on this by
porting the next source-order PageList erase operation without reopening the
row-rotation or managed-memory mechanics.
