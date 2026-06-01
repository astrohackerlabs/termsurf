# Experiment 159: Port Key Event Value Types

## Description

Port Ghostty's key input value types into Roastty as an internal, testable input
foundation with Roastty naming.

The next keyboard-related subsystem cannot start with live macOS input or public
C ABI. Ghostty's key encoder depends first on a normalized key event model:

- `vendor/ghostty/src/input/key.zig`
  - `KeyEvent`;
  - key `Action`;
  - W3C-style physical `Key` enum;
  - key helper methods such as ASCII mapping, W3C names, keypad detection, and
    macOS `ctrlOrSuper` behavior.
- `vendor/ghostty/src/input/key_mods.zig`
  - packed modifier flags;
  - modifier-side tracking;
  - effective/binding modifiers;
  - macOS option-as-alt translation behavior.

Roastty already has terminal-side Kitty keyboard runtime state from Experiment
155, but it does not yet have the app/input-side key event model that future key
encoding, public key C ABI, and macOS Swift event translation will consume.

This experiment ports only the value types and pure helpers. It must not port
the key encoder, key C ABI, live macOS input, keybindings, modifier remap
config, PTY writes, or Swift integration.

## Changes

1. Add an internal input module.
   - Create `roastty/src/input/mod.rs`.
   - Add `roastty/src/input/key.rs`.
   - Add `roastty/src/input/key_mods.rs`.
   - Register the input module from `roastty/src/lib.rs` or the crate root in
     the narrowest way that lets tests and later terminal/app slices use it.
   - Use `roastty` / `Roastty` names in comments and tests unless citing the
     upstream Ghostty source paths.

2. Port key modifier value types from `input/key_mods.zig`.
   - Add a `Mods` type with the same bit layout as upstream where practical:
     shift, ctrl, alt, super, caps-lock, num-lock, and side bits for shift,
     ctrl, alt, and super.
   - Add a small internal `OptionAsAlt` value type with the same pure choices
     used by upstream (`false`, `true`, `left`, `right`) so modifier translation
     can be tested without importing the config subsystem.
   - Add modifier helper types for:
     - individual modifier identity;
     - left/right side;
     - bindable modifier keys.
   - Add helpers equivalent to:
     - `int()`;
     - `empty()`;
     - equality;
     - `keys()`;
     - `binding()`;
     - `unset()`;
     - `withoutLocks()`;
     - `translation()` for macOS option-as-alt behavior;
     - `ctrl_or_super()` using macOS `super`, not Linux `ctrl`.
   - Keep the implementation macOS-only. Do not preserve live Linux branches.
   - Do not port `RemapSet`, modifier-remap config parsing, or any config parser
     dependency in this experiment. That belongs to the config/keybinding
     subsystem.

3. Port key event value types from `input/key.zig`.
   - Add `KeyAction` with release, press, and repeat values.
   - Add `KeyEvent` with:
     - action;
     - physical key;
     - modifiers;
     - consumed modifiers;
     - composing flag;
     - UTF-8 text bytes;
     - unshifted codepoint.
   - Add `effective_mods()` and a binding hash or equivalent deterministic
     helper that covers the same fields Ghostty uses for bindings.
   - Preserve the distinction between all modifiers and effective modifiers:
     when UTF-8 text exists, consumed modifiers must be removed from the
     effective set.

4. Port the physical key enum and pure helpers.
   - Add the full W3C-derived `Key` enum from `input/key.zig`, renamed to
     Roastty/Rust style.
   - Preserve the upstream integer ordering because later public C ABI and key
     encoder work will depend on stable values.
   - Add helpers equivalent to:
     - `from_ascii()`;
     - `codepoint()`;
     - `keypad()`;
     - `w3c()`;
     - `from_w3c()`;
     - `ctrl_or_super()`;
     - `left_or_right_shift()`;
     - `left_or_right_alt()`.
   - Ensure `from_ascii()` prefers non-keypad keys, matching upstream's
     codepoint-map ordering.
   - Ensure `codepoint()` returns normal printable/keypad codepoints where
     upstream does and `None` for non-printable keys.

5. Add parity tests.
   - Test modifier bit layout against the upstream examples:
     - empty is `0`;
     - shift is bit `0`;
     - representative side bits have stable values.
   - Test modifier helpers:
     - binding modifiers drop lock/side state;
     - `unset()` removes consumed modifiers;
     - `without_locks()` clears caps/num lock only;
     - macOS option-as-alt translation clears the configured option side and
       keeps the non-configured side.
   - Test key event helpers:
     - effective modifiers equal raw modifiers when UTF-8 text is empty;
     - consumed modifiers are removed only when UTF-8 text is non-empty;
     - binding hash changes for key/modifier/unshifted-codepoint changes and
       does not depend on action or UTF-8 text.
   - Test key enum helpers:
     - `from_ascii('0')` returns `Digit0`, not `Numpad0`;
     - `codepoint()` returns representative printable codepoints, keypad
       codepoints, and `None` for non-printable keys;
     - keypad detection distinguishes numpad keys from normal digit keys;
     - every enum value round-trips through `w3c()` and `from_w3c()` unless it
       intentionally has no W3C name;
     - macOS `ctrl_or_super()` returns true for left/right meta keys and false
       for left/right control keys.

