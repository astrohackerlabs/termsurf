# Experiment 101: Port Terminal Keyboard and Pwd Formatter Extras

## Description

Port the remaining post-screen `TerminalFormatter.Extra` fields from upstream
Ghostty's terminal formatter: `keyboard` and `pwd`.

Experiment 100 completed the tabstops terminal formatter extra. Upstream Ghostty
then emits two more terminal-level VT extras after tabstops:

- keyboard mode state for `modify_other_keys_2`, emitted as `CSI > 4 ; 2 m`;
- present working directory state, emitted as `OSC 7`.

Roastty already has the screen-level Kitty keyboard extra, but it does not yet
have the terminal-level `modify_other_keys_2` flag or terminal PWD state. This
experiment adds only the private state and opt-in formatter serialization needed
to match upstream formatter behavior. It must not add a VT parser, OSC parser,
runtime terminal mutation, PTY integration, public API, public ABI, app
behavior, renderer behavior, clipboard behavior, or UI behavior.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/formatter.zig` for:
     - `TerminalFormatter.Extra.keyboard`;
     - `TerminalFormatter.Extra.pwd`;
     - post-screen ordering after `scrolling_region` and `tabstops`;
     - pin-map behavior for post-screen terminal extras.
   - Use `vendor/ghostty/src/terminal/Terminal.zig` for:
     - `flags.modify_other_keys_2`;
     - `pwd` storage;
     - `setPwd()` and `getPwd()` behavior.
   - Do not modify `vendor/ghostty/`.

2. Add private terminal state.
   - Add a private terminal flags struct with at least
     `modify_other_keys_2: bool`.
   - Initialize `modify_other_keys_2` to `false`.
   - Add private PWD storage to `Terminal`.
   - Store PWD in the same logical shape as upstream: empty means no PWD; a
     non-empty PWD has a terminator in storage and a getter that exposes the
     logical value without the terminator.
   - Add `#[cfg(test)] pub(super)` helpers to:
     - set `modify_other_keys_2`;
     - inspect `modify_other_keys_2`;
     - set PWD;
     - clear PWD;
     - inspect logical PWD.
   - Keep all state private. Do not expose public API or ABI.

3. Extend `TerminalFormatterExtra`.
   - Add `keyboard: bool`.
   - Add `pwd: bool`.
   - Extend `none()`.
   - Add `.keyboard(bool)` and `.pwd(bool)` builders.
   - Keep `TerminalFormatter::init()` defaulting to no extras.

