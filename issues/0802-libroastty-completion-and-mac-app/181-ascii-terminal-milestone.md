# Experiment 181: Phase C — ASCII terminal milestone

## Description

Close the final Phase C milestone by proving the copied Roastty macOS app
launches and shows a working ASCII terminal through the live render path.

Experiment 180 proved ownership of the live draw path: Swift creates surfaces
with its AppKit `NSView`, Rust owns `SurfaceLiveRenderer`, the IOSurface layer
is attached to the app view, and the copied app no longer uses the interim
render-state pull path. The remaining Phase C checkbox is not a new subsystem;
it is the user-visible milestone that those pieces produce a working terminal.

This experiment uses the existing live A/B harness with the `ascii-grid` recipe.
That recipe launches real Ghostty and Roastty in the same run, injects a command
that clears the screen and prints a unique marker plus representative ASCII
rows, captures both windows, crops the terminal content region, and diffs the
pixels. Passing this experiment means the copied app launches, receives shell
output, renders ASCII glyphs, and presents those pixels on-screen closely enough
to Ghostty to check the Phase C milestone.

## Changes

- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Link this experiment as `Designed`.
  - After verification, mark it `Pass`, `Partial`, or `Fail`.
  - Check the Phase C milestone only if the copied app rebuilds, launches, and
    the `ascii-grid` live A/B run shows a visible Roastty ASCII terminal with
    acceptable content-region diff metrics and a live present-driver log.

- `issues/0802-libroastty-completion-and-mac-app/181-ascii-terminal-milestone.md`
  - Record the command output, screenshot artifact paths, diff metrics,
    present-driver evidence, result, conclusion, and AI completion review.

- Production code
  - No code change is expected. If verification exposes a real milestone bug,
    record the failed evidence here and design the next implementation
    experiment from that result.

## Verification

Before verification:

- Codex-native adversarial design review approves this experiment.
- Commit the reviewed plan separately from the result.

Source/harness sanity:

- Confirm the harness still has an `ascii-grid` recipe that emits the marker,
  uppercase letters, lowercase letters, digits, and punctuation:

  ```bash
  rg -n "ascii-grid|ABCDEFGHIJKLMNOPQRSTUVWXYZ|abcdefghijklmnopqrstuvwxyz|0123456789" \
    scripts/roastty-app/live-ab-smoke.sh
  ```

- Confirm the copied app still has no render-state pull usage:

  ```bash
  rg -n "render_state|RenderState|surface_render_state_update|roastty_render_state" \
    roastty/macos/Sources
  ```

Build and regression gates:

- `cargo test -p roastty live_renderer_options -- --test-threads=1`
- `cargo test -p roastty live_cursor_blink -- --test-threads=1`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt --check -p roastty`
- `cd roastty && macos/build.nu --action build`

Live milestone proof:

- Stop any old debug app:

  ```bash
  scripts/roastty-app/stop-app.sh
  ```

- Run the ASCII terminal live A/B recipe:

  ```bash
  TERMSURF_AB_HOLD_SECONDS=10 \
  ROASTTY_PRESENT_DRIVER_LOG=1 \
    scripts/roastty-app/live-ab-smoke.sh \
      --recipe ascii-grid \
      --comparison-region content \
      --max-mismatch-ratio 0.03 \
      --max-mean-channel-delta 5
  ```

- Record the JSON verdict, marker, process IDs, content-region metrics,
  full-window metrics, screenshot artifact paths, present-driver log line, and
  cleanup status.

Documentation hygiene:

- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/181-ascii-terminal-milestone.md issues/0802-libroastty-completion-and-mac-app/README.md`
- `git diff --check`

**Pass** = the copied app rebuilds, the `ascii-grid` live A/B run returns JSON
verdict `PASS`, Roastty's content screenshot visibly contains the unique marker
and representative ASCII rows, the present-driver log shows live CoreVideo
presentation, cleanup leaves no debug Roastty app PID, regression/documentation
checks pass, and the Phase C ASCII terminal milestone can be checked.

**Partial** = the app launches and renders some terminal content, but the ASCII
recipe fails thresholds, the screenshots/logs do not prove the full milestone,
or cleanup/regression/documentation checks need follow-up. Record the exact gap.

**Fail** = the copied app does not build, launch, receive the recipe output, or
present visible ASCII terminal content.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Fermat`, fresh context.

**Verdict:** Approved.

Findings: None. The reviewer confirmed the README links Experiment 181 as
`Designed`, the experiment has the required sections, the scope is limited to
the Phase C ASCII terminal milestone, the existing `ascii-grid` live A/B recipe
exercises marker, uppercase, lowercase, digits, and punctuation through the live
path, and the verification includes plan/result commit gates, regression, build,
formatting, documentation hygiene, present-driver evidence, artifact recording,
cleanup, and no production changes unless the milestone fails.
