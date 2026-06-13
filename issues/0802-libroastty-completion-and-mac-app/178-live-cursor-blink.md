# Experiment 178: Phase C — live cursor blink parity

## Description

Finish the cursor-blink half of Phase C's render-thread item.

Experiment 175 added a continuous present driver and Experiment 177 proved the
copied `Roastty.app` selects the real CoreVideo display-link driver and renders
the smoke marker. The remaining note on the Phase C render-thread checklist is
cursor-blink timer parity. The current live draw path does not yet carry
Ghostty's renderer-thread cursor blink state into frame rendering: upstream
`renderer/Thread.zig` owns a 600 ms cursor timer, toggles
`cursor_blink_visible`, wakes the renderer, resets the cursor visible state on
`reset_cursor_blink`, and passes that state into `renderer.updateFrame(...)`.
Roastty's `renderer::cursor::style` already contains the faithful blink-visible
predicate, but `FrameRenderState::from_terminal` currently derives the cursor
solely from terminal visibility/style and the live path calls
`render_and_present_frame...` without a blink-visible input. A blinking cursor
therefore cannot be hidden by the live timer.

This experiment should make cursor blink visibility an explicit renderer input
and add live-surface timer/reset state to the present-driver path, without
claiming the broader renderer mailbox or interim `render_state` retirement
items.

## Changes

- `roastty/src/renderer/frame_renderer.rs`
  - Add a render-input path that takes `cursor_blink_visible` and focused state
    and feeds them through `renderer::cursor::style`.
  - Keep existing convenience render paths defaulting to a visible focused
    cursor unless a caller supplies dynamic blink/focus options.
  - Add focused unit tests proving a focused blinking cursor is omitted when
    `cursor_blink_visible = false`, shown when `true`, and shown as a hollow
    block when the surface is unfocused.

- `roastty/src/lib.rs`
  - Add live-surface cursor blink state matching upstream's shape:
    `cursor_blink_visible`, a 600 ms interval, the next deadline, and a
    `last_cursor_reset` timestamp for upstream's 500 ms heavy-output reset
    throttle.
  - On present-driver ticks, toggle blink visibility when the focused surface's
    deadline expires, mark the surface dirty, and present the frame even if no
    PTY bytes arrived.
  - Reset the blink to visible and push the deadline forward on terminal-output
    pump events that read bytes, but only when at least 500 ms elapsed since the
    previous reset. This matches upstream `Termio.processOutputLocked`, which
    sends `reset_cursor_blink` on output and rate-limits it under heavy read
    load.
  - On focus loss, stop blink toggling while keeping unfocused rendering on the
    hollow-block path. On focus gain, immediately show the cursor, push the next
    deadline forward, mark the surface dirty, and let the present driver resume
    toggling.
  - Add focused unit tests around the surface blink state using the existing
    present-driver/test-surface helpers where possible, including heavy-output
    reset throttling and focus loss/gain lifecycle.

- `scripts/roastty-app/live-ab-smoke.sh` or its recipe support
  - Add a bounded cursor recipe only if it can produce a reliable machine oracle
    in this experiment. The recipe should hold a simple visible prompt with a
    blinking cursor long enough to capture both cursor-visible and cursor-hidden
    phases, and should not depend on committed screenshots.
  - If a reliable live oracle is not feasible in this slice, record why and keep
    the pass criteria on deterministic Rust tests plus the existing live smoke
    sanity check.

- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Link this experiment as `Designed`.
  - After the run, mark it `Pass`, `Partial`, or `Fail`.
  - Check the Phase C render-thread item only if both display-link startup
    rendering and cursor-blink timer behavior are proven.

- `issues/0802-libroastty-completion-and-mac-app/178-live-cursor-blink.md`
  - Record implementation details, verification output, live artifact paths if
    used, result, conclusion, and AI completion review.

## Verification

Before implementation:

- Codex-native adversarial design review approves this experiment.
- Commit the reviewed plan separately from the result.

Focused tests:

- `cargo test -p roastty cursor_blink -- --test-threads=1`
- `cargo test -p roastty live_cursor_blink -- --test-threads=1`

Regression checks:

- `cargo test -p roastty --test abi_harness`
- `cargo test -p roastty -- --test-threads=1`
- `cargo fmt --check -p roastty`
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/178-live-cursor-blink.md issues/0802-libroastty-completion-and-mac-app/README.md`
- `git diff --check`

Live sanity:

- Rebuild the copied app:

  ```bash
  cd roastty && macos/build.nu --action build
  ```

- Re-run the known-good smoke proof so the cursor change does not regress the
  copied app startup/render path:

  ```bash
  scripts/roastty-app/stop-app.sh
  TERMSURF_AB_HOLD_SECONDS=10 \
  ROASTTY_PRESENT_DRIVER_LOG=1 \
    scripts/roastty-app/live-ab-smoke.sh \
      --recipe smoke \
      --comparison-region content \
      --max-mismatch-ratio 1 \
      --max-mean-channel-delta 255
  ```

- If a cursor-specific live recipe is added, run it with a machine oracle that
  proves the cursor changes visibility across captures and record the screenshot
  paths outside the repo.

**Pass** = focused tests prove blink-visible renderer input and live-surface
timer/reset behavior, full regression checks pass, the copied app rebuilds, the
live smoke marker still renders with
`present-driver=display-link reason=core-video`, and the roadmap render-thread
item can be checked because both frame pacing and cursor-blink timer behavior
are proven.

**Partial** = deterministic blink behavior is implemented but the copied app
live proof is unavailable or too weak to check the roadmap item, or the live
recipe cannot prove both blink phases. Record the exact blocker.

**Fail** = the change breaks rendering, app startup, display-link presentation,
cursor style semantics, or the Rust/ABI test gates.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Lorentz`, fresh context.

**Initial verdict:** Changes required.

Findings and fixes:

- Required: the design reset the cursor blink whenever terminal input/output
  made the surface dirty, but upstream `Termio.processOutputLocked` sends
  `reset_cursor_blink` only for terminal output and throttles resets to once per
  500 ms under heavy read load. Fixed by adding `last_cursor_reset`, the 500 ms
  throttle, terminal-output-only reset wording, and required tests proving
  frequent output does not continuously push the blink deadline forward.
- Required: the design underspecified focus transitions. Upstream cancels the
  cursor timer on focus loss and, on focus gain, immediately sets
  `cursor_blink_visible = true` before restarting the timer. Fixed by requiring
  focus-loss/focus-gain behavior and tests: focus loss disables toggling and
  renders hollow, while focus gain immediately shows the cursor, moves the
  deadline forward, marks the surface dirty, and resumes toggling.

**Final verdict:** Approved.

Final findings: None. The reviewer confirmed the corrected design covers the 500
ms reset throttle and focus loss/gain lifecycle without introducing new required
issues.
