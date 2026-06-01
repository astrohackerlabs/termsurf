# Experiment 131: Port DECRQM Mode Reports

## Description

Port the first response-producing stream command needed by the mode subsystem:
DECRQM / request mode (`CSI ? Ps $ p`) for DEC-private modes.

Experiments 128-130 completed mode set/reset, basic mode effects, and
DEC-private mode save/restore. Upstream Ghostty also answers mode-status queries
by encoding a DECRPM response and writing it back to the PTY. Roastty already
has the upstream-derived mode report encoder in `roastty/src/terminal/modes.rs`,
but the terminal stream currently has no response-output surface and the CSI
parser currently treats `$` as invalid.

This experiment adds the smallest honest response foundation for Roastty's
terminal core and uses it to implement DEC-private DECRQM:

- parse `CSI ? Ps $ p`;
- route known modes as `RequestMode`;
- route unknown modes as `RequestModeUnknown`;
- encode responses with `ModeState::get_report(...).encode_vt()`;
- store emitted PTY responses in a private terminal response buffer that tests
  can drain.

Do not expose this response buffer through the public C ABI yet. Upstream
Ghostty writes responses through a `write_pty` callback; Roastty's ABI callback
surface and terminal-core integration are not ready for that contract. This
experiment proves the terminal-core parser/executor behavior first. A later ABI
experiment will forward buffered or callback-based PTY responses to the app.

Do not implement device status reports, device attributes, size reports,
XTVERSION, OSC replies, DCS replies, ANSI-mode DECRQM, SGR, OSC, DCS,
alternate-screen, mouse encoding, keypad behavior, public ABI, or non-macOS
behavior here.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/stream.zig` for DECRQM parsing.
   - Use `vendor/ghostty/src/terminal/stream_terminal.zig::requestMode`,
     `requestModeUnknown`, and `sendModeReport` for terminal execution.
   - Use `vendor/ghostty/src/terminal/modes.zig::Report.encode` as the
     response-format reference.
   - Use Roastty's existing `modes::Report::encode_vt()` rather than adding a
     second formatter.
   - Do not modify `vendor/ghostty/`.

2. Add minimal CSI intermediate support for `$`.
   - Extend `CsiState` with enough intermediate-byte storage to recognize
     `CSI ? Ps $ p`.
   - Keep the parser no-allocation.
   - Do not broadly port all CSI intermediates. The only newly supported
     intermediate in this experiment is `$` when final byte is `p` and private
     marker is `?`.
   - Existing unsupported intermediate forms must remain no-dispatch/no-leak.
   - Existing commands that currently reject `$`, spaces, quotes, or other
     intermediate bytes must not start dispatching accidentally.
   - Any stored CSI intermediate must disable all existing CSI dispatch paths
     unless that command explicitly supports the exact intermediate shape.
     Adding `$` support must not make cursor, erase, mode, tab, scroll, or line
     commands dispatch while silently ignoring the intermediate.
   - Colon-separated CSI params remain invalid.
   - Raw C1 `0x9b` remains out of scope and must keep current raw-UTF-8
     replacement behavior.

3. Add stream actions.
   - Add `Action::RequestMode { mode: modes::Mode }`.
   - Add `Action::RequestModeUnknown { value: u16, ansi: bool }`.
   - Keep both actions internal to the terminal module.
   - Do not add public API or ABI surface.

4. Parse DEC-private DECRQM.
   - Recognize `CSI ? Ps $ p` as DEC-private DECRQM.
   - Require exactly one param.
   - Use `modes::mode_from_int(value, false)`.
   - Dispatch `RequestMode` for known DEC-private modes.
   - Dispatch `RequestModeUnknown { value, ansi: false }` for unknown
     DEC-private modes.
   - Empty params (`CSI ? $ p`) dispatch no action and do not leak final `p`,
     matching upstream's exactly-one-param requirement.
   - Explicit zero (`CSI ? 0 $ p`) dispatches `RequestModeUnknown` for DEC mode
     `0`.
   - Over-capacity params, colon-separated params, wrong private marker, missing
     `$`, extra intermediates, and extra params dispatch no action and do not
     leak the final `p` byte.
   - ANSI-mode DECRQM (`CSI Ps $ p`) remains deferred. Current Roastty should
     keep treating it as unsupported/no-dispatch/no-leak. This matches the
     observed upstream Ghostty source used for this port: the outer `p` branch
     only enters DECRQM for the DEC-private `? $` intermediate shape, even
     though the inner code still contains an ANSI-looking arm.

5. Add a private terminal PTY-response buffer.
   - Add an internal `Vec<u8>` or equivalent response buffer to `Terminal`.
   - Add a private helper such as `write_pty_response(&mut self, bytes: &str)`.
   - Add test-only helpers to inspect and drain the buffer.
   - The helper appends owned response strings to terminal-core state. It has no
     public exposure except `#[cfg(test)]` peek/drain helpers.
   - Treat append as infallible under normal Rust allocation behavior. If the
     implementation ever introduces a fallible writer later, parser state must
     still return to ground before the write is attempted.
   - Before implementing reset behavior, verify whether Roastty currently has an
     `ESC c` / full-reset terminal path. If it exists, clear the response buffer
     on that path. If it does not, record "not applicable" and do not invent
     terminal reset in this experiment.
   - The buffer is terminal-core state only. Do not call ABI callbacks or add C
     exports in this experiment.

