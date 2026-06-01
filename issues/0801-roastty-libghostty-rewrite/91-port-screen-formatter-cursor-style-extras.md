# Experiment 91: Port Screen Formatter Cursor and Style Extras

## Description

Port the first usable subset of upstream
`terminal/formatter.zig::ScreenFormatter.Extra`: VT-only cursor position and
cursor style emission.

Experiment 90 added a private terminal-level formatter wrapper, but it
intentionally emitted no extras because Roastty had no screen or terminal state
for them. Upstream emits screen extras after content so replay restores state
that content formatting changes, especially cursor position and active SGR
style. Roastty already has the `Style` value type and VT style formatter, so the
next small faithful slice is to add minimal screen cursor state and wire only
the cursor/style extra output.

This experiment must not attempt all upstream extras. Hyperlink, protection,
Kitty keyboard, and charset extras require additional screen/parser state and
should be separate experiments. Terminal-level extras such as palette, modes,
scrolling region, tabstops, keyboard mode, and PWD also remain deferred.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/formatter.zig` for:
     - `ScreenFormatter.Extra`;
     - the ordering of screen extra output after content;
     - cursor position output using CUP (`CSI row;col H`);
     - cursor style output using the active SGR style stored on
       `screen.cursor.style`, not the visual cursor shape;
     - pin-map handling for extra bytes.
   - Use `vendor/ghostty/src/terminal/Screen.zig` for the cursor state shape,
     but port only the fields needed by this experiment.
   - Do not modify `vendor/ghostty/`.

2. Add minimal private screen cursor state.
   - In `roastty/src/terminal/screen.rs`, extend private `Screen` with a private
     cursor field, for example:

     ```rust
     struct ScreenCursor {
         x: CellCountInt,
         y: CellCountInt,
         style: style::Style,
     }
     ```

   - Initialize the cursor to `(0, 0)` and default style in `Screen::init`.
   - Keep cursor state private to the `terminal` module.
   - Add `#[cfg(test)] pub(super)` helpers to set cursor position and style for
     tests.
   - Do not add parser-driven cursor movement, saved cursor, hyperlink state,
     protection state, semantic cursor state, or page-pin cursor caches in this
     experiment.

3. Add a narrow private `ScreenFormatterExtra`.
   - Add a private extra type in `screen.rs` that currently supports only:
     - `cursor: bool`;
     - `style: bool`.
   - Add a `none` constructor/default.
   - Add a `with_extra(...)` builder on `ScreenFormatter`.
   - Do not add placeholder fields for hyperlink, protection, Kitty keyboard, or
     charsets. Keep this as a two-boolean slice so the code does not imply
     unsupported screen state exists.

4. Emit extras only for VT output.
   - `PageOutputFormat::Plain` and `PageOutputFormat::Html` must ignore screen
     extras, matching upstream's rule that screen state extras are VT-only.
   - For VT output, append extras after content.
   - If `extra.style` is set, append `screen.cursor.style.formatter_vt()`.
   - If `extra.cursor` is set, append CUP with 1-indexed cursor coordinates:
     `\x1b[{row};{col}H`.
   - Preserve upstream ordering for the ported subset: style before cursor.
   - Do not emit cursor/style extras when both booleans are false.

5. Preserve pin-map semantics for extra bytes.
   - `format_with_pin_map()` must append one pin entry for every extra byte.
   - Choose the extra pin after content formatting and before appending extras.
     If the pin map is non-empty, clone its last pin. Otherwise resolve the
     screen top-left pin from `screen.pages`.
   - This must inspect the actual post-content pin-map length rather than guess
     from the requested content mode, because selections can be invalid or
     empty.
   - Pin maps must remain byte-indexed: `text.len() == pin_map.len()` for VT
     output with extras.

6. Keep TerminalFormatter delegation intact.
   - Do not add terminal extras.
   - If `ScreenFormatter` gains an `extra` field, make sure `TerminalFormatter`
     still delegates default content output unchanged.
   - Do not add `TerminalFormatterExtra` yet unless required to keep the code
     compiling. Terminal-level forwarding of screen extras belongs in a later
     experiment.

