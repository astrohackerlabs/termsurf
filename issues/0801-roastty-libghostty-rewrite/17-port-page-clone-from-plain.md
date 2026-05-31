# Experiment 17: Port Page CloneFrom Plain Rows

## Description

Port the plain-row fast path of Ghostty's `Page.cloneFrom`.

The next upstream Page tests after whole-page clone are:

- `Page cloneFrom`
- `Page cloneFrom shrink columns`
- `Page cloneFrom partial`

These tests exercise the non-managed-memory path: copy rows from one page into
another, preserve destination row cell offsets, truncate source columns when the
destination is narrower, preserve upstream row metadata rules, leave uncopied
destination rows alone, and keep source and destination independent after the
copy.

Ghostty's full `cloneFrom` also handles styles, graphemes, hyperlinks,
destination cleanup, partial row copy, and integrity checks. Roastty now has
style and grapheme storage, but it does not yet have hyperlink storage,
`clearCells`, or full row-copy managed-memory cleanup. This experiment should
therefore add the plain-row core without pretending that full `cloneFrom`
semantics are complete.

## Changes

1. Inspect upstream source.
   - Use `vendor/ghostty/src/terminal/page.zig` as the source of truth.
   - Re-read:
     - `Page.CloneFromError`
     - `Page.cloneFrom`
     - `Page.cloneRowFrom`
     - `Page.clonePartialRowFrom`
     - upstream tests:
       - `Page cloneFrom`
       - `Page cloneFrom shrink columns`
       - `Page cloneFrom partial`
   - Read the managed-memory branches only to define this experiment's explicit
     boundaries. Do not implement style, grapheme, or hyperlink cloning here.
   - Do not modify `vendor/ghostty/`.

2. Add a narrow plain-row clone API.
   - Add an internal Page method with a name that makes the temporary scope
     explicit, such as:

     ```rust
     fn clone_plain_rows_from(
         &mut self,
         other: &Page,
         y_start: usize,
         y_end: usize,
     ) -> Result<(), CloneFromError>
     ```

   - `y_start` and `y_end` match upstream semantics:
     - `y_start <= y_end`;
     - `y_end <= other.size.rows`;
     - `y_end - y_start <= self.size.rows`;
     - destination rows start at row `0`.
   - Define `CloneFromError` now, but keep it focused on the temporary
     unsupported cases:
     - source row has managed memory;
     - destination row has managed memory;
     - source or destination cell in the copied range has managed-memory state
       inconsistent with its row flag.
   - Cell-range managed-memory checks are unconditional. They are not debug-only
     assertions because this temporary API must reject unsupported cases in all
     builds rather than silently copying dangling metadata.
   - Use assertions for upstream preconditions, matching the existing Page API
     style.
   - Do not expose this outside `terminal::page`.
   - Do not name this method simply `clone_from` yet. That name should be
     reserved for the later experiment that implements the full Ghostty behavior
     or explicitly wraps this fast path with the remaining managed memory
     branches.

3. Port the plain row-copy mechanics.
   - For each destination/source row pair:
     - preserve the destination row's `cells` offset;
     - copy the source row metadata that is valid for plain rows;
     - copy cells over `0..min(self.size.cols, other.size.cols)`;
     - if `other.size.cols > self.size.cols`, truncate the source row to the
       destination width;
     - leave rows outside `0..(y_end - y_start)` unchanged.
   - Follow Ghostty's observed row metadata behavior from `clonePartialRowFrom`,
     not only the broad comment above `cloneFrom`:
     - start from the source row metadata;
     - always preserve the destination row's `cells` offset;
     - when the copied width is smaller than the destination width, preserve the
       destination row's `wrap`, `wrap_continuation`, `grapheme`, `hyperlink`,
       and `styled` flags and OR the source/destination dirty bits;
     - otherwise use the source row metadata, except for `cells`.
   - Do not zero cells after the copied range when the source row is narrower
     than the destination row. Ghostty's comment says extra columns are zeroed,
     but the implementation only copies the overlapping range and then clears a
     trailing spacer-head edge case. Roastty should follow the implementation
     for now and record this as an observed comment/code mismatch.
   - If `self.size.cols > other.size.cols` and the last copied source cell is a
     spacer head, clear that copied destination cell back to narrow, matching
     Ghostty's grow-column spacer-head cleanup.
   - Do not copy the source row's `cells` offset into the destination row.
   - Do not rewrite Page `size`, `capacity`, layout, or backing memory.

4. Add managed-memory scope guards.
   - Before copying a row, reject rows whose managed-memory flags are currently
     set:
     - `Row::styled`
     - `Row::grapheme`
     - `Row::hyperlink`
   - Also scan the copied cell range and reject cells with:
     - `Cell::has_grapheme()`
     - non-default `Cell::style_id()`
     - `Cell::hyperlink()`
   - These checks are unconditional for this experiment's copied range. They are
     temporary safety guards until later experiments add the managed-memory
     migration branches.
   - Add tests for these guards so a later experiment must deliberately replace
     the temporary limitation rather than silently corrupting style, grapheme,
     or hyperlink storage.

5. Port the three upstream plain `cloneFrom` tests.
   - `Page cloneFrom`:
     - write row-index codepoints in source column `1`;
     - clone all source rows into an equal-sized destination;
     - verify copied values;
     - mutate the source;
     - verify destination remains unchanged.
   - `Page cloneFrom shrink columns`:
     - clone from a wider page into a narrower page;
     - verify destination column count remains the narrower size;
     - verify copied cells survive truncation.
   - `Page cloneFrom partial`:
     - clone only the first five rows;
     - verify copied rows contain source values;
     - verify uncopied destination rows remain zero.
   - Add one extra Roastty-specific test for the observed implementation
     behavior where the source page is narrower than the destination page:
     destination cells after the copied range are preserved, not zero-filled,
     and a trailing copied spacer head is cleared back to narrow.

