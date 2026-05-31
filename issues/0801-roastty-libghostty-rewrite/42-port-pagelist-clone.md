# Experiment 42: Port PageList Clone

## Description

Port upstream PageList `clone`.

Experiment 41 added PageList page-iterator row chunks, which is the immediate
dependency for upstream `clone`. This experiment should use that iterator to
copy a tagged PageList region into a new PageList, preserving page-local row
data and tracked-pin remapping semantics.

Upstream clone:

- walks the requested region with `pageIterator(.right_down, top, bot)`;
- creates one destination page per source chunk;
- copies only `chunk.start..chunk.end`;
- moves the clone viewport to active;
- creates a fresh viewport pin;
- optionally remaps tracked pins that fall inside cloned chunks;
- pads the clone with blank rows if the cloned region has fewer rows than the
  active area requires.

Roastty already has `Page::clone_rows_from` and exact-row-capacity logic, but
those helpers are private inside `page.rs`. This experiment may expose the
narrow `pub(super)` surface needed by PageList clone. Do not widen visibility
beyond what PageList needs.

This remains PageList-local clone work. It must not implement resize/reflow,
erase, dirty tracking, prompt scrolling, row/cell/prompt iterators,
screen/parser integration, renderer/app integration, or public ABI.

## Changes

1. Re-read upstream source.
   - Use `vendor/ghostty/src/terminal/PageList.zig` as the source of truth for:
     - `Clone`;
     - `Clone.TrackedPinsRemap`;
     - `PageList.clone`;
     - `PageList clone`;
     - `PageList clone partial trimmed right`;
     - `PageList clone partial trimmed left`;
     - `PageList clone partial trimmed left reclaims styles`;
     - `PageList clone partial trimmed both`;
     - `PageList clone less than active`;
     - `PageList clone remap tracked pin`;
     - `PageList clone remap tracked pin not in cloned area`.
   - Use `PageList clone full dirty` only as a deferred reference because
     PageList dirty tracking is not implemented yet.
   - Do not modify `vendor/ghostty/`.

2. Add clone option types.
   - Add a private `CloneOptions` struct with:
     - `top: point::Point`;
     - `bottom: Option<point::Point>`;
     - optional tracked-pin remap output.
   - Add a private tracked-pin remap representation appropriate for Rust tests,
     such as `Vec<(NonNull<Pin>, NonNull<Pin>)>` or a small wrapper around it.
   - Keep the type private to PageList for now.
   - Do not expose any clone API through the public C ABI.

3. Expose narrow Page helpers if needed.
   - Make `Page::exact_row_capacity` callable from `page_list.rs` if clone needs
     exact destination page capacities for partial chunks.
   - Make `Page::clone_rows_from` callable from `page_list.rs`.
   - Keep both helpers `pub(super)`, not public.
   - Do not expose lower-level clone internals unless a compiler error proves
     they are required.

4. Implement `PageList::clone_region`.
   - Name can be private and Rust-idiomatic; it does not need to be exactly
     `clone`, especially if that conflicts with Rust trait expectations.
   - Inputs should mirror upstream `CloneOptions`.
   - Use `page_iterator(Direction::RightDown, opts.top, opts.bottom)`.
   - If the requested iterator is empty, return a `PageList` allocation error or
     a small clone-specific error rather than panicking. Document the chosen
     behavior in tests.
   - Count chunks first if useful, matching upstream's two-pass shape, but do
     not add allocation preheat scaffolding that Rust does not need.

5. Create destination pages chunk-by-chunk.
   - For each source `PageChunk`:
     - find the source node/page from the chunk;
     - compute destination capacity from the source chunk, preferably with
       `source_page.exact_row_capacity(chunk.start, chunk.end)`;
     - initialize a new destination `Page`;
     - set destination row count to `chunk.end - chunk.start`;
     - copy rows with `clone_rows_from(source_page, chunk.start, chunk.end)`;
     - copy page-level dirty state only if this is already represented on
       `Page`; do not add PageList dirty tracking in this experiment;
     - append the node with a fresh serial;
     - update `page_size` and `total_rows`.
   - Preserve page-local row data, style references, graphemes, hyperlinks, and
     row metadata through the existing Page clone helpers.

