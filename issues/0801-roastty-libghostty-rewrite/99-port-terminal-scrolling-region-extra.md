# Experiment 99: Port Terminal Scrolling Region Formatter Extra

## Description

Port the terminal formatter `scrolling_region` extra.

Experiments 97 and 98 ported terminal-level prefix extras: palette and modes.
Upstream Ghostty emits scrolling region state after screen content because it is
terminal state that should not affect the emitted screen bytes. This experiment
adds Roastty's private scrolling region state and wires only the
formatter-facing serialization of that state.

This must not add VT parser support for DECSTBM/DECSLRM, runtime scroll-region
mutation from escape sequences, left/right-margin mode behavior, scroll
behavior, resize behavior, public API, public ABI, render behavior, app
behavior, or UI behavior.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/formatter.zig` for:
     - `TerminalFormatter.Extra.scrolling_region`;
     - DECSTBM output shape;
     - DECSLRM output shape;
     - after-screen-content ordering;
     - post-screen terminal extra pin-map behavior.
   - Use `vendor/ghostty/src/terminal/Terminal.zig` and
     `vendor/ghostty/src/terminal/stream_terminal.zig` for:
     - scrolling region field shape;
     - default full-screen region;
     - row/column bounds conventions.
   - Do not modify `vendor/ghostty/`.

2. Add private terminal grid and scrolling region state.
   - Add private terminal size fields or an equivalent private helper so
     `TerminalFormatter` can know the terminal's current `cols` and `rows`
     without reaching into page internals.
   - Add a private `ScrollingRegion` value owned by `Terminal`, with:
     - `top`;
     - `bottom`;
     - `left`;
     - `right`.
   - Use 0-indexed inclusive `CellCountInt` values, matching upstream.
   - Preserve upstream invariants:
     - `top < bottom`;
     - `left < right`;
     - `bottom <= rows - 1`;
     - `right <= cols - 1`.
   - Initialize it to the full screen:
     - `top = 0`;
     - `bottom = rows - 1`;
     - `left = 0`;
     - `right = cols - 1`.
   - Add `#[cfg(test)] pub(super)` helpers to set and inspect the region for
     formatter tests.
   - Test helpers must assert or reject invalid/out-of-bounds regions. Do not
     add parser-style clamp/ignore behavior in this formatter-only experiment.
   - Keep the state private. Do not expose public API or ABI.

3. Extend `TerminalFormatterExtra`.
   - Add `scrolling_region: bool`.
   - Extend `none()`.
   - Add a `.scrolling_region(bool)` builder.
   - Keep `TerminalFormatter::init()` defaulting to no extras.

4. Emit scrolling region after screen content.
   - Only VT output emits scrolling-region bytes.
   - Plain and HTML ignore the scrolling-region extra.
   - Emit DECSTBM only when the vertical region is not full-screen:

     ```text
     \x1b[{top + 1};{bottom + 1}r
     ```

   - Emit DECSLRM only when the horizontal region is not full-width:

     ```text
     \x1b[{left + 1};{right + 1}s
     ```

   - Preserve upstream ordering:
     `palette -> modes -> content -> screen extras -> scrolling region`.
   - If both DECSTBM and DECSLRM are needed, emit DECSTBM first, then DECSLRM.
   - Do not require `enable_left_and_right_margin` mode to be set before
     emitting DECSLRM; upstream formatter emits the saved horizontal margin
     state directly when it differs from full width.

5. Preserve post-screen pin-map semantics.
   - Scrolling-region bytes are generated terminal-state bytes appended after
     screen formatter output.
   - Map appended scrolling-region bytes to the last existing pin when output
     already has content, screen extras, palette bytes, or mode bytes.
   - If the formatter emits only scrolling-region bytes, map them to active
     screen top-left.
   - Pin maps must remain byte-indexed.

