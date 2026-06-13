# Experiment 175: Phase C — display-link present driver

## Description

Replace Roastty's current ad hoc continuous present driver with a
display-link-shaped driver that is closer to upstream Ghostty's macOS renderer
loop.

Experiment 19 made the terminal live by spawning one thread per surface,
sleeping for about 16 ms, and dispatching a tick to the main queue. That was
enough to prove the end-to-end live terminal path, but the Phase C roadmap still
has an open render-thread/frame-pacing item because this driver is not tied to
the active display, does not use the `window-vsync` config, and ignores the
display ID that the copied macOS app already forwards through
`roastty_surface_set_display_id`.

Upstream uses a macOS `CVDisplayLink` when vsync is enabled, updates its display
ID when the window changes screens, and only draws from the renderer loop when
the renderer owns vsync. This experiment should move Roastty in that direction
without rewriting the already-working Metal presenter: keep `present_live()`,
`tick_termio()`, dirty gating, drag autoscroll ticking, and main-thread-only
surface dereferencing unchanged, while replacing the sleep-thread scheduler with
a small driver abstraction that can run either from a real display link or the
existing timer fallback.

This experiment does not implement the full upstream renderer mailbox,
occlusion/visibility/focus options, or removal of `render_state_*`. It is a
bounded Phase C driver/fidelity slice.

## Changes

- `roastty/src/lib.rs`
  - Replace `present_driver_running: Option<Arc<AtomicBool>>` with an owned
    present-driver handle that stops on `surface_free` and `Drop`.
  - Move the current tick body into one shared helper, so every driver tick:
    - checks the stop flag before dereferencing the surface;
    - runs on the main queue;
    - calls `tick_termio()`;
    - calls `selection_autoscroll_tick()`;
    - presents only when `dirty`;
    - clears `dirty` only after attempting `present_live()`.
  - Add a macOS display-link-backed scheduler for surfaces whose finalized
    config has `window-vsync = true`.
    - Bind the minimal CoreVideo FFI locally if no existing crate exposes the
      needed APIs.
    - Create the display link with active displays.
    - Set the output callback to enqueue the shared main-queue tick.
    - Start the link after surface registration and stop/release it before the
      surface box is dropped.
    - Keep the callback free of `Surface` dereferences; it may only read the
      atomic stop flag and enqueue the main-queue tick.
  - Keep a fallback scheduler for `window-vsync = false` or display-link
    creation/start failure. The fallback may reuse the existing sleep-thread
    behavior, but it must go through the same owned handle and shared tick path.
  - Store the current display ID from `roastty_surface_set_display_id`; when a
    display-link driver is active and the display ID changes, update the display
    link's current display without rebuilding the renderer.
  - Add focused tests for the driver decision/state machine that do not require
    CoreVideo at runtime:
    - `window-vsync = false` selects the fallback scheduler.
    - `window-vsync = true` attempts the display-link scheduler and falls back
      cleanly when test-injected creation/start fails.
    - `roastty_surface_set_display_id` records the ID and routes an update to an
      active test driver.
    - stopping the handle before drop prevents later queued ticks from
      dereferencing the surface.

- `roastty/Cargo.toml`
  - Add only the minimal dependency needed for display-link callback blocks or
    CoreVideo bindings if local FFI is insufficient. Prefer no new dependency if
    the FFI can stay small and auditable.

- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Link this experiment as `Designed`.
  - After result, update the Phase C render-thread/frame-pacing roadmap item
    only if the display-link path and fallback path are both wired and verified.

## Verification

Before implementation:

- Codex-native adversarial design review approves this experiment.
- Commit the reviewed plan separately from the result.

After implementation:

