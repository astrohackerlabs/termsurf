# Experiment 174: Quick Terminal GUI Runtime

## Description

`RUNTIME-011B2B` still owns quick-terminal visuals after Experiment 173 split
out native fullscreen and command-palette visibility. The next narrow slice is
the live Quick Terminal window: it is triggered through the same native menu
surface, but it creates and hides a distinct terminal window with size and
placement controlled by quick-terminal config.

This experiment will split out a focused live GUI row for:

- `View > Quick Terminal` dispatch through the native menu;
- appearance of a distinct Quick Terminal panel/window owned by the launched
  debug app PID;
- PID-scoped CoreGraphics geometry showing the Quick Terminal is a
  top-positioned overlay with the configured size;
- screenshot evidence for the Quick Terminal window;
- hide/close behavior returning the launched app to the pre-toggle window set.

This experiment will not claim titlebar visual parity, split visual/layout
parity, broad screenshot/pixel parity, cursor/pointer pixels, or full
keyboard/mouse walkthrough parity.

## Changes

- `issues/0805-roastty-ghostty-parity/macos_quick_terminal_runtime.py`
  - Add a live debug-app guard derived from the Experiment 173 launch
    discipline: absolute `Roastty.app` bundle, isolated config, exact launched
    Unix PID targeting through System Events, scoped cleanup, and
    new-crash-report failure.
  - Configure deterministic quick-terminal behavior: `macos-applescript = true`,
    `quit-after-last-window-closed = true`,
    `quick-terminal-animation-duration = 0`, `quick-terminal-position = top`,
    and `quick-terminal-size = 40%`.
  - Resolve the pre-toggle PID-owned visible window set through CoreGraphics,
    including window id, layer, bounds, and name.
  - Click `View > Quick Terminal` through System Events after proving the
    frontmost process is the exact launched PID.
  - Wait up to 15 seconds for a new PID-owned visible window id that was not in
    the pre-toggle set and is not the primary terminal window.
  - Treat Quick Terminal appearance as proven only if:
    - the new window belongs to the launched PID;
    - its CoreGraphics layer is nonzero, matching the Quick Terminal source
      behavior: `QuickTerminalWindow` is an `NSPanel`, and
      `QuickTerminalController` uses popup/floating window levels during and
      after animation;
    - its width is at least `70%` of the primary screen/window width observed in
      the run;
    - its height is between `25%` and `55%` of the primary screen/window height
      observed in the run, matching the configured `40%` with allowance for menu
      bar, notch, and VM display differences;
    - its top edge is within `120pt` of the smallest `y` coordinate among
      PID-owned layer-0 windows after it appears;
    - a screenshot of the exact newly detected Quick Terminal CGWindowID can be
      captured and has nonzero dimensions.
  - Hide the Quick Terminal with the same menu item or Escape, then require the
    extra Quick Terminal window id to disappear from the PID-owned visible
    window set within 15 seconds.
  - Do not use the existing PID-only screenshot helper for the Quick Terminal
    screenshot. It prefers layer-0 windows and can capture the primary terminal
    while the Quick Terminal panel is visible. Instead, capture the exact Quick
    Terminal CGWindowID with `screencapture -x -o -l{window_id}` and verify the
    captured image dimensions are nonzero.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Add a new Oracle-complete row under the macOS app group for live Quick
    Terminal GUI visibility/geometry proof if the guard passes.
  - Reduce `RUNTIME-011B2B` so it no longer owns the focused Quick Terminal
    visibility and geometry evidence.
  - Update CFG-223 counts only if the new row is added and passing; CFG-223 must
    remain `Gap`.
- Existing CFG-223 guard scripts
  - Update only the shared runtime-row, Oracle-complete, and closed-row counts.
  - Remove stale assertions that focused quick-terminal visibility/geometry
    remains in `RUNTIME-011B2B`.
- Generated docs
  - Regenerate `config-runtime-inventory.md`, `config-matrix.md`, and
    `platform-runtime-classification.md`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Keep the Experiment 174 line at `Designed` until implementation and result
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
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_quick_terminal_runtime.py
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
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_gui_state_runtime.py
```

- Format and hygiene:

```bash
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/174-quick-terminal-gui-runtime.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/platform-runtime-classification.md
git diff --check
```

Pass criteria:

- The new guard passes only after proving exact-PID targeting, scoped cleanup,
  no new `roastty-*.ips` crash report, new PID-owned non-layer-0 Quick Terminal
  panel/window appearance, configured-size geometry evidence, exact-CGWindowID
  screenshot capture, and disappearance of the extra Quick Terminal window after
  hiding.
- Generated CFG-223 counts are internally consistent.
- CFG-223 remains `Gap`.
- `RUNTIME-011B2B` remains open and still lists titlebar visuals, split
  visual/layout parity, screenshot/pixel evidence beyond the focused live GUI
  rows, cursor/pointer pixels, and broader input walkthrough parity.

Fail criteria:

- The guard can pass without observing a distinct PID-owned Quick Terminal
  panel/window id that was absent before the menu action.
- The guard can pass without geometry evidence tied to the configured top
  position and `40%` size.
- The guard can pass without screenshot evidence for the exact Quick Terminal
  CGWindowID.
- The guard relies on a non-scoped installed app or ambiguous process name
  instead of the launched debug app PID.
- CFG-223 is marked complete.
- The experiment claims titlebar, split visual/layout, broad pixel parity, or
  broad walkthrough parity without directly proving those behaviors.

## Design Review

Fresh-context adversarial reviewer `Mencius the 2nd` reviewed the initial design
and returned `CHANGES REQUIRED`.

Required findings:

- The initial plan incorrectly required the Quick Terminal to appear as a
  PID-owned layer-0 window. The reviewer pointed to `QuickTerminalWindow.swift`,
  where the window is an `NSPanel`, and `QuickTerminalController.swift`, where
  Quick Terminal uses popup/floating window levels during and after animation.
- The screenshot proof was not tightly tied to the detected Quick Terminal
  window id. The existing PID-only screenshot helper prefers layer-0 windows and
  could capture the primary terminal while the Quick Terminal panel is visible.

Fixes made:

- The plan now detects a new PID-owned visible window id absent from the
  pre-toggle set and expects a nonzero CoreGraphics layer instead of layer 0.
- The plan now requires exact-CGWindowID screenshot capture with
  `screencapture -x -o -l{window_id}` and nonzero captured dimensions, rather
  than PID-only screenshot selection.

Re-review approved both fixes. The reviewer confirmed that the plan now waits
for a new PID-owned visible window id with nonzero CoreGraphics layer and that
the screenshot proof is tied to exact-CGWindowID capture instead of PID-only
window selection.

Final design verdict: **Approved**.