7. Add upstream-equivalent tests.
   - Add ScreenFormatter tests for:
     - VT cursor extra appends the expected CUP sequence after content;
     - VT style extra appends a non-default active SGR style sequence after
       content, using a visible style such as bold or palette foreground;
     - style and cursor extras together emit in upstream order;
     - plain and HTML output ignore cursor/style extras;
     - `Content::None` with cursor/style extras emits only extras;
     - VT pin maps with content map extra bytes to the last content pin;
     - VT pin maps with no content map extra bytes to the top-left pin;
     - default/no-extra ScreenFormatter output is unchanged.
   - Add a TerminalFormatter regression test proving the Experiment 90 default
     path still matches ScreenFormatter output when no extras are requested.
   - Keep existing PageList, ScreenFormatter content, and TerminalFormatter
     content tests passing.

8. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty screen_formatter
     cargo test -p roastty terminal_formatter
     cargo test -p roastty styled_pin_map
     cargo test -p roastty pin_map
     cargo test -p roastty page_string
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

9. Independent review.
   - Before implementation, get Codex review of this experiment design.
   - Fix all real design findings before implementation.
   - Record the design-review outcome in this experiment file before
     implementation.
   - After implementation and verification, get Codex review of the completed
     result.
   - Fix all real result findings before proceeding.

10. Record the result.
    - Append `## Result` and `## Conclusion` to this file.
    - Include:
      - the screen cursor state added and its visibility;
      - the exact VT extra sequences emitted;
      - how plain/HTML ignore extras;
      - how pin-map entries for extra bytes are assigned;
      - why hyperlink/protection/Kitty keyboard/charset and terminal-level
        extras remain deferred;
      - verification command output summary;
      - Codex design-review outcome;
      - Codex result-review outcome.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `Screen` has private minimal cursor position/style state;
- `ScreenFormatter` can emit VT cursor and style extras after content;
- style extra output uses the existing `style::Style::formatter_vt()` path;
- cursor extra output uses 1-indexed CUP coordinates;
- style is emitted before cursor when both extras are enabled;
- plain and HTML output ignore screen extras;
- no-extra formatter output remains unchanged;
- `Content::None` can emit VT extras without content;
- pin maps remain byte-indexed and map extra bytes to the last content pin or
  top-left pin when there is no content;
- TerminalFormatter's default content delegation from Experiment 90 remains
  unchanged;
- no parser state, saved cursor, cursor page-pin caches, hyperlink state,
  protection state, Kitty keyboard state, charset state, terminal extras, public
  API, public ABI, app behavior, renderer behavior, PTY behavior, clipboard
  behavior, or UI behavior is added;
- `cargo fmt`, targeted formatter tests, PageList formatter tests, PageList
  tests, and full `cargo test -p roastty` pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- cursor/style extra output requires a broader Screen cursor-state port than the
  minimal private cursor shape can honestly provide.

The experiment fails if:

- extras are emitted for plain or HTML output;
- extras are emitted before content;
- cursor coordinates are zero-indexed in VT output;
- pin maps become character-indexed or shorter than output bytes;
- TerminalFormatter content delegation regresses;
- the implementation adds unrelated parser, terminal, app, renderer, PTY, public
  API, or ABI behavior.

## Design Review

Codex reviewed the design and found no blockers. It agreed that cursor position
and active SGR style extras are the right next screen-level slice after
Experiment 90 because they stay inside `ScreenFormatter`, rely on existing
Roastty style machinery, and avoid premature parser, terminal, public API, or
ABI work.

Codex asked for three clarifications, all applied above:

- describe the style extra as the active SGR style stored on
  `screen.cursor.style`, not Ghostty's visual cursor shape;
- require style-extra tests to use a visible non-default style so an empty
  default style sequence cannot pass accidentally;
- choose the extra pin from the actual post-content pin map before appending
  extras, falling back to the screen top-left pin only when the map is empty.

Codex also recommended avoiding no-op placeholder fields for deferred extras.
The experiment now keeps `ScreenFormatterExtra` to the two implemented booleans:
`cursor` and `style`.
