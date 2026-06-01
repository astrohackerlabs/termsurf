# Experiment 160: Port Key Encoder Core

## Description

Port Ghostty's pure key encoder into Roastty on top of the key input value types
from Experiment 159.

Ghostty's key encoder lives primarily in:

- `vendor/ghostty/src/input/key_encode.zig`
  - encoder `Options`;
  - legacy terminal key encoding;
  - Kitty keyboard protocol encoding;
  - ctrl-sequence encoding;
  - alt-escape prefix behavior;
  - xterm modifyOtherKeys / CSI-u behavior;
  - PC-style function-key lookup;
  - macOS option-as-alt handling.
- `vendor/ghostty/src/input/function_keys.zig`
  - PC-style function-key tables;
  - modifier-code table used by modifyOtherKeys.
- `vendor/ghostty/src/input/kitty.zig`
  - Kitty functional-key table.

This experiment should add the pure encoder and focused parity tests, but it
must not wire live keyboard input, public key C ABI, terminal-handle
`setopt_from_terminal`, PTY process writes, app runtime, Swift frontend,
renderer behavior, browser overlay behavior, keybindings, keymaps, or config
parsing.

The encoder returns bytes to its caller. It does not send them anywhere.

## Changes

1. Add a pure key encoder module.
   - Create `roastty/src/input/key_encode.rs`.
   - Register it from `roastty/src/input/mod.rs`.
   - Reuse `roastty/src/input/key.rs`, `key_mods.rs`, and
     `roastty/src/terminal/kitty.rs::KeyFlags`.
   - If visibility needs to change for `KeyFlags`, make the narrowest
     `pub(crate)` adjustment needed. Do not expose it through public C ABI.

2. Add encoder options.
   - Add an internal `Options` struct equivalent to upstream:
     - `cursor_key_application`;
     - `keypad_key_application`;
     - `backarrow_key_mode`;
     - `ignore_keypad_with_numlock`;
     - `alt_esc_prefix`;
     - `modify_other_keys_state_2`;
     - `kitty_flags`;
     - `macos_option_as_alt`.
   - Defaults must match upstream.
   - Do not add `from_terminal()` in this experiment unless a real public
     terminal/surface terminal handle exists at implementation time. Current
     Roastty does not expose one, so this is expected to remain deferred.

3. Port Kitty keyboard encoding.
   - Add the Kitty functional-key table from `input/kitty.zig`, adapted to
     Roastty `Key` variants.
   - Port the Kitty sequence formatter:
     - basic `CSI ... u` / `CSI ... ~` / arrow final-byte forms;
     - modifier encoding;
     - press/release/repeat event reporting;
     - report-all behavior for enter/tab/backspace;
     - report-alternates behavior;
     - report-associated-text behavior;
     - composed-text and composing-state behavior.
   - Keep the implementation pure: it writes to a provided output buffer/String
     only.

4. Port legacy encoding core.
   - Add PC-style function-key table behavior from `function_keys.zig` for:
     - arrows;
     - home/end/insert/delete/page up/page down;
     - F1-F12;
     - keypad keys;
     - enter/tab/backspace/escape;
     - modifier-sensitive variants;
     - cursor-key and keypad-application modes;
     - DECBKM backarrow mode;
     - modifyOtherKeys table switching.
   - Port ctrl-sequence encoding for control-letter and representative
     non-letter cases.
   - Port alt-escape prefix handling, including macOS option-as-alt behavior.
   - Port modifyOtherKeys state 2 / CSI-u for single-codepoint text with
     modifiers.
   - Preserve macOS behavior that command/super text does not encode in legacy
     mode.

5. Choose a pragmatic first-pass test set.
   - Do not mechanically port every upstream key encoder test in this first
     encoder experiment if doing so would make diagnosis too broad.
   - Port enough upstream-named cases to prove each core branch:
     - Kitty plain text;
     - Kitty repeat with disambiguate;
     - Kitty enter/backspace/tab with report-all off and on;
     - Kitty shift+backspace / shift+enter / shift+tab;
     - Kitty delete and one arrow key;
     - Kitty composing with no modifier and with modifier;
     - Kitty report alternates for shift+a and one non-US-layout example if easy
       to express with existing `KeyEvent`;
     - Kitty report associated text for macOS option/alt distinction;
     - Kitty keypad number;
     - legacy ctrl+c;
     - legacy alt+c;
     - legacy alt-prefix with `macos_option_as_alt = true`;
     - legacy translated option text with `macos_option_as_alt = false`, proving
       Option is not encoded as Alt in that mode;
     - one sided option-as-alt case (`left` or `right`) using Experiment 159's
       `OptionAsAlt`;
     - legacy ctrl+space;
     - legacy backspace with DECBKM reset and set;
     - legacy modifyOtherKeys state 2 for a representative character;
     - legacy modifyOtherKeys state 2 with consumed shift modifiers, matching
       the upstream consumed-modifier parity case;
     - legacy F1 and shift+function key;
     - legacy keypad enter and keypad `1` with application keypad mode;
     - legacy Super-only text and Super+Shift text producing no output on macOS.
   - For every upstream test intentionally not ported, add a short comment or
     issue-result note grouping what remains, such as "remaining Kitty
     alternate-layout matrix" or "remaining PC function-key table expansion."

