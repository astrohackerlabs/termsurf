# Experiment 81: Port Dump String Helpers

## Description

Port the reusable core of upstream `Screen.dumpString()`,
`Screen.dumpStringAlloc()`, and `Screen.dumpStringAllocUnwrapped()` into
Roastty's current PageList-centered terminal model.

Experiment 79 added private plain selection-string formatting. Upstream's next
helper after prompt-click movement is `dumpString()`, which is mostly a
screen-dump wrapper around the same formatter surface:

- it emits plain text;
- it does not trim trailing spaces;
- it accepts top-left and optional bottom-right pins;
- it ignores the x values of those pins and dumps complete rows;
- it can either unwrap soft-wrapped rows or preserve visual rows.

Roastty does not have `Screen` yet, so this experiment should port the reusable
PageList part only. The future `Screen` wrapper can later supply screen-domain
pins and expose the app-facing helper once Screen exists.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/Screen.zig` for:
     - `dumpString`;
     - `dumpStringAlloc`;
     - `dumpStringAllocUnwrapped`;
     - nearby tests that call `dumpStringAlloc(...)`.
   - Use existing Roastty code:
     - `roastty/src/terminal/page_list.rs`;
     - the `PlainPageFormat`, `SelectionStringOptions`, and
       `PageList::selection_string()` code added in Experiment 79.
   - Do not modify `vendor/ghostty/`.

2. Refactor the private plain formatter options.
   - Add an unwrap flag to the private formatting path.
   - Preferred shape:

     ```rust
     #[derive(Debug, Clone, Copy, PartialEq, Eq)]
     struct PlainStringOptions {
         selection: Option<selection::Selection>,
         trim: bool,
         unwrap: bool,
     }
     ```

   - Replace or extend `SelectionStringOptions` only inside `page_list.rs`. Keep
     all formatter types private.
   - Preserve `PageList::selection_string()` behavior by routing it through the
     shared formatter with `unwrap: true`, matching upstream selection-copy
     semantics and the Experiment 79 tests.

3. Teach `PlainPageFormat` to respect `unwrap`.
   - When `unwrap` is `true`, preserve Experiment 79 behavior:
     - soft-wrapped rows join without newlines;
     - wrap-continuation rows preserve pending trailing spaces;
     - a selection ending on `Wide::SpacerHead` can extend into the next row.
   - When `unwrap` is `false`, dump visual rows:
     - visual row boundaries can emit newlines when followed by later text;
     - pending trailing spaces do not carry across rows;
     - the `Wide::SpacerHead` end extension must not join the following row.
   - Keep rectangular selection behavior unchanged.
   - Keep trim behavior independent from unwrap behavior. `dump_string` should
     pass `trim: false`.
   - Match upstream trailing-row behavior: pending trailing blank rows are not
     flushed at the end of formatting. A dump may include leading blank rows
     before later text, but it must not automatically end with newlines for
     empty trailing screen rows.

4. Add private PageList dump-string helpers.
   - Preferred shape:

     ```rust
     fn dump_string(&self, top_left: Pin, bottom_right: Option<Pin>, unwrap: bool) -> String
     ```

   - Match upstream `dumpString()` semantics:
     - if `bottom_right` is `None`, use `get_bottom_right(point::Tag::Screen)`;
     - if either endpoint is invalid or garbage, return an empty string instead
       of panicking;
     - ignore endpoint x values by normalizing the start x to `0` and the end x
       to the last column of the relevant row/page;
     - create a non-rectangular selection spanning those rows;
     - emit plain text with `trim: false` and the caller's `unwrap` value.
   - Add private convenience wrappers only if they reduce test duplication. If
     added, keep them point- or pin-based and private to the test/module layer.
   - Do not add public API, C ABI, `Screen`, `Terminal`, writer traits, or
     allocation APIs in this experiment.

5. Add upstream-equivalent tests.
   - Add tests for dump-string behavior that cover:
     - basic single-row and multi-row screen dumps;
     - ignoring top-left and bottom-right x values;
     - `bottom_right: None` using the screen bottom-right pin;
     - `unwrap: true` joining soft-wrapped rows;
     - `unwrap: false` preserving visual row boundaries;
     - no trimming of trailing spaces;
     - wide characters and `Wide::SpacerHead` at row boundaries;
     - invalid and garbage pins returning an empty string;
     - tracked pins using their current locations;
     - screen-domain dumping across scrollback pages when present.
   - Add explicit cross-page formatter-state tests:
     - `unwrap: true` with a soft-wrapped line and pending trailing spaces
       spanning two PageList nodes;
     - `unwrap: false` spanning two PageList nodes, proving pending blank cells
       reset per visual row while blank rows before later text still bridge page
       chunks correctly.
   - Add final-newline tests that prove upstream behavior:
     - leading blank rows before later text are emitted;
     - trailing blank screen rows are omitted instead of forcing final newlines.
   - Keep existing Experiment 79 selection-string tests unchanged and add a
     regression assertion that selection strings still unwrap soft-wrapped rows.

6. Keep scope narrow.
   - Do not add `Screen`, `Terminal`, cursor state, parser state, VT formatter,
     HTML formatter, pin-map formatter, writer abstraction, public ABI, app,
     renderer, clipboard, PTY, or UI behavior.
   - Do not expose `dump_string` outside the terminal module.
   - Do not change selection, line iterator, or prompt-click behavior except for
     the internal formatter refactor required to share the plain formatter.

7. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty dump_string
     cargo test -p roastty selection_string
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

8. Independent review.
   - Before implementation, get Codex review of this experiment design.
   - Record the design-review outcome in this experiment file before
     implementation.
   - After implementation and verification, get Codex review of the completed
     result.
   - Fix all real findings before proceeding.

9. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - helper names and location;
     - formatter refactor details;
     - which upstream dump-string behaviors were ported;
     - which Screen/writer/allocation pieces are intentionally deferred;
     - verification command output summary;
     - Codex design-review outcome;
     - Codex result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- Roastty has private PageList dump-string helpers equivalent to the reusable
  PageList/core behavior of upstream `Screen.dumpString()`;
- `dump_string` ignores endpoint x values, defaults a missing bottom-right pin
  to the screen bottom-right, emits full rows, uses `trim: false`, and supports
  both unwrapped and visual-row output;
- existing selection-string behavior remains unchanged and still unwraps
  soft-wrapped rows;
- invalid or garbage pins return an empty string instead of panicking;
- no `Screen`, `Terminal`, cursor state, parser state, VT formatter, HTML
  formatter, pin-map formatter, writer abstraction, public ABI, app, renderer,
  clipboard, PTY, or UI behavior is added;
- `cargo fmt`, targeted dump-string tests, selection-string regression tests,
  PageList tests, and full `cargo test -p roastty` pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- unwrapped dump strings work, but preserving visual rows exposes a missing
  lower-level formatter behavior that should be split into the next experiment;
- same-page dump strings work, but cross-page trailing-space or newline state
  exposes a formatter state bug that needs a narrower follow-up.

The experiment fails if:

- dump-string behavior cannot be implemented without adding `Screen`, public
  API, writer/allocation APIs, parser state, renderer, PTY, clipboard, app, or
  UI behavior;
- selection-string behavior regresses;
- soft-wrap unwrapping or visual-row preservation is incorrect;
- leading blank rows before content or trailing blank screen rows diverge from
  upstream formatter behavior;
- invalid pins panic;
- tests or formatting fail.

## Design Review

Codex reviewed the initial design and found no blockers. It identified two
high-value improvements before implementation:

- make cross-page formatter-state coverage explicit for both `unwrap: true` and
  `unwrap: false`, because carrying pending blank rows/cells across PageList
  chunks is the riskiest part of this formatter refactor;
- clarify upstream final-newline behavior. The formatter carries pending blank
  rows and flushes them only before later text, so leading blank rows before
  content are emitted but trailing blank screen rows are not automatically
  flushed as final newlines.

The design now requires those tests and clarifies that visual-row output emits
newlines only when followed by later text, matching upstream's trailing-state
behavior. Codex found the slice otherwise coherent and appropriately narrow.

## Result

**Result:** Pass

Implemented private dump-string support in `roastty/src/terminal/page_list.rs`:

- added private `PlainStringOptions` so plain formatting can choose `trim` and
  `unwrap` independently;
- kept `PageList::selection_string()` routed through the shared formatter with
  `unwrap: true`, preserving Experiment 79 selection-copy behavior;
- added `PlainPageFormat::unwrap` so unwrapped output joins soft-wrapped rows
  while visual-row output resets trailing cell state at row boundaries;
- added private `PageList::dump_string(top_left, bottom_right, unwrap)`, which
  normalizes endpoint x values to full rows, defaults a missing bottom-right pin
  to the screen bottom-right, emits plain text with `trim: false`, and returns
  an empty string for invalid or garbage pins.

The implementation intentionally does not add `Screen`, `Terminal`, cursor
state, parser state, VT formatter, HTML formatter, pin-map formatter, writer
abstraction, public ABI, app, renderer, clipboard, PTY, or UI behavior.

Added 13 dump-string tests covering:

- basic single-row and multi-row output;
- endpoint x values being ignored;
- default bottom-right screen pin behavior;
- `unwrap: true` soft-wrap joining;
- `unwrap: false` visual-row preservation;
- explicit spaces not being trimmed;
- wide cells and `Wide::SpacerHead` behavior in both unwrap modes;
- invalid and garbage pins returning an empty string;
- tracked pin locations;
- screen-domain dumping across scrollback;
- cross-page trailing-state behavior for both unwrapped and visual-row output;
- leading blank rows before later text being emitted while trailing blank screen
  rows are not flushed.

Verification passed:

```bash
cargo fmt
cargo test -p roastty dump_string
cargo test -p roastty selection_string
cargo test -p roastty terminal::page_list
cargo test -p roastty
```

Observed results:

- `cargo test -p roastty dump_string`: 13 passed.
- `cargo test -p roastty selection_string`: 22 passed.
- `cargo test -p roastty terminal::page_list`: 439 passed.
- `cargo test -p roastty`: 732 unit tests passed, ABI harness passed, and
  doctests passed.

Codex reviewed the completed implementation and found one real test gap: the
wide/spacer test proved `unwrap: true` extended from an end pin on
`Wide::SpacerHead`, but it did not directly prove `unwrap: false` avoided that
extension. Added an explicit `unwrap: false` assertion ending on the
`Wide::SpacerHead` itself and reran the full verification sequence.

Follow-up Codex review approved the updated result with no remaining blockers.
It confirmed that the formatter unwrap refactor, dump-string row normalization,
cross-page trailing-state tests, spacer-head end handling, and final-newline
behavior satisfy the Experiment 81 design.

## Conclusion

Experiment 81 successfully ports the reusable plain dump-string core behind
upstream `Screen.dumpString()` into Roastty's private PageList layer. Roastty
can now produce full-row plain dumps with either upstream unwrapped soft-wrap
semantics or visual-row semantics, without exposing public API or introducing
the later `Screen` wrapper.

The next experiment can continue with the remaining formatter surface after the
plain dump-string path, most likely the VT/HTML/pin-map formatter variants or
the next upstream terminal helper that depends on copied screen text.
