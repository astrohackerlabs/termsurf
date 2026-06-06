+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5"
reasoning = "medium"
+++

# Experiment 697: Inspector ABI Foundation

## Description

Roastty's C ABI still has no inspector surface, while upstream Ghostty exports a
surface-owned inspector handle plus input and geometry forwarding functions. The
full upstream inspector depends on renderer integration, Metal resources, Dear
ImGui-style UI state, and core inspector data collection. Roastty does not have
those subsystems yet.

This experiment adds the missing inspector ABI shape and a safe state-owning
foundation: a surface can create or reuse one inspector handle, free it through
the surface, and forward size, scale, focus, mouse, key, and text events into
inspector state for later renderer/core-inspector integration.

This does not implement Metal initialization/render/shutdown, inspector UI
rendering, terminal event collection, action keybindings for toggling the
inspector, or Swift/frontend presentation.

## Changes

- `roastty/include/roastty.h`
  - Add `roastty_inspector_t` as an opaque handle.
  - Add Roastty-named equivalents of the non-Metal upstream inspector exports:
    - `roastty_surface_inspector(roastty_surface_t)`;
    - `roastty_inspector_free(roastty_surface_t)`;
    - `roastty_inspector_set_size(roastty_inspector_t, uint32_t, uint32_t)`;
    - `roastty_inspector_set_content_scale(roastty_inspector_t, double, double)`;
    - `roastty_inspector_mouse_button(roastty_inspector_t, roastty_mouse_button_state_e, roastty_mouse_button_e, roastty_input_mods_e)`;
    - `roastty_inspector_mouse_pos(roastty_inspector_t, double, double)`;
    - `roastty_inspector_mouse_scroll(roastty_inspector_t, double, double, roastty_input_scroll_mods_t)`;
    - `roastty_inspector_key(roastty_inspector_t, roastty_key_action_e, roastty_key_e, roastty_input_mods_e)`;
    - `roastty_inspector_text(roastty_inspector_t, const char*)`;
    - `roastty_inspector_set_focus(roastty_inspector_t, bool)`.
  - Do not add Metal-specific inspector exports in this experiment.

- `roastty/src/lib.rs`
  - Add an `Inspector` state struct owned by its surface and stored as an
    optional handle on `Surface`.
  - Make `roastty_surface_inspector` return the existing inspector for repeated
    calls, or create one when the surface is valid and still attached to an app.
  - Make `roastty_inspector_free(surface)` free and detach the surface-owned
    inspector; freeing twice or freeing a null/detached surface is a no-op.
  - Free any live inspector from `roastty_surface_free`.
  - Make inspector input/geometry functions safe no-ops for null inspector
    handles.
  - Keep the same raw-pointer ownership contract as upstream: after
    `roastty_inspector_free(surface)` or `roastty_surface_free(surface)`, any
    previously returned inspector handle is invalid and must not be used.
  - Store the latest inspector size, content scale, focus state, mouse position,
    mouse button event, mouse scroll event, key action/key/mods tuple, and
    sentinel text string. These state fields are deliberately
    internal/test-facing only; they are the future integration points for
    renderer/core-inspector work.
  - Validate inspector forwarded values with the same conservative rules used by
    surface input where practical:
    - invalid mouse button states/buttons are ignored;
    - non-finite mouse positions, scroll deltas, and content scales are ignored
      or sanitized to the existing default scale contract;
    - invalid key actions/keys are ignored;
    - null text is ignored, and non-null text is read as a NUL-terminated C
      string like upstream.

- `roastty/tests/abi_harness.c`
  - Add compile/link smoke coverage for the new inspector prototypes.

- Tests in `roastty/src/lib.rs`
  - Cover lifecycle behavior:
    - null/detached surfaces return null;
    - valid surfaces return a stable inspector handle on repeated calls;
    - freeing clears the surface inspector and a later call creates a new one;
    - surface free releases any live inspector.
  - Cover state forwarding and validation for size, scale, focus, mouse, key,
    and text events.
  - Do not call stale handles after `roastty_inspector_free(surface)` or
    `roastty_surface_free(surface)` because they are outside the raw-pointer ABI
    contract.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty inspector -- --nocapture`
- `cargo test -p roastty surface_free -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex initially blocked the design because the inspector key/text signatures did
not match upstream, the mouse-button prototype named a nonexistent modifier
type, and stale-handle safety was claimed without a raw-pointer-safe ownership
strategy. The design was revised to match upstream's non-Metal inspector ABI
shape with Roastty names, use `roastty_input_mods_e`, and narrow the ownership
contract to null-handle no-ops while treating handles as invalid after
`roastty_inspector_free(surface)` or `roastty_surface_free(surface)`.

Codex then approved the revised design for the plan commit.
