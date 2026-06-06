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

# Experiment 692: Surface Mouse Button Position Dispatch

## Description

Experiment 689 added state-only mouse callback foundations:
`roastty_surface_mouse_button`, `roastty_surface_mouse_pos`,
`roastty_surface_mouse_scroll`, and `roastty_surface_mouse_pressure` validate
inputs and remember the latest frontend mouse state, but button callbacks still
return `false` even when terminal mouse reporting is active.

Upstream Ghostty encodes terminal mouse reports from button and cursor-position
callbacks when the terminal has mouse reporting enabled, queues the encoded
bytes to termio, and returns `true` for consumed button events. Roastty already
has the terminal mouse encoder, terminal mouse-mode tracking, surface pixel
size, content scale, and worker write queue. This experiment wires the first
dispatch slice: button press/release and pointer motion.

This does not implement scroll dispatch, alternate-scroll cursor-key behavior,
selection clearing while reporting, mouse shift-capture policy, or frontend pane
selection side effects. Those remain separate experiments because they involve
scroll normalization, selection ownership, and frontend policy beyond the
button/position report path.

## Changes

- `roastty/src/lib.rs`
  - Add surface mouse dispatch helpers that:
    - derive a `mouse_encode::Geometry` from the current `Surface` size, using
      `width_px` / `height_px` as the screen size, existing `cell_width_px` /
      `cell_height_px` when available, and otherwise a safe nonzero cell size
      derived from the attached worker terminal's current columns/rows or the
      `Surface::pty_size()` fallback;
    - read the attached worker terminal's current `mouse_event` mode and mouse
      format state through `with_termio`;
    - build `mouse_encode::Event` values for button press/release callbacks and
      pointer motion callbacks;
    - for motion callbacks, include the first currently pressed mouse button
      when one exists so Button-mode drag motion can be reported; otherwise use
      `button = None` for buttonless Any-mode motion;
    - pass `any_button_pressed` from `SurfaceMouseState` and keep a per-surface
      last-cell cache for motion deduplication;
    - queue nonempty encoded reports to the termio worker.
  - Extend `SurfaceMouseState` with the dispatch-only state needed by the
    encoder, such as `last_reported_cell`.
  - Change `Surface::mouse_button` so valid attached-surface button events:
    - still update stored button/modifier state;
    - encode and queue a report when terminal mouse reporting accepts the event;
    - return `true` only when a report was queued.
  - Change `Surface::mouse_pos` so valid attached-surface finite positions:
    - update stored position/modifier state;
    - encode and queue a motion report when the terminal's mouse reporting mode
      accepts it;
    - remain a `void` ABI callback.
  - Preserve existing no-op behavior for null surfaces, detached surfaces,
    invalid enum values, nonfinite positions, missing workers, missing terminal
    mouse reporting, and worker write failures except that a worker write error
    is recorded through the existing termio error path.
  - Keep `roastty_surface_mouse_scroll` and `roastty_surface_mouse_pressure`
    state-only in this experiment.
  - Add focused tests for:
    - no report and `false` return when mouse reporting is disabled;
    - press/release reports and `true` return when reporting is enabled;
    - motion reports in Any mode and no motion reports in Normal mode;
    - Button-mode drag motion reports carry the pressed button identity;
    - last-cell motion deduplication;
    - geometry works after the normal `roastty_surface_set_size` path with an
      attached worker even when `columns`, `rows`, `cell_width_px`, and
      `cell_height_px` remain unset on the surface;
    - invalid/detached/no-worker cases remain safe no-ops;
    - worker queue failure records an error without panicking.

- `roastty/tests/abi_harness.c`
  - Keep the existing surface mouse smoke calls compiling against the same ABI.
    No C ABI shape change is expected for this experiment.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty surface_mouse -- --nocapture`
- `cargo test -p roastty mouse -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex approved the revised design after two fixes: motion dispatch now preserves
the first pressed button for Button-mode drag reports, and geometry derivation
no longer depends only on surface grid/cell fields populated by the frontend.
The test plan now covers both cases.

## Result

**Result:** Pass.

Roastty now dispatches surface mouse button press/release callbacks and finite
pointer-position callbacks through the terminal mouse encoder when an attached
worker terminal has mouse reporting enabled. Button callbacks return `true` only
when a report is encoded and queued successfully; disabled reporting,
unsupported events, missing workers, detached surfaces, invalid values, and
worker queue failures remain safe no-ops from the ABI caller's perspective.

The implementation derives encoder geometry from the surface pixel size and
stored cell dimensions when available, with a fallback to the attached terminal
grid or `Surface::pty_size()` defaults. Button-mode drag motion carries the
first currently pressed button, and motion deduplication uses a per-surface
last-reported-cell cache.

Verification passed:

- `cargo fmt -p roastty`
- `cargo test -p roastty surface_mouse -- --nocapture`
- `cargo test -p roastty mouse -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Conclusion

Surface mouse button and pointer-position callbacks now reach terminal mouse
reporting instead of only updating stored frontend state. The next mouse slice
should handle scroll dispatch, including upstream's scroll normalization,
alternate-scroll cursor-key behavior, and the selection side effects that were
left out of this focused experiment.

## Completion Review

Codex reviewed the staged result and found one correctness issue in geometry
fallback ordering. The implementation now prefers the attached terminal grid
when surface cell dimensions are unset and falls back to `Surface::pty_size()`
only after that; a smaller-grid regression covers this case. No other
correctness issues were found.
