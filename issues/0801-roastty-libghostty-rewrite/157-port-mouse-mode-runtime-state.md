# Experiment 157: Port Mouse Mode Runtime State

## Description

Port Ghostty's runtime mouse tracking state that sits between CSI mode changes
and the pure mouse encoder added in Experiment 156.

Experiment 156 added a pure `mouse_encode` module, but nothing in `Terminal`
currently maintains the encoder-facing mouse event mode or mouse output format.
Roastty's `ModeState` can record whether individual DEC modes are set, but
Ghostty explicitly does not derive mouse encoder state from mode bits alone:
multiple mouse modes can be set over time, and the correct encoder state is the
last mode command that affected the mouse event or format family.

Upstream Ghostty source references:

- `vendor/ghostty/src/terminal/Terminal.zig`
  - stores `flags.mouse_event`, `flags.mouse_format`, and
    `flags.mouse_shift_capture` separately from normal mode bits.
- `vendor/ghostty/src/terminal/stream_terminal.zig`
  - updates `flags.mouse_event` when `?9`, `?1000`, `?1002`, and `?1003` are set
    or reset;
  - updates `flags.mouse_format` when `?1005`, `?1006`, `?1015`, and `?1016` are
    set or reset;
  - updates `flags.mouse_shift_capture` from the XTSHIFTESCAPE action.
- `vendor/ghostty/src/terminal/stream.zig`
  - parses `CSI > s`, `CSI > 0 s`, and `CSI > 1 s` as XTSHIFTESCAPE false/false
    /true;
  - ignores invalid XTSHIFTESCAPE forms.
- `vendor/ghostty/src/input/mouse_encode.zig`
  - consumes the resulting `mouse_event` and `mouse_format` values through
    encoder options.

This experiment should add the terminal runtime state and query helpers needed
for future mouse input wiring. It must not wire live macOS mouse events, public
ABI, Swift frontend, renderer event dispatch, PTY event writing, browser overlay
behavior, or TermSurf protocol behavior.

## Changes

1. Extend `TerminalFlags` in `roastty/src/terminal/terminal.rs`.
   - Add `mouse_event: mouse::MouseEventMode`, defaulting to `None`.
   - Add `mouse_format: mouse::MouseFormat`, defaulting to `X10`.
   - Add `mouse_shift_capture: MouseShiftCapture` or equivalent tri-state,
     defaulting to unset/null.
   - Preserve existing `modify_other_keys_2` behavior.
   - Ensure RIS/full reset restores all new mouse flags to defaults.

2. Update mouse event mode runtime behavior in `set_mode_basic()`.
   - `?9h` sets `flags.mouse_event = X10`; `?9l` sets it to `None`.
   - `?1000h` sets `Normal`; `?1000l` sets `None`.
   - `?1002h` sets `Button`; `?1002l` sets `None`.
   - `?1003h` sets `Any`; `?1003l` sets `None`.
   - Keep updating `ModeState` exactly as before; the new flag is an additional
     last-command runtime cache, not a replacement for mode bits.
   - Match Ghostty's behavior even if another mouse event mode remains set in
     `ModeState`: resetting one mouse event mode sets the runtime event flag to
     `None` rather than falling back to an older mode bit.

3. Update mouse format runtime behavior in `set_mode_basic()`.
   - `?1005h` sets `flags.mouse_format = Utf8`; `?1005l` sets `X10`.
   - `?1006h` sets `Sgr`; `?1006l` sets `X10`.
   - `?1015h` sets `Urxvt`; `?1015l` sets `X10`.
   - `?1016h` sets `SgrPixels`; `?1016l` sets `X10`.
   - Keep updating `ModeState` exactly as before.
   - Match Ghostty's last-command behavior: resetting any mouse format mode sets
     the runtime format to `X10`, even if another mouse format mode bit remains
     set.

4. Parse and apply XTSHIFTESCAPE.
   - Extend `stream::Action` with a mouse-shift-capture action carrying a bool.
   - Parse `CSI > s` and `CSI > 0 s` as false.
   - Parse `CSI > 1 s` as true.
   - Ignore `CSI > 2 s`, extra params, colon params, and unrelated `CSI s`
     forms.
   - Do not let the XTSHIFTESCAPE parser steal existing `CSI ? Ps s` mode-save
     behavior. In Roastty's parser representation, `>` may be carried as the CSI
     private marker rather than as a Ghostty-style intermediate byte; the
     implementation should match Roastty's parser shape while preserving the
     existing mode-save path for `?`.
   - Apply the action to `TerminalFlags::mouse_shift_capture` as explicit false
     or explicit true. Leave the default unset/null state intact until the
     action is received.

