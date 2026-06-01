# Experiment 163: Port Key Encoder C ABI

## Description

Experiments 159-162 completed the internal key event value types and pure key
encoder behavior. The next step is to expose that completed behavior through the
renamed Roastty C ABI, following the existing mouse event/encoder ABI pattern in
`roastty/include/roastty.h` and `roastty/src/lib.rs`.

The upstream source material is:

- `vendor/ghostty/src/terminal/c/key_event.zig`
  - key event handle allocation/free;
  - action/key/modifier setters and getters;
  - consumed modifier, composing, UTF-8, and unshifted-codepoint accessors.
- `vendor/ghostty/src/terminal/c/key_encode.zig`
  - key encoder handle allocation/free;
  - key encoder option enum;
  - typed option setting;
  - encode-to-buffer with out-of-space length reporting.
- `vendor/ghostty/src/lib_vt.zig`
  - exported `ghostty_key_*` / `ghostty_key_encoder_*` names, which must be
    renamed to `roastty_key_*` / `roastty_key_encoder_*`.

This experiment should add the public C ABI needed to construct key events,
configure a key encoder, and encode key events into caller-provided buffers. It
must not wire live macOS/Swift input, runtime dispatch, PTY writes, keybindings,
keymaps, config parsing, renderer behavior, browser overlay behavior, or
terminal-handle `setopt_from_terminal` behavior.

## Changes

1. Update the public header.
   - Add opaque handles:
     - `roastty_key_event_t`;
     - `roastty_key_encoder_t`.
   - Add C-facing enums/structs:
     - `roastty_key_action_e` matching `KeyAction` values;
     - `roastty_key_e` covering every existing Roastty `Key` variant and value;
     - `roastty_key_side_e` with `ROASTTY_KEY_SIDE_LEFT = 0` and
       `ROASTTY_KEY_SIDE_RIGHT = 1`;
     - `roastty_key_mods_s` with `bool shift`, `bool ctrl`, `bool alt`,
       `bool super`, `bool caps_lock`, `bool num_lock`, and raw integer side
       fields `int shift_side`, `int ctrl_side`, `int alt_side`,
       `int super_side`;
     - `roastty_option_as_alt_e`;
     - `roastty_key_encoder_option_e` matching internal `Options` fields;
     - a key-encoder option value convention compatible with the existing
       `setopt(handle, option, const void*)` pattern.
   - Define exact key-encoder option payload types and validation:
     - `ROASTTY_KEY_ENCODER_OPTION_CURSOR_KEY_APPLICATION = 0`: `const bool*`;
     - `ROASTTY_KEY_ENCODER_OPTION_KEYPAD_KEY_APPLICATION = 1`: `const bool*`;
     - `ROASTTY_KEY_ENCODER_OPTION_IGNORE_KEYPAD_WITH_NUMLOCK = 2`:
       `const bool*`;
     - `ROASTTY_KEY_ENCODER_OPTION_ALT_ESC_PREFIX = 3`: `const bool*`;
     - `ROASTTY_KEY_ENCODER_OPTION_MODIFY_OTHER_KEYS_STATE_2 = 4`:
       `const bool*`;
     - `ROASTTY_KEY_ENCODER_OPTION_KITTY_FLAGS = 5`: `const uint8_t*` raw Kitty
       flag bitset, with only known bits accepted;
     - `ROASTTY_KEY_ENCODER_OPTION_MACOS_OPTION_AS_ALT = 6`: `const int*`
       containing a validated `roastty_option_as_alt_e` value;
     - `ROASTTY_KEY_ENCODER_OPTION_BACKARROW_KEY_MODE = 7`: `const bool*`.
   - `value == NULL` returns `ROASTTY_INVALID_VALUE` for every
     `roastty_key_encoder_setopt(...)` option.
   - Use Roastty names only. Do not add compatibility `ghostty_*` aliases.