6. Initialize clone metadata.
   - Preserve:
     - `cols`;
     - `rows`;
     - `explicit_max_size`;
     - `min_max_size`.
   - Set:
     - `page_serial_min = 0`;
     - fresh sequential page serials starting from 0;
     - `viewport = Viewport::Active`;
     - `viewport_pin` to the first cloned page at `(0, 0)`;
     - `viewport_pin_row_offset = None`;
     - `tracked_pins` containing at least the viewport pin.

7. Pad clones smaller than the active area.
   - If cloned `total_rows < rows`, grow the clone until it has at least `rows`
     rows, matching upstream.
   - Ensure the padded rows are blank.
   - Set `total_rows = rows` after padding.
   - Verify that integrity passes after padding.

8. Remap tracked pins.
   - If remap output is requested:
     - inspect all source tracked pins;
     - for each pin whose node is the source chunk node and whose `y` is inside
       `chunk.start..chunk.end`, allocate/store a new tracked pin in the clone;
     - set the new pin's node to the destination chunk node;
     - subtract `chunk.start` from `y`;
     - preserve `x` and `garbage`;
     - record the old-to-new mapping in the requested remap output.
   - Pins outside the cloned area must not be remapped.
   - The clone's viewport pin remains separately tracked and should not be
     replaced by remapped pins.

9. Add tests.
   - Basic clone:
     - clone the full screen/active region;
     - clone has the same `total_rows` as source;
     - clone viewport is active;
     - clone integrity passes.
   - Partial trimmed right:
     - source has history;
     - clone screen rows through explicit bottom;
     - clone `total_rows` matches inclusive range length.
   - Partial trimmed left:
     - source has history;
     - clone from screen row 10 to the default bottom;
     - clone `total_rows` matches upstream expectation.
   - Partial trimmed both:
     - clone an explicit inclusive screen range;
     - clone `total_rows` matches exact range length.
   - Less than active:
     - clone from active row 5;
     - clone pads to `rows`;
     - padded rows are blank;
     - integrity passes.
   - Row data copy:
     - write visible cell data into source rows;
     - clone a range;
     - destination rows contain the expected copied cells and no rows outside
       the range.
   - Trimmed managed-memory reclamation:
     - create styled/grapheme/hyperlink data in rows that are trimmed away;
     - clone only the unmarked later rows;
     - destination managed-memory counts/capacity reflect the cloned rows, not
       trimmed rows.
   - Tracked pin remap:
     - track a pin inside the cloned range;
     - request remap output;
     - remapped pin exists in clone and maps to the expected active point.
   - Tracked pin outside cloned area:
     - track a pin before the cloned range;
     - request remap output;
     - no remap entry exists.
   - Invalid/empty clone request:
     - invalid endpoints produce the documented clone error rather than a panic.

10. Deferred dirty clone test.
    - Do not implement `PageList clone full dirty` yet unless PageList dirty
      tracking already exists by the time this experiment runs.
    - Record in the result that dirty-specific clone verification is deferred to
      the dirty tracking experiment.

11. Preserve scope.
    - Do not implement:
      - resize/reflow;
      - erase/compact/split;
      - dirty tracking;
      - prompt scrolling;
      - row/cell/prompt iterators;
      - screen/parser integration;
      - renderer or app integration;
      - public C ABI additions.
    - Do not add `ghostty` names except when citing upstream paths or test
      provenance.

12. Verify.
    - Run:

      ```bash
      cargo fmt
      cargo test -p roastty terminal::page_list
      cargo test -p roastty
      ```

    - `cargo fmt` output must be accepted as-is.

13. Record the result.
    - Append `## Result` and `## Conclusion` to this file.
    - Include:
      - clone behavior implemented;
      - partial range behavior;
      - active-area padding behavior;
      - tracked-pin remapping behavior;
      - managed-memory copy/reclamation behavior;
      - dirty test deferral note;
      - tests added;
      - verification command output summary.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `PageList::clone_region` copies the requested tagged region with
  upstream-compatible inclusive range semantics;