6. Keep scope boundaries hard.
   - Do not port `input/key_encode.zig`.
   - Do not add `roastty_key_event_t`, `roastty_key_encoder_t`, or any other
     public key C ABI.
   - Do not wire live Swift/macOS input, app runtime, renderer, PTY process,
     browser overlay, or TermSurf protocol behavior.
   - Do not port keybindings, keymap, command binding, or modifier remap config.
   - Do not add non-macOS platform branches.

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
cargo fmt -- roastty/src/lib.rs roastty/src/input/mod.rs roastty/src/input/key.rs roastty/src/input/key_mods.rs
cargo test -p roastty key_event
cargo test -p roastty key_mods
cargo test -p roastty
```

Required coverage:

- New input modules compile without public ABI changes.
- Modifier bit layout and helper behavior match the upstream tests named in this
  experiment.
- Key event effective-modifier and binding-hash behavior matches Ghostty's field
  choices.
- Physical key enum ordering is stable and documented with tests for total
  variant count, `KeyAction` discriminants (`release = 0`, `press = 1`,
  `repeat = 2`), and boundary/sentinel discriminants from each upstream key enum
  section.
- W3C helper tests cover every enum value that has a W3C name.
- `codepoint()` helper tests cover normal printable keys, keypad keys, and
  non-printable keys.
- ASCII mapping prefers non-keypad keys.
- macOS-only `ctrl_or_super()` behavior uses `super` / `meta`, not `ctrl`.
- Existing Kitty keyboard runtime tests still pass.
- Existing mouse, terminal, formatter, and ABI harness tests still pass through
  the full suite.
- Codex design review and result review both pass before moving to the next
  stage.

## Non-Negotiable Invariants

- Use Roastty names in implementation-facing comments, tests, and modules.
- Preserve the upstream key enum integer order for later ABI compatibility.
- Preserve modifier bit layout where practical and test the layout.
- Keep option-as-alt as a small internal input value type in this experiment; do
  not import or port config parsing to support it.
- Keep this experiment pure and internal: no public C ABI, live input, key
  encoding, PTY writes, Swift integration, renderer behavior, browser overlay,
  TermSurf protocol behavior, keybindings, or config remapping.
- Keep the port macOS-only. Do not preserve Linux behavior gates.
- Run `cargo fmt` and accept its output.
- Pass Codex design and result reviews before moving to the next stage.

## Failure Criteria

This experiment fails if:

- public `ghostty_*` or compatibility key ABI names are introduced;
- key enum integer values diverge from upstream ordering without a documented
  reason and test coverage;
- `KeyAction` discriminants do not match upstream ordering;
- modifier bit layout differs from upstream in a way that would break future ABI
  or key encoding work;
- option-as-alt translation pulls in config parsing or broader config behavior;
- `codepoint()` is omitted or diverges from upstream printable/keypad behavior;
- effective modifiers ignore consumed modifiers when UTF-8 text exists;
- ASCII mapping returns keypad keys before non-keypad keys;
- macOS `ctrl_or_super()` behavior uses Linux control-key semantics;
- key encoder, key C ABI, live input, PTY writes, Swift/app/runtime integration,
  renderer behavior, browser overlay, TermSurf protocol behavior, keybindings,
  config remapping, or non-macOS platform behavior is added;
- existing Kitty keyboard, mouse, terminal, formatter, or ABI tests regress;
- the design or result proceeds without the required Codex review gate.

## Codex Design Review

Codex reviewed the initial design before implementation.

Initial review artifacts:

- Prompt: `logs/codex-review/20260601-135827-873565-prompt.md`
- Result: `logs/codex-review/20260601-135827-873565-last-message.md`

Codex found three real design issues:

- `Key::codepoint()` was missing from the value-type foundation even though the
  future key encoder depends on it.
- `translation()` needed an internal option-as-alt value type so the experiment
  would not accidentally pull in config parsing.
- enum-order verification needed to be stronger than representative values
  because the key enum will later be ABI-sensitive.

All three findings were fixed.

Clean design re-review artifacts:

- Prompt: `logs/codex-review/20260601-140030-144817-prompt.md`
- Result: `logs/codex-review/20260601-140030-144817-last-message.md`

Codex found no remaining blockers and approved the experiment for
implementation.
