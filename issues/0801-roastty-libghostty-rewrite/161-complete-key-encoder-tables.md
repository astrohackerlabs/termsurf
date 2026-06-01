# Experiment 161: Complete Key Encoder Tables

## Description

Experiment 160 established the pure internal key encoder and proved the major
Kitty and legacy branches with representative cases. It intentionally deferred
full table parity. This experiment completes that table layer before any live
input integration.

The upstream source material is:

- `vendor/ghostty/src/input/kitty.zig`
  - full Kitty functional-key table;
  - modifier-key entries;
  - numpad and lock-key entries.
- `vendor/ghostty/src/input/function_keys.zig`
  - PC-style legacy function-key table;
  - cursor/application mode entries;
  - keypad/application mode entries;
  - modifier-specific function-key sequences;
  - backspace/tab/enter/escape special tables.
- `vendor/ghostty/src/input/key_encode.zig`
  - `pcStyleFunctionKey(...)` table selection semantics.

The goal is to replace Experiment 160's minimal ad hoc tables with faithful Rust
equivalents for every upstream table entry whose `Key` variant already exists in
Roastty. This is still a pure internal encoder experiment. It must not add
public ABI, live keyboard input, keybindings, config parsing, PTY writes,
runtime dispatch, Swift/macOS event handling, renderer behavior, browser overlay
behavior, or terminal-handle `Options::from_terminal` wiring.

## Changes

1. Complete the Kitty functional-key table.
   - Expand `kitty_entry(...)` in `roastty/src/input/key_encode.rs` to cover
     every entry in `vendor/ghostty/src/input/kitty.zig` that has a matching
     Roastty `Key` variant:
     - escape, enter, tab, backspace;
     - insert, delete, arrows, page up/down, home/end;
     - caps lock, scroll lock, num lock, print screen, pause;
     - F1-F25;
     - numpad digits, decimal, divide, multiply, subtract, add, enter, equal,
       separator, navigation, insert/delete, page, home/end/begin;
     - left/right shift, control, meta, and alt.
   - Keep unsupported upstream entries omitted only if Roastty lacks the `Key`
     variant. Document any omissions in the result.
   - Preserve Experiment 160's pure sequence formatter; this experiment should
     only complete the lookup data and table-driven behavior.

2. Replace the legacy PC-style lookup with a real table model.
   - Add small internal table types in `key_encode.rs`, equivalent to upstream
     `function_keys.zig`:
     - cursor mode requirement: any/normal/application;
     - keypad mode requirement: any/normal/application;
     - modifyOtherKeys requirement: any/set/set-other;
     - exact binding modifiers;
     - `mods_empty_is_any`;
     - default sequence and optional DECBKM sequence.
   - Port the upstream table entries for all supported Roastty keys:
     - arrows, home/end, insert/delete, page up/down;
     - F1-F12;
     - numpad digits/operators/navigation;
     - backspace, tab, enter, escape.
   - Preserve upstream selection semantics from `pcStyleFunctionKey(...)`:
     - compare binding modifiers only;
     - honor cursor application mode;
     - honor keypad application mode and `ignore_keypad_with_numlock`;
     - distinguish normal modify-key mode from `modify_other_keys_state_2`;
     - prefer DECBKM sequence when `backarrow_key_mode` is set and the entry
       provides one.

3. Keep non-table encoder behavior out of scope.
   - Do not expand `ctrl_seq(...)` beyond cases needed by Experiment 160.
   - Do not expand CSI-u / modifyOtherKeys character fallback beyond existing
     behavior unless a table case requires it.
   - Do not add `Options::from_terminal`.
   - Do not wire the encoder into terminal/runtime input delivery.

4. Add table-focused parity tests.
   - Add tests that assert the Kitty table produces the upstream sequence class
     for representative members of every completed group:
     - insert/delete/page/home/end;
     - F1, F3, F5, F12, F13, F25;
     - lock/system keys;
     - numpad digit/operator/navigation keys;
     - left/right modifier keys with `report_all`;
     - arrow special final-byte behavior.
   - Add tests that assert legacy PC-style behavior for representative members
     of every completed group:
     - all four arrows in normal and cursor-application mode;
     - home/end and page/insert/delete default plus one modified form;
     - F1-F12 plain and Ctrl forms;
     - keypad digits/operators/default application mode and
       `ignore_keypad_with_numlock`;
     - numpad navigation keys;
     - backspace DECBKM and modifyOtherKeys state 2 forms;
     - tab/enter/escape normal, Alt, and modifyOtherKeys state 2 forms.
   - Add omission-proof table integrity tests, not only representative behavior
     tests:
     - define an expected Kitty entry list in the test module containing every
       supported upstream tuple as `(Key, code, final_byte, modifier)`;
     - assert the implemented Kitty table has exactly that length;
     - assert every expected Kitty tuple is returned exactly by the lookup
       helper;
     - define an expected legacy PC-style supported-key list grouped by behavior
       family (cursor keys, edit/navigation keys, F1-F12, keypad
       digits/operators/navigation, and backspace/tab/enter/escape);
     - assert every expected legacy key has table entries;
     - assert each behavior family has at least one exact sequence test covering
       its mode/modifier selection semantics.
   - Representative behavior tests are still required, but they are not enough
     to prove table completion.