6. Preserve scope.
   - Do not implement:
     - full `Page::clone_from`;
     - `cloneRowFrom`;
     - `clonePartialRowFrom`;
     - `clearCells`;
     - style migration or `style::Set::add_with_id` use from Page;
     - grapheme migration;
     - hyperlink storage or migration;
     - `exactRowCapacity`;
     - integrity checking.
   - Do not change existing whole-page `Page::clone_page` behavior.

7. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal::page
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

8. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - the method name added;
     - which upstream tests were ported;
     - the exact unsupported managed-memory cases;
     - the trailing-cell preservation/truncation behavior;
     - verification command output summary.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- the plain-row clone method copies rows from `other[y_start..y_end]` into
  destination rows starting at `0`;
- destination row cell offsets remain destination-owned;
- wider source rows are truncated to destination width;
- narrower source rows preserve destination cells past the copied range;
- grow-column spacer-head cleanup matches Ghostty's implementation;
- rows outside the copied destination range remain unchanged;
- source and destination are independent after the copy;
- source/destination managed-memory rows and copied managed-memory cells are
  rejected rather than silently copied;
- the three upstream plain `Page cloneFrom` tests are ported and pass;
- existing Page clone, grapheme, style, and layout tests remain green;
- `cargo fmt`, targeted Page tests, and full `cargo test -p roastty` pass;
- Codex reviews the completed result and approves it or all real findings are
  fixed.

The experiment is partial if:

- the plain-row clone behavior works for equal-width pages, but truncation,
  grow-column spacer-head cleanup, or partial row-range preservation requires
  another small Page storage prerequisite.

The experiment fails if:

- the method is named or exposed as complete `clone_from` while managed-memory
  behavior is still unsupported;
- the destination row's `cells` offset is overwritten with a source-page offset;
- source and destination share mutable backing memory after the copy;
- style, grapheme, or hyperlink markers are copied without migrating their
  backing storage;
- existing whole-page clone behavior regresses.

## Codex Review

This experiment design must be reviewed by Codex before implementation. Any real
design issues must be fixed before committing the plan or implementing the
slice.

## Result

**Result:** Pass

Experiment 17 added the plain-row fast path for upstream `Page.cloneFrom` under
the deliberately scoped internal method:

- `Page::clone_plain_rows_from`
- `Page::clone_plain_row_from`

The method copies rows from `other[y_start..y_end]` into destination rows
starting at `0`, preserves each destination row's cell offset, copies only the
overlapping cell range, truncates wider source rows to the destination width,
and leaves destination rows outside the copied range unchanged.

The method is not named `clone_from` yet. That name remains reserved for the
later full Ghostty behavior that migrates managed memory.

### Upstream Tests Ported

The three upstream plain `cloneFrom` tests were ported:

- `Page cloneFrom` -> `page_clone_from_plain_rows`
- `Page cloneFrom shrink columns` -> `page_clone_from_plain_rows_shrink_columns`
- `Page cloneFrom partial` -> `page_clone_from_plain_rows_partial`

The tests verify full-row copy, source/destination independence after source
mutation, narrower destination truncation, and partial row-range copy.

### Extra Roastty Checks

The experiment added a Roastty-specific test for Ghostty's observed
source-narrower-than-destination behavior:

- trailing destination cells past the copied range are preserved, not zeroed;
- the copied trailing spacer-head cell is cleared back to narrow.

This follows the actual upstream `clonePartialRowFrom` implementation. Ghostty's
comment above `cloneFrom` says extra destination columns are zeroed, but the
code does not do that. The design and implementation follow the code.

The experiment also added temporary managed-memory guards. Until later
experiments implement style, grapheme, and hyperlink migration for row copy, the
plain-row API rejects:

- source rows with `Row::styled`, `Row::grapheme`, or `Row::hyperlink`;
- destination rows with those managed-memory flags;
- copied source cells with grapheme, style, or hyperlink markers;
- copied destination cells with those markers.

These guards are unconditional, not debug-only.

### Style Initialization Bug Found

The new managed-memory cell guard exposed a real bug from the prior Page style
wiring: `Page::init` initialized `style::Set` through an `OffsetBuf`, but
`RefCountedSet` treats its base argument as an address and `OffsetBuf`'s
`BaseAddress` implementation returns the underlying base pointer without the
offset. That caused style-set initialization to write at the start of Page
memory instead of the style region, corrupting cells.

This experiment fixed the initialization by passing the actual style-region
pointer (`memory + layout.styles_start`) to `style::Set::init`. Existing style
lookup/use paths already used the style-region pointer, so this makes
initialization match the rest of the style-set API.

### Verification

Commands run:

```bash
cargo fmt
cargo test -p roastty terminal::page
cargo test -p roastty
```

Results:

- `cargo test -p roastty terminal::page`: 58 passed.
- `cargo test -p roastty`: 167 Rust unit tests passed, ABI harness passed, doc
  tests passed.

## Conclusion

Roastty now has the plain-row foundation of Ghostty's `Page.cloneFrom`. The
slice intentionally stops before full managed-memory row copy. The next Page
clone experiment should replace the temporary guards one managed-memory class at
a time, likely starting with grapheme or style migration before hyperlinks,
because hyperlink storage is not implemented yet.