- `cargo test -p roastty present_driver`
- `cargo test -p roastty content_scale_change_drops_renderer_for_rebuild`
- `cargo test -p roastty app_tick_drains_worker_output_into_surface_dirty_state`
- `cargo test -p roastty --test abi_harness`
- `cargo test -p roastty -- --test-threads=1`
- `cargo fmt -p roastty`
- `cargo fmt --check -p roastty`
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/175-display-link-present-driver.md issues/0802-libroastty-completion-and-mac-app/README.md`
- `git diff --check`

Live verification, when the desktop is available:

- Rebuild and launch the copied Roastty app.
- Confirm a live terminal still updates after shell output and typed input.
- Move the window between displays or trigger the copied app's
  `windowDidChangeScreen` path, and confirm `roastty_surface_set_display_id`
  reaches the driver update path without a crash.
- Quit the app and confirm no dangling Roastty processes remain.

**Pass** = the vsync-enabled path owns a display-link-backed scheduler, the
fallback path remains available, both paths share the same main-thread dirty
pump, display-ID updates reach an active display-link driver, shutdown remains
use-after-free safe, the full Roastty suite passes, and live terminal rendering
still updates.

**Partial** = the owned driver abstraction and fallback behavior land, but the
real display-link path cannot be enabled safely in this experiment. Record the
specific missing API or lifetime blocker.

**Fail** = replacing the scheduler breaks live terminal updates, main-thread
safety, clean shutdown, or the existing dirty-pump behavior.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Huygens`, fresh context.

**Verdict:** Approved.

Findings: None.

## Result

**Result:** Partial

Implemented the display-link-shaped present driver in `roastty/src/lib.rs`.

Key outcomes:

- Replaced the bare `present_driver_running: Option<Arc<AtomicBool>>` surface
  field with an owned `PresentDriver` handle.
- Centralized the present tick so both schedulers share the same main-thread
  behavior: check the stop flag, drain `tick_termio()`, run drag-selection
  autoscroll, present only when `dirty`, and clear `dirty` after the present
  attempt.
- Added a macOS CoreVideo `CVDisplayLink` scheduler for `window-vsync = true`
  surfaces, with local minimal FFI and a callback that only enqueues the
  main-thread tick.
- Preserved the fallback sleep-thread scheduler for `window-vsync = false` and
  display-link creation/start failures.
- Routed `roastty_surface_set_display_id` into the active display-link driver
  through `CVDisplayLinkSetCurrentCGDisplay`.
- Added test-only fake/failing scheduler modes so driver selection, fallback,
  display-ID update routing, and stop-state behavior are covered without
  requiring a live CoreVideo display link in unit tests.

Verification run:

- `cargo test -p roastty present_driver -- --test-threads=1` — pass: 4 passed.
- `cargo test -p roastty content_scale_change_drops_renderer_for_rebuild -- --test-threads=1`
  — pass.
- `cargo test -p roastty app_tick_drains_worker_output_into_surface_dirty_state -- --test-threads=1`
  — pass.
- `cargo test -p roastty --test abi_harness` — pass; existing 10 C
  enum-conversion warnings only.
- `cargo test -p roastty -- --test-threads=1` — pass: 4877 passed, 0 failed, 4
  ignored; ABI harness and doc tests also passed.

Live copied-app verification was not run in this pass, so the experiment remains
Partial rather than Pass. The implemented code compiles and links against
CoreVideo, the fake display-link path proves the state machine, and the fallback
path remains covered, but the real `CVDisplayLink` callback path still needs a
desktop app run to prove live rendering and display-ID changes with an active
display link.

## Conclusion

Roastty now has an owned present-driver abstraction with a real macOS
display-link scheduler for vsync-enabled surfaces and a fallback scheduler for
non-vsync or display-link failure cases. The shared tick preserves the existing
dirty-pump semantics that made the copied app live in Experiment 19, and display
ID changes now reach the active driver.

The remaining work before marking this slice Pass is live desktop validation:
launch the copied Roastty app, confirm terminal updates still render through the
new scheduler, and trigger a display change so the active CoreVideo display link
receives the new display ID without crashing.

## Completion Review

**Reviewer:** Codex-native adversarial review subagent `Singer`, fresh context.

**Verdict:** Approved.

Findings:

- Optional: `present_driver_stop_marks_queued_ticks_inert_before_surface_drop`
  was stronger than the test body, which checked the stopped driver flag rather
  than executing a late queued tick.

Fix:

- Renamed the test to
  `present_driver_stop_marks_driver_not_running_before_surface_drop`, matching
  the behavior it proves. The production stop path already checks the same
  running flag before every surface dereference.