2. Add Rust ABI wrappers in `roastty/src/lib.rs`.
   - Import `input::key`, `input::key_mods`, and `input::key_encode`.
   - Add internal wrappers:
     - `KeyEvent { event: key::KeyEvent }`;
     - `KeyEncoder { opts: key_encode::Options }`.
   - Add handle conversion helpers mirroring the existing mouse helpers.
   - Add enum/value conversion helpers for:
     - key action;
     - key enum;
     - modifier sides;
     - option-as-alt;
     - key encoder options;
     - Kitty keyboard flags from an integer bitset.
   - Every exported Rust function that receives C enum-like values must accept
     raw integer parameters at the FFI boundary, validate them, and then convert
     them to Rust types. Do not expose Rust `repr(C)` enum parameters as the
     validation boundary for:
     - `roastty_key_action_e`;
     - `roastty_key_e`;
     - `roastty_key_side_e`;
     - `roastty_key_encoder_option_e`;
     - `roastty_option_as_alt_e`;
     - Kitty flag bitsets.

3. Export key event functions.
   - `roastty_key_event_new(out)`;
   - `roastty_key_event_free(event)`;
   - action setter/getter;
   - key setter/getter;
   - mods setter/getter;
   - consumed-mods setter/getter;
   - composing setter/getter;
   - UTF-8 setter/getter;
   - unshifted-codepoint setter/getter.
   - Use exact UTF-8 signatures:
     - `roastty_key_event_set_utf8(event, const uint8_t* bytes, size_t len)`;
     - `roastty_key_event_get_utf8(event, size_t* len) -> const uint8_t*`.
   - UTF-8 ABI semantics:
     - `NULL + 0` clears the stored text and returns `ROASTTY_SUCCESS`;
     - `NULL + len > 0` returns `ROASTTY_INVALID_VALUE`;
     - invalid UTF-8 bytes return `ROASTTY_INVALID_VALUE`;
     - accepted bytes are copied into the wrapper-owned event;
     - `get_utf8(...)` returns event-owned memory valid until the next
       `set_utf8(...)` call or `free(...)`;
     - `get_utf8(...)` writes the current length when `len != NULL`;
     - invalid handles return `NULL` and write `0` when `len != NULL`;
     - empty text returns `NULL` and writes `0` when `len != NULL`.
   - Follow the current Roastty ABI style:
     - return `ROASTTY_INVALID_VALUE` for null handles, null required output
       pointers, invalid enum values, or invalid UTF-8 input;
     - tolerate `free(NULL)`;
     - do not store borrowed caller UTF-8 memory.

4. Export key encoder functions.
   - `roastty_key_encoder_new(out)`;
   - `roastty_key_encoder_free(encoder)`;
   - `roastty_key_encoder_setopt(encoder, option, value)`;
   - `roastty_key_encoder_encode(encoder, event, out, out_len, out_written)`.
   - Match mouse encoder buffer semantics:
     - if encoded length is greater than `out_len`, write required length and
       return `ROASTTY_OUT_OF_SPACE`;
     - allow `out == NULL` only when `out_len == 0`;
     - return `ROASTTY_INVALID_VALUE` for null encoder/event/out_written or
       invalid options.
   - Do not add `roastty_key_encoder_setopt_from_terminal` yet. There is not a
     real public terminal/surface terminal handle to read from.

5. Add ABI and Rust tests.
   - Add Rust unit tests in `roastty/src/lib.rs` for:
     - key event allocation/free and null handling;
     - setters/getters for action, key, mods, consumed mods, composing, UTF-8,
       and unshifted codepoint;
     - invalid enum/options returning `ROASTTY_INVALID_VALUE`;
     - invalid key action, key enum, key side, option enum, option-as-alt, and
       unknown Kitty flag bits all returning `ROASTTY_INVALID_VALUE`;
     - exact `KeyAction` discriminants, total key count, and representative
       `roastty_key_e` constants mapping to internal `Key` variants from every
       section of the key enum, including first and last values;
     - left/right shift, ctrl, alt, and super modifier side round trips;
     - UTF-8 copy semantics after the caller buffer is dropped or mutated;
     - key encoder option setting for every option;
     - key encoder encode success and out-of-space behavior;
     - one Kitty encoded case and one legacy encoded case through the C ABI.
   - Extend `roastty/tests/abi_harness.c` to compile and exercise the new C
     declarations:
     - construct event/encoder;
     - set Ctrl+C and verify encoded ETX;
     - set Kitty flags and verify one Kitty sequence;
     - assert first/last and section-boundary key constants have the expected C
       values;
     - verify out-of-space length reporting;
     - free all handles.
   - Existing mouse ABI harness coverage must keep passing.

