# Experiment 40: Port PageList Reset

## Description

Port upstream PageList `reset`.

Experiment 39 finished PageList growth, including scrollback pruning and page
reuse. The next PageList-local lifecycle behavior in upstream is `reset`, which
clears all scrollback and active content while preserving tracked pin pointer
stability.

Upstream Ghostty implements reset with memory pools: it frees non-standard page
backing memory, resets page/node pools while retaining enough capacity for the
active area, rebuilds active pages, invalidates old page serials, moves every
tracked pin to the new top-left, marks external tracked pins as garbage, keeps
the viewport pin non-garbage, and returns the viewport to active.

Roastty does not currently have upstream's memory pool shape. In Rust, this
experiment should preserve the same observable behavior by reusing existing
`Box<Node>` and `Page` allocations for the active area, resetting those pages in
place with `Page::reinit_with_capacity`, and dropping any extra history pages.
That keeps reset infallible for the current PageList model without introducing
fresh allocation into a path upstream explicitly treats as non-failing.

This remains PageList-local lifecycle work. It must not implement clone, resize,
reflow, erase, dirty tracking, prompt scrolling, screen/parser integration,
renderer/app integration, or public ABI.

## Changes

1. Re-read upstream source.
   - Use `vendor/ghostty/src/terminal/PageList.zig` as the source of truth for:
     - `PageList.reset`;
     - `PageList reset`;
     - `PageList reset invalidates stale untracked refs even if node memory is reused`;
     - `PageList reset across two pages`;
     - `PageList reset moves tracked pins and marks them as garbage`;
     - `PageList clears history`.
   - Do not modify `vendor/ghostty/`.

2. Add `PageList::reset`.
   - The method should be infallible for the current Rust PageList model.
   - It should not call `init_pages` or otherwise allocate fresh page memory in
     the normal reset path.
   - It should reuse enough existing page nodes to cover the active area and
     drop all remaining history pages.

3. Invalidate old page serials before rebuilding pages.
   - Set `page_serial_min = page_serial` before assigning reset-page serials.
   - Assign each retained/reset node a fresh serial from `page_serial`, then
     increment `page_serial`.
   - The old first page serial must become invalid after reset even if the
     allocator later reuses the same node address.

4. Rebuild only the active-area pages.
   - Compute `cap = initial_capacity(cols)`.
   - Compute the number of pages needed to cover `rows`.
   - Retain exactly that many nodes from the front of `pages`.
   - For each retained node:
     - reset the page in place with `reinit_with_capacity(cap)`;
     - set its row count to the active rows represented by that page;
     - set its serial to the next fresh page serial.
   - Drop all extra nodes and their page memory.
   - Recompute `page_size` from the retained pages.
   - Set `total_rows = rows`.

5. Preserve active-area geometry.
   - After reset, `pages` must be non-empty.
   - `total_rows == rows`.
   - `get_top_left(Active)` should be the first page at `(x=0, y=0)`.
   - Multi-page active areas must retain enough pages to cover `rows`.

6. Remap tracked pins.
   - Move every tracked pin to the first retained page at `(x=0, y=0)`.
   - Mark every tracked pin as `garbage = true`.
   - Then set `viewport_pin.garbage = false`, matching upstream.
   - Preserve the stable `Box<Pin>` allocations already used by Rust
     `tracked_pin_storage`; reset should mutate pins, not replace the tracked
     pin storage.

7. Reset viewport state.
   - Set `viewport = Viewport::Active`.
   - Clear `viewport_pin_row_offset`.
   - Keep the viewport pin valid and non-garbage.

8. Preserve integrity.
   - `verify_integrity` must pass after reset.
   - Add or adjust integrity checks only if reset reveals a real gap.
   - Do not weaken existing integrity checks.

9. Add tests.
   - Basic reset:
     - initialized list can reset;
     - viewport is active;
     - `total_rows == rows`;
     - active top-left is first page `(0, 0)`;
     - integrity passes.
   - Reset clears history:
     - grow history beyond the active area;
     - reset;
     - history is gone, `total_rows == rows`, and active top-left is first page.
   - Reset across two active pages:
     - choose dimensions where the active area requires more than one page;
     - reset;
     - enough pages remain to cover `rows`;
     - integrity passes.
   - Tracked pins:
     - track a pin away from `(0, 0)`;
     - reset;
     - the tracked pin points to the new first page at `(0, 0)`;
     - the tracked pin is garbage;
     - the viewport pin points to the new first page at `(0, 0)`;
     - the viewport pin is not garbage.
   - Serial invalidation:
     - capture an old first-page serial;
     - reset;
     - the old serial is below `page_serial_min`;
     - new page serials are in `[page_serial_min, page_serial)`.
   - Non-standard active page reset:
     - use a column count that produces non-standard page backing;
     - grow history;
     - reset;
     - extra history pages are dropped;
     - `page_size` matches the retained active pages only;
     - integrity passes.
   - Viewport cache clearing:
     - create a cached `Viewport::Pin` state;
     - reset;
     - viewport is active and `viewport_pin_row_offset` is `None`.

10. Preserve scope.
    - Do not implement:
      - clone;
      - resize/reflow;
      - erase/compact/split;
      - dirty tracking;
      - prompt scrolling;
      - screen/parser integration;
      - renderer or app integration;
      - public C ABI additions.
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
      - reset behavior implemented;
      - active-area page retention behavior;
      - tracked-pin remapping behavior;
      - serial invalidation behavior;
      - viewport reset behavior;
      - tests added;
      - verification command output summary.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `PageList::reset` clears scrollback and active content back to a fresh active
  area;
- reset is infallible for the current Rust PageList model and does not allocate
  fresh page memory in the normal path;
- old page serials are invalidated via `page_serial_min`;
- retained active-area pages receive fresh serials;
- multi-page active areas remain fully represented;
- extra history pages are dropped;
- `page_size` and `total_rows` are correct after reset;
- tracked pins are moved to top-left and marked garbage;
- the viewport pin remains valid and non-garbage;
- viewport state returns to active and cached viewport offset is cleared;
- no clone, resize/reflow, erase, dirty tracking, prompt scrolling,
  screen/parser, renderer/app, or ABI work is introduced;
- `cargo fmt`, targeted PageList tests, and full `cargo test -p roastty` pass;
- an independent agent reviews the experiment design and completed result and
  approves them, or all real findings are fixed.

The experiment is partial if:

- basic reset behavior works, but infallible reset cannot be preserved without a
  larger Rust memory-pool adaptation. In that case, record the exact allocation
  dependency and design the next experiment around it.

The experiment fails if:

- reset can leave fewer rows than the active viewport requires;
- tracked pins point to dropped pages incorrectly;
- viewport pin becomes garbage or invalid;
- stale page serials remain valid after reset;
- page size or row accounting becomes inconsistent;
- the implementation expands beyond PageList reset;
- tests or formatting fail.