6. Add focused regression checks.
   - `cargo test -p roastty key_encode` must run the new encoder suite.
   - `cargo test -p roastty key_event` must still pass.
   - `cargo test -p roastty kitty_keyboard` must still pass.
   - `cargo test -p roastty` must still pass.
   - `rg -n "ghostty|Ghostty|ghostty_" roastty/src/input roastty/src/lib.rs`
     must still produce no matches.

7. Keep scope boundaries hard.
   - Do not add `roastty_key_event_t`, `roastty_key_encoder_t`, or any public
     key C ABI.
   - Do not add live Swift/macOS input, app runtime, renderer, PTY process,
     browser overlay, or TermSurf protocol behavior.
   - Do not port keybindings, keymap, command binding, or modifier remap config.
   - Do not add non-macOS platform branches.
   - Do not fake `from_terminal()` without a real terminal handle.

8. Independent review.
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
cargo fmt -- roastty/src/lib.rs roastty/src/input/mod.rs roastty/src/input/key.rs roastty/src/input/key_mods.rs roastty/src/input/key_encode.rs roastty/src/terminal/kitty.rs
cargo test -p roastty key_encode
cargo test -p roastty key_event
cargo test -p roastty kitty_keyboard
cargo test -p roastty
rg -n "ghostty|Ghostty|ghostty_" roastty/src/input roastty/src/lib.rs
```

Required coverage:

- Encoder options default to upstream values.
- Kitty encoding tests cover text, special keys, modifiers, event reporting,
  alternates, associated text, composing behavior, and keypad behavior.
- Legacy encoding tests cover plain text, control sequences, alt-prefix, macOS
  option-as-alt, macOS Super/Command text suppression, DECBKM, modifyOtherKeys
  including consumed modifiers, function keys, cursor/application mode, and
  keypad mode behavior.
- The encoder returns bytes to the caller only; it does not write to any PTY or
  runtime path.
- No public ABI or live input path is added.
- Existing key event, Kitty keyboard runtime, terminal, mouse, formatter, and
  ABI tests still pass through the full suite.
- Codex design review and result review both pass before moving to the next
  stage.

## Non-Negotiable Invariants

- Use Roastty names in implementation-facing comments, tests, and modules.
- Keep the encoder pure and internal.
- Do not add public key C ABI in this experiment.
- Do not wire live input or PTY writes.
- Do not fake `from_terminal()` without a real terminal handle.
- Keep macOS behavior and omit live non-macOS branches.
- Run `cargo fmt` and accept its output.
- Pass Codex design and result reviews before moving to the next stage.

## Failure Criteria

This experiment fails if:

- public `ghostty_*` or compatibility key ABI names are introduced;
- key encoding sends bytes to PTY/runtime/app code instead of returning them;
- Kitty keyboard protocol output diverges from the upstream cases named in this
  experiment;
- legacy control/function/keypad/alt-prefix output diverges from the upstream
  cases named in this experiment;
- macOS command/super text behavior is replaced with Linux behavior;
- option-as-alt handling pulls in config parsing or broader config behavior;
- `from_terminal()` is faked without a real terminal handle;
- public key C ABI, live input, PTY process behavior, Swift/app/runtime
  integration, renderer behavior, browser overlay, TermSurf protocol behavior,
  keybindings, keymap, config remapping, or non-macOS platform behavior is
  added;
- existing key event, Kitty keyboard runtime, terminal, mouse, formatter, or ABI
  tests regress;
- the design or result proceeds without the required Codex review gate.

## Codex Design Review

Codex reviewed the initial design before implementation.

Initial review artifacts:

- Prompt: `logs/codex-review/20260601-141147-695689-prompt.md`
- Result: `logs/codex-review/20260601-141147-695689-last-message.md`

Codex found three real design issues:

- legacy macOS option-as-alt behavior was in scope but not pinned by required
  tests;
- macOS Super/Command text suppression was required but not explicitly verified;
- modifyOtherKeys state 2 needed a consumed-modifier parity case.

All three findings were fixed.

Clean design re-review artifacts:

- Prompt: `logs/codex-review/20260601-141314-432966-prompt.md`
- Result: `logs/codex-review/20260601-141314-432966-last-message.md`

Codex found no remaining blockers and approved the experiment for
implementation.