- cloned pages contain only the requested rows and use page-local row chunks
  from `page_iterator`;
- clone metadata, serials, viewport state, page size, and total row accounting
  are consistent;
- clones shorter than the active area are padded with blank rows;
- tracked pins inside the clone region are remapped to clone pins with adjusted
  row offsets;
- tracked pins outside the clone region are not remapped;
- copied row data preserves the existing Page-level text, style, grapheme, and
  hyperlink behavior;
- managed-memory data from trimmed-away rows is not retained in the clone;
- invalid clone requests return a documented error rather than panicking;
- no resize/reflow, erase, dirty tracking, prompt scrolling, row/cell/prompt
  iterators, screen/parser, renderer/app, or ABI work is introduced;
- `cargo fmt`, targeted PageList tests, and full `cargo test -p roastty` pass;
- an independent agent reviews the experiment design and completed result and
  approves them, or all real findings are fixed.

The experiment is partial if:

- basic and partial clone work, but managed-memory exact-capacity behavior needs
  a narrower Page helper follow-up.

The experiment fails if:

- clone copies rows outside the requested range;
- clone loses copied text/style/grapheme/hyperlink data;
- clone retains managed memory from trimmed rows;
- clone produces invalid tracked-pin remaps;
- clone can create invalid PageList metadata or serial state;
- the implementation expands beyond PageList clone;
- tests or formatting fail.

## Result

**Result:** Pass

Experiment 42 implemented a private `PageList::clone_region` that mirrors
upstream PageList clone behavior for the current Roastty storage model:

- clone options and tracked-pin remap state are private to PageList;
- `Page::exact_row_capacity`, `Page::clone_rows_from`, and `Page::set_dirty` are
  exposed only as `pub(super)` helpers for PageList clone;
- clone traversal uses `page_iterator(Direction::RightDown, top, bottom)`;
- each source chunk becomes one destination page with exact row capacity;
- row data is copied through the existing Page clone helpers;
- clone metadata preserves sizing and max-size state while resetting page
  serials and viewport state;
- clones shorter than the active area are padded with blank rows;
- tracked pins inside cloned chunks are remapped to clone-owned pins with
  adjusted row offsets;
- tracked pins outside the cloned region are not remapped;
- invalid/empty clone requests return `CloneRegionError::Empty` instead of
  panicking.

Tests added:

- basic full-region clone;
- partial trimmed-right clone;
- partial trimmed-left clone;
- partial trimmed-both clone;
- less-than-active clone with blank-row padding;
- plain row-data copy;
- managed style/grapheme/hyperlink data copy inside the cloned range;
- managed-memory reclamation when marked rows are trimmed away;
- tracked-pin remap inside the cloned range;
- tracked-pin non-remap outside the cloned range;
- invalid clone request error handling.

The upstream `PageList clone full dirty` test remains deferred because
PageList-level dirty tracking is not implemented yet. Page-level dirty state is
copied when the source page has it, but dirty-region behavior belongs to the
future dirty-tracking experiment.

Verification:

```bash
cargo fmt
cargo test -p roastty terminal::page_list
cargo test -p roastty
```

Observed result:

- `cargo test -p roastty terminal::page_list`: 107 passed;
- `cargo test -p roastty`: 388 unit tests passed, plus 1 ABI harness test
  passed.

Independent review:

- Initial result review found two real issues: test-only visibility expansion
  for `Page::set_graphemes_at`, and missing PageList-level verification that
  style/grapheme/hyperlink data inside the cloned region survives clone.
- Both issues were fixed.
- Follow-up review found no required changes and approved Experiment 42 as ready
  to record as `Pass`.

## Conclusion

PageList can now clone tagged regions using upstream-compatible row-chunk
semantics. The clone path preserves Page-local row data, trims managed memory
outside the selected region, pads short clones to the active row count, resets
viewport/serial metadata, and remaps tracked pins when requested.

The next experiment should continue with the next PageList operation that builds
on clone/page-iterator behavior. Dirty-specific clone verification should remain
deferred until PageList dirty tracking exists.