6. Keep scope boundaries hard.
   - Do not wire live input, runtime dispatch, PTY writes, Swift/macOS event
     translation, app/surface methods, keybindings, keymaps, config parsing,
     renderer behavior, browser overlay behavior, or terminal-handle
     `Options::from_terminal`.
   - Do not expose `ghostty_*` names or compatibility aliases.
   - Do not add non-macOS platform behavior.

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
cargo fmt -- roastty/src/lib.rs
cargo test -p roastty key_event
cargo test -p roastty key_encode
cargo test -p roastty key_encoder_abi
cargo test -p roastty
! rg -n "ghostty|Ghostty|ghostty_" roastty/src/input roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c
```

Required evidence:

- `roastty/include/roastty.h` declares the new key event and key encoder ABI
  with Roastty names only.
- Rust exports match the header declarations.
- The C ABI can allocate, mutate, encode, and free key events/encoders.
- Invalid enum/options/null inputs return `ROASTTY_INVALID_VALUE` rather than
  panicking or dereferencing null.
- UTF-8 set through the key event ABI is safe after caller memory changes.
- Encode success and `ROASTTY_OUT_OF_SPACE` length reporting match the mouse
  encoder style.
- At least one legacy and one Kitty encoding path are verified through the C
  ABI.
- Existing key encoder, key event, mouse encoder, terminal, formatter, and ABI
  tests still pass.
- No live input, PTY write, runtime dispatch, config/keybinding/keymap,
  renderer, browser overlay, or terminal-handle behavior is added.
- Codex design review and result review both pass before moving to the next
  stage.

## Non-Negotiable Invariants

- Use Roastty names in public ABI, implementation-facing comments, tests, and
  modules.
- Do not add public `ghostty_*` compatibility names.
- Keep this as a C ABI exposure for the existing pure encoder only.
- Do not wire live input, runtime dispatch, or PTY writes.
- Do not add keybindings, keymaps, config parsing, modifier remap config, or
  `Options::from_terminal`.
- Do not add non-macOS platform behavior.
- Run `cargo fmt` and accept its output.
- Pass Codex design and result reviews before moving to the next stage.

## Failure Criteria

This experiment fails if:

- any public `ghostty_*` or compatibility key ABI names are introduced;
- the key encoder ABI sends bytes to PTY/runtime/app code instead of returning
  them through the caller buffer;
- invalid handles, enum values, options, UTF-8, or output buffers can panic or
  dereference null;
- Rust exported functions accept C enum-like values as Rust enum parameters
  instead of raw integers validated at the FFI boundary;
- the UTF-8 setter stores borrowed caller memory with unsafe lifetime
  assumptions;
- modifier side values, option payload types, or Kitty flag bits are ambiguous
  or accepted without validation;
- `roastty_key_encoder_setopt_from_terminal` is added without a real terminal
  handle;
- live input, PTY process behavior, Swift/app/runtime integration, renderer
  behavior, browser overlay behavior, keybindings, keymap, config remapping, or
  non-macOS platform behavior is added;
- existing key encoder, key event, mouse encoder, terminal, formatter, or ABI
  tests regress;
- the design or result proceeds without the required Codex review gate.

## Codex Design Review

**Result:** Approved after revision.

Codex's first review found real ABI specification gaps: exported functions
needed raw integer FFI boundaries before enum conversion, UTF-8 setter/getter
ownership needed exact signatures and null behavior, key enum stability needed
stronger tests, modifier side fields needed a concrete ABI shape, and encoder
option payload types needed to be listed explicitly.

The design was updated to address those findings. Codex's second review found no
remaining blocking design issues and approved the experiment for implementation.