4. Emit keyboard and PWD after tabstops.
   - Only VT output emits these extras.
   - Plain and HTML ignore these extras.
   - Preserve upstream post-screen ordering:
     `scrolling region -> tabstops -> keyboard -> pwd`.
   - When `keyboard` is enabled and `modify_other_keys_2` is true, emit:

     ```text
     \x1b[>4;2m
     ```

   - When `keyboard` is enabled and `modify_other_keys_2` is false, emit
     nothing.
   - When `pwd` is enabled and the stored PWD is non-empty, emit:

     ```text
     \x1b]7;{stored_pwd}\x1b\
     ```

   - When `pwd` is enabled and no PWD is stored, emit nothing.
   - Match upstream's exact PWD byte behavior:
     - `setPwd()` stores the logical PWD bytes followed by a trailing NUL byte;
     - `getPwd()` exposes the logical PWD without the trailing NUL;
     - `TerminalFormatter` writes the stored `pwd.items` bytes directly, not the
       logical getter.
   - Therefore, Roastty formatter output must include the stored trailing NUL
     byte before the OSC string terminator when PWD is non-empty:

     ```text
     \x1b]7;file://host/home/user\0\x1b\
     ```

   - Do not escape, sanitize, normalize, or URL-encode PWD bytes in this
     experiment. Upstream emits the stored bytes raw and terminates the OSC with
     ST (`ESC \`). Parser-side validation and sanitization are outside this
     formatter-only slice.

5. Preserve post-screen pin-map semantics.
   - Keyboard and PWD bytes are generated terminal-state bytes appended after
     screen formatter output and earlier terminal suffix extras.
   - Map appended keyboard and PWD bytes to the last existing pin when output
     already has content, screen extras, palette bytes, mode bytes,
     scrolling-region bytes, or tabstop bytes.
   - If the formatter emits only keyboard or PWD bytes, map them to active
     screen top-left.
   - Pin maps must remain byte-indexed.

6. Add upstream-equivalent tests.
   - Add TerminalFormatter tests for:
     - default output does not emit keyboard or PWD bytes even when stored state
       is non-default;
     - default pin maps remain unchanged when stored keyboard/PWD state is
       non-default but `TerminalFormatterExtra::none()` is used;
     - `keyboard` extra emits `CSI > 4 ; 2 m` only when `modify_other_keys_2` is
       true;
     - `keyboard` extra emits nothing when `modify_other_keys_2` is false;
     - `pwd` extra emits `OSC 7` only when a PWD is stored;
     - `pwd` exact output includes raw stored bytes, the stored trailing NUL
       byte, and ST termination;
     - `pwd` extra emits nothing for empty PWD;
     - keyboard and PWD emit after scrolling-region and tabstop bytes when all
       suffix extras are enabled;
     - palette, modes, content, screen extras, scrolling region, tabstops,
       keyboard, and PWD combine with ordering
       `palette -> modes -> content -> screen extras -> scrolling region -> tabstops -> keyboard -> pwd`;
     - plain and HTML ignore both extras;
     - `Content::None` can emit only keyboard/PWD bytes for VT;
     - pin maps are byte-indexed;
     - content plus prior suffix extras plus keyboard plus PWD in
       `format_with_pin_map()` has `text.len() == pin_map.len()` across the
       exact output bytes, including the PWD trailing NUL byte;
     - post-screen keyboard/PWD bytes map to the last existing pin when one
       exists;
     - post-screen keyboard/PWD bytes map to top-left when no prior bytes exist.
   - Keep existing tabstops, modes, TerminalFormatter, ScreenFormatter, PageList
     formatter, and PageList tests passing.

7. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal_formatter
     cargo test -p roastty modes
     cargo test -p roastty tabstops
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
     - terminal flag and PWD state names and visibility;
     - default state;
     - exact `CSI > 4 ; 2 m` sequence behavior;
     - exact `OSC 7` sequence behavior, including raw byte emission, the stored
       trailing NUL byte, and ST termination;
     - plain/HTML no-op behavior;
     - ordering relative to palette, modes, content, screen extras, scrolling
       region, and tabstops;
     - pin-map behavior for post-screen generated bytes;
     - why parser/runtime mutation, PTY integration, public API, and ABI remain
       deferred;
     - verification command output summary;
     - Codex design-review outcome;
     - Codex result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `Terminal` owns private keyboard/PWD state matching upstream's logical
  formatter needs;
- `TerminalFormatterExtra` has opt-in `keyboard` and `pwd` flags;
- default TerminalFormatter output and pin maps remain unchanged;
- VT keyboard output emits `CSI > 4 ; 2 m` only when `modify_other_keys_2` is
  true;
- VT PWD output emits `OSC 7` only when PWD is non-empty;
- VT PWD output emits raw stored PWD bytes, including the stored trailing NUL
  byte before ST;
- keyboard and PWD bytes emit after scrolling-region and tabstop bytes;
- palette, modes, content, screen extras, scrolling region, tabstops, keyboard,
  and PWD can combine with ordering
  `palette -> modes -> content -> screen extras -> scrolling region -> tabstops -> keyboard -> pwd`;
- plain and HTML output ignore keyboard and PWD extras;
- generated keyboard/PWD bytes are byte-indexed in pin maps and map to the last
  existing pin, or top-left when there is no prior output;
- no VT parser/runtime mutation, OSC parser, public API, public ABI, PTY
  integration, app behavior, renderer behavior, clipboard behavior, or UI
  behavior is added;
- `cargo fmt`, targeted formatter tests, tabstops tests, modes tests, PageList
  formatter tests, PageList tests, and full `cargo test -p roastty` pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- keyboard/PWD formatter serialization cannot be represented honestly without
  first adding terminal parser/runtime state, and that prerequisite is
  identified precisely.

The experiment fails if:

- default TerminalFormatter output changes;
- keyboard or PWD bytes emit without explicit `TerminalFormatter::with_extra()`;
- HTML or plain output emits keyboard/PWD bytes;
- keyboard or PWD bytes emit before content, screen extras, scrolling-region
  bytes, or tabstop bytes;
- keyboard output emits when `modify_other_keys_2` is false;
- PWD output emits for empty state;
- PWD output serializes a different byte sequence than upstream, including
  missing or moving the stored trailing NUL byte;
- generated keyboard/PWD pin maps become character-indexed, shorter than output
  bytes, or map to top-left when prior content pins exist;
- runtime parser, public API, ABI, PTY, app, render, or UI behavior is added.

## Design Review

Codex reviewed this design before implementation.

Initial review artifacts:

- Prompt: `logs/codex-review/20260601-002439-201214-prompt.md`
- Result: `logs/codex-review/20260601-002439-201214-last-message.md`

Codex found three real design gaps:

- PWD serialization had to specify the exact upstream trailing-NUL behavior
  before implementation;
- OSC 7 output had to specify raw stored byte emission and ST termination, with
  no escaping or sanitization in this formatter-only slice;
- pin-map tests had to cover exact PWD bytes, including the trailing NUL byte.

All three findings were applied.

Re-review artifacts:

- Prompt: `logs/codex-review/20260601-002623-862778-prompt.md`
- Result: `logs/codex-review/20260601-002623-862778-last-message.md`

Codex found no remaining real findings and approved implementation.
