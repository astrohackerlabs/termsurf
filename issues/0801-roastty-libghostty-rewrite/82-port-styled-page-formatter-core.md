# Experiment 82: Port Styled Page Formatter Core

## Description

Port the reusable styled-output core of upstream
`terminal/formatter.zig::PageFormatter` into Roastty's private PageList
formatter layer.

Experiment 81 finished the plain dump-string path and left Roastty with a
private plain formatter that can preserve either unwrapped soft-wrap semantics
or visual rows. Upstream's formatter also emits styled VT and HTML output from
the same page-walking model. Roastty already has the lower-level style value
formatters:

- `Style::formatter_vt()`;
- `Style::formatter_html()`.

This experiment should connect those existing style formatters to PageList text
formatting, while keeping the scope below `Screen`, `Terminal`, public API, and
pin-map support.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/formatter.zig` for:
     - `Format`;
     - `Options`;
     - `PageListFormatter`;
     - `PageFormatter`;
     - `PageFormatter.writeCell`;
     - `PageFormatter.writeCodepoint`;
     - `formatStyleOpen`;
     - `formatStyleClose`;
     - page-level VT and HTML tests.
   - Use existing Roastty code:
     - `roastty/src/terminal/page_list.rs`;
     - `roastty/src/terminal/page.rs`;
     - `roastty/src/terminal/style.rs`.
   - Do not modify `vendor/ghostty/`.

2. Add private formatter value types.
   - Preferred shape:

     ```rust
     #[derive(Debug, Clone, Copy, PartialEq, Eq)]
     enum PageOutputFormat {
         Plain,
         Vt,
         Html,
     }

     #[derive(Debug, Clone, Copy)]
     struct PageStringOptions<'a> {
         selection: Option<selection::Selection>,
         trim: bool,
         unwrap: bool,
         emit: PageOutputFormat,
         palette: Option<&'a color::Palette>,
     }
     ```

   - Keep all new types private to `page_list.rs`.
   - `Plain` output must keep using the Experiment 81 plain semantics.
   - `Vt` and `Html` output may use a separate private formatter implementation
     if sharing the existing plain formatter would make the code harder to
     understand.

3. Add private PageList styled-string helper.
   - Preferred shape:

     ```rust
     fn page_string(&self, options: PageStringOptions<'_>) -> String
     ```

   - `PageList::selection_string()` and `PageList::dump_string()` should keep
     their current behavior. They may route through the new helper only if the
     refactor is straightforward and all existing tests remain unchanged.
   - Add private test-only convenience wrappers if they reduce duplication.
   - Do not add public API, C ABI, writer traits, `Screen`, or `Terminal`.

4. Match upstream styled output semantics for the core cell path.
   - For VT:
     - style changes emit `Style::formatter_vt()` output;
     - closing a non-default style emits `\x1b[0m`;
     - visual newlines emit `\r\n`, matching upstream VT formatter behavior;
     - unstyled text emits raw Unicode text.
   - For HTML:
     - output is wrapped in
       `<div style="font-family: monospace; white-space: pre;">...</div>`;
     - style changes emit inline `<div style="display: inline;...">...</div>`
       wrappers using `Style::formatter_html()`;
     - HTML special characters are escaped: `<`, `>`, `&`, `"`, and `'`;
     - non-ASCII codepoints are emitted as decimal numeric entities, matching
       upstream's encoding-detection guard.
   - For both styled formats:
     - codepoint and codepoint-plus-grapheme cells emit their base codepoint and
       attached grapheme codepoints;
     - blank cells inside formatted styled rows emit spaces so alignment is
       preserved;
     - background-only cells emit a space under the cell's background style if
       Roastty already has the corresponding cell content representation;
     - style state changes only when the effective cell style changes;
     - style state is closed at the end of formatting.

5. Keep known formatter features explicitly deferred.
   - Do not implement pin maps in this experiment.
   - Do not implement formatter `codepoint_map` replacement in this experiment.
   - Do not implement HTML hyperlink `<a>` emission in this experiment.
   - Do not implement terminal/screen extras such as cursor, modes, palette OSC
     emission, tabstops, keyboard modes, scrolling regions, or current working
     directory.
   - If a cell has a hyperlink, this experiment may format its text without the
     hyperlink wrapper. Add a TODO or result note if the implementation touches
     that path.

6. Add upstream-equivalent tests.
   - Add PageList/Page formatter tests for VT:
     - unstyled single line;
     - bold style;
     - multiple style transitions;
     - foreground and background colors without a palette, proving palette
       indices emit indexed SGR;
     - foreground and background colors with `Some(palette)`, proving palette
       indices emit RGB SGR;
     - styled output with a grapheme cell;
     - background-only palette and RGB cells emitting styled spaces;
     - VT newline output as `\r\n`;
     - style reset at the end.
   - Add PageList/Page formatter tests for HTML:
     - plain text wrapper;
     - basic bold style;
     - foreground/background color style without a palette, proving palette
       indices emit CSS variables;
     - foreground/background color style with `Some(palette)`, proving palette
       indices emit RGB CSS;
     - escaping `<`, `>`, `&`, `"`, and `'`;
     - non-ASCII numeric entity output;
     - styled output with a grapheme cell;
     - background-only palette and RGB cells emitting styled spaces;
     - wrapper close at the end.
   - Add guard tests:
     - existing `selection_string` and `dump_string` tests still pass;
     - cross-page formatting carries blank-row/blank-cell trailing state the
       same way Experiment 81's plain formatter does;
     - invalid or garbage selection endpoints return an empty string instead of
       panicking.
     - a hyperlinked cell formats its text without emitting `<a>` and does not
       panic, with hyperlink output explicitly recorded as deferred.

7. Keep scope narrow.
   - Do not add `Screen`, `Terminal`, parser state, cursor state, terminal
     extras, pin maps, `codepoint_map`, hyperlinks, writer abstraction, public
     ABI, app, renderer, clipboard, PTY, or UI behavior.
   - Do not expose styled formatting outside the terminal module.
   - Do not change selection, line iterator, prompt-click, or plain dump-string
     behavior except for internal refactors required to share row iteration.

8. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty page_string
     cargo test -p roastty dump_string
     cargo test -p roastty selection_string
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

9. Independent review.
   - Before implementation, get Codex review of this experiment design.
   - Record the design-review outcome in this experiment file before
     implementation.
   - After implementation and verification, get Codex review of the completed
     result.
   - Fix all real findings before proceeding.

10. Record the result.
    - Append `## Result` and `## Conclusion` to this file.
    - Include:
      - helper names and location;
      - whether the implementation reused or paralleled the plain formatter;
      - which upstream styled formatter behaviors were ported;
      - which formatter features remain deferred;
      - verification command output summary;
      - Codex design-review outcome;
      - Codex result-review outcome.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Verification

The experiment passes if:

- Roastty can privately format PageList/Page text as VT and HTML for the core
  styled cell path;
- VT output emits upstream-style SGR transitions, reset sequences, raw Unicode
  text, and `\r\n` line endings;
- HTML output emits the upstream monospace wrapper, inline style wrappers, HTML
  escaping, and non-ASCII numeric entities;
- grapheme cells, blank cells inside styled rows, and background-only cells
  behave like the scoped upstream formatter path;
- styled palette colors are covered both with and without a concrete palette;
- hyperlinked cells do not panic and are explicitly formatted without hyperlink
  wrappers until the deferred hyperlink formatter slice;
- existing plain `selection_string` and `dump_string` behavior remains
  unchanged;
- invalid or garbage endpoints return an empty string instead of panicking;
- no `Screen`, `Terminal`, parser state, cursor state, terminal extras, pin
  maps, `codepoint_map`, hyperlinks, writer abstraction, public ABI, app,
  renderer, clipboard, PTY, or UI behavior is added;
- `cargo fmt`, targeted styled formatter tests, plain formatter regression
  tests, PageList tests, and full `cargo test -p roastty` pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- unstyled and basic styled output works, but a specific style transition,
  grapheme, background-only, or cross-page trailing-state behavior exposes a
  missing lower-level primitive that should be split into the next experiment.

The experiment fails if:

- styled formatting cannot be implemented without adding `Screen`, `Terminal`,
  parser state, public API, app, renderer, PTY, clipboard, or UI behavior;
- plain selection or dump-string behavior regresses;
- VT or HTML output diverges from the scoped upstream formatter behavior;
- background-only cells or palette-vs-indexed color modes are untested;
- invalid pins panic;
- tests or formatting fail.

## Design Review

Codex reviewed the initial design and found no blockers. It agreed that the
styled formatter core is a coherent next slice after Experiment 81 because it
stays private to PageList/Page formatting, uses the already-ported style
formatters, and explicitly defers `Screen`, `Terminal`, pin maps, codepoint
maps, hyperlink wrappers, and public API.

Codex identified three high-value improvements before implementation:

- require explicit background-only cell tests, since upstream emits
  background-only cells as styled spaces and Roastty already has background-only
  cell representations;
- test both styled color modes: palette indexes without a concrete palette and
  RGB output when a palette is supplied;
- add a hyperlink guard test proving hyperlinked cells do not panic and format
  their text without `<a>` while HTML hyperlink emission remains deferred.

The design now requires those tests and keeps hyperlink wrappers explicitly out
of this experiment's scope.

## Result

**Result:** Pass

Implemented private styled PageList/Page formatting in
`roastty/src/terminal/page_list.rs`:

- added private `PageOutputFormat` for `Plain`, `Vt`, and `Html`;
- added private `PageStringOptions` with selection, trim, unwrap, output format,
  and optional palette inputs;
- added private `StyledPageFormat` for VT/HTML cell emission;
- routed the existing plain formatter through `page_string()` while preserving
  Experiment 81 plain `selection_string()` and `dump_string()` behavior;
- implemented VT style transitions with existing `Style::formatter_vt()`;
- implemented HTML monospace wrapper, inline style wrappers, escaping, and
  non-ASCII numeric entities using existing `Style::formatter_html()`;
- implemented styled grapheme output, background-only cells in rows that contain
  text, optional concrete-palette color output, invalid/garbage endpoint guards,
  and style reset before pending blank-row newlines.

The implementation remains private to the terminal module. It does not add
`Screen`, `Terminal`, parser state, cursor state, terminal extras, pin maps,
`codepoint_map`, HTML hyperlink wrappers, writer abstraction, public ABI, app,
renderer, clipboard, PTY, or UI behavior.

Added 12 styled formatter tests covering:

- VT unstyled single-line output;
- VT bold style output and final reset;
- VT multiple style transitions;
- VT palette-index output and concrete-palette RGB output;
- VT background-only cells inside a row that also contains text;
- upstream-compatible all-background-row skipping;
- VT grapheme output and `\r\n` line endings;
- VT style close before pending blank-row newlines across a page boundary;
- invalid/garbage styled endpoints returning an empty string for VT and HTML;
- HTML plain text wrapper;
- HTML style wrappers, palette-index CSS variables, and concrete-palette RGB
  CSS;
- HTML escaping, non-ASCII numeric entities, grapheme output, and hyperlinked
  cells formatting text without `<a>` while hyperlink wrappers remain deferred.

Verification passed:

```bash
cargo fmt
cargo test -p roastty page_string
cargo test -p roastty dump_string
cargo test -p roastty selection_string
cargo test -p roastty terminal::page_list
cargo test -p roastty
```

Observed results after the final fixes:

- `cargo test -p roastty page_string`: 12 passed.
- `cargo test -p roastty dump_string`: 13 passed.
- `cargo test -p roastty selection_string`: 22 passed.
- `cargo test -p roastty terminal::page_list`: 451 passed.
- `cargo test -p roastty`: 744 unit tests passed, ABI harness passed, and
  doctests passed.

Codex reviewed the completed implementation and found three real issues:

- active style was not closed before pending blank-row newlines, which could let
  VT/HTML style state bleed into leading cells on later rows;
- styled invalid/garbage endpoint coverage was missing;
- styled cross-page trailing-state coverage was missing.

The implementation now closes active style before emitting pending blank-row
newlines, and the missing guard/cross-page tests were added.

Codex also initially flagged all-background rows. Re-checking upstream showed
that `PageFormatter` calls `Cell.hasTextAny(...)` before cell emission, and
`BgColorPalette` / `BgColorRgb` cells do not count as text. The implemented
behavior now has explicit tests for both cases:

- a background-only cell in a row with text emits a styled space;
- a row containing only background-only cells is skipped, matching upstream's
  precheck.

Follow-up Codex review approved the corrected implementation with no remaining
blockers.

## Conclusion

Experiment 82 successfully ports the scoped styled PageFormatter core into
Roastty's private PageList layer. Roastty can now produce upstream-style VT and
HTML strings for text, styles, palette color modes, graphemes, HTML escaping,
and scoped background-only cells without adding `Screen`, `Terminal`, public
API, pin maps, codepoint maps, hyperlink wrappers, or terminal extras.

The next experiment can continue with the deferred formatter pieces. The most
natural next slices are pin-map support, codepoint replacement maps, HTML
hyperlink emission, or the higher-level ScreenFormatter extras once their
dependencies are ready.
