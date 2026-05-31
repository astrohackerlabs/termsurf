# Experiment 51: Port PageList Erase Rows

## Description

Port the upstream PageList `eraseRows` layer and its narrow `eraseHistory` /
`eraseActive` wrappers.

Experiments 48-50 implemented the lower-level row and page erasure primitives.
Upstream `eraseRows` is the first helper that owns full range erasure: it
iterates page chunks in top-to-bottom order, removes whole edge pages, shifts
partial chunks within a page, updates `total_rows`, regrows erased active rows,
and then fixes the viewport. This experiment should add that internal range
behavior and the two wrappers that make the allowed ranges explicit.

This is still PageList-only work. It must not connect erasure to the terminal
parser, scroll-clear behavior, renderer delivery, app behavior, or public ABI.

## Changes

1. Re-read upstream source.
   - Use `vendor/ghostty/src/terminal/PageList.zig` as the source of truth for:
     - `PageList.eraseRows`;
     - `PageList.eraseHistory`;
     - `PageList.eraseActive`;
     - the `PageList erase` tests around lines 9390-9630;
     - the viewport-offset erase tests around lines 7190-7430.
   - Do not modify `vendor/ghostty/`.

2. Add a narrow Rust error type if needed.
   - Upstream `eraseRows` is `void`, but Rust `grow()` can fail and existing
     helpers use `Result` for fallible internals.
   - Add an internal `EraseRowsError` only if the implementation needs to
     propagate grow failure or invalid internal points.
   - Keep the error narrow. Do not expose it through public ABI.

3. Add `PageList::erase_history` and `PageList::erase_active`.
   - `erase_history(bottom_left: Option<point::Point>)` should erase from the
     start of history to the optional history bottom-left bound.
   - `erase_active(y: CellCountInt)` should erase from the top of active area
     through active row `y`, inclusive.
   - `erase_active` must reject or assert out-of-bounds `y >= self.rows` in the
     same internal-helper style used by current PageList code.
   - These wrappers are the supported callers for `erase_rows`; they are also
     the reason full-page deletion remains restricted to front/back pages.

4. Add private `PageList::erase_rows`.
   - Signature should mirror upstream conceptually:
     - top-left `point::Point`;
     - optional bottom-left `point::Point`.
   - Because Rust cannot mutate `self` while holding the immutable
     `PageIterator` borrow, collect the `PageChunk`s into a `Vec<PageChunk>`
     before mutating pages.
   - Preflight the collected chunks before mutating:
     - history erasure is a prefix range, so process chunks in collected
       top-to-bottom order; every full-page chunk must be the current front page
       when it is deleted;
     - active erasure may begin in the middle of a page when history and active
       rows share a page, so it must not assume the first partial chunk starts
       at row `0`;
     - active erasure must only delete full pages that are the current back page
       at the moment of deletion, or else reject the request before mutation
       with a narrow internal error. Do not bypass `erase_page` to remove a
       middle page.
   - Track the total number of erased rows in a local `erased` counter.
   - For each chunk:
     - if it is a full page and it is not the only page, add its row count to
       `erased` and remove it with `erase_page`;
     - if it is a full page and it is the only page, reinitialize that page in
       place, set its row count to `0`, add its previous row count to `erased`,
       and stop;
     - if it is partial, shift rows after `chunk.end` upward to `chunk.start`,
       clear the vacated rows at the end of the now-smaller page, update tracked
       pins in that page, shrink the page row count by
       `chunk.end - chunk.start`, and add the erased count.
   - Partial chunk pin behavior should generalize upstream's row-0 case:
     - pins at or after `chunk.end` shift upward by the erased count;
     - pins inside `[chunk.start, chunk.end)` move to `(chunk.start, 0)`, unless
       `chunk.start == 0`, in which case this is the upstream `(0, 0)` behavior.
   - If `Page::reinit` needs to be called from PageList for the only-page case,
     expose it as `pub(super)` in `page.rs`; do not otherwise change Page
     behavior.
   - Preserve managed memory correctness by using existing `Page` row clone,
     move, clear, and reinit helpers rather than manipulating cell backing
     storage directly.