5. Add internal test helpers only.
   - Add `#[cfg(test)]` helpers to inspect the mouse event mode, mouse format,
     and mouse shift capture state.
   - Do not add public ABI or app-facing APIs.
   - Optionally add a small internal helper that returns the current
     encoder-facing `(MouseEventMode, MouseFormat)` pair for future app wiring,
     but keep it crate-internal and unused by live input paths.

6. Keep scope boundaries hard.
   - Do not call `mouse_encode::encode()` from terminal runtime or any app path
     in this experiment.
   - Do not wire live mouse events.
   - Do not add C ABI wrappers.
   - Do not add Swift, renderer, PTY write, browser overlay, TermSurf protocol,
     or Kitty graphics behavior.
   - Do not add Linux or other non-macOS platform paths.

7. Independent review.
   - Before implementation, get Codex review of this experiment design.
   - Fix every real finding and re-review until Codex finds no remaining
     blocking design issues.
   - Record the design-review outcome in this experiment file before committing
     the design.
   - After implementation and verification, get Codex review of the completed
     result before committing the result.
   - Do not proceed to the next experiment until the completed result review is
     approved or every real result finding has been fixed and re-reviewed.

## Verification

Run:

```bash
cargo fmt
cargo test -p roastty mouse
cargo test -p roastty modes
cargo test -p roastty
```

Required test coverage:

- Stream parser tests:
  - `CSI > s` dispatches mouse-shift-capture false;
  - `CSI > 0 s` dispatches false;
  - `CSI > 1 s` dispatches true;
  - `CSI > 2 s`, extra params, colon params, wrong private markers, and
    unrelated `CSI s` forms are ignored and do not leak the final byte.
- Runtime mouse event mode tests:
  - default runtime event mode is `None`;
  - `?9h/?9l` sets X10/None;
  - `?1000h/?1000l` sets Normal/None;
  - `?1002h/?1002l` sets Button/None;
  - `?1003h/?1003l` sets Any/None;
  - last set wins across event modes;
  - resetting any event mode sets runtime event mode to `None` even if another
    event mode bit remains true in `ModeState`.
  - saving and restoring a mouse event mode with `CSI ? Ps s` / `CSI ? Ps r`
    updates both `ModeState` and the runtime mouse event cache through the
    normal mode-setting path.
- Runtime mouse format tests:
  - default runtime format is X10;
  - `?1005h/?1005l` sets Utf8/X10;
  - `?1006h/?1006l` sets SGR/X10;
  - `?1015h/?1015l` sets URXVT/X10;
  - `?1016h/?1016l` sets SGR-pixels/X10;
  - last set wins across format modes;
  - resetting any format mode sets runtime format to X10 even if another format
    bit remains true in `ModeState`.
  - saving and restoring a mouse format mode with `CSI ? Ps s` / `CSI ? Ps r`
    updates both `ModeState` and the runtime mouse format cache through the
    normal mode-setting path.
- Runtime XTSHIFTESCAPE tests:
  - default mouse shift capture is unset/null;
  - `CSI > s` and `CSI > 0 s` store explicit false;
  - `CSI > 1 s` stores explicit true;
  - invalid forms leave the previous value unchanged.
  - `CSI ? Ps s` mode-save for a mouse mode still dispatches as mode-save, not
    as XTSHIFTESCAPE or an ignored final byte.
- Reset/regression tests:
  - RIS/full reset restores mouse event mode, mouse format, and shift capture to
    defaults;
  - existing mouse encoder tests still pass;
  - existing mouse-shape and OSC 22 tests still pass;
  - existing mode table and mode runtime tests still pass;
  - no public ABI, app integration, renderer, PTY process, browser overlay,
    protocol, or platform-input behavior changes.
- Review gates:
  - Codex design review approves the experiment before implementation, or every
    real design finding is fixed and re-reviewed cleanly;
  - Codex result review approves the completed experiment before result commit,
    or every real result finding is fixed and re-reviewed cleanly.

## Non-Negotiable Invariants

- Port runtime state only; do not encode or send live mouse events.
- Keep `ModeState` behavior intact and add the Ghostty-style runtime cache
  beside it.
- Do not add public ABI or app-facing API.
- Do not add renderer, Swift, app runtime, PTY process, browser overlay,
  TermSurf protocol, Kitty graphics, or non-macOS platform behavior.
