# Experiment 179: Phase C — renderer options propagation

## Description

Bring Roastty's live render surface closer to Ghostty's renderer-thread mailbox
semantics for the remaining Phase C `Options` item: focus, visibility/occlusion,
and config-change propagation.

Upstream sends renderer-thread messages for `.focus`, `.visible`, and
`.change_config`. The renderer thread uses them to update QoS/display-link
state, restart or stop focus-sensitive timers, redraw immediately when a surface
becomes visible again, mark custom shader focus changes, and apply derived
renderer configuration before future frames. Roastty does not have a separate
renderer thread yet, but it now has a continuous present driver and an
in-process live renderer. The equivalent state changes therefore need to land on
`Surface` and `SurfaceLiveRenderer` directly.

There is also an ABI semantic mismatch to fix before richer option propagation
is trustworthy: Ghostty's `ghostty_surface_set_occlusion(surface, visible)`
takes a `visible` boolean, and the copied Swift caller passes
`window.occlusionState.contains(.visible)`. Roastty's Rust implementation names
that parameter `occluded` and stores it directly. That means the live surface
can record a visible window as occluded and an invisible window as visible. This
experiment should make the Rust side match the upstream/caller contract without
changing the public symbol.

This is a scoped Phase C slice. It should not retire the interim
`render_state_update` pull path, redesign presentation around a real Rust
renderer thread, or check the final ASCII-terminal milestone.

## Changes

- `roastty/src/lib.rs`
  - Rename the `Surface` visibility field and helper logic so the stored state
    is `visible` rather than `occluded`, matching upstream's ABI semantics and
    the copied Swift caller.
  - Make `roastty_surface_set_occlusion(surface, visible)` treat its bool as
    visibility: unchanged values are no-ops; becoming visible marks live
    `NSView` surfaces dirty, wakes the app, and allows the next present-driver
    tick to rebuild/present immediately; becoming invisible suppresses live
    presentation work.
  - Gate `present_live` and present-driver frame submission on visibility for
    live `NSView` surfaces. Timers may keep firing, matching upstream's
    low-cost-timer choice, but invisible surfaces should not rebuild cells or
    submit Metal frames until visible again.
  - Keep focus behavior from Experiment 178, but route it through a small
    renderer-options helper so focus changes update cursor blink state,
    custom-shader focus-change state, dirty state, and app wakeup in one place.
  - Extend config-change propagation so `roastty_surface_update_config` not only
    updates terminal/config fields but also marks the live renderer's
    renderer-derived state dirty. At minimum, custom shader config must resync
    on the next live frame, the live frame must be requested, and any renderer
    state that cannot be safely updated in place must be rebuilt explicitly.
  - Add focused unit tests for:
    - the occlusion ABI bool being interpreted as `visible`, not `occluded`;
    - invisible live surfaces not submitting/rebuilding live frames;
    - becoming visible marking a live surface dirty and preserving ABI-only
      no-op behavior for surfaces without an `NSView`;
    - config updates requesting a live frame or renderer rebuild when live
      renderer-derived state is affected;
    - focus changes still mark custom shader focus changes and cursor blink
      state without regressing the ABI-only focus behavior fixed in
      Experiment 178.

- `roastty/macos/Sources/Features/Terminal/BaseTerminalController.swift`
  - No behavior change should be needed if the Rust ABI is corrected. Only edit
    comments or wrapper naming if the implementation reveals misleading local
    wording that would make the visible/occluded polarity unclear.

- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Link this experiment as `Designed`.
  - After the run, mark it `Pass`, `Partial`, or `Fail`.
  - Check the Phase C renderer mailbox / `Options` item only if focus,
    visibility/occlusion, and config-change propagation are all implemented and
    verified by deterministic tests plus live smoke.

- `issues/0802-libroastty-completion-and-mac-app/179-renderer-options-propagation.md`
  - Record implementation details, verification output, live artifact paths,
    result, conclusion, and AI completion review.

## Verification

Before implementation:

- Codex-native adversarial design review approves this experiment.
- Commit the reviewed plan separately from the result.

Focused tests:

- `cargo test -p roastty live_renderer_options -- --test-threads=1`
- `cargo test -p roastty live_cursor_blink -- --test-threads=1`

Regression checks:

- `cargo test -p roastty --test abi_harness`
- `cargo test -p roastty -- --test-threads=1`
- `cargo fmt --check -p roastty`
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/179-renderer-options-propagation.md issues/0802-libroastty-completion-and-mac-app/README.md`
- `git diff --check`

Live sanity:

- Rebuild the copied app:

  ```bash
  cd roastty && macos/build.nu --action build
  ```

- Re-run the known-good smoke proof so option propagation does not regress the
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

**Pass** = the occlusion ABI polarity matches upstream/caller semantics, live
visibility suppresses presentation while invisible and requests an immediate
frame when visible again, focus/config propagation are covered by deterministic
tests, full regression checks pass, the copied app rebuilds, live smoke still
renders with `present-driver=display-link reason=core-video`, and the Phase C
renderer mailbox / `Options` checklist item can be checked.

**Partial** = a subset is implemented and verified, but one of focus,
visibility/occlusion, or config-change propagation remains unproven or too
weakly wired to check the roadmap item. Record the exact missing piece.

**Fail** = the change breaks rendering, app startup, display-link presentation,
cursor/focus semantics, config reload behavior, or the Rust/ABI test gates.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Maxwell`, fresh context.

**Verdict:** Approved.