6. Add upstream-equivalent tests.
   - Add TerminalFormatter tests for:
     - default output does not emit scrolling-region bytes;
     - default pin maps remain unchanged when the stored scrolling region is
       non-default but `TerminalFormatterExtra::none()` is used;
     - full-screen default region emits no bytes even when the extra is enabled;
     - vertical-only region emits DECSTBM with 1-indexed top/bottom;
     - horizontal-only region emits DECSLRM with 1-indexed left/right;
     - combined vertical and horizontal region emits DECSTBM before DECSLRM;
     - scrolling-region bytes emit after content;
     - scrolling-region bytes emit after forwarded screen extras;
     - scrolling-region bytes combine after palette and modes, preserving
       `palette -> modes -> content -> screen extras -> scrolling region`;
     - plain and HTML ignore the extra;
     - `Content::None` can emit only scrolling-region bytes for VT;
     - pin maps are byte-indexed;
     - post-screen scrolling-region bytes map to the last content/screen-extra
       pin when one exists;
     - post-screen scrolling-region bytes map to top-left when no prior bytes
       exist.
   - Keep existing modes, TerminalFormatter, ScreenFormatter, PageList
     formatter, and PageList tests passing.

7. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal_formatter
     cargo test -p roastty modes
     cargo test -p roastty screen_formatter
     cargo test -p roastty styled_pin_map
     cargo test -p roastty pin_map
     cargo test -p roastty page_string
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

8. Independent review.
   - Before implementation, get Codex review of this experiment design.
   - Fix all real design findings before implementation.
   - Record the design-review outcome in this experiment file before
     implementation.
   - After implementation and verification, get Codex review of the completed
     result.
   - Fix all real result findings before proceeding.

9. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - names and visibility of the scrolling-region state;
     - how terminal size is tracked for formatter comparisons;
     - exact DECSTBM and DECSLRM sequence shapes;
     - plain/HTML no-op behavior;
     - ordering relative to palette, modes, content, and forwarded screen
       extras;
     - pin-map behavior for post-screen generated bytes;
     - why runtime mutation, resize behavior, scroll behavior, public API, and
       ABI remain deferred;
     - verification command output summary;
     - Codex design-review outcome;
     - Codex result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `Terminal` owns private full-screen-default scrolling-region state;
- `TerminalFormatterExtra` has an opt-in scrolling-region flag;
- default TerminalFormatter output and pin maps remain unchanged;
- VT scrolling-region output emits only when the stored region differs from
  full-screen/full-width defaults;
- DECSTBM and DECSLRM sequence shapes match upstream and use 1-indexed values;
- DECSTBM emits before DECSLRM when both are present;
- scrolling-region bytes emit after screen content and forwarded screen extras;
- palette, modes, content, screen extras, and scrolling region can combine with
  ordering `palette -> modes -> content -> screen extras -> scrolling region`;
- plain and HTML output ignore the scrolling-region extra;
- generated scrolling-region bytes are byte-indexed in pin maps and map to the
  last existing pin, or top-left when there is no prior output;
- no VT parser/runtime scroll-region mutation, left/right-margin runtime
  behavior, scroll behavior, resize behavior, public API, public ABI, app
  behavior, renderer behavior, PTY behavior, clipboard behavior, or UI behavior
  is added;
- `cargo fmt`, targeted formatter tests, modes tests, PageList formatter tests,
  PageList tests, and full `cargo test -p roastty` pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- scrolling-region serialization cannot be represented honestly without first
  porting broader terminal size/resize state, and that prerequisite is
  identified precisely.

The experiment fails if:

- default TerminalFormatter output changes;
- scrolling-region bytes emit without explicit
  `TerminalFormatter::with_extra()`;
- HTML or plain output emits scrolling-region bytes;
- full-screen/full-width default regions emit bytes;
- DECSTBM/DECSLRM values are zero-indexed in output;
- scrolling-region bytes emit before content or before screen extras;
- generated scrolling-region pin maps become character-indexed, shorter than
  output bytes, or map to top-left when prior content pins exist;
- runtime parser, public API, ABI, scroll, resize, render, or UI behavior is
  added.

## Design Review

Codex reviewed this design before implementation.

Review artifacts:

- Prompt: `logs/codex-review/20260601-000740-480654-prompt.md`
- Result: `logs/codex-review/20260601-000740-480654-last-message.md`

Codex approved the overall scope, sequence shapes, ordering, and post-screen
pin-map model, with two required design fixes:

- add a default pin-map regression test where the stored scrolling region is
  non-default but no extra is enabled;
- specify that the private region state uses 0-indexed inclusive `CellCountInt`
  values and preserves upstream validity invariants, with test helpers asserting
  or rejecting invalid regions.

Both findings were applied before implementation.
