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
agent = "codex"
model = "gpt-5"
reasoning = "medium"
+++

# Experiment 689: Surface Mouse Callback Foundation

## Description

Experiment 688 added the surface IME point query. The next remaining frontend
surface boundary is mouse input: upstream exposes
`ghostty_surface_mouse_button`, `ghostty_surface_mouse_pos`,
`ghostty_surface_mouse_scroll`, and `ghostty_surface_mouse_pressure`.

Full upstream mouse behavior is broad. It includes renderer hover state,
hyperlink hover actions, selection gestures, autoscroll timers, click counts,
terminal mouse report encoding, `mouse-shift-capture`, pending scroll
accumulation, and inspector routing. Roastty already has standalone mouse
event/encoder ABI support and a `surface_mouse_captured` query, but it does not
yet have the live renderer/selection/input dispatch machinery needed to port
those callbacks faithfully.

This experiment adds the C ABI entry points and a small surface-owned mouse
state foundation. The functions become safe for frontends to call and keep
enough state for later dispatch experiments, but they intentionally do not yet
write encoded mouse reports to the PTY, update selections, update hyperlink
hover state, perform inspector routing, or schedule autoscroll.

## Changes

- `roastty/include/roastty.h`
  - Add `typedef int roastty_input_scroll_mods_t;` to preserve upstream's C ABI
    shape while still allowing the implementation to truncate to the low 8 bits
    internally.
  - Add `roastty_mouse_button_state_e` with upstream-compatible button-state
    values:
    - `ROASTTY_MOUSE_BUTTON_RELEASE = 0`
    - `ROASTTY_MOUSE_BUTTON_PRESS = 1`
  - Add public surface mouse callback functions next to
    `roastty_surface_mouse_captured`:
    - `ROASTTY_API bool roastty_surface_mouse_button(roastty_surface_t, roastty_mouse_button_state_e, roastty_mouse_button_e, roastty_input_mods_e);`
    - `ROASTTY_API void roastty_surface_mouse_pos(roastty_surface_t, double, double, roastty_input_mods_e);`
    - `ROASTTY_API void roastty_surface_mouse_scroll(roastty_surface_t, double, double, roastty_input_scroll_mods_t);`
    - `ROASTTY_API void roastty_surface_mouse_pressure(roastty_surface_t, uint32_t, double);`
- `roastty/src/lib.rs`
  - Add a `SurfaceMouseState` stored on `Surface`:
    - latest cursor position as `Option<(f64, f64)>`;
    - latest input modifier state using the `roastty_input_mods_e` conversion
      from Experiment 687;
    - per-button press/release state for buttons `0..=11`;
    - latest scroll offsets and raw scroll modifier byte;
    - latest pressure stage and pressure.
  - Implement `roastty_surface_mouse_pos`:
    - null and detached surfaces are no-ops;
    - finite `x`/`y` positions are stored, including negative values for
      outside-viewport state;
    - NaN or infinite positions clear the stored position;
    - known input modifier bits are stored and unknown bits are dropped.
  - Implement `roastty_surface_mouse_button`:
    - null and detached surfaces return `false`;
    - invalid button-state or button values return `false` and leave state
      unchanged;
    - valid calls update button state and latest modifiers;
    - the return value is always `false` until full terminal mouse-report
      dispatch and selection routing exist, so the public ABI does not claim an
      event was consumed when Roastty only stored state.
  - Implement `roastty_surface_mouse_scroll`:
    - null and detached surfaces are no-ops;
    - finite offsets and the low 8 bits of `scroll_mods` are stored;
    - NaN or infinite offsets are ignored.
  - Implement `roastty_surface_mouse_pressure`:
    - null and detached surfaces are no-ops;
    - stages `0..=2` are accepted and stored with finite pressure;
    - invalid stages or non-finite pressure values leave state unchanged.
- `roastty/tests/abi_harness.c`
  - Assert the new button-state constants.
  - Exercise null and live surface mouse callback calls through `roastty.h`.
- Tests
  - Constant values match upstream button-state layout.
  - Null and detached surfaces are safe no-ops.
  - Mouse position stores finite coordinates and drops unknown modifier bits.
  - Non-finite mouse positions clear stored position.
  - Mouse button validates state/button values, updates button/modifier state,
    and returns `false` even when terminal mouse capture is active.
  - Mouse scroll stores finite offsets and truncates named scroll mods to 8
    bits.
  - Mouse pressure validates stage and finite pressure.

## Verification

- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/689-surface-mouse-callback-foundation.md`
- `cargo fmt -p roastty`
- `cargo test -p roastty surface_mouse`
- `cargo test -p roastty mouse`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

**Result:** Approved after design fixes.

Codex initially blocked the design on two ABI/contract issues. First, returning
`roastty_surface_mouse_captured(surface)` from `roastty_surface_mouse_button`
would falsely claim the event was consumed even though this experiment does not
dispatch encoded mouse reports or update selection state. The design now returns
`false` until full dispatch exists.

Second, Codex asked for the scroll modifier argument to preserve upstream's C
shape. The design now adds `typedef int roastty_input_scroll_mods_t` and uses
that type in `roastty_surface_mouse_scroll`, while the implementation stores the
low 8 bits.

Codex approved the revised state-only callback foundation as an appropriately
incremental step before renderer hover, selection routing, terminal report
encoding, autoscroll, and inspector routing.

## Result

**Result:** Pass.

Roastty now exposes the four surface mouse callback entry points in the public C
ABI: `roastty_surface_mouse_button`, `roastty_surface_mouse_pos`,
`roastty_surface_mouse_scroll`, and `roastty_surface_mouse_pressure`. It also
adds the upstream-shaped `roastty_input_scroll_mods_t` typedef and
`roastty_mouse_button_state_e` constants.

The implementation stores a small `SurfaceMouseState` on each surface: latest
finite pointer position, latest known modifier bits, per-button press/release
state, latest finite scroll offsets plus low-8-bit scroll mods, and latest valid
pressure stage/pressure. Null and detached surfaces are safe no-ops. Invalid
button states/buttons, non-finite scroll offsets, invalid pressure stages, and
non-finite pressures leave state unchanged. `roastty_surface_mouse_button`
returns `false` even when terminal mouse capture is active, because this slice
does not yet dispatch encoded mouse reports or update selection state.

Verification passed:

- `cargo fmt -p roastty`
- `cargo test -p roastty surface_mouse -- --nocapture`
- `cargo test -p roastty mouse -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Conclusion

The surface mouse callback ABI is now present and safe for frontends to call,
with enough stored state for later dispatch work. Full upstream behavior remains
future work: renderer hover/link updates, selection gestures, autoscroll,
terminal mouse report encoding and PTY writes, `mouse-shift-capture`, and
inspector routing.

## Completion Review

**Result:** Approved after provenance update.

Codex found no code blockers. It confirmed the ABI uses
`typedef int roastty_input_scroll_mods_t`, `roastty_surface_mouse_scroll` uses
that named type, scroll state stores the low 8 bits, unknown key modifier bits
are dropped through `key_mods_from_raw`, null and detached surfaces are no-ops,
and `roastty_surface_mouse_button` returns `false` even when capture is active.
The tests cover the important state-only contract points.

Codex initially blocked the result commit only because result-review provenance,
this completion-review section, and the final README agent tuple were not
recorded yet. Those workflow records are now present.
