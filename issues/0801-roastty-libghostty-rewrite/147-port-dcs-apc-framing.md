# Experiment 147: Port DCS and APC Framing

## Description

Port Ghostty's DCS and APC string framing into Roastty's terminal stream.

Roastty currently handles CSI and OSC, but `ESC P ... ESC \` DCS strings and
`ESC _ ... ESC \` APC strings are not first-class stream states. That is a
correctness gap: until these states exist, DCS/APC payload bytes can fall back
to ordinary printable text after the introducer is ignored.

This experiment adds the stream-level framing and terminal no-op runtime needed
before later experiments implement the command-specific DCS/APC protocols:

- DCS DECRQSS / XTGETTCAP / tmux control-mode parsing and responses;
- APC Kitty graphics parsing and responses.

The goal here is not to implement those protocols yet. The goal is to make
Roastty correctly recognize, contain, and consume DCS/APC string bodies without
leaking their bytes into terminal content, while exposing the same event shape
Ghostty's stream uses for future protocol handlers.

## Changes

1. Extend `roastty/src/terminal/stream.rs` actions:
   - Add `Action::DcsHook { value }`.
   - Add `Action::DcsPut { byte }`.
   - Add `Action::DcsUnhook`.
   - Add `Action::ApcStart`.
   - Add `Action::ApcPut { byte }`.
   - Add `Action::ApcEnd`.
   - Add a typed DCS hook value matching Ghostty's event shape:
     - `intermediates`: up to four collected bytes from `0x20..=0x2f`;
     - `params`: up to the existing fixed CSI capacity, with semicolon-only
       parameter separation;
     - `final_byte`.
   - Do not add a separate DCS private field or separator list. Ghostty exposes
     DCS hook metadata as intermediates, params, and final byte only.

2. Extend escape recognition:
   - `ESC P` enters DCS entry state.
   - `ESC _` enters APC string state.
   - C1 DCS/APC bytes are out of scope for this experiment because the current
     Roastty stream has focused on 7-bit escape forms. Add tests documenting
     that the implemented target is 7-bit framing.

3. Implement DCS framing:
   - Parse DCS header bytes with explicit Ghostty-compatible DCS state-machine
     rules, not by reusing CSI parsing wholesale:
     - DCS entry ignores C0 controls and DEL.
     - `0x20..=0x2f` from entry enters DCS intermediate and collects
       intermediates.
     - `:` from entry enters DCS ignore.
     - digits and `;` from entry enter DCS param and build semicolon-separated
       `u16` params.
     - `<`, `=`, `>`, and `?` from entry enter DCS param through collect
       behavior, matching Ghostty's parser table.
     - in DCS param, digits and `;` continue param parsing.
     - in DCS param, `:` and `<`..`?` enter DCS ignore.
     - in DCS param, `0x20..=0x2f` enters DCS intermediate and collects
       intermediates.
     - in DCS intermediate, more `0x20..=0x2f` bytes are collected.
     - in DCS intermediate, `0x30..=0x3f` enters DCS ignore.
     - any final byte `0x40..=0x7e` from entry, param, or intermediate enters
       DCS passthrough and may dispatch `Action::DcsHook` if metadata capacity
       was not exceeded.
   - On a valid final byte, dispatch `Action::DcsHook` and enter DCS
     passthrough.
   - While in DCS passthrough, dispatch `Action::DcsPut` for payload bytes.
   - ESC exits DCS passthrough and dispatches `Action::DcsUnhook`. `ESC \` is
     the common ST spelling, but Ghostty's parser exits DCS on ESC before
     processing the byte after ESC. Therefore `ESC X`, `ESC P`, `ESC [` and
     other ESC-prefixed sequences also end the prior DCS before the next escape
     path is interpreted.
   - Do not terminate DCS on BEL. BEL inside DCS payload is delivered as
     `DcsPut { byte: 0x07 }`, matching the VT parser model.
   - Unknown DCS command identity does not matter at this layer. The stream
     still emits hook/put/unhook so a later DCS protocol handler can decide
     whether to ignore or handle it.
   - Malformed DCS header forms enter an ignore state that consumes bytes until
     the next ESC transition and dispatches no DCS actions.
   - Exceeding param or intermediate capacity drops the hook action and keeps
     the string contained until ESC, matching Ghostty's "too many params" DCS
     behavior.
   - Payload bytes must never print as terminal content.

4. Implement APC framing:
   - `ESC _` dispatches `Action::ApcStart` and enters APC string state.
   - While in APC string state, dispatch `Action::ApcPut` for payload bytes.
   - ESC exits APC and dispatches `Action::ApcEnd`. As with DCS, `ESC \` is the
     common ST spelling, but `ESC X`, `ESC P`, `ESC [` and other ESC-prefixed
     sequences also end the prior APC before the next escape path is
     interpreted.
   - BEL inside APC payload is delivered as `ApcPut { byte: 0x07 }`.
   - Unknown APC command identity is intentionally not interpreted here.
   - Payload bytes must never print as terminal content.

5. Extend terminal runtime behavior:
   - `TerminalStreamHandler` ignores all DCS/APC actions for now.
   - This matches Ghostty's current terminal runtime shape for DCS actions.
     Ghostty already routes APC into a Kitty graphics handler, so Roastty's APC
     no-op is an explicit deferment until a later Kitty graphics experiment.
   - Ignoring these actions must not mutate display cells, cursor position,
     dirty flags, modes, or PTY responses.

6. Add tests:
   - Stream DCS tests:
     - `ESC P $ q m ESC \` dispatches hook with intermediate `$`, final `q`,
       payload byte `m`, then unhook.
     - `ESC P + q 536D ESC \` dispatches hook with intermediate `+`, final `q`,
       payload bytes, then unhook.
     - DCS with params, semicolon separators, multiple intermediates, and a
       final byte preserves the parsed Ghostty-shaped hook metadata.
     - DCS colon from entry and colon from param enter ignore and do not
       dispatch DCS actions.
     - DCS `<`..`?` before params follows Ghostty's entry collect path, while
       `<`..`?` after params enters ignore.
     - DCS with too many params or too many intermediates remains contained and
       dispatches no hook.
     - BEL inside DCS payload dispatches `DcsPut { byte: 0x07 }` rather than
       terminating.
     - malformed DCS header consumes through ESC without dispatching DCS actions
       or printing payload/final bytes.
     - `ESC P ... ESC X B` dispatches `DcsUnhook`, does not print DCS payload,
       and handles the `ESC X` path without leaking `X` as printable text.
     - split-feed DCS sequences preserve state across `next_slice` calls.
   - Stream APC tests:
     - `ESC _ G payload ESC \` dispatches start, put bytes, and end.
     - unknown APC payload dispatches the same framing actions; protocol
       interpretation is out of scope.
     - BEL inside APC payload dispatches `ApcPut { byte: 0x07 }`.
     - `ESC _ ... ESC [ ... B` dispatches `ApcEnd`, does not print APC payload,
       and handles the next escape path without leaking APC bytes.
     - split-feed APC sequences preserve state across `next_slice` calls.
   - No-leak tests:
     - DCS/APC payloads surrounded by printable bytes print only the surrounding
       printable bytes.
     - After DCS/APC termination, normal printing resumes.
     - malformed DCS and APC sequences interrupted by ESC do not leak payload or
       final bytes.
     - if a handler returns an error during DCS/APC hook/put/end actions, the
       stream returns to a recoverable state before the error is surfaced, and a
       later printable byte can be processed normally.
   - Terminal tests:
     - DCS/APC sequences do not mutate visible display content or PTY response
       state.

## Verification

1. Run formatting:

   ```bash
   cargo fmt
   ```

2. Run focused tests:

   ```bash
   cargo test -p roastty dcs_apc
   cargo test -p roastty stream_dcs
   cargo test -p roastty stream_apc
   ```

3. Run the full Roastty test suite:

   ```bash
   cargo test -p roastty
   ```

## Design Review

Codex reviewed the initial design and agreed the scope was right, but did not
approve until several Ghostty parser semantics were fixed:

- DCS/APC strings exit on ESC, not only on complete ST (`ESC \`).
- DCS hook metadata must match Ghostty's `intermediates`, `params`, and `final`
  shape rather than a CSI-like private/intermediate/separator shape.
- DCS header parsing must use explicit DCS entry/param/intermediate/ignore rules
  instead of "similar to CSI."
- APC terminal no-op behavior is an intentional Roastty deferment, not a match
  for Ghostty's APC runtime, which already feeds Kitty graphics.
- No-leak tests must include ESC interruption and handler-error recovery cases.

Codex approved the revised design after those updates. No remaining required
design fixes.

Implementation note from Codex: be precise in tests around `ESC X` after
DCS/APC. Ghostty treats `ESC X` as SOS/PM/APC string entry; Roastty's narrower
7-bit APC scope may consume it as unsupported. Either is acceptable only if the
test expectation explicitly pins which bytes print or remain contained.

## Result

**Result:** Pass

Roastty now has first-class DCS and APC stream framing:

- `ESC P` enters DCS header parsing, emits typed `DcsHook` metadata on valid
  final bytes, emits `DcsPut` for payload bytes, and emits `DcsUnhook` when ESC
  exits the DCS string.
- `ESC _` enters APC string parsing, emits `ApcStart`, emits `ApcPut` for
  payload bytes, and emits `ApcEnd` when ESC exits the APC string.
- DCS hook metadata matches Ghostty's event shape: intermediates, params, and
  final byte.
- DCS malformed headers and metadata-capacity overflow enter an ignore state
  that contains payload bytes until ESC without dispatching DCS actions.
- BEL is payload for both DCS and APC; it does not terminate either string.
- Split-feed sequences preserve DCS/APC state across `next_slice` calls.
- Handler errors during DCS hook, DCS payload, DCS unhook, APC start, APC
  payload, and APC end leave the stream in a recoverable state before the error
  is surfaced.
- `TerminalStreamHandler` intentionally treats all DCS/APC actions as runtime
  no-ops for now, so framed payloads do not mutate visible content or PTY
  responses.

This experiment does not implement command-specific DCS or APC protocols.
DECRQSS, XTGETTCAP, tmux control mode, and Kitty graphics remain future work on
top of this framing layer.

Verification run:

```bash
cargo fmt
cargo test -p roastty dcs_apc
cargo test -p roastty stream_dcs
cargo test -p roastty stream_apc
cargo test -p roastty
```

All tests passed. The full Roastty suite reported 1610 unit tests, 1 ABI harness
test, and 0 doc tests passing.

## Result Review

Codex reviewed the completed implementation and initially found one missing
test-coverage item: handler-error recovery was only covered for `DcsHook` and
`ApcStart`, while the approved design required recovery coverage for hook,
payload, and end actions.

The implementation was updated with recovery tests for `DcsPut`, `DcsUnhook`,
`ApcPut`, and `ApcEnd`. Codex reviewed the revised result and approved it with
no remaining code or test findings. It specifically confirmed that the added
tests adequately cover the previously missing error paths and that the DCS/APC
stream implementation is correct for this experiment's scope.

## Conclusion

Experiment 147 completed the DCS/APC containment and framing layer needed before
Roastty can implement protocol-specific DCS and APC behavior. The next
experiment should build on this by selecting a coherent DCS/APC protocol slice,
most likely one of DECRQSS, XTGETTCAP, tmux control mode, or Kitty graphics.