Findings: None. The reviewer confirmed the design links Experiment 179 as
`Designed`, has the required sections, stays scoped to the Phase C renderer
mailbox / `Options` item, matches upstream focus / visible / `change_config`
semantics closely enough for this slice, and includes concrete verification and
hygiene gates.

## Result

**Result:** Pass.

Implemented the scoped renderer-options propagation slice:

- Replaced the misleading live-surface `occluded` state with `visible`, matching
  upstream's renderer-thread flag default and the copied Swift caller's
  `window.occlusionState.contains(.visible)` argument.
- Made `roastty_surface_set_occlusion(surface, visible)` interpret its bool as
  visibility. Becoming visible requests a live frame for `NSView` surfaces;
  becoming invisible suppresses live presentation work.
- Added live-render helpers for `has_live_view`, `should_present_live`,
  `request_live_render`, focus option propagation, and visibility option
  propagation.
- Gated present-driver submission and `present_live` on live visibility, so
  timer work may continue but invisible surfaces do not rebuild or submit Metal
  frames.
- Routed focus changes through `apply_focus_options`, preserving Experiment
  178's cursor-blink behavior and ABI-only quietness while keeping custom shader
  focus-change marking on the same path.
- Made config updates on live `NSView` surfaces drop the live renderer, mark the
  surface dirty, and wake the app so renderer-derived state is rebuilt on the
  next visible frame. ABI-only surfaces without a worker remain quiet.

Focused verification:

- `cargo test -p roastty live_renderer_options -- --test-threads=1` — **Pass**,
  6 tests passed; the package integration harness had 0 matching filtered tests.
- `cargo test -p roastty live_cursor_blink -- --test-threads=1` — **Pass**, 4
  tests passed; the package integration harness had 0 matching filtered tests.

Regression verification:

- `cargo test -p roastty --test abi_harness` — **Pass**, 1 test passed. The
  existing 10 enum-conversion warnings and `[unknown](scope): message` remained.
- `cargo test -p roastty -- --test-threads=1` — **Pass**, 4896 passed, 0 failed,
  4 ignored; ABI harness and doc-tests also passed.
- `cargo fmt --check -p roastty` — **Pass**.
- `git diff --check` — **Pass**.

Live sanity:

- `cd roastty && macos/build.nu --action build` — **Pass**. The copied app build
  completed with `** BUILD SUCCEEDED **`; only the existing Swift actor,
  retroactive Sendable, linker deployment-target, and terminfo warnings
  appeared.
- `scripts/roastty-app/stop-app.sh && TERMSURF_AB_HOLD_SECONDS=10 ROASTTY_PRESENT_DRIVER_LOG=1 scripts/roastty-app/live-ab-smoke.sh --recipe smoke --comparison-region content --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  — **Pass**. The harness launched Ghostty PID `67244` and Roastty PID `67252`
  with marker `ISSUE802_AB_SMOKE_20260613-021538` and returned JSON verdict
  `PASS`.
- Content-region diff metrics for that run:

  ```text
  mismatch_ratio=0.005615972222222222
  mean_channel_delta=0.5575102430555555
  compared_pixels=1440000
  mismatched_pixels=8087
  ```

- Full-window diff metrics for that run:

  ```text
  mismatch_ratio=0.0583534414556962
  mean_channel_delta=1.500798556170886
  compared_pixels=2022400
  mismatched_pixels=118014
  ```

- Screenshot artifacts:

  ```text
  /Users/ryan/.cache/termsurf/shots/ghostty-ab-content-20260613-021538.png
  /Users/ryan/.cache/termsurf/shots/roastty-ab-content-20260613-021538.png
  /Users/ryan/.cache/termsurf/shots/ghostty-ab-crop-20260613-021538.png
  /Users/ryan/.cache/termsurf/shots/roastty-ab-crop-20260613-021538.png
  /Users/ryan/.cache/termsurf/shots/roastty-ab-full-20260613-021538.png
  ```

- Present-driver log check:

  ```text
  /Users/ryan/.cache/termsurf/shots/roastty-ab-stderr-20260613-021538.log:1:[roastty] present-driver=display-link reason=core-video
  ```

- Cleanup check: `no debug Roastty app PID remains`.

The Phase C renderer mailbox / `Options` checklist item is now checked. The
remaining Phase C gaps are the broader `surface_draw` ownership wording,
retiring the interim `render_state` pull divergence, and the final ASCII
terminal milestone.

## Conclusion

The in-process live renderer now has the state transitions that matter for the
upstream renderer mailbox's `Options` slice. The most important behavioral fix
was semantic: the copied macOS app passes `visible`, not `occluded`, so the Rust
side now stores and uses visibility directly. With visibility, focus, and config
updates wired into live rendering, the next Phase C experiment can focus on
removing the remaining `render_state_update` pull-path divergence or proving the
broader `surface_draw` ownership milestone, depending on which gap is still most
limiting in the current app path.

## Completion Review

**Reviewer:** Codex-native adversarial review subagent `Franklin`, fresh
context.

**Initial verdict:** Approved.

Findings and fixes:

- Optional: the original focused tests asserted `should_present_live()` and
  dirty/wakeup state, but did not directly call through the invisible
  `present_live` gate. Fixed by adding
  `live_renderer_options_present_live_is_noop_while_invisible`, which creates a
  dirty live surface with a dangling `NSView`, sets `visible = false`, calls
  `present_live()`, and proves no live renderer is built and the dirty frame
  remains pending.

**Final verdict:** Approved.

The reviewer independently verified the focused renderer-options and
cursor-blink tests, `cargo fmt --check -p roastty`, prettier check, and
`git diff --check`, and confirmed the result remained uncommitted at review
time.
