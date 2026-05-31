# Experiment 65: Port Selection Codepoints

## Description

Port upstream `selection_codepoints.zig` into Roastty.

Upstream Ghostty keeps default selection word-boundary and line-whitespace
codepoint tables in a separate file so selection, selection gestures, and C
selection wrappers can share them without depending on the full selection
module. Roastty does not have these tables yet, but later selection work needs
the exact same defaults.

This experiment should add only the shared codepoint constants and tests. It
must not add selection logic, selection gestures, C ABI selection wrappers,
formatters, Screen behavior, search, renderer, parser, or app behavior.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/selection_codepoints.zig`.
   - Port:
     - `default_word_boundaries`;
     - `default_line_whitespace`.
   - Do not modify `vendor/ghostty/`.

2. Add a Roastty module.
   - Add `roastty/src/terminal/selection_codepoints.rs`.
   - Add it to `roastty/src/terminal/mod.rs` with the same internal visibility
     style as the other terminal modules.
   - Use Roastty/Rust naming:
     - `DEFAULT_WORD_BOUNDARIES`;
     - `DEFAULT_LINE_WHITESPACE`.
   - Give both constants terminal-internal visibility:
     - `pub(super) const DEFAULT_WORD_BOUNDARIES`;
     - `pub(super) const DEFAULT_LINE_WHITESPACE`.
   - Do not expose these constants through crate-public or C ABI surfaces.
   - Represent entries as `u32` codepoints, matching Roastty `Cell` codepoint
     storage and upstream `u21` values.

3. Preserve exact upstream table contents and order.
   - `DEFAULT_WORD_BOUNDARIES` must contain, in order:
     - null;
     - space;
     - tab;
     - single quote;
     - double quote;
     - U+2502 box drawing vertical line;
     - backtick;
     - pipe;
     - colon;
     - semicolon;
     - comma;
     - left and right parens;
     - left and right brackets;
     - left and right braces;
     - less-than and greater-than;
     - dollar.
   - `DEFAULT_LINE_WHITESPACE` must contain, in order:
     - null;
     - space;
     - tab.

4. Add tests.
   - Add tests in `selection_codepoints.rs` proving:
     - word-boundary values exactly match the upstream contents and order;
     - line-whitespace values exactly match the upstream contents and order;
     - every line-whitespace codepoint is also present in
       `DEFAULT_WORD_BOUNDARIES`;
     - no duplicate values exist in either table.

5. Keep scope narrow.
   - Do not add `Selection`.
   - Do not add selection gestures.
   - Do not add C ABI.
   - Do not add formatter behavior.
   - Do not add Screen, search, renderer, parser, or app behavior.
   - Do not alter highlight, PageList, Page, style, or existing terminal
     behavior.

6. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal::selection_codepoints
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

7. Independent review.
   - Before implementation, get an independent agent review of this experiment
     design.
   - After implementation and verification, get an independent result review.
   - Fix all real findings before proceeding.

8. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - constants added;
     - exact table parity;
     - tests added;
     - verification command output summary;
     - independent result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `roastty/src/terminal/selection_codepoints.rs` exists;
- `terminal::selection_codepoints` is registered in `terminal/mod.rs`;
- both constants have terminal-internal `pub(super)` visibility or equivalent;
- neither constant is exposed through crate-public or C ABI surfaces;
- `DEFAULT_WORD_BOUNDARIES` exactly matches upstream contents and order;
- `DEFAULT_LINE_WHITESPACE` exactly matches upstream contents and order;
- line-whitespace values are all included in word-boundary values;
- tests prove exact values, ordering, and duplicate-free tables;
- no selection, selection gesture, C ABI, formatter, Screen, search, renderer,
  parser, app, highlight, PageList, Page, style, or other terminal behavior
  changes are introduced;
- `cargo fmt`, targeted selection-codepoint tests, and full
  `cargo test -p roastty` pass;
- independent design and result reviews approve the experiment, or all real
  findings are fixed before proceeding.

The experiment is partial if:

- the constants exist, but a naming, visibility, or test-parity gap needs a
  follow-up.

The experiment fails if:

- the constants differ from upstream contents or order;
- the constants are private to the module and therefore unusable by future
  sibling terminal modules;
- the constants become crate-public or C ABI surface;
- duplicate or omitted values are introduced;
- selection logic, ABI, Screen, formatter, search, renderer, parser, app, or
  unrelated terminal behavior is added prematurely;
- tests or formatting fail.

## Result

**Result:** Pass

Experiment 65 added `roastty/src/terminal/selection_codepoints.rs` and
registered it in `roastty/src/terminal/mod.rs` using the existing internal
module pattern.

The new module contains terminal-internal constants:

- `DEFAULT_WORD_BOUNDARIES`;
- `DEFAULT_LINE_WHITESPACE`.

Both constants are `pub(super)`, so future sibling terminal modules can use them
without exposing them as crate-public API or C ABI surface. The entries are
stored as `u32` codepoints, matching Roastty cell codepoint storage and the
upstream `u21` table values.

The tests added coverage for:

- exact `DEFAULT_WORD_BOUNDARIES` contents and order;
- exact `DEFAULT_LINE_WHITESPACE` contents and order;
- every line-whitespace codepoint being present in the word-boundary table;
- duplicate-free word-boundary and line-whitespace tables.

Verification:

```bash
cargo fmt
cargo test -p roastty terminal::selection_codepoints
cargo test -p roastty
```

Results:

- `cargo fmt` passed.
- `cargo test -p roastty terminal::selection_codepoints` passed: 4 tests, 0
  failed.
- `cargo test -p roastty` passed: 569 unit tests plus 1 ABI harness test, 0
  failed.

Independent result review approved the experiment with no blocking findings. The
reviewer confirmed exact upstream parity, correct `pub(super)` visibility, no
crate-public or ABI exposure, and no scope drift into selection logic, gestures,
C ABI, formatter, Screen, search, renderer, parser, app, highlight, PageList,
Page, style, or other behavior.

## Conclusion

Roastty now has the shared default selection codepoint tables that future word
selection, line selection, selection gesture, and selection formatting slices
can reuse. The next experiment can begin porting a narrow selection value-type
slice without first inventing or duplicating these defaults.