6. Execute mode report requests.
   - `RequestMode` calls
     `self.modes.get_report(modes::ModeTag::from_mode(mode)).encode_vt()` and
     appends the encoded bytes to the response buffer.
   - `RequestModeUnknown { value, ansi }` calls
     `self.modes.get_report(modes::ModeTag::new(value, ansi)).encode_vt()` and
     appends the encoded bytes to the response buffer.
   - Multiple requests in one input slice append responses in dispatch order.
   - Response generation must not mutate screen cells, cursor position, dirty
     rows, scrolling margins, or current mode state.

7. Add stream parser tests.
   - `CSI ? 7 $ p` dispatches `RequestMode(Wraparound)`.
   - `CSI ? 2004 $ p` dispatches `RequestMode(BracketedPaste)`.
   - `CSI ? 9999 $ p` dispatches
     `RequestModeUnknown { value: 9999, ansi: false }`.
   - Empty param `CSI ? $ p` dispatches no action and does not leak final `p`.
   - Explicit zero `CSI ? 0 $ p` dispatches unknown DEC mode `0`.
   - Extra params such as `CSI ? 7 ; 8 $ p` dispatch no action and do not leak
     final `p`.
   - Missing `$`, wrong private marker, ANSI `CSI 4 $ p`, invalid colon params,
     and extra intermediate bytes dispatch no action and do not leak final `p`.
   - Existing command families reject the new `$` intermediate instead of
     ignoring it:
     - mode set/reset: `CSI 4 $ h`, `CSI ? 7 $ h`;
     - cursor movement: `CSI 3 $ A`;
     - erase display/line: `CSI 2 $ J`, `CSI 2 $ K`;
     - tab control: `CSI 0 $ W`.
   - Trailing-separator ambiguity is pinned down explicitly: `CSI ? 7 ; $ p`
     must either dispatch no action or be documented as exactly matching
     Roastty's existing finalized-param semantics. Prefer no dispatch/no-leak
     unless implementation proves current parser semantics make that
     inconsistent with nearby CSI behavior.
   - Split-feed DECRQM dispatches correctly, including splits around `$`.
   - Pending invalid UTF-8 emits `U+FFFD` before DECRQM dispatch.
   - Raw C1 `0x9b ? 7 $ p` is not treated as CSI.
   - Handler errors from request-mode actions leave the parser in ground state.

8. Add terminal tests.
   - Default wraparound query `CSI ? 7 $ p` emits `ESC [?7;1$y`.
   - After `CSI ? 7 l`, querying `?7` emits `ESC [?7;2$y`.
   - Bracketed paste query reflects set and reset state.
   - Unknown DEC-private mode query emits `ESC [?9999;0$y`.
   - Multiple queries in one input append multiple responses in order.
   - Draining the test response buffer empties it and returns prior bytes.
   - Formatter output remains unchanged after a DECRQM query; responses must not
     leak into visible terminal content or formatter suffixes.
   - Response generation does not mutate visible cells, cursor position,
     scrolling region, dirty rows, or mode state.
   - Full reset applicability is checked explicitly:
     - if Roastty currently has an `ESC c` / full-reset path, it clears pending
       response output;
     - if not, the result documents that reset behavior was not applicable.
   - No C ABI structs, callbacks, exports, or headers change in this experiment;
     the existing ABI harness continues to pass.
   - Existing parser, terminal, formatter, mode-state, and ABI tests keep
     passing.

9. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty stream_csi_mode
     cargo test -p roastty terminal_stream_csi_mode
     cargo test -p roastty terminal::modes
     cargo test -p roastty terminal::terminal
     cargo test -p roastty stream
     cargo test -p roastty terminal_formatter
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

10. Independent review.
    - Before implementation, get Codex review of this experiment design.
    - Fix all real design findings before implementation.
    - Record the design-review outcome in this experiment file before
      implementation.
    - Commit the approved design before implementation.
    - After implementation and verification, get Codex review of the completed
      result.
    - Fix all real result findings before proceeding.
    - Commit the approved result separately from the design commit.

11. Record the result.
    - Append `## Result` and `## Conclusion` to this file.
    - Include:
      - exact parser behavior for `CSI ? Ps $ p`;
      - response buffer behavior and reset/drain behavior;
      - exact encoded responses for set, reset, and unknown modes;
      - explicitly deferred ANSI DECRQM, device reports, attributes, size
        reports, ABI forwarding, and other response-producing commands;
      - verification command output summary;
      - Codex design-review outcome;
      - Codex result-review outcome.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Verification

The experiment passes if:

- real `CSI ? Ps $ p` input dispatches DEC-private mode requests;
- known DEC-private modes encode `Set` or `Reset` responses from current
  `ModeState`;
- unknown DEC-private modes encode `NotRecognized`;
- responses are appended to a private terminal-core PTY response buffer in
  order;
- tests can drain the response buffer;
- response generation does not mutate screen, cursor, dirty rows, margins, or
  mode state;
- unsupported ANSI/malformed/intermediate/raw-C1 forms remain
  no-dispatch/no-leak;
- existing CSI command families reject unsupported intermediates instead of
  dispatching while ignoring them;
- response output is invisible to normal terminal formatting and has no public
  ABI exposure;
- no public ABI, callback, device status, device attribute, size report, OSC,
  DCS, SGR, alternate-screen, mouse, keypad, or non-macOS behavior is added;
- `cargo fmt` and the listed tests pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- parser support lands but response output needs a smaller response-buffer
  foundation first;
- response encoding works but the current terminal reset model cannot yet define
  buffer-clear semantics cleanly;
- minimal `$` intermediate support exposes a broader parser-structure issue that
  should be split out before DECRQM lands.

The experiment fails if:

- `CSI ? Ps $ p` still dispatches no action;
- known/unknown mode reports encode the wrong DECRPM status;
- response generation mutates terminal display state;
- malformed forms leak final `p` as printable text;
- the implementation silently adds public ABI forwarding or unrelated
  response-producing commands.

## Design Review

Codex reviewed the initial design and found six real issues:
`logs/codex-review/20260601-071520-673887-last-message.md`.

The design was updated to:

- require empty `CSI ? $ p` to dispatch no action and not leak final `p`, while
  explicit `CSI ? 0 $ p` reports unknown DEC mode `0`;
- require all existing CSI command families to reject unsupported intermediates
  rather than dispatching while silently ignoring `$`;
- frame ANSI `CSI Ps $ p` as deliberately deferred and no-dispatch, matching the
  observed upstream Ghostty branch shape used for this port;
- define the response buffer as private terminal-core state with test-only
  peek/drain helpers and no public ABI exposure;
- require reset-applicability checking before adding any reset behavior;
- require formatter non-leak checks and ABI no-change verification.

Codex re-reviewed the updated design and found no blocking design issues:
`logs/codex-review/20260601-071745-178900-last-message.md`.

Codex also suggested a non-blocking trailing-separator parser test. The design
now requires pinning down `CSI ? 7 ; $ p` behavior during implementation.

The design is approved for implementation.
