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
