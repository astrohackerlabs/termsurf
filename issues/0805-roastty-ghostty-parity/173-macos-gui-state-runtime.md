# Experiment 173: macOS GUI State Runtime

## Description

`RUNTIME-011B2B` still tracks broad live macOS GUI behavior after the
AppleScript and native-menu slices. Experiment 172 proved the real native menu
can be targeted and dispatched through System Events, but it intentionally
stopped at New Tab and Split Right state changes.

This experiment will split out a narrower live GUI state row for:

- `Window > Toggle Full Screen` dispatch through the native menu;
- observed fullscreen window geometry/state transitions in the launched debug
  app;
- `View > Command Palette` dispatch through the native menu;
- observed command-palette overlay visibility in the real app, with screenshot
  evidence and accessibility/window-state checks where available.

This experiment will not claim full `RUNTIME-011B2B` closure. Quick-terminal
visual behavior, split layout screenshots, titlebar visual parity, broad
keyboard/mouse walkthrough parity, and broad screenshot/pixel parity will remain
in the reduced gap row unless the guard proves them directly.

## Changes

- `issues/0805-roastty-ghostty-parity/macos_gui_state_runtime.py`
  - Add a live debug-app guard derived from the Experiment 172 launch
    discipline: absolute `Roastty.app` bundle, isolated config, exact launched
    Unix PID targeting through System Events, scoped cleanup, and
    new-crash-report failure.
  - Configure the app with `macos-applescript = true`,
    `quit-after-last-window-closed = true`, and a deterministic fullscreen mode
    suitable for repeatable geometry checks. Use a quiet long-running shell
    command and disable cursor blinking if the existing config surface supports
    it, so screenshot deltas are dominated by intentional GUI changes.
  - Reuse native menu clicking through System Events after proving the target
    process is the exact launched PID.
  - Capture screenshots outside the repo through
    `scripts/roastty-app/screenshot.sh` before and after GUI state changes.
  - Resolve the primary layer-0 window for the exact PID with CoreGraphics data
    equivalent to `scripts/roastty-app/list-windows.swift`: window id, owner
    PID, layer, name, and point bounds.
  - Click `Window > Toggle Full Screen`, wait up to 15 seconds for the same PID
    to expose a layer-0 window whose bounds prove fullscreen entry, capture a
    fullscreen screenshot, then toggle back and require the layer-0 window to
    return near its original geometry.
  - Treat fullscreen entry as proven only if:
    - the observed fullscreen window belongs to the launched PID;
    - the observed window area is at least `1.40x` the baseline area;
    - either width grows by at least `200pt` or height grows by at least
      `150pt`;
    - if System Events exposes `AXFullScreen`, it is `true` while entered.
  - Treat fullscreen exit as proven only if, within 15 seconds:
    - the observed layer-0 window still belongs to the launched PID;
    - width and height are each within `80pt` of the baseline dimensions;
    - area is within `20%` of the baseline area;
    - if System Events exposes `AXFullScreen`, it is `false` after exit.
  - Click `View > Command Palette`, wait for command-palette UI evidence, and
    require both accessibility and screenshot evidence where the VM exposes the
    relevant accessibility nodes.
  - Treat command-palette visibility as proven only if:
    - native menus are dismissed before the baseline screenshot and before each
      AppleScript app object query;
    - the same launched PID and same primary layer-0 window id are used for the
      baseline and palette screenshots;
    - System Events finds either an accessibility text field or static text
      whose value/name contains an expected command-palette cue such as
      `Search`, `Focus:`, or a configured command title; if the VM does not
      expose those SwiftUI accessibility nodes, the guard must record that
      fallback path;
    - `scripts/roastty-app/pngdiff.swift` metrics for baseline versus palette
      screenshot report identical dimensions, `mismatch_ratio >= 0.02`, and
      `mean_channel_delta >= 1.0`;
    - after sending Escape to dismiss the palette, a post-dismiss screenshot of
      the same window id returns near baseline with `mismatch_ratio <= 0.01` and
      `mean_channel_delta <= 2.0`, allowing minor cursor/animation noise but
      rejecting persistent overlay pixels.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Add a new Oracle-complete row under the macOS app group for live fullscreen
    and command-palette GUI state proof if the guard passes.
  - Reduce `RUNTIME-011B2B` so it no longer owns the specific fullscreen state
    transition and command-palette visibility evidence proven by this guard.
  - Update CFG-223 counts only if the new row is added and passing; CFG-223 must
    remain `Gap`.
- Existing CFG-223 guard scripts
  - Update only the shared runtime-row, Oracle-complete, and closed-row counts.
  - Remove stale assertions that the newly proven fullscreen/command-palette
    slice remains in `RUNTIME-011B2B`.
