# Experiment 78: Port Line Iterator

## Description

Port upstream `Screen.LineIterator` / `Screen.lineIterator` into Roastty's
current PageList-centered terminal model.

Experiments 75-77 completed the private PageList line, select-all, and
semantic-output selection primitives. Upstream `LineIterator` is the next small
selection-layer helper: starting at a pin, repeatedly return full soft-wrapped
line selections with whitespace trimming disabled and semantic prompt boundaries
disabled, then advance to the row after the selected line.

This experiment should add only the PageList-local line iterator and tests. It
must not add `Screen`, `Terminal`, `ScreenFormatter`, `selectionString`,
string-map support, gesture state, public ABI, renderer, parser, app, platform
input, mouse event behavior, clipboard behavior, or UI wiring.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/Screen.zig` for:
     - `LineIterator`;
     - `lineIterator`;
     - `test "Screen: lineIterator"`;
     - `test "Screen: lineIterator soft wrap"`.
   - Use existing Roastty code:
     - `roastty/src/terminal/page_list.rs`;
     - `roastty/src/terminal/selection.rs`;
     - the private `SelectLineOptions` and `PageList::select_line` helper from
       Experiment 75.
   - Do not modify `vendor/ghostty/`.

2. Add a private PageList line iterator type.
   - Preferred shape:

     ```rust
     struct LineIterator<'a> {
         list: &'a PageList,
         current: Option<Pin>,
     }

     impl Iterator for LineIterator<'_> {
         type Item = selection::Selection;
     }
     ```

   - `next()` should match upstream:
     - if `current` is `None`, return `None`;
     - call `select_line` with:
       - `pin = current`;
       - `whitespace = None`;
       - `semantic_prompt_boundary = false`;
     - if `select_line` returns `None`, set `current = None` and return `None`;
     - otherwise set `current` to one row below `result.end()` using the
       existing PageList pin movement helper. If there is no row below, set
       `current = None`;
     - return the selected line.
   - Returned selections should be untracked and non-rectangular because they
     come from `select_line`.

3. Add a private PageList constructor helper.
   - Preferred shape:

     ```rust
     fn line_iterator(&self, start: Pin) -> LineIterator<'_>
     ```

   - Invalid or garbage start pins should produce an iterator whose first
     `next()` returns `None`.
   - Keep the helper private. Do not add public API or C ABI.

4. Add upstream-equivalent tests.
   - Port `Screen: lineIterator` as selection-bound tests:
     - with a `5x5` PageList containing `1ABCD` and `2EFGH`, starting at the
       viewport/screen top-left yields `(0, 0)..(4, 0)`, `(0, 1)..(4, 1)`,
       `(0, 2)..(4, 2)`, `(0, 3)..(4, 3)`, and `(0, 4)..(4, 4)`, then `None`.
       This matches upstream because `LineIterator` disables whitespace
       trimming, so empty physical rows are still yielded as full-row
       selections.
   - Port `Screen: lineIterator soft wrap` as selection-bound tests:
     - with a `5x5` PageList containing `1ABCD2EFGH` as a soft-wrapped line
       followed by `3ABCD`, starting at the viewport/screen top-left yields
       `(0, 0)..(4, 1)`, `(0, 2)..(4, 2)`, `(0, 3)..(4, 3)`, and
       `(0, 4)..(4, 4)`, then `None`.
   - Add Roastty-specific coverage:
     - starting from a non-wrapped second physical row yields that row first;
     - starting from the continuation row of a soft-wrapped line yields the full
       soft-wrapped line, not only that physical row;
     - invalid and garbage start pins immediately return `None`;
     - iterator selections are untracked and non-rectangular;
     - a scrollback/history fixture proves the iterator uses the supplied start
       pin's coordinate domain and can yield selections outside the active
       viewport when started from a screen pin.
   - Because Roastty does not yet have `selectionString`, verify selection
     bounds rather than copied text.

5. Keep scope narrow.
   - Do not add selection string extraction or formatter behavior.
   - Do not add gesture, keyboard shortcut, clipboard, app, terminal, or UI
     behavior.
   - Do not add public API or C ABI exposure.

6. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty line_iterator
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

7. Independent review.
   - Before implementation, get Codex review of this experiment design.
   - Record the design-review outcome in this experiment file before
     implementation.
   - After implementation and verification, get Codex review of the completed
     result.
   - Fix all real findings before proceeding.

8. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - helper names and location;
     - upstream test coverage;
     - Roastty-specific edge tests;
     - verification command output summary;
     - Codex design-review outcome;
     - Codex result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- Roastty has a private PageList line iterator equivalent to upstream
  `LineIterator`;
- each yielded selection is the full soft-wrapped line with whitespace trimming
  disabled and semantic boundaries disabled;
- iteration advances to the row after the selected line and stops at the bottom
  of the screen domain;
- invalid or garbage start pins return no selections;
- iterator selections are untracked and non-rectangular;
- no `Screen`, `Terminal`, `ScreenFormatter`, `selectionString`, string-map
  support, gesture state, public ABI, renderer, parser, app, platform input,
  mouse event behavior, clipboard behavior, or UI wiring is added;
- `cargo fmt`, targeted line-iterator tests, PageList tests, and full
  `cargo test -p roastty` pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- ordinary line iteration works, but scrollback or start-domain behavior exposes
  a missing PageList fixture that must be split into its own prerequisite
  experiment.

The experiment fails if:

- line iteration cannot be implemented without adding Screen, Terminal,
  formatter, ABI, renderer, parser, app, or platform input behavior;
- line iteration trims whitespace or applies semantic boundaries;
- soft-wrapped and hard-bounded rows are yielded incorrectly;
- invalid pins panic instead of returning `None`;
- tests or formatting fail.

## Design Review

Codex reviewed the initial design and found two real blockers:

- the upstream-equivalent tests incorrectly expected iteration to stop after the
  written rows, even though upstream disables whitespace trimming and therefore
  yields empty physical rows through the bottom of the screen;
- "starting from the second physical row" was ambiguous for soft-wrap
  continuations.

The design now requires full-screen empty-row yields for the basic and soft-wrap
upstream-equivalent tests, and separates non-wrapped second-row behavior from a
soft-wrap-continuation start test.

Follow-up Codex review approved the updated design with no remaining blockers.

## Result

**Result:** Pass

Implemented private PageList line iteration in
`roastty/src/terminal/page_list.rs`:

- `LineIterator<'a>` stores a `&PageList` and the next `Pin`.
- `impl Iterator for LineIterator<'_>` calls `PageList::select_line()` with
  `whitespace = None` and `semantic_prompt_boundary = false`, matching upstream
  `Screen.LineIterator`.
