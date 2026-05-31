# Experiment 16: Port Page Style Storage and Clone

## Description

Wire Experiment 15's `style::Set` into `Page` and port Ghostty's first
style-backed Page behavior: `Page clone styles`.

Roastty already reserves style-set memory in `PageLayout`, and cells/rows
already expose the style marker bits:

- `Row::styled`
- `Cell::style_id`
- `Cell::has_styling`

Until this experiment, however, `Page` does not initialize or carry a real style
set. Experiment 15 made that possible without changing Page behavior. This
experiment should add the `styles` field to `Page`, initialize it from the
existing style layout, include it in whole-page clone, and port the upstream
`Page clone styles` test.

Do not port style mutation helpers, integrity checks, exact-capacity behavior,
`cloneFrom`, or row-copy style behavior in this experiment.

## Changes

1. Inspect upstream source.
   - Use `vendor/ghostty/src/terminal/page.zig` as the source of truth.
   - Re-read:
     - `Page.initBuf`
     - `Page.clone`
     - `Page clone styles`
     - style portions of `cloneFrom` only for future context
   - Use `vendor/ghostty/src/terminal/style.zig` and the current
     `roastty/src/terminal/style.rs` for style-set semantics.
   - Do not modify `vendor/ghostty/`.

2. Add real style set storage to Page.
   - Add `styles: style::Set` to `Page`.
   - Initialize it in `Page::init` from:
     - `layout.styles_start`
     - `layout.styles_layout`
     - the Page backing memory
   - Prefer replacing `StyleSetLayout`'s temporary layout helper with
     `style::Set::layout` only if this is mechanical and existing Page layout
     numeric tests stay unchanged.
   - If the temporary wrapper remains, add a conversion/helper so Page
     initialization uses the same values as `style::Set`.

3. Preserve whole-page clone behavior.
   - Update `Page::clone_page` to copy the `styles` field by value.
   - Do not rebuild style set entries during whole-page clone.
   - Do not rewrite style IDs.
   - The clone works because Page memory is byte-copied and style set offsets
     are relative to the cloned backing memory.
   - The copied `style::Set` field must contain only offset/layout/value
     metadata. It must not store a base pointer into the source page.
   - Assert in tests that source and clone have different `PageMemory` backing
     pointers.

4. Add minimal Page style access used by tests.
   - Add only narrow wrappers needed to express the ported test:
     - add style to the page;
     - get style by ID;
     - increment style use count;
     - release style use count for independence tests;
     - read style ref count if useful for assertions.
   - Keep these wrappers internal to `terminal::page`.
   - Do not introduce a general style mutation API such as upstream `setStyle`;
     that belongs with later Page style operations.

5. Port upstream `Page clone styles`.
   - Create a page with styles capacity.
   - Add a bold style to the page's style set.
   - Write the first row's cells with codepoints and the returned style ID.
   - Mark the row styled.
   - Increment the style ref count for each styled cell, matching upstream's
     explicit `page.styles.use(...)` calls.
   - Clone the page.
   - Verify on the clone:
     - row styled flag is set;
     - every styled cell has the copied style ID;
     - looking up that style in the clone returns bold style data;
     - style ref count is `1 + styled_cell_count`, matching upstream's
       add-reference plus one explicit use per styled cell.
   - Add source/clone independence checks:
     - release the source style references after clone, down to zero, using the
       narrow Page wrapper;
     - optionally add another style to the source to force legal set reuse;
     - assert the clone still returns the original bold style and original ref
       count;
     - dropping source before reading clone leaves clone readable.
   - Add a zero-style-capacity check:
     - `Page` must initialize `style::Set` even when `capacity.styles == 0`,
       using the zero layout;
     - inserting a style into such a page must fail through `style::Set`;
     - do not add a heap fallback or optional-style-map special case.

6. Preserve scope.
   - Do not port:
     - Page `setStyle`;
     - Page `clearCells` style release behavior;
     - Page `moveCells` style behavior;
     - Page `verifyIntegrity styles ...`;
     - Page `exactRowCapacity styles ...`;
     - `cloneFrom`;
     - `cloneRowFrom`;
     - hyperlink behavior.
   - Do not touch hyperlink layout, grapheme behavior, or PageList.

7. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal::page
     cargo test -p roastty terminal::style
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

8. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - Page fields/API added;
     - whether `StyleSetLayout` was replaced or kept as a wrapper;
     - whole-page clone strategy;
     - upstream tests ported;
     - deferred Page style tests and why;
     - verification command output summary.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `Page` initializes a real `style::Set` inside its existing backing-memory
  style region;
