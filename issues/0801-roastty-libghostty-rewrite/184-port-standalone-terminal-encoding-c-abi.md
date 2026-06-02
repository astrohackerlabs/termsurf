# Experiment 184: Port Standalone Terminal Encoding C ABI

## Description

Experiment 183 completed the terminal formatter C ABI. The next remaining
upstream C ABI gap that can be solved without new app/runtime integration is the
small group of standalone terminal encoding helpers:

- `terminal/c/focus.zig::encode`
- `terminal/c/paste.zig::is_safe`
- `terminal/c/paste.zig::encode`
- `terminal/c/modes.zig::report_encode`

These helpers all expose deterministic byte encoders with the same caller-buffer
measurement pattern used by the formatter and size-report ABIs. Grouping them is
appropriate because they share the same implementation surface: small internal
Rust modules plus C functions that validate inputs, write required lengths, and
return `ROASTTY_OUT_OF_SPACE` for measuring or short buffers.

This experiment must not add app focus behavior, paste dispatch, clipboard
integration, terminal input delivery, or new terminal mode mutation. It only
ports the standalone helpers and their C ABI.

## Changes

1. Add the private internal focus encoder.
   - Create `roastty/src/terminal/focus.rs` from
     `vendor/ghostty/src/terminal/focus.zig`.
   - Expose `FocusEvent::{Gained, Lost}` and `encode(event) -> &'static [u8]` or
     equivalent.
   - Preserve upstream bytes exactly: gained = `ESC [ I`, lost = `ESC [ O`.
   - Add Rust unit tests for gained/lost and max encoded length.

2. Add the private internal paste encoder.
   - Create `roastty/src/input/paste.rs` from
     `vendor/ghostty/src/input/paste.zig`.
   - Add `PasteOptions { bracketed: bool }`.
   - Add `is_safe(data: &[u8]) -> bool` matching upstream: unsafe if it contains
     newline or `ESC [ 201 ~`.
   - Add `encode(data: &mut [u8], options) -> [&[u8]; 3]` or equivalent internal
     helpers.
   - Preserve upstream mutation behavior for the C ABI path:
     - always replace xterm strip bytes with spaces;
     - when unbracketed, replace `\n` with `\r`;
     - when bracketed, wrap with `ESC [ 200 ~` and `ESC [ 201 ~`.
   - The C path must mutate the caller's non-null input buffer in place for
     every valid call before checking output capacity. This includes measuring
     calls and short-output-buffer calls that return `ROASTTY_OUT_OF_SPACE`.
   - Add Rust unit tests ported from upstream for safe checks, bracketed paste,
     unbracketed paste, newline conversion, stripped unsafe bytes, empty input,
     and null-equivalent empty behavior through the C API.

3. Expose mode report encoding to the C ABI.
   - Reuse existing `roastty/src/terminal/modes.rs` `ModeTag`, `Report`, and
     `ReportState` behavior.
   - Add only the visibility or helper method needed by `roastty/src/lib.rs`; do
     not change terminal runtime mode state.
   - Add `roastty_mode_report_state_e` to `roastty/include/roastty.h` with
     upstream discriminants: not recognized = 0, set = 1, reset = 2, permanently
     set = 3, permanently reset = 4.
   - Add
     `roastty_mode_report_encode(roastty_mode_tag_t, roastty_mode_report_state_e, uint8_t*, size_t, size_t*)`.
   - Decode the raw `roastty_mode_tag_t` with the existing
     `ROASTTY_MODE_TAG_ANSI_BIT` and `ROASTTY_MODE_TAG_VALUE_MASK` helper before
     constructing the internal `ModeTag`.
   - Accept unknown decoded mode values and encode them as upstream does. Reject
     only an invalid report state.

4. Add focus and paste C ABI functions to `roastty/include/roastty.h` and
   `roastty/src/lib.rs`:
   - `roastty_focus_event_e` with gained = 0, lost = 1.
   - `roastty_focus_encode(roastty_focus_event_e, uint8_t*, size_t, size_t*)`.
   - `roastty_paste_is_safe(const uint8_t*, size_t)`.
   - `roastty_paste_encode(uint8_t*, size_t, bool, uint8_t*, size_t, size_t*)`.
   - `roastty_focus_encode` returns `ROASTTY_INVALID_VALUE` for an invalid focus
     event and writes `0` to `out_written` before returning when `out_written`
     is non-null.
   - `roastty_mode_report_encode` returns `ROASTTY_INVALID_VALUE` for an invalid
     mode report state and writes `0` to `out_written` before returning when
     `out_written` is non-null. Unknown mode tags are not invalid.

5. Use one shared buffer-writing helper for these new byte encoders if it keeps
   the implementation simpler without changing existing APIs.
   - `out_written == null` returns `ROASTTY_INVALID_VALUE`.
   - `out == null` is a valid measuring call.
   - Non-empty output with null or short output buffer writes the required byte
     length and returns `ROASTTY_OUT_OF_SPACE`.
   - Empty output with null output buffer and length 0 returns
     `ROASTTY_SUCCESS`.
   - `out == null && out_len > 0` is not invalid for these new functions; this
     matches the upstream fixed-writer measurement convention and Experiment
     183's corrected formatter behavior.

6. Define input-pointer validation explicitly.
   - `roastty_paste_is_safe(null, 0)` returns `true`.
   - `roastty_paste_is_safe(null, len > 0)` also returns `true`, matching
     upstream's null-as-empty C ABI behavior.
   - `roastty_paste_encode(null, 0, ...)` encodes an empty paste.
   - `roastty_paste_encode(null, len > 0, ...)` also encodes an empty paste,
     matching upstream's null-as-empty C ABI behavior. The implementation must
     never construct a slice from a null pointer with nonzero length.

7. Update `roastty/tests/abi_harness.c` to compile and exercise the new
   enums/functions from C.

## Verification

Run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/input/mod.rs roastty/src/input/paste.rs roastty/src/terminal/mod.rs roastty/src/terminal/focus.rs roastty/src/terminal/modes.rs
cargo test -p roastty focus
cargo test -p roastty paste
cargo test -p roastty modes_report
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

The experiment passes when C callers can encode focus events, inspect and encode
paste payloads, and encode mode reports with upstream-compatible bytes and
buffer-measurement behavior.

Required edge-case coverage:

- invalid focus event returns `ROASTTY_INVALID_VALUE` and writes required length
  `0`;
- invalid mode report state returns `ROASTTY_INVALID_VALUE` and writes required
  length `0`;
- DEC, ANSI, unknown DEC, and max-value raw packed mode tags encode correctly;
- null output with nonzero output length follows measuring semantics;
- empty paste encodes correctly in bracketed and unbracketed modes;
- paste input mutation happens on successful, measuring, and short-output calls;
- the paste strip-byte tests include ESC plus representative non-ESC control
  bytes from upstream's xterm strip set.

## Non-Negotiable Invariants

- Do not expose any `ghostty_*` ABI names.
- Do not add app focus dispatch or terminal focus state changes.
- Do not add clipboard integration or paste delivery into a PTY.
- Do not change existing terminal mode mutation or query behavior.
- Do not change existing size-report, formatter, key, mouse, selection, or
  render-state APIs except for sharing a local byte-buffer helper if it is
  purely mechanical.
- Do not skip Codex result review. If the result review finds a real gap, fix it
  and re-review before recording the result.
