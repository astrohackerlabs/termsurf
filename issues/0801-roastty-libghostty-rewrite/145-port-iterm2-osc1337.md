# Experiment 145: Port iTerm2 OSC 1337

## Description

Port Ghostty's `terminal/osc/parsers/iterm2.zig` behavior for OSC 1337 into
Roastty.

OSC 1337 is iTerm2's extension namespace. Ghostty recognizes a broad set of
iTerm2 keys, but only two keys currently produce runtime commands:

- `Copy=:<base64>` dispatches clipboard contents with clipboard kind `c`.
- `CurrentDir=<value>` dispatches a PWD report.

All other recognized iTerm2 keys are intentionally unimplemented and produce no
command. Unknown keys also produce no command. The parser must preserve that
distinction internally only as validation behavior; Roastty must not leak
unimplemented or unknown OSC 1337 inputs into terminal state.

This experiment completes the remaining meaningful OSC parser slice after OSC
133, OSC 3008, OSC 52/5522, OSC 777, OSC 7, and title/color OSC handling.

## Changes

1. In `roastty/src/terminal/osc.rs`, add OSC 1337 parsing:
   - Add a `b"1337" if split.is_some()` match arm in `Parser::command`.
   - Add a small `parse_iterm2_extension(rest)` helper.
   - Split `rest` into `key` and optional `value` at the first `=`.
   - Match keys ASCII case-insensitively against Ghostty's iTerm2 key list:
     `AddAnnotation`, `AddHiddenAnnotation`, `Block`, `Button`,
     `ClearCapturedOutput`, `ClearScrollback`, `Copy`, `CopyToClipboard`,
     `CurrentDir`, `CursorShape`, `Custom`, `Disinter`, `EndCopy`, `File`,
     `FileEnd`, `FilePart`, `HighlightCursorLine`, `MultipartFile`, `OpenURL`,
     `PopKeyLabels`, `PushKeyLabels`, `RemoteHost`, `ReportCellSize`,
     `ReportVariable`, `RequestAttention`, `RequestUpload`,
     `SetBackgroundImageFile`, `SetBadgeFormat`, `SetColors`, `SetKeyLabel`,
     `SetMark`, `SetProfile`, `SetUserVar`, `ShellIntegrationVersion`,
     `StealFocus`, and `UnicodeVersion`.

2. Implement `Copy` exactly like Ghostty:
   - Require a value.
   - Reject an empty value.
   - Require the value to start with `:`.
   - Strip the leading `:`.
   - Reject an empty stripped value.
   - Reject stripped value `?`.
   - Do not validate base64 at parse time. Ghostty intentionally skips this for
     performance so the normal path does not parse the data twice.
   - Dispatch existing `Command::ClipboardContents` with kind `b'c'` and the
     stripped data bytes.

3. Implement `CurrentDir` using Roastty's existing PWD command path:
   - Require a value.
   - Reject an empty value.
   - Require valid UTF-8 before dispatching because Roastty's existing
     `Command::ReportPwd` stores `&str` and the terminal PWD state stores a
     string.
   - Dispatch existing `Command::ReportPwd { url }`.

4. Keep unimplemented and unknown OSC 1337 keys as no-command results:
   - Valid but unimplemented keys return `None`.
   - Unknown keys return `None`.
   - Inputs with no value, empty value, and non-empty value must all remain
     inert for unimplemented keys.
   - Lowercase recognized keys must be recognized case-insensitively and still
     remain inert when unimplemented.

5. Keep capture bounded:
   - OSC 1337 remains on the fixed OSC buffer path.
   - Do not add OSC 1337 to `growable_osc_limit`.
   - Oversized OSC 1337 payloads must invalidate and dispatch nothing, matching
     the current fixed-buffer OSC families.

6. Add tests:
   - Parser tests for separator edge cases: `1337` without a semicolon must not
     dispatch, and `1337;` with an empty key must not dispatch.
   - Parser tests for Ghostty's valid unimplemented-key cases: `SetBadgeFormat`,
     `SetBadgeFormat=`, `SetBadgeFormat=abc123`, and the same forms with
     lowercase `setbadgeformat`.
   - Parser tests for unknown key cases: no value, empty value, and non-empty
     value.
   - Parser tests for invalid `Copy` forms: `Copy`, `Copy=`, `Copy=:`,
     `Copy=:?`, and `Copy=YWJjMTIz`.
   - Parser tests for valid `Copy`: `Copy=:YWJjMTIz` dispatches clipboard kind
     `c` with data `YWJjMTIz`.
   - Parser test confirming `copy=:YWJjMTIz` also dispatches clipboard kind `c`
     with data `YWJjMTIz`, because Ghostty applies ASCII case-insensitive
     matching to command-producing keys as well as inert keys.
   - Parser test confirming non-empty colon-prefixed data is dispatched even
     when it is not valid base64, because Ghostty does not validate base64 in
     this parser.
   - Parser tests for `CurrentDir`, `CurrentDir=`, and `CurrentDir=abc123`.
   - Parser test confirming `currentdir=abc123` dispatches `ReportPwd`.
   - Parser test for invalid UTF-8 in `CurrentDir`, which must return no command
     under Roastty's string-backed PWD model.
   - Parser test for oversized fixed-buffer OSC 1337 input, such as
     `1337;Copy=:` plus more than `MAX_BUF` bytes, returning no command.
   - Stream test confirming OSC 1337 `Copy` and `CurrentDir` dispatch through
     `Stream`.
   - Stream no-leak tests confirming recognized-but-unimplemented and unknown
     OSC 1337 keys are consumed without printing their bytes and without
     dispatching OSC actions. Cover no-value, empty-value, and non-empty-value
     forms.
   - Stream oversized fixed-buffer test confirming an oversized OSC 1337
     sequence between printable bytes prints only the surrounding bytes and
     dispatches no OSC action for the oversized sequence.
   - Terminal runtime test confirming OSC 1337 `CurrentDir` updates the existing
     PWD state.
   - Terminal runtime test confirming OSC 1337 `Copy` remains a no-op at the
     terminal layer, matching the existing OSC 52 clipboard runtime behavior.

## Verification

1. Run formatting:

   ```bash
   cargo fmt -- roastty/src/terminal/osc.rs roastty/src/terminal/stream.rs roastty/src/terminal/terminal.rs
   ```

2. Run focused tests:

   ```bash
   cargo test -p roastty osc_parser_iterm2_osc1337
   cargo test -p roastty terminal_stream_osc1337
   ```

3. Run the full Roastty test suite:

   ```bash
   cargo test -p roastty
   ```

## Design Review

Codex reviewed the initial design and found the scope correct, but not yet
approved. Required fixes were added:

- parser and stream tests for oversized fixed-buffer OSC 1337 rejection;
- stream no-leak tests for recognized-unimplemented and unknown keys;
- lowercase command-producing key tests for `copy` and `currentdir`;
- separator edge tests for `1337` and `1337;`.

Codex approved the revised design after those additions. No remaining required
design fixes.