5. Update accounting and viewport only after the chunk loop.
   - Subtract `erased` from `total_rows` after all chunks are processed.
   - If erasing active rows, call `grow()` once per erased row so the active
     area is restored to `self.rows`.
   - After row accounting and any active regrowth, call
     `fixup_viewport(erased)`.
   - Successful `erase_rows` should end with full `verify_integrity`.
   - If regrowth fails, preserve the current Rust helper style by returning a
     narrow error. The result should document the chosen failure behavior.

6. Preserve scope.
   - Do not implement:
     - parser CSI/scroll-clear integration;
     - terminal screen APIs;
     - renderer/app notifications;
     - public C ABI additions;
     - resize/reflow;
     - row/cell/prompt iterators;
     - search or selection integration.
   - Do not add `ghostty` names except when citing upstream paths or test
     provenance.

7. Add tests based on upstream behavior.
   - History erase:
     - grow enough rows to create several pages;
     - erase all history;
     - verify `total_rows == rows`;
     - verify the list has only the active-area pages required by the current
       Rust page capacity model;
     - verify `page_size` is re-accounted downward.
   - History erase with tracked pin:
     - track a pin in erased history;
     - erase history;
     - verify the pin moves to the first remaining page at `(0, 0)`.
   - Bounded history erase:
     - erase only part of history;
     - verify top viewport behavior remains top when it should not become
       active;
     - verify row counts and integrity.
   - Active erase:
     - erase active rows through a small `y`;
     - include a case where active starts mid-page because history rows share
       the first active page;
     - verify active regrowth restores `total_rows == rows` when there is no
       history;
     - verify tracked pins below the erased active range shift up;
     - verify tracked pins inside the erased range move to `(0, 0)`;
     - include the one-row active case from upstream.
   - Full-page deletion order:
     - include a history range that deletes multiple front pages and verify each
       full-page deletion is front-removable at the time it happens;
     - include an active range with history before it and pages after it that
       would require deleting a middle full page if handled naively; verify the
       implementation rejects without mutation rather than corrupting serial
       invalidation.
   - Partial chunk behavior:
     - erase a range that leaves a partial first or last page;
     - erase a range whose partial chunk starts at a nonzero row;
     - verify rows move upward, vacated rows are cleared, dirty state is set,
       and managed memory does not leak stale grapheme/style/hyperlink data.
   - Viewport behavior:
     - carry forward the upstream viewport cases:
       - pinned/offset cache is invalidated or decremented correctly;
       - top viewport becomes active if the first remaining row is active;
       - top viewport remains top if erased history still leaves history above
         active.
   - Rejection/failure behavior:
     - verify invalid points or out-of-bounds active erasure fail without
       silently corrupting the list, if the Rust implementation returns errors.

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
     - wrappers implemented;
     - full-page erase behavior;
     - only-page reinit behavior;
     - partial chunk shift behavior;
     - active regrowth behavior;
     - tracked-pin and viewport behavior;
     - accounting behavior;
     - verification command output summary;
     - independent result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `erase_history` removes history rows from the front of the list and restores
  the list to the active area when called without a bound;
- bounded history erasure removes only the requested inclusive history range;
- `erase_active` removes rows from the top of active through the requested row
  and regrows active space afterward;
- full-page chunks are removed through `erase_page` and preserve Experiment 50's
  serial, pin, and byte-accounting guarantees;
- deleting the only page reinitializes that page to zero rows before regrowth
  instead of leaving the list empty;
- partial chunks shift remaining rows upward, clear vacated cells, mark dirty
  state, update tracked pins, and shrink the page row count;
- partial chunks with nonzero `start` are handled safely when active starts
  mid-page;
- any requested full-page deletion that would remove a middle page is rejected
  before mutation rather than bypassing serial invalidation;
- `total_rows` is updated by the total erased count, then active regrowth
  restores the active-area invariant when applicable;
- `fixup_viewport` runs after row accounting and regrowth;
- full `verify_integrity` passes after successful erasure;
- no parser, renderer, app, terminal API, public ABI, resize/reflow, or
  selection/search work is introduced;
