# Experiment 176: macOS Hidden Titlebar Runtime

## Description

`RUNTIME-011B2B` still owns live macOS titlebar visuals after Experiment 175
split out focused right-split visual layout. The next narrow slice is the
visible effect of `macos-titlebar-style = hidden`: the hidden titlebar style
should remove the normal macOS titlebar/traffic-light controls from the live
window, while the default transparent titlebar style should keep them.

This experiment will split out one focused live GUI row: a debug Roastty window
launched with `macos-titlebar-style = hidden` must produce a screenshot without
the native red/yellow/green window controls, while an otherwise equivalent
window launched with `macos-titlebar-style = transparent` must show those
controls in the top-left titlebar region.

This experiment will not claim full titlebar parity, tab-integrated titlebar
behavior, proxy icon behavior, titlebar text/font parity, update/zoom titlebar
accessories, broader split interactions, cursor/pointer pixels, or full GUI
walkthrough parity.

## Changes

- `issues/0805-roastty-ghostty-parity/macos_titlebar_runtime.py`
  - Add a live debug-app guard using the established macOS launch discipline:
    absolute `roastty/macos/build/Debug/Roastty.app`, isolated config,
    `ROASTTY_CLEAR_USER_DEFAULTS=1`, a unique user-defaults suite, exact
    launched Unix PID targeting, scoped cleanup, and new-crash-report failure.
  - Launch two separate app runs with matching deterministic terminal config:
    fixed window size, opaque background, cursor blink disabled,
    `quit-after-last-window-closed = true`, and `macos-applescript = true`.
  - For the control run, set `macos-titlebar-style = transparent`.
  - For the test run, set `macos-titlebar-style = hidden`.
  - In each run, create one terminal window through AppleScript with a stable
    child command that sleeps long enough for screenshot capture.
  - Immediately before each screenshot, activate the launched app and use System
    Events plus AppleScript to prove:
    - the frontmost process Unix PID is the exact launched debug-app PID;
    - the front window is main/key from the scriptable app perspective;
    - the CGWindowID selected for capture belongs to that PID-owned foreground
      layer-0 window.
  - Capture the exact PID-owned layer-0 CoreGraphics window id with
    `screencapture -x -o -l{window_id}`.
  - Sample the top-left titlebar/control region of the captured PNG with an
    inline Swift/AppKit helper.
  - Treat the focused hidden-titlebar visual effect as proven only if:
    - the transparent-style screenshot has nonzero dimensions and contains
      red-dominant, yellow-dominant, and green-dominant traffic-light samples in
      the top-left titlebar/control region;
    - the hidden-style screenshot has nonzero dimensions and does not contain
      those red/yellow/green traffic-light samples in the same top-left region;
    - both screenshots are tied to exact PID-owned layer-0 CGWindowIDs from
      their respective launched debug app processes;
    - both screenshots are taken only after proving the launched PID is
      frontmost and the captured window is the active/main window, so
      traffic-light absence cannot be explained by an inactive window;
    - no new `roastty-*.ips` crash report appears during either run.
  - Save the latest control/test screenshots, sampled metrics, frontmost PID,
    and active-window evidence to `/tmp` for failure diagnosis.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Add a new Oracle-complete macOS app row for focused live hidden-titlebar
    visual proof if the guard passes.
  - Reduce `RUNTIME-011B2B` so it no longer owns this focused hidden-titlebar
    traffic-light visibility evidence.
  - Keep `RUNTIME-011B2B` open for broader titlebar styles and details, titlebar
    tabs, proxy icons, accessories, broader split interactions, cursor/pointer
    pixels, broader screenshot/pixel parity, and broader input walkthrough
    effects.
  - Update CFG-223 counts only if the new row is added and passing; CFG-223 must
    remain `Gap`.
- Existing CFG-223 guard scripts
  - Update only runtime-row, Oracle-complete, closed-row, and remaining-gap text
    that becomes stale after the titlebar row is split out.
- Generated docs
  - Regenerate `config-runtime-inventory.md`, `config-matrix.md`, and
    `platform-runtime-classification.md`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Keep the Experiment 176 line at `Designed` until implementation and result
    review complete.
  - Add a learning only if the experiment discovers a reusable titlebar or
    screenshot-sampling constraint.

## Verification

- Build the debug app:

```bash
(cd roastty && macos/build.nu --action build)
```

- Run the new live guard:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_titlebar_runtime.py
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