- Page layout numeric tests remain unchanged;
- `Page::clone_page` preserves styles through byte-copy and copied offset
  metadata;
- source and clone have different `PageMemory` backing pointers;
- style IDs remain unchanged across whole-page clone;
- cloned style ref count is explicitly verified as add-reference plus styled
  cell uses;
- upstream `Page clone styles` behavior is ported and passes;
- clone/source style storage independence is tested;
- zero-style-capacity style insertion fails without heap fallback;
- no `cloneFrom`, row-copy, integrity, exact-capacity, hyperlink, or PageList
  behavior is introduced;
- `cargo fmt`, targeted Page/style tests, and full `cargo test -p roastty` pass;
- Codex reviews the completed result and approves it or all real findings are
  fixed.

The experiment is partial if:

- `Page` can initialize and use `style::Set`, but whole-page clone reveals a
  missing style-set copy invariant that requires a focused prerequisite fix.

The experiment fails if:

- Page style storage uses heap maps/vectors instead of the Page backing-memory
  style region;
- style IDs are rebased or rewritten during whole-page clone;
- source and clone share mutable style backing memory;
- Page layout numeric tests regress;
- the experiment drifts into `cloneFrom`, integrity, exact-capacity, or
  hyperlink behavior.

## Codex Review

This experiment design must be reviewed by Codex before implementation. Any real
design issues must be fixed before committing the plan or implementing the
slice.

## Result

**Result:** Pass

Experiment 16 wired the real `style::Set` from Experiment 15 into `Page` and
ported the first style-backed Page behavior, `Page clone styles`.

### Page Fields and API

`Page` now owns:

- `styles: style::Set`

`Page::init` initializes that set from the existing style region in Page backing
memory:

- `layout.styles_start`
- `layout.styles_layout`

The Page wrappers added for tests and future internal Page style behavior are:

- `Page::add_style`
- `Page::get_style`
- `Page::use_style`
- `Page::release_style`
- `Page::style_ref_count`
- `Page::style_count`

These wrappers remain internal to `terminal::page`. The experiment did not add a
general `setStyle` API.

### Layout

`StyleSetLayout::init` now delegates to `style::Set::layout`, and
`StyleSetLayout::BASE_ALIGN` uses `style::Set::BASE_ALIGN`.

The style layout remains byte-for-byte compatible with the existing Page layout
tests. No Page layout numeric tests changed.

### Clone Strategy

`Page::clone_page` continues to use whole-page byte-copy:

1. allocate fresh `PageMemory`;
2. copy the source page backing bytes;
3. copy offset-backed metadata fields by value, including `styles`.

The copied `style::Set` carries only offset/layout/value metadata. It does not
store a base pointer into the source page, so style lookups on the clone use the
clone's style-region base pointer.

Style IDs are not rewritten.

### Tests Ported

The upstream test ported is:

- `Page clone styles`

Additional Roastty checks cover:

- source and clone have different backing pointers;
- cloned style IDs are unchanged;
- cloned style ref count is `1 + styled_cell_count`, matching upstream's
  add-reference plus explicit cell uses;
- releasing all source style references after clone does not affect the clone;
- adding another style to the source after release does not affect the clone;
- dropping the source before reading the clone leaves clone style data readable;
- zero-style-capacity pages still initialize `style::Set`, and style insertion
  fails with `OutOfMemory` rather than using a heap fallback.

### Deferred

This experiment intentionally did not add:

- Page `setStyle`;
- Page `clearCells` style release behavior;
- Page `moveCells` style behavior;
- Page `verifyIntegrity styles ...`;
- Page `exactRowCapacity styles ...`;
- `cloneFrom`;
- `cloneRowFrom`;
- hyperlink behavior.

### Verification

The required verification passed:

```bash
cargo fmt
cargo test -p roastty terminal::page
cargo test -p roastty terminal::style
cargo test -p roastty
```

Observed results:

- `terminal::page`: 53 passed
- `terminal::style`: 21 passed
- full `roastty` suite: 162 Rust unit tests passed, C ABI harness passed, doc
  tests passed

## Conclusion

Roastty Page storage now includes real style metadata and whole-page clone
preserves it through the same byte-copy/offset-copy invariant used for rows,
cells, and graphemes.

The next Page style work can move to a distinct operation such as style-aware
`cloneFrom`, style clearing/release behavior, style integrity checks, or
`exactRowCapacity styles`; it should remain one focused experiment.