- Generated docs
  - Regenerate `config-runtime-inventory.md`, `config-matrix.md`, and
    `platform-runtime-classification.md`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Keep the Experiment 173 line at `Designed` until implementation and result
    review complete.
  - Add a learning only if the experiment teaches a reusable macOS GUI
    automation constraint.

## Verification

- Build the debug app:

```bash
(cd roastty && macos/build.nu --action build)
```

- Run the new live guard:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_gui_state_runtime.py
```

- Regenerate CFG-223 inventory and matrix:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
```

- Regenerate platform runtime classification:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/platform_runtime_classification.py --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/platform-runtime-classification.md
```

- Run the existing CFG-223 guard set:

```bash
for f in issues/0805-roastty-ghostty-parity/*_runtime_parity.py issues/0805-roastty-ghostty-parity/terminal_runtime_residual_audit.py issues/0805-roastty-ghostty-parity/link_hover_preview_dispatch_parity.py issues/0805-roastty-ghostty-parity/link_hover_modifier_refresh_parity.py issues/0805-roastty-ghostty-parity/link_preview_context_runtime_parity.py; do
  PYTHONDONTWRITEBYTECODE=1 python3 "$f"
done
```

- Run the existing macOS app workflow guards that reference the macOS app gap:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_app_workflow_plumbing_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_applescript_workflow_runtime.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_native_menu_runtime.py
```

- Format and hygiene:

```bash
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/173-macos-gui-state-runtime.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/platform-runtime-classification.md
git diff --check
```

Pass criteria:

- The new guard passes only after proving exact-PID targeting, scoped cleanup,
  no new `roastty-*.ips` crash report, fullscreen enter/exit state evidence, and
  command-palette overlay visibility evidence.
- Fullscreen evidence uses PID-scoped CoreGraphics layer-0 window bounds and the
  numeric thresholds in Changes: enter area `>= 1.40x` baseline plus width
  growth `>= 200pt` or height growth `>= 150pt`; exit width/height within `80pt`
  and area within `20%` of baseline. If `AXFullScreen` is exposed, it must agree
  with the entered/exited state.
- Command-palette evidence uses the same PID and same primary window id for
  baseline/palette/post-dismiss screenshots, dismisses native menus before
  object queries, requires accessibility evidence or records the accessibility
  fallback path, requires baseline-to-palette `mismatch_ratio >= 0.02` and
  `mean_channel_delta >= 1.0`, and requires post-dismiss-to-baseline
  `mismatch_ratio <= 0.01` and `mean_channel_delta <= 2.0`.
- Generated CFG-223 counts are internally consistent.
- CFG-223 remains `Gap`.
- `RUNTIME-011B2B` remains open and still lists quick-terminal visuals, split
  visual/layout parity, titlebar visual parity, screenshot/pixel evidence beyond
  this narrow proof, and broader input walkthrough parity.

Fail criteria:

- The guard can pass without observing an actual fullscreen state/geometry
  transition.
- The guard can pass without observing real command-palette UI visibility.
- The command-palette screenshot delta can be caused by an open native menu,
  changed window id, or unrelated persistent UI churn after Escape dismissal.
- The guard relies on a non-scoped installed app or an ambiguous process name
  instead of the launched debug app PID.
- CFG-223 is marked complete.
- The experiment claims quick-terminal, titlebar, split visual/layout, or broad
  pixel parity without directly proving those behaviors.

## Design Review

Fresh-context adversarial reviewer `Franklin the 2nd` reviewed the initial
design and returned `CHANGES REQUIRED`.

Required findings:

- Fullscreen pass criteria were not concrete enough. The original design
  required a measurable transition and near-original geometry but did not define
  the measurement source, thresholds, tolerance, or state oracle.
- Command-palette visibility criteria were too loose. The original screenshot
  fallback did not define numeric thresholds, require native menus to be closed,
  or include a post-dismiss control screenshot.

Fixes made:

- Defined the fullscreen oracle as exact-PID CoreGraphics layer-0 window bounds,
  with 15-second timeouts, `1.40x` baseline area growth, `200pt` width or
  `150pt` height growth, `80pt` exit dimension tolerance, `20%` exit area
  tolerance, and `AXFullScreen` agreement when that accessibility attribute is
  available.
- Defined the command-palette oracle as same-PID and same-window-id screenshots,
  native-menu dismissal before object queries, accessibility evidence where
  exposed, baseline-to-palette `pngdiff` thresholds, and a post-dismiss
  near-baseline screenshot control.

Re-review approved the fixes. The reviewer confirmed that the fullscreen oracle
now defines PID-scoped layer-0 CoreGraphics bounds, 15-second waits, enter
thresholds, exit tolerances, and optional `AXFullScreen` agreement. The reviewer
also confirmed that the command-palette oracle now requires native menu
dismissal, same PID/window id, accessibility evidence or a recorded fallback,
numeric `pngdiff` thresholds, and a post-dismiss near-baseline control
screenshot.

Final design verdict: **Approved**.
