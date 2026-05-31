# Experiment 36: Port PageList Scrollbar State

## Description

Port the read-only PageList scrollbar state calculation from upstream Ghostty:

- `Scrollbar`
- `scrollbar`
- `viewportRowOffset`
- `pinIsActive`
- `pinIsTop`

Experiments 33-35 created initialized PageLists, point-to-pin conversion, and
tracked-pin ownership. This experiment should add the next narrow read-only
viewport behavior needed by rendering/UI code: reporting the scrollable row
range, the current viewport offset, and the visible viewport length.

This experiment should not implement the `scroll` command, grow/prune behavior,
row erasure, resize, reflow, mutation-time viewport fixups, or public ABI.

## Changes

1. Re-read upstream source.
   - Use `vendor/ghostty/src/terminal/PageList.zig` as the source of truth for:
     - `Scrollbar`;
     - `scrollbar`;
     - `viewportRowOffset`;
     - `pinIsActive`;
     - `pinIsTop`.
   - Inspect upstream scrollbar tests for expected values, but port only the
     cases reachable from the current Roastty PageList surface.
   - Do not modify `vendor/ghostty/`.

2. Add the scrollbar value type.
   - Add an internal Rust `Scrollbar` struct with:
     - `total: usize`;
     - `offset: usize`;
     - `len: usize`.
   - Derive `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`.
   - Add `Scrollbar::ZERO` only if it is useful for tests or later callsites.
   - Do not add the C ABI mirror yet. Upstream's `Scrollbar.C` maps to
     app-facing action data, but Roastty does not expose that boundary in the
     current skeleton.

3. Add viewport row offset cache storage.
   - Add `viewport_pin_row_offset: Option<usize>` to `PageList`.
   - Initialize it to `None`.
   - Use it only for `Viewport::Pin`, matching upstream.
   - Do not add mutation invalidation beyond the code touched in this
     experiment, because grow/prune/erase/reflow are not implemented yet.

4. Extend integrity for pin viewports.
   - Add integrity errors equivalent to upstream's:
     - `ViewportPinOffsetMismatch`;
     - `ViewportPinInsufficientRows`.
   - When `viewport == Viewport::Pin`, compute the viewport pin's absolute row
     offset by direct PageList traversal.
   - If `viewport_pin_row_offset` is populated, verify that it matches the
     direct traversal result.
   - Verify that `total_rows - actual_offset >= rows`, so a pinned viewport has
     enough rows available to render a full viewport.
   - Add tests for both integrity failures.

5. Port `pin_is_active`.
   - Shape it as:

     ```rust
     fn pin_is_active(&self, pin: Pin) -> bool
     ```

   - Match upstream semantics:
     - compare the pin against `get_top_left(Tag::Active)`;
     - a pin in the active top-left node is active if its row is at or below the
       active top-left row;
     - a pin in any later node is active;
     - earlier nodes are not active.
   - Use current `Vec<Box<Node>>` index traversal instead of upstream intrusive
     linked-list traversal.

6. Port `pin_is_top`.
   - Shape it as:

     ```rust
     fn pin_is_top(&self, pin: Pin) -> bool
     ```

   - Return true only when the pin is row zero in the first page node.

7. Port `viewport_row_offset`.
   - Shape it as an internal method:

     ```rust
     fn viewport_row_offset(&mut self) -> usize
     ```

   - Match upstream semantics:
     - `Viewport::Top` returns `0`;
     - `Viewport::Active` returns `total_rows - rows`;
     - `Viewport::Pin` returns the cached offset if present;
     - otherwise, compute the pin's absolute row offset from the top, cache it,
       and return it.
   - It is acceptable for the method to take `&mut self` so it can populate the
     cache in safe Rust.
   - Keep the calculation based on current page sizes and node identity, not on
     `point_from_pin`, so the cache path remains a faithful adaptation of
     upstream's direct PageList traversal.
   - On every `Viewport::Pin` path, including cache hits, call or otherwise
     satisfy integrity so the cached value and "enough rows" invariants are
     checked in the same spirit as upstream's `defer self.assertIntegrity()`.

8. Port `scrollbar`.
   - Shape it as:

     ```rust
     fn scrollbar(&mut self) -> Scrollbar
     ```

   - Match upstream's no-scrollback special case:
     - if `explicit_max_size == 0`, return `total = rows`, `offset = 0`,
       `len = rows`;
     - otherwise, return `total = total_rows`, `offset = viewport_row_offset()`,
       `len = rows`.
   - Do not hide allocated extra page capacity except through the existing
     `total_rows` field, matching upstream.

9. Add tests.
   - Initial PageList reports `Scrollbar { total: rows, offset: 0, len: rows }`.
   - A `max_size = Some(0)` PageList reports no-scrollback scrollbar semantics
     even if tests manually create extra rows.
   - Active viewport with simulated history reports `total = total_rows`,
     `offset = total_rows - rows`, and `len = rows`.
   - Top viewport reports offset zero.
   - Pin viewport computes the absolute row offset across one page.
   - Pin viewport computes the absolute row offset across multiple pages.
   - Calling `scrollbar` twice for a pin viewport returns the same value and
     populates `viewport_pin_row_offset`.
   - Integrity rejects a populated pin offset cache that disagrees with direct
     traversal.
   - Integrity rejects a pin viewport that does not have enough rows remaining
     to render `rows` visible rows.
   - `pin_is_active` distinguishes pins before the active area, at the active
     top-left, and after the active top-left.
   - `pin_is_top` is true only for row zero in the first node.

10. Preserve scope.
    - Do not implement:
      - `Scroll`;
      - `scroll`;
      - grow/prune/erase/reset/resize/reflow;
      - viewport fixups after mutation;
      - scrollbar C ABI;
      - renderer or app integration;
      - public API additions.
    - Do not add `ghostty` names except when citing upstream paths or test
      provenance.

11. Verify.
    - Run:

      ```bash
      cargo fmt
      cargo test -p roastty terminal::page_list
      cargo test -p roastty
      ```

    - `cargo fmt` output must be accepted as-is.

12. Record the result.
    - Append `## Result` and `## Conclusion` to this file.
    - Include:
      - scrollbar value shape;
      - viewport cache behavior;
      - integrity checks added;
      - tests added;
      - verification command output summary.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `scrollbar` matches upstream no-scrollback, active, top, and pin viewport
  semantics reachable from the current PageList surface;
- pin viewport offsets are cached and reused;
- `pin_is_active` and `pin_is_top` match upstream behavior;
- no scroll/grow/prune/erase/reset/resize/reflow mutation behavior or public ABI
  is introduced;
- `cargo fmt`, targeted PageList tests, and full `cargo test -p roastty` pass;
- an independent agent reviews the experiment design and completed result and
  approves them, or all real findings are fixed.

The experiment is partial if:

- active/top scrollbar behavior works, but pin viewport offset calculation
  exposes a structural mismatch in the current safe Rust PageList storage that
  requires a separate experiment.

The experiment fails if:

- no-scrollback mode reports allocated scrollback capacity instead of
  `rows/0/rows`;
- active viewport offset is not `total_rows - rows`;
- pin viewport offset is computed relative to the active area instead of the top
  of the PageList;
- the cache can return stale values within the non-mutating behavior covered by
  this experiment;
- the implementation expands into scrolling or mutation behavior;
- tests or formatting fail.
