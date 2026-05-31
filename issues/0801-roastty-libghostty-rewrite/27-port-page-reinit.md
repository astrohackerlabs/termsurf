# Experiment 27: Port Page Reinit

## Description

Port upstream `Page.reinit` to Roastty.

Upstream `reinit` reuses a Page's existing allocation, zeroes the backing
memory, and rebuilds the row/cell/metadata layout with the same capacity. This
is a Page lifecycle primitive, separate from allocation and cloning. Roastty can
already initialize and drop Pages, but it does not yet expose the equivalent
same-capacity reset operation.

This experiment should add internal Page reinitialization only. It should not
add terminal screen lifecycle, parser integration, public ABI, or app-facing
APIs.

## Changes

1. Re-read upstream source.
   - Use `vendor/ghostty/src/terminal/page.zig` as the source of truth for:
     - `Page.reinit`;
     - `Page.initBuf`;
     - row/cell offset initialization after memory is zeroed.
   - Preserve upstream's "same allocation, same capacity" behavior.
   - Do not modify `vendor/ghostty/`.

2. Add an internal reinit method.
   - Add an internal method:

     ```rust
     fn reinit(&mut self)
     ```

   - The method should:
     - preserve the existing allocation pointer and length;
     - preserve the existing `Capacity`;
     - clear old backing contents before rebuilding Page metadata;
     - rebuild row offsets, layout-derived handles, size, allocator handles,
       set/map handles, dirty state, and optional maps exactly as `Page::init`
       would for the same capacity.
   - Do not allocate a new Page backing buffer.
   - Do not depend on `clone_page` or any clone path.

3. Avoid destructor/double-free hazards.
   - `PageMemory` owns the backing allocation. Reinitialization must not drop or
     replace it with a newly allocated buffer.
   - If temporary values are used, ensure they do not free the backing memory
     while `self` still owns it.
   - Prefer factoring a small `init_in_existing_memory` helper if it keeps
     ownership clear.

4. Reset all managed-memory state.
   - After `reinit`, the Page should behave like a fresh `Page::init` with the
     same capacity. The final backing buffer will not be all zero because row
     offsets and allocator/map metadata are rebuilt after clearing:
     - no graphemes;
     - no styles;
     - no hyperlinks;
     - string allocator empty;
     - row flags clear;
     - cells zeroed;
     - dirty state false;
     - size restored to full capacity rows/cols.
   - Any old style/hyperlink/grapheme IDs become invalid and must not remain
     reachable through cell data or maps.

5. Add focused tests.
   - Basic lifecycle:
     - record `backing_ptr`, `backing_len`, and `capacity`;
     - mutate cells and row flags;
     - shrink `page.size` below capacity before calling `reinit`, so size
       restoration is proven non-vacuously;
     - call `reinit`;
     - verify pointer, length, and capacity are unchanged;
     - verify size is restored to capacity;
     - verify all cells are zero and row cell offsets are correct.
   - Managed-memory reset:
     - create style, grapheme, hyperlink, and string allocation state;
     - call `reinit`;
     - verify style count, grapheme count, hyperlink count, hyperlink set count,
       grapheme used bytes, and string used bytes are zero;
     - verify row flags are clear.
   - Dirty and row metadata reset:
     - set dirty/wrap/wrap-continuation/semantic prompt on rows;
     - call `reinit`;
     - verify dirty is false and row metadata returns to defaults.
   - Reuse after reinit:
     - after `reinit`, insert a new style, grapheme, and hyperlink;
     - verify the Page remains usable and counts/refcounts are correct.

6. Preserve scope.
   - Do not implement:
     - terminal screen reset behavior;
     - parser/screen integration;
     - public ABI or app-facing APIs;
     - integrity checking beyond tests needed for this method.
   - Do not change Page layout constants or allocator semantics.

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
     - reinit API added;
     - how backing ownership is preserved;
     - state reset behavior;
     - tests added;
     - any deferred lifecycle methods;
     - verification command output summary.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `Page::reinit` reuses the existing allocation;
- capacity and backing length are unchanged;
- size is restored to capacity;
- old backing contents are cleared, cells/maps/allocators return to empty state,
  and row offsets are rebuilt correctly;
- dirty state, row flags, row metadata, cells, styles, graphemes, hyperlinks,
  and strings reset to fresh-Page state;
- the Page is usable after reinit for style, grapheme, and hyperlink operations;
- existing clone, clear, move, swap, exact capacity, style, grapheme, and
  hyperlink tests do not regress;
- `cargo fmt`, targeted Page tests, and full `cargo test -p roastty` pass;
- Codex reviews the completed result and approves it or all real findings are
  fixed.

The experiment is partial if:

- plain cell/row reset works, but one managed-memory allocator or map needs a
  focused follow-up;
- reinit works functionally, but ownership factoring needs a cleanup experiment.

The experiment fails if:

- `reinit` allocates a new backing buffer;
- old cell or managed-memory contents remain reachable after reinit;
- stale managed-memory IDs or map entries remain reachable;
- row offsets are wrong after reinit;
- the implementation expands into parser/screen lifecycle, public ABI, or
  unrelated behavior.