- Do not use `ghostty_*` names except when citing upstream Ghostty source paths
  or behavior.
- Run `cargo fmt` and accept its output.
- Pass Codex design and result reviews before moving to the next stage.

## Failure Criteria

This experiment fails if:

- mouse event or format runtime flags are derived from current mode bits instead
  of updated according to Ghostty's last-command behavior;
- mode save/restore updates `ModeState` but leaves stale mouse event or mouse
  format runtime cache state;
- resetting a mouse event mode falls back to a previously enabled event mode;
- resetting a mouse format mode falls back to a previously enabled non-X10
  format;
- XTSHIFTESCAPE invalid forms mutate state or leak final bytes;
- RIS leaves stale mouse event, format, or shift-capture state;
- existing mode, mouse encoder, mouse shape, or OSC 22 tests regress;
- the design or result proceeds without the required Codex review gate;
- the patch wires live mouse input, calls the encoder from app/runtime paths,
  adds public ABI, renderer behavior, PTY write behavior, browser overlay
  behavior, TermSurf protocol behavior, Kitty graphics, or non-macOS platform
  paths.

## Codex Design Review

Codex reviewed the initial design before implementation.

Initial review artifacts:

- Prompt: `logs/codex-review/20260601-133252-153475-prompt.md`
- Result: `logs/codex-review/20260601-133252-153475-last-message.md`

Codex found two real design gaps:

- mode save/restore needed explicit coverage for the new mouse event and format
  runtime caches;
- XTSHIFTESCAPE needed to preserve existing `CSI ? Ps s` mode-save behavior
  rather than treating every private-marker `s` form as an ignored XTSHIFTESCAPE
  variant.

Both findings were fixed in the design.

Clean design re-review artifacts:

- Prompt: `logs/codex-review/20260601-133439-361587-prompt.md`
- Result: `logs/codex-review/20260601-133439-361587-last-message.md`

Codex found no remaining blockers and approved the experiment for
implementation.

## Result

**Result:** Pass

Implemented Ghostty-style runtime mouse mode state in Roastty without wiring
live mouse input or calling the mouse encoder from runtime paths.

Code changes:

- `roastty/src/terminal/terminal.rs`
  - extended `TerminalFlags` with `mouse_event`, `mouse_format`, and
    `mouse_shift_capture`;
  - updates the runtime event cache from `?9`, `?1000`, `?1002`, and `?1003`
    set/reset commands using Ghostty's last-command behavior;
  - updates the runtime format cache from `?1005`, `?1006`, `?1015`, and `?1016`
    set/reset commands using Ghostty's last-command behavior;
  - applies XTSHIFTESCAPE to the runtime shift-capture flag;
  - preserves `ModeState` as the mode table source and keeps runtime mouse
    caches beside it;
  - RIS/full reset restores all new flags to defaults;
  - added internal test helpers for the new runtime state only.
- `roastty/src/terminal/stream.rs`
  - added `Action::MouseShiftCapture`;
  - parses `CSI > s` / `CSI > 0 s` as explicit false and `CSI > 1 s` as explicit
    true;
  - rejects invalid XTSHIFTESCAPE forms without leaking final bytes;
  - preserves existing `CSI ? Ps s` mouse-mode save behavior.

Verification run:

```bash
cargo fmt -- roastty/src/terminal/terminal.rs roastty/src/terminal/stream.rs
cargo test -p roastty mouse
cargo test -p roastty modes
cargo test -p roastty
```

Results:

- `cargo test -p roastty mouse`: 43 passed, 0 failed.
- `cargo test -p roastty modes`: 34 passed, 0 failed.
- `cargo test -p roastty`: 1744 unit tests passed, ABI harness passed, 0
  doctests.

## Codex Result Review

Codex reviewed the completed implementation and result before commit.

Result review artifacts:

- Prompt: `logs/codex-review/20260601-133920-212006-prompt.md`
- Result: `logs/codex-review/20260601-133920-212006-last-message.md`

Codex found no implementation, test, scope, or result-recording findings and
approved the result for commit.

## Conclusion

Roastty now carries the Ghostty-style mouse runtime state required by the pure
encoder from Experiment 156. The key upstream parity point is preserved: the
runtime mouse event and format values are not derived from currently enabled
mode bits; they follow the last set/reset command, including mode restore.

The encoder is still intentionally unwired. A later experiment can connect
terminal/app input to the encoder once the surrounding input geometry and PTY
write path are ready.