- `cargo fmt`, targeted PageList tests, and full `cargo test -p roastty` pass;
- an independent agent reviews the experiment design and completed result and
  approves them, or all real findings are fixed.

The experiment is partial if:

- history erasure works, but active regrowth exposes a missing grow/error model
  that needs a separate design before safe completion;
- range erasure works structurally, but viewport cache behavior differs from
  upstream in a way that requires a separate viewport experiment.

The experiment fails if:

- `erase_rows` can delete a middle page and leave serial invalidation ambiguous;
- active erasure assumes all partial chunks start at row `0` and mishandles the
  active-starts-mid-page case;
- `total_rows`, `page_size`, `page_serial_min`, remaining page order, or tracked
  pins become inconsistent;
- active erasure can leave fewer than `rows` active rows after successful
  completion;
- partial chunk erasure leaks stale managed memory or leaves dirty state
  incorrect;
- viewport fixup is skipped or occurs before row accounting/regrowth;
- the implementation expands into parser, renderer, app, ABI, resize/reflow, or
  unrelated terminal work;
- tests or formatting fail.

## Result

**Result:** Pass

`PageList::erase_rows` is implemented as the private range-erasure layer, with
`erase_history` and `erase_active` wrappers. The helper collects `PageIterator`
chunks before mutation, validates that full history-page deletion is
front-removable, validates that full active-page deletion is back-removable, and
rejects middle full-page deletion before mutation.

Full-page chunks are removed through Experiment 50's `erase_page` helper. The
only-page full-erasure case reinitializes the page in place, sets its row count
to zero, and moves pins in that page to `(0, 0)` so active regrowth can rebuild
the page list without leaving dangling or invalid pin state. `Page::reinit` was
made `pub(super)` only for this PageList-only case.

Partial chunks now support nonzero starts. Rows after the erased range rotate up
to fill the gap, vacated rows are cleared, the page row count shrinks, the page
is marked dirty, and tracked pins are updated. Pins below the erased range shift
up by the erased row count. Pins inside the erased range move to the first row
after the erased range when that row is in the next page, or to the replacement
position in the same page otherwise. This covers the Rust PageIterator case
where active rows begin mid-page because history and active rows share a page.

After chunk mutation, `erase_rows` subtracts the total erased row count from
`total_rows`. Active erasure then regrows one row per erased row, and
`fixup_viewport` runs after accounting and regrowth. Successful erasure ends
with full `verify_integrity`.

Tests added coverage for:

- erasing all history and re-accounting page size;
- bounded history erasure that leaves history above active;
- active erasure with regrowth and tracked-pin shifting;
- active erasure where active starts mid-page;
- active erasure to the end of a mid-page chunk, with erased pins moving to the
  next page;
- rejection of a full active-page deletion that would remove a middle page;
- one-row active erasure;
- managed-memory cleanup and preservation for style, grapheme, and hyperlink
  data in partial range erasure;
- viewport top becoming active after all history is erased;
- pinned viewport cache adjustment after bounded history erasure.

Verification:

- `cargo fmt -- roastty/src/terminal/page.rs roastty/src/terminal/page_list.rs`
- `cargo test -p roastty terminal::page_list` — 184 PageList tests passed, ABI
  harness filtered out
- `cargo test -p roastty` — 465 unit tests passed, ABI harness passed, doc-tests
  passed

Independent result review initially found three real gaps: end-of-page
nonzero-start pin remapping, missing managed-memory coverage, and missing erase
viewport coverage. Those were fixed and re-reviewed. The final review approved
Experiment 51 as a Pass with no remaining blockers.

## Conclusion

Experiment 51 completed the PageList range-erasure layer that ties together the
row and page erasure primitives from Experiments 48-50. Roastty now has internal
history and active range erasure with edge-safe full-page deletion, arbitrary
partial-chunk starts, active regrowth, viewport fixup, and integrity coverage.

The implementation deliberately rejects middle full-page deletion rather than
creating serial invalidation gaps. The next experiment can move to the next
unported PageList surface above range erasure, such as scroll-clear or the
row/cell iteration helpers, using this erase stack as a foundation.