- Run the macOS app guards that reference the macOS app gap:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_app_workflow_plumbing_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_applescript_workflow_runtime.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_native_menu_runtime.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_gui_state_runtime.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_quick_terminal_runtime.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_split_layout_runtime.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_titlebar_runtime.py
```

- Format and hygiene:

```bash
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/176-macos-hidden-titlebar-runtime.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/platform-runtime-classification.md
git diff --check
```

Pass criteria:

- The new guard passes only after proving exact-PID targeting, scoped cleanup,
  no new crash report, frontmost/main-window state before both captures,
  exact-CGWindowID screenshots for both titlebar styles, traffic-light color
  presence in the transparent-style control screenshot, and traffic-light color
  absence in the hidden-style test screenshot.
- The sampled-color oracle cannot pass if screenshots are blank, capture the
  wrong process/window, or only test the hidden style without a positive control
  proving the sampler can see traffic-light controls.
- Generated CFG-223 counts are internally consistent.
- CFG-223 remains `Gap`.
- `RUNTIME-011B2B` remains open and still lists broader titlebar behavior,
  broader split interactions, cursor/pointer pixels, broader screenshot/pixel
  parity, and broader GUI walkthrough effects.

Fail criteria:

- The guard can pass without exact-CGWindowID screenshot evidence for both the
  transparent-style control and hidden-style test windows.
- The guard can pass without proving both captured windows are frontmost/main
  immediately before screenshot capture.
- The guard can pass without a positive control that detects red/yellow/green
  traffic-light samples in the default titlebar.
- The guard relies on an installed app, ambiguous process name, or PID-only
  screenshot selection instead of the launched debug app and exact CGWindowID.
- CFG-223 is marked complete.
- The experiment claims full titlebar parity, titlebar tabs parity, proxy icon
  parity, titlebar text/accessory parity, pointer/cursor parity, broad renderer
  pixel parity, or full GUI walkthrough parity without directly proving those
  behaviors.

## Design Review

Fresh-context adversarial reviewer `Boole the 3rd` reviewed the initial design
and returned `CHANGES REQUIRED`.

Required finding:

- The initial hidden-style absence oracle could false-pass if the hidden run was
  not focused/frontmost. The reviewer noted that an inactive transparent
  titlebar can lose colored traffic-light samples, so the hidden run needed
  active-window proof instead of relying only on traffic-light absence.

Fix made:

- The design now requires both the transparent-style control run and the
  hidden-style test run to activate the launched app and prove immediately
  before screenshot capture that the frontmost process Unix PID is the exact
  launched debug-app PID, that the front window is main/key from the scriptable
  app perspective, and that the captured CGWindowID belongs to that PID-owned
  foreground layer-0 window.
- The pass/fail criteria now require frontmost/main-window evidence for both
  captures, and the debug evidence saved to `/tmp` must include frontmost PID
  and active-window evidence.

Re-review verdict: **Approved**. The reviewer confirmed that the design now
requires pre-capture frontmost/main-window proof for both runs, includes it in
the proof criteria and saved evidence, and makes missing frontmost/main proof a
fail condition.

Final design verdict: **Approved**.

## Result

**Result:** Pass

Experiment 176 added `macos_titlebar_runtime.py`, a live macOS GUI guard for
`macos-titlebar-style = hidden`.

The guard launches the debug Roastty app bundle twice with isolated temporary
config and defaults: once with `macos-titlebar-style = transparent` as the
positive control, and once with `macos-titlebar-style = hidden` as the test
case. Each run creates a terminal window through the app's AppleScript
interface, activates the exact launched process, proves the frontmost Unix PID
matches the launched debug-app PID, raises the target window, proves the window
is `AXMain`, proves the process `AXFocusedWindow` is also main, finds the exact
PID-owned foreground layer-0 CGWindowID whose CoreGraphics bounds match the
accessibility-focused window bounds, captures that window with
`screencapture -l`, and samples the top-left titlebar region with a Swift image
sampler.

The transparent control screenshot contained red/yellow/green traffic-light
samples. The hidden-style screenshot contained no traffic-light samples above
the guard threshold. This proves the hidden-titlebar style has a visible runtime
effect in the actual macOS app without claiming broader titlebar parity.

Debug artifacts from the passing run:

- `/tmp/termsurf-issue805-exp176-transparent-titlebar.png`
- `/tmp/termsurf-issue805-exp176-transparent-titlebar.json`
- `/tmp/termsurf-issue805-exp176-hidden-titlebar.png`
- `/tmp/termsurf-issue805-exp176-hidden-titlebar.json`

Verification run:

```bash
(cd roastty && macos/build.nu --action build)
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_titlebar_runtime.py
```

Output:

```text
macos_titlebar_runtime=pass transparent_window=1806 hidden_window=1819
```

Inventory impact:

- Added `RUNTIME-011B2J` for live hidden-titlebar visual proof.
- CFG-223 now has 82 runtime rows.
- CFG-223 now has 75 `Oracle complete` rows.
- CFG-223 now has 78 closed rows.
- CFG-223 still has 4 incomplete rows, all `Gap`.
- CFG-223 remains `Gap`.
- `RUNTIME-011B2B` remains open for broader titlebar behavior, broader split
  interactions, cursor/pointer pixels, broader screenshot/pixel parity, and
  broader GUI walkthrough effects.

## Conclusion

The hidden-titlebar runtime slice is now covered by a durable live GUI guard
with a positive traffic-light control and exact-window screenshot evidence.
Future titlebar or GUI pixel experiments should use the same pattern: launch the
debug app with isolated config, prove frontmost/main-window state immediately
before capture, prove the main accessibility window maps to the captured
CGWindowID, and pair absence checks with a positive control.

## Result Review

Fresh-context adversarial reviewer `Nash the 3rd` reviewed the completed
experiment result and initially returned `CHANGES REQUIRED`.

Required findings:

- The first implementation proved `AXMain`/`AXFocusedWindow` for `window 1` but
  selected the screenshot target through a separate CoreGraphics
  `primary_window(pid)` lookup. If multiple PID-owned layer-0 windows existed,
  the guard could prove one window was main while capturing another.
- The recorded verification command used
  `cd roastty && macos/build.nu --action build` followed by the Python guard
  path, which would leave the shell in `roastty/` and make the relative issue
  path invalid.

Fixes made:

- `macos_titlebar_runtime.py` now reads the `AXFocusedWindow` position and size,
  then selects the exact PID-owned layer-0 CoreGraphics window whose bounds
  match that focused accessibility window before capture.
- The verification command now uses
  `(cd roastty && macos/build.nu --action build)`, so the Python guard still
  runs from the repository root.

Re-review verdict: **Approved**. The reviewer confirmed both required findings
were resolved and that no new required findings were introduced.

Final result verdict: **Approved**.