5. Keep verification broad enough to catch table regressions.
   - Run the focused key encoder suite.
   - Re-run the key event and Kitty keyboard state suites because this
     experiment changes visibility and shares the `KeyFlags` type.
   - Re-run the full Roastty suite.
   - Re-run the Roastty naming grep.

6. Independent review.
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
cargo fmt -- roastty/src/input/key_encode.rs roastty/src/input/mod.rs roastty/src/terminal/kitty.rs roastty/src/terminal/mod.rs
cargo test -p roastty key_encode
cargo test -p roastty key_event
cargo test -p roastty kitty_keyboard
cargo test -p roastty
! rg -n "ghostty|Ghostty|ghostty_" roastty/src/input roastty/src/lib.rs
```

Required evidence:

- Kitty lookup has an exact integrity test covering every upstream `kitty.zig`
  entry with an existing Roastty `Key` variant: expected count, expected key,
  expected code, expected final byte, and expected modifier flag.
- Legacy PC-style lookup has an integrity test covering every upstream
  `function_keys.zig` supported key group with an existing Roastty `Key`
  variant, plus exact sequence tests for each group.
- Cursor application mode and keypad application mode select the upstream
  sequences.
- `ignore_keypad_with_numlock` forces normal numeric keypad behavior.
- `modify_other_keys_state_2` selects the upstream set-other entries for
  backspace/tab/enter/escape.
- DECBKM still selects the alternate backspace sequence.
- Existing Experiment 160 representative tests still pass.
- The encoder remains pure and internal.
- No public ABI, live input path, PTY write path, keybinding/keymap/config
  behavior, runtime dispatch, renderer behavior, browser overlay behavior, or
  terminal-handle `Options::from_terminal` behavior is added.
- Codex design review and result review both pass before moving to the next
  stage.

## Non-Negotiable Invariants

- Use Roastty names in implementation-facing comments, tests, and modules.
- Keep the encoder pure and internal.
- Do not add public key C ABI.
- Do not wire live input, runtime dispatch, or PTY writes.
- Do not add keybindings, keymaps, config parsing, modifier remap config, or
  `Options::from_terminal`.
- Do not add non-macOS platform behavior.
- Do not change the external `roastty_*` ABI.
- Run `cargo fmt` and accept its output.
- Pass Codex design and result reviews before moving to the next stage.

## Failure Criteria

This experiment fails if:

- any public `ghostty_*` or compatibility key ABI names are introduced;
- the encoder sends bytes to PTY/runtime/app code instead of returning them;
- the table omits supported upstream Kitty entries without documenting why;
- the table omits supported upstream PC-style legacy entry groups without
  documenting why;
- cursor/keypad application mode, DECBKM, or modifyOtherKeys state 2 behavior
  diverges from the upstream cases in this experiment;
- table completion is mixed with live input, public ABI, config/keybinding,
  runtime, renderer, browser overlay, or terminal-handle wiring;
- existing key encoder, key event, Kitty keyboard runtime, terminal, mouse,
  formatter, or ABI tests regress;
- the design or result proceeds without the required Codex review gate.

## Codex Design Review

Codex reviewed the initial design before implementation.

Initial review artifacts:

- Prompt: `logs/codex-review/20260601-142712-565890-prompt.md`
- Result: `logs/codex-review/20260601-142712-565890-last-message.md`

Codex found one real design issue: the original table-completion requirement
could still pass with only representative behavior tests while silently omitting
supported upstream table entries. The design was tightened to require exact
Kitty tuple integrity checks and legacy PC-style supported-key/group integrity
checks.

Codex also noted that Experiment 160 needed to be committed before Experiment
161 implementation. That process condition was already satisfied by
`e42807116646f`.

Clean design re-review artifacts:

- Prompt: `logs/codex-review/20260601-142843-694803-prompt.md`
- Result: `logs/codex-review/20260601-142843-694803-last-message.md`

Codex found no remaining design blockers and approved the experiment for
implementation.