- `PageList::line_iterator()` creates the private iterator and makes invalid or
  garbage start pins yield no selections.
- Iteration advances with `pin_down(result.end(), 1)` and stops at the bottom of
  the screen domain.

The implementation stays private to PageList. It does not add `Screen`,
`Terminal`, `ScreenFormatter`, `selectionString`, string-map support, gesture
state, public ABI, renderer, parser, app, platform input, mouse behavior,
clipboard behavior, or UI wiring.

The tests cover:

- upstream-equivalent basic line iteration over a `5x5` screen, including empty
  physical rows through the bottom;
- upstream-equivalent soft-wrap iteration, including the full wrapped first line
  and empty physical rows through the bottom;
- non-wrapped second-row starts;
- soft-wrap continuation starts returning the full soft-wrapped line;
- invalid and garbage starts returning no selections;
- screen-domain behavior across scrollback/history rows;
- untracked non-rectangular selection shape.

Verification passed:

```bash
cargo fmt
cargo test -p roastty line_iterator
cargo test -p roastty terminal::page_list
cargo test -p roastty
```

Observed results:

- `cargo test -p roastty line_iterator`: 6 passed.
- `cargo test -p roastty terminal::page_list`: 383 passed.
- `cargo test -p roastty`: 676 unit tests passed, ABI harness passed, and
  doctests passed.

Codex reviewed the implementation and found no implementation blockers. It also
ran `cargo test -p roastty line_iterator`, which passed. Its only findings were
to record this result and update the README status.

## Conclusion

Experiment 78 successfully ports upstream `LineIterator` into Roastty's
PageList-centered terminal layer. Roastty can now iterate selections over full
soft-wrapped lines, including empty rows, with whitespace trimming and semantic
boundaries disabled exactly as upstream does.

The next experiment can continue upward into selection text extraction or the
formatter layer that will make these selection spans copyable as strings.
