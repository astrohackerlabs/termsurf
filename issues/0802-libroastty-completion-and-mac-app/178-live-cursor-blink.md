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

## Result

**Result:** Pass.

Implemented live cursor blink parity across the renderer input and live present
driver:

- `FrameRenderState` now has a `FrameCursorOptions` input carrying focused state
  and `cursor_blink_visible`.
- Live Metal presentation calls the cursor-options render paths, so focused
  blinking cursors disappear when the blink state is hidden, while unfocused
  surfaces render the upstream hollow block.
- `Surface` now owns live blink state: `cursor_blink_visible`, a 600 ms next
  deadline, and `last_cursor_reset` for upstream's 500 ms heavy-output reset
  throttle.
- Present-driver ticks advance the blink deadline and mark the live surface
  dirty even when no PTY bytes arrive.
- Terminal-output pump events reset the cursor to visible, but only when at
  least 500 ms elapsed since the prior output reset.
- Focus loss stops blink toggling; focus gain immediately shows the cursor,
  pushes the deadline forward, and marks only live `NSView` surfaces dirty. This
  preserves the C ABI expectation that ABI-only surfaces do not become dirty
  just because focus metadata changed.

Focused verification:

- `cargo test -p roastty cursor_blink -- --test-threads=1` — **Pass**, 8 tests
  passed.
- `cargo test -p roastty live_cursor_blink -- --test-threads=1` — **Pass**, 4
  tests passed.

Regression verification:

- `cargo test -p roastty --test abi_harness` initially failed because the first
  focus-change implementation marked ABI-only surfaces dirty. Fixed by making
  focus repaint requests conditional on a live `NSView`, while still updating
  blink state. Rerun: **Pass**, 1 test passed; the existing enum-conversion
  warnings and `[unknown](scope): message` remained.
- `cargo test -p roastty -- --test-threads=1` — **Pass**, 4890 passed, 0 failed,
  4 ignored; ABI harness and doc-tests also passed.
- `cargo fmt --check -p roastty` — **Pass**.
- `git diff --check` — **Pass**.

Live sanity:

- `cd roastty && macos/build.nu --action build` — **Pass**. The copied app build
  completed with `** BUILD SUCCEEDED **`; only the existing Swift actor,
  retroactive Sendable, linker deployment-target, and terminfo warnings
  appeared.
- `scripts/roastty-app/stop-app.sh && TERMSURF_AB_HOLD_SECONDS=10 ROASTTY_PRESENT_DRIVER_LOG=1 scripts/roastty-app/live-ab-smoke.sh --recipe smoke --comparison-region content --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  — **Pass**. The harness launched Ghostty PID `57177` and Roastty PID `57185`
  with marker `ISSUE802_AB_SMOKE_20260613-015502` and returned JSON verdict
  `PASS`.
- Content-region diff metrics for that run:

  ```text
  mismatch_ratio=0.005620833333333334
  mean_channel_delta=0.5585425347222223
  ```

- `grep -n 'present-driver' /Users/ryan/.cache/termsurf/shots/roastty-ab-stderr-20260613-015502.log`
  returned `1:[roastty] present-driver=display-link reason=core-video`.
- The explicit cleanup check printed `no debug Roastty app PID remains`.
- Captured artifacts:
  - `/Users/ryan/.cache/termsurf/shots/ghostty-ab-content-20260613-015502.png`
  - `/Users/ryan/.cache/termsurf/shots/ghostty-ab-crop-20260613-015502.png`
  - `/Users/ryan/.cache/termsurf/shots/ghostty-ab-full-20260613-015502.png`
  - `/Users/ryan/.cache/termsurf/shots/ghostty-ab-stderr-20260613-015502.log`
  - `/Users/ryan/.cache/termsurf/shots/ghostty-ab-stdout-20260613-015502.log`
  - `/Users/ryan/.cache/termsurf/shots/roastty-ab-content-20260613-015502.png`
  - `/Users/ryan/.cache/termsurf/shots/roastty-ab-crop-20260613-015502.png`
  - `/Users/ryan/.cache/termsurf/shots/roastty-ab-full-20260613-015502.png`
  - `/Users/ryan/.cache/termsurf/shots/roastty-ab-stderr-20260613-015502.log`
  - `/Users/ryan/.cache/termsurf/shots/roastty-ab-stdout-20260613-015502.log`

No cursor-specific live recipe was added in this slice. The deterministic Rust
tests are the stronger oracle for the 600 ms timer, 500 ms output-reset
throttle, focus lifecycle, and blink-hidden renderer input; the live smoke run
serves as the copied-app startup/display-link regression proof.

## Conclusion

The Phase C render-thread item is now complete: Experiment 177 proved the copied
app uses the CoreVideo display-link present driver and renders startup output,
and this experiment proves the cursor blink timer and reset lifecycle that ride
on that driver.

The broader Phase C items remain open: renderer mailbox/options propagation,
retiring the interim `render_state` pull divergence, and the final
working-ASCII-terminal milestone still need separate experiments.

## Completion Review

**Reviewer:** Codex-native adversarial review subagent `James`, fresh context.

**Verdict:** Approved.

Findings: None.

Independent checks performed by the reviewer:

- `git log -1 --oneline` confirmed the result commit had not been made yet; HEAD
  was still the plan commit `140b71d54d564 Plan live cursor blink parity`.
- `git status --short` showed only the four expected modified files.
- `cargo test -p roastty cursor_blink -- --test-threads=1` — **Pass**, 8 tests
  passed.
- `cargo test -p roastty live_cursor_blink -- --test-threads=1` — **Pass**, 4
  tests passed.
- `cargo test -p roastty --test abi_harness` — **Pass**, 1 test passed with the
  existing warnings.
- `cargo test -p roastty -- --test-threads=1` — **Pass**, 4890 passed, 0 failed,
  4 ignored; ABI harness and doc-tests passed.
- `cargo fmt --check -p roastty` — **Pass**.
- `prettier --check ...` — **Pass**.
- `git diff --check ...` — **Pass**.
- Verified the live artifact log contains
  `present-driver=display-link reason=core-video`.

The reviewer did not re-run the macOS app build or live smoke harness; they
verified the recorded smoke artifacts and log marker where feasible.
