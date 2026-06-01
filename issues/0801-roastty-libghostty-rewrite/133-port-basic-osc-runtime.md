# Experiment 133: Port Basic OSC Runtime

## Description

Port Roastty's first runtime Operating System Command (`OSC`) path.

Experiment 132 connected CSI SGR parsing to live styled printing. The next
parser gap is OSC: upstream Ghostty uses `ESC ] ... BEL` and `ESC ] ... ESC \`
for window title changes, current working directory reports, hyperlinks, palette
mutation, clipboard operations, semantic prompts, mouse shape, notifications,
and several terminal-specific protocols.

This experiment should stand up the OSC state machine and dispatch the low-risk,
state-only commands that Roastty already has a place to store:

- OSC 0 / OSC 2: change window title;
- OSC 1: parse and ignore icon title;
- OSC 7: report current working directory URL;
- OSC 8: start/end active cursor hyperlink state.

This experiment must not add page-cell hyperlink writes yet. Runtime OSC 8
should update only the active cursor hyperlink state added in Experiment 95.
Writing active hyperlinks into printed cells requires extending the managed cell
write path from Experiment 132 and should be a follow-up experiment with its own
ref-count and rollback tests.

Do not implement OSC 4/10-19/104/110-119 palette mutation, OSC 52 clipboard, OSC
9/777 notifications, OSC 22 mouse shape, OSC 133 semantic prompt state, Kitty
clipboard/color/text-sizing protocols, ConEmu protocols, public ABI, renderer
behavior, app callbacks, PTY spawning, non-macOS behavior, or byte buffer
formatter rewrites here.

## Changes

1. Re-read upstream source of truth.
   - Use `vendor/ghostty/src/terminal/osc.zig` for:
     - OSC parser states;
     - supported command numbers;
     - BEL and ST terminators;
     - title, pwd, and hyperlink command shapes.
   - Use `vendor/ghostty/src/terminal/stream.zig` for:
     - OSC dispatch routing;
     - invalid UTF-8 title handling;
     - ignored OSC 1 icon behavior;
     - action ordering and parser reset behavior.
   - Use Experiment 95 for current Roastty active cursor hyperlink state.
   - Do not modify `vendor/ghostty/`.

2. Add a private OSC parser module.
   - Add `roastty/src/terminal/osc.rs` and wire it into the terminal module.
   - Keep it private to `roastty::terminal`.
   - Use a fixed-capacity buffer matching upstream's normal OSC capacity (`2048`
     bytes).
   - Parse both terminators:
     - BEL (`0x07`);
     - ST (`ESC \`).
   - Preserve split-feed state across `Stream::next_slice` calls.
   - Invalid, unsupported, or over-capacity OSC commands must be consumed
     without leaking payload bytes into visible text.

- A new `ESC ]` while an OSC is open invalidates the current OSC and consumes
  input until the next BEL or ST terminator. Do not restart into a nested OSC in
  this experiment.

3. Extend stream state for OSC.
   - Add an `EscapeState::Osc(...)` shape, or an equivalent state split that can
     represent:
     - normal OSC payload collection;
     - pending `ESC` inside OSC while deciding whether the next byte is `\`;
     - invalid OSC consuming until BEL or ST.
   - Ensure regular escape and CSI behavior remains unchanged.
   - If UTF-8 decoding is pending when `ESC ]` begins, preserve the existing
     replacement-character-before-control behavior used by CSI/ESC paths.
   - Raw C1 OSC (`0x9d`) remains out of scope, matching the current raw C1 CSI
     policy.

4. Add OSC stream dispatch without breaking `Action: Copy`.
   - Keep the existing `stream::Action` enum `Copy`. CSI dispatch relies on
     fixed `[Option<Action>; CSI_PARAM_CAPACITY]` storage, so this experiment
     must not add owned `String` variants to `Action`.
   - Add a separate private OSC dispatch path, for example:

     ```rust
     pub(super) enum OscAction<'a> {
         WindowTitle { title: &'a str },
         ReportPwd { url: &'a str },
         StartHyperlink { id: Option<&'a str>, uri: &'a str },
         EndHyperlink,
     }

     pub(super) trait Handler {
         type Error;

         fn vt(&mut self, action: Action) -> Result<(), Self::Error>;
         fn osc(&mut self, action: OscAction<'_>) -> Result<(), Self::Error>;
     }
     ```

   - Existing stream tests can implement `osc` by recording owned copies in
     test-only vectors. Terminal handling can clone the borrowed payload into
     terminal state at the handler boundary.
   - Parse OSC 0 and OSC 2 as title changes.
   - Parse OSC 1 as an ignored icon command that dispatches no action.
   - Parse OSC 7 as a pwd report action.
   - Parse OSC 8 as hyperlink start/end:
     - `OSC 8 ; ; URI ST` starts an implicit hyperlink;
     - `OSC 8 ; id=ID ; URI ST` starts an explicit hyperlink;
     - `OSC 8 ; ; ST` ends the active hyperlink;
     - malformed parameter forms are ignored without visible leakage.
   - Because current Roastty cursor hyperlink state stores `String`, accept only
     valid UTF-8 title, pwd, URI, and explicit ID payloads in this experiment.
     Invalid UTF-8 title/pwd/hyperlink payloads are ignored and consumed. A
     future byte-buffer formatter/state pass can revisit exact arbitrary-byte
     OSC parity.
   - Do not percent-decode, URL-validate, shell-expand, or otherwise normalize
     OSC 7 or OSC 8 payloads. The logical terminal state should preserve the
     UTF-8 string exactly as received. `TerminalPwd` may keep its current
     internal trailing-NUL representation for formatter output, but
     `logical_str()` must return the original OSC 7 URL exactly.

5. Apply actions to terminal state.
   - Add private terminal state for the current window title:

     ```rust
     #[derive(Debug, Clone, Default, PartialEq, Eq)]
     struct TerminalTitle {
         text: String,
     }
     ```

   - Reuse the existing private `TerminalPwd` state and `TerminalPwd::set`. The
     stored form includes a trailing NUL for formatter parity; the logical value
     remains the OSC 7 URL without that internal NUL.
   - Add a private terminal-owned implicit hyperlink counter, for example
     `next_implicit_hyperlink_id: u32`.
   - Extend `Terminal::next_slice` destructuring and `TerminalStreamHandler` so
     OSC handlers have mutable access to:
     - active screen;
     - title state;
     - pwd state;
     - implicit hyperlink counter.
   - Add `#[cfg(test)]` helpers to inspect title, pwd, and active cursor
     hyperlink state.
   - `WindowTitle` mutates only the private terminal title state.
   - `ReportPwd` mutates only the private terminal pwd state.
   - `StartHyperlink` mutates only active cursor hyperlink state:
     - explicit ID uses the parsed `id=` value;
     - implicit ID gets a private monotonically increasing ID or equivalent
       stable identity that does not appear in formatter OSC 8 output.
   - `EndHyperlink` clears active cursor hyperlink state.
   - Add non-test private `Screen` helpers for setting and clearing active
     cursor hyperlink state; keep existing test helpers or adapt them to call
     the private runtime helpers.
   - None of these actions should dirty rows, move the cursor, affect pending
     wrap, change modes, append PTY responses, or modify already-written cells.

6. Add tests.
   - Add OSC parser tests for:
     - OSC 0 title with BEL terminator;
     - OSC 2 title with ST terminator;
     - OSC 1 icon parses and dispatches no action;
     - OSC 7 pwd;
     - OSC 8 implicit start;
     - OSC 8 explicit `id=` start;
     - OSC 8 end;
     - unsupported OSC numbers consume without visible leakage;
     - malformed OSC 8 forms consume without visible leakage;
     - invalid UTF-8 payloads consume without action;
     - over-capacity payloads consume until terminator without visible leakage;
     - nested `ESC ]` inside OSC invalidates and consumes until terminator
       without visible leakage;
     - split-feed OSC across introducer, payload, ESC, and ST bytes;
     - handler-error recovery returns the parser to ground.
   - Add terminal tests proving:
     - title and pwd state update;
     - OSC 1 icon leaves state unchanged;
     - OSC actions do not dirty rows, move cursor, change pending wrap, mutate
       visible content, change modes, or append PTY responses;
     - OSC 8 start/end updates cursor hyperlink state;
     - `ScreenFormatterExtra::hyperlink(true)` observes the OSC 8 active cursor
       state for VT output;
     - printed cells are not assigned hyperlink metadata in this experiment;
     - split-feed OSC still works through `Terminal::next_slice`.
   - Keep existing CSI, SGR, print, formatter, page, page-list, and ABI tests
     passing.

7. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty stream_osc
     cargo test -p roastty terminal_stream_osc
     cargo test -p roastty screen_formatter_vt_hyperlink
     cargo test -p roastty terminal::stream
     cargo test -p roastty terminal::terminal
     cargo test -p roastty terminal_formatter
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

8. Independent review.
   - Before implementation, get Codex review of this experiment design.
   - Fix all real design findings before implementation.
   - Record the design-review outcome in this experiment file before
     implementation.
   - Commit the approved experiment design before implementation.
   - After implementation and verification, get Codex review of the completed
     result.
   - Fix all real result findings before proceeding.
   - Commit the recorded experiment result separately from the design commit.

9. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - OSC parser state shape;
     - supported command numbers;
     - BEL/ST terminator behavior;
     - invalid/unsupported/over-capacity consumption behavior;
     - why OSC dispatch uses a separate borrowed `OscAction` path instead of
       owned `String` variants on `Action`;
     - title and pwd state names and visibility;
     - OSC 8 explicit/implicit/end behavior;
     - `TerminalPwd` logical versus internal trailing-NUL representation;
     - why page-cell hyperlink writes remain deferred;
     - verification command output summary;
     - Codex design-review outcome;
     - Codex result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `ESC ]` OSC sequences are parsed across feed boundaries;
- BEL and ST terminate OSC sequences correctly;
- stream `Action` remains `Copy`, with owned OSC payload mutation happening
  through a separate borrowed OSC dispatch path;
- OSC 0 and OSC 2 update private terminal title state;
- OSC 1 is consumed and ignored;
- OSC 7 updates private terminal pwd URL state while preserving the exact
  logical URL despite `TerminalPwd`'s internal trailing-NUL storage;
- OSC 8 start/end updates only active cursor hyperlink state;
- `ScreenFormatterExtra::hyperlink(true)` can emit the active OSC 8 hyperlink
  opened through the stream;
- unsupported, malformed, invalid UTF-8, and over-capacity OSC payloads are
  consumed without leaking bytes into visible text;
- nested `ESC ]` inside OSC invalidates the open OSC and consumes until
  terminator without visible leakage;
- OSC actions do not dirty rows, move the cursor, affect pending wrap, change
  modes, append PTY responses, or modify already-written cells;
- printed cells are not assigned hyperlink metadata in this experiment;
- existing CSI, SGR, print, formatter, page, page-list, and ABI behavior remains
  intact;
- no palette mutation, clipboard, notifications, mouse shape, semantic prompt,
  Kitty protocols, ConEmu protocols, page-cell hyperlink writes, public API,
  public ABI, renderer, app callback, PTY, or non-macOS behavior is added;
- `cargo fmt`, targeted tests, and full `cargo test -p roastty` pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- OSC parser support lands, but applying one of the selected actions reveals a
  missing prerequisite state type that should be split into the next experiment;
- OSC 8 parser support lands, but active cursor hyperlink state cannot be
  updated without a broader byte-buffer state rewrite.

The experiment fails if:

- OSC payload bytes leak into visible terminal text;
- unterminated or invalid OSC leaves the stream stuck after a valid terminator;
- OSC dispatch adds owned `String` variants to the existing `Copy` CSI `Action`
  enum;
- OSC parsing regresses CSI, ESC, UTF-8 replacement, or SGR behavior;
- title/pwd/hyperlink actions mutate visible cells or cursor position;
- invalid UTF-8 is accepted into current String-backed state;
- page-cell hyperlink writes are added in this experiment;
- unrelated terminal protocols or public API/ABI are added.

## Design Review

Codex reviewed the initial design and agreed that basic OSC runtime support is
the right next experiment after SGR styled printing, and that title, pwd, and
active cursor hyperlink state are the right first OSC scope. It found five real
design issues: `logs/codex-review/20260601-075648-687841-last-message.md`.

The design was updated to:

- keep `stream::Action` `Copy` and route OSC payloads through a separate
  borrowed `OscAction<'_>` dispatch path;
- specify terminal state and handler plumbing for `TerminalTitle`, the existing
  `TerminalPwd`, `next_implicit_hyperlink_id`, and active-screen hyperlink
  mutation;
- pin nested `ESC ]` behavior as invalid-until-terminator;
- clarify that OSC 7 exactness means the logical URL is preserved, while
  `TerminalPwd` may keep its existing internal trailing-NUL storage;
- add the required separate design and result commit gates.

Codex re-reviewed the updated design and found no remaining required changes:
`logs/codex-review/20260601-075917-927945-last-message.md`.

The design is approved for implementation.

## Result

**Result:** Pass

Experiment 133 implemented the first private OSC runtime path for Roastty.

The new `roastty/src/terminal/osc.rs` parser uses a fixed 2048-byte buffer and
parses BEL (`0x07`) and ST (`ESC \`) terminated OSC sequences. It supports:

- OSC 0 and OSC 2 as `WindowTitle`;
- OSC 1 as a valid but ignored icon title command with no stream action;
- OSC 7 as `ReportPwd`;
- OSC 8 implicit start, explicit `id=` start, and end.

Unsupported commands, malformed OSC 8 parameters, invalid UTF-8, over-capacity
payloads, and nested `ESC ]` inside an open OSC are consumed without leaking
payload bytes into visible text. The stream also handles BEL correctly when it
arrives after a pending `ESC` inside either normal or invalid OSC state.

`stream::Action` remains `Copy`. OSC dispatch uses the separate borrowed
`stream::OscAction<'_>` path so test handlers can copy payloads into owned test
records and terminal handlers can clone payloads only at the state mutation
boundary. This preserves CSI dispatch storage and avoids owned string variants
in the existing CSI/VT action enum.

Terminal state now includes private `TerminalTitle`, reuses private
`TerminalPwd`, and adds a private `next_implicit_hyperlink_id` counter. OSC 0/2
updates only `TerminalTitle`. OSC 7 updates `TerminalPwd`; its logical test view
returns the exact OSC 7 URL, while the internal stored string keeps the existing
trailing NUL used by formatter output. OSC 8 updates only active cursor
hyperlink state on the active screen. Explicit links keep the parsed `id=`
value; implicit links get terminal-owned numeric identities that are not emitted
by the VT hyperlink formatter.

Page-cell hyperlink writes remain deferred. This experiment deliberately does
not extend managed cell writes from Experiment 132, so printing while an OSC 8
link is active does not mark page cells as hyperlink cells. That keeps the
ref-count and rollback surface for stored hyperlink metadata isolated for a
follow-up experiment.

No palette mutation, clipboard protocol, notifications, mouse shape, semantic
prompt state, Kitty protocols, ConEmu protocols, public API, public ABI,
renderer behavior, app callbacks, PTY spawning, or non-macOS behavior was added.

Verification passed after `cargo fmt`:

- `cargo test -p roastty stream_osc` — 13 passed;
- `cargo test -p roastty terminal_stream_osc` — 6 passed;
- `cargo test -p roastty screen_formatter_vt_hyperlink` — 9 passed;
- `cargo test -p roastty terminal::stream` — 206 passed;
- `cargo test -p roastty terminal::terminal` — 333 passed;
- `cargo test -p roastty terminal_formatter` — 67 passed;
- `cargo test -p roastty` — 1460 unit tests passed, ABI harness passed, 0 doc
  tests.

Codex result review first found two real issues:
`logs/codex-review/20260601-080739-726907-last-message.md`.

- OSC state did not treat BEL as a terminator after a pending `ESC` inside
  normal or invalid OSC.
- OSC 1 incorrectly dispatched an observable action even though the design
  required parse-and-ignore behavior.

Both issues were fixed and covered by regression tests. Codex re-reviewed the
revised implementation and approved the result with no findings:
`logs/codex-review/20260601-081001-821081-last-message.md`.

## Conclusion

Roastty now has a basic OSC runtime foundation for state-only commands. OSC
state survives split feeds, terminates on BEL and ST, consumes bad payloads
without visible leakage, and can mutate private terminal title, pwd, and active
cursor hyperlink state through a borrowed dispatch path.

The next OSC-related work should be a separate experiment for page-cell OSC 8
hyperlink writes. That experiment needs to extend the managed print path so
active hyperlinks are attached to newly printed cells with the same ref-count
and rollback discipline used by style and existing page hyperlink storage.
