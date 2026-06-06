+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5"
reasoning = "medium"

[review.result]
agent = "pending"
model = "pending"
reasoning = "pending"
+++

# Experiment 710: Binding Action Scroll Fractional

## Description

Experiment 709 added `scroll_page_lines:<i16>` binding-action support by
applying signed row-delta viewport movement. Upstream Ghostty's
`performBindingAction` also supports `scroll_page_fractional:<f32>`, which
multiplies the visible grid height by the parsed fraction, truncates the result,
and applies that signed row count to the viewport:

- positive values scroll downwards;
- negative values scroll upwards;
- `+N` and exponent syntax are accepted by Zig's decimal float parser;
- empty, whitespace, malformed, non-finite, or out-of-range values are invalid
  for Roastty;
- finite fractions that truncate to zero return `true` without moving the
  viewport;
- the action returns `true` when performed on an attached surface.

Upstream's `std.fmt.parseFloat(f32, value)` accepts non-finite values such as
`nan` and `inf`, but upstream then converts the truncated product to `isize`.
That conversion is not a useful terminal-scroll value; a local Zig check showed
`inf` traps while `nan` converts to zero. This experiment therefore keeps
Roastty's binding-action ABI total and rejects non-finite values at parse time
instead of exposing trap-like behavior.

Roastty already has the row-delta helper used by Experiments 708 and 709. This
experiment adds only finite float parsing, row-count truncation, and the
`scroll_page_fractional:<f32>` binding-action path.

This does not implement `clear_screen`, `scroll_to_row`, `scroll_to_selection`,
prompt jumps, search actions, clipboard actions, cursor-key actions, full
keybind storage/lookup, or app-scoped actions.

## Changes

- `roastty/src/lib.rs`
  - Add a small ASCII finite `f32` parser for this action, using Rust's float
    parser after validating that the bytes are UTF-8 without whitespace and
    rejecting non-finite results.
  - Extend the internal parsed binding-action enum with
    `ScrollPageFractional(f32)`.
  - Extend `parse_binding_action` to accept `scroll_page_fractional:<f32>` and
    reject missing, empty, malformed, whitespace, extra-colon, non-finite, and
    out-of-range parameters.
  - Add/use a surface helper that locks the active termio worker, multiplies
    `surface.size.rows` by the parsed fraction, truncates toward zero, applies
    the resulting signed row delta to the terminal viewport, and requests a
    render.
  - Treat zero surface rows and fractions that truncate to zero as consumed
    no-ops.
  - Return `true` for attached parsed fractional-scroll actions, even when no
    termio worker exists, matching action-consumed semantics.
  - Return `false` for null or detached surfaces.
  - Keep split, close, `text:`, `csi:`, `esc:`, `reset`, top/bottom scroll, page
    up/down, and line-scroll semantics unchanged.

- `roastty/tests/abi_harness.c`
  - Add C ABI smoke coverage that malformed/non-finite fractional-scroll forms
    are rejected and representative negative, positive, explicit-plus, exponent,
    and zero forms can be invoked.

- Tests in `roastty/src/lib.rs`
  - Cover invalid forms returning false: missing parameter, empty parameter,
    whitespace, malformed bytes, extra colon, non-finite values, and values that
    overflow the eventual integer delta.
  - Cover null and detached surfaces returning false.
  - Cover attached no-worker surfaces returning true without side effects.
  - Cover worker-backed `scroll_page_fractional:-0.5` moving the viewport up by
    `trunc(0.5 * rows)` rows.
  - Cover worker-backed `scroll_page_fractional:+0.5` and exponent/unsigned
    positive forms moving the viewport down by the truncated row count.
  - Cover fractional values that truncate to zero returning true without moving
    the viewport.
  - Cover zero-row attached worker-backed surfaces returning true without moving
    the viewport.
  - Re-run existing binding-action tests to prove previous action semantics did
    not change.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty binding_action -- --nocapture`
- `cargo test -p roastty scroll_page_fractional -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the Experiment 710 design and approved it technically. The review
confirmed that `scroll_page_fractional:<f32>` is a coherent continuation of the
row-delta viewport binding-action work and does not pull in row, selection,
prompt, or clear-screen behavior.

The review also approved rejecting non-finite floats as a documented,
safety-preserving divergence from upstream's trap-prone `inf` behavior while
keeping finite upstream-compatible parsing and truncation semantics. The planned
implementation path was accepted: parse finite ASCII/UTF-8 `f32`, multiply by
`surface.size.rows`, truncate toward zero, reject integer-delta overflow before
casting, and route through `Terminal::scroll_selection_gesture_viewport(delta)`.

The proposed tests were accepted as sufficient for finite sign/exponent forms,
malformed/whitespace/extra-colon/missing/empty/non-finite/overflow rejection,
exact positive/negative truncated movement, zero-truncation no-op, zero-row
no-op, null/detached, no-worker, ABI smoke coverage, and prior-action regression
coverage.

The only required fix before plan commit was workflow provenance: replacing the
pending design-review metadata, adding this design-review section, and updating
the README provenance tuple to `Codex/Codex/-`.
