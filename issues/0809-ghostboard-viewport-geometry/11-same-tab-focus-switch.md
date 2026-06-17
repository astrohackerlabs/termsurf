# Experiment 11: Same-tab focus switching

## Description

Experiments 4 through 10 proved that a browser overlay can survive split,
resize, rebalance, zoom, sibling close, and browser-pane close transitions. The
next viewport matrix rows are focus transitions inside the same terminal tab:

- focusing a different non-browser pane in the same tab must not move, hide, or
  retarget the browser overlay;
- focusing back to the browser-owning pane must make the existing browser
  overlay interactive again with the same pane id, browser tab id, context id,
  frame, and AppKit-presented pixel size.

This experiment should start with a browser in the left pane of a split-right
layout and a plain terminal sibling in the right pane. It should click the right
sibling pane, prove keyboard input goes to the terminal pane and not the
browser, prove the browser overlay remains attached to the original left pane,
then click back into the browser pane and prove browser hit testing and keyboard
forwarding resume for the original browser context.

This experiment intentionally covers only focus switching between two panes in
one tab. New tabs, tab switching, tab close, window switching, multiple windows,
and focus behavior after browser-pane close remain later matrix rows.

If current Ghostboard already passes, the experiment should record that and
avoid product source changes. If it fails, the harness must first localize
whether the failure is focus state, visibility gating, hit testing, keyboard
forwarding, or geometry re-presentation before any fix is designed in this
experiment.

## Changes

Planned files:

- `scripts/ghostboard-geometry-matrix.sh`
  - add a `split-right-focus-switch` scenario;
  - add scenario-local keybinding:
    - `keybind = ctrl+d=new_split:right`;
  - launch the same repo-built `TermSurf.app`, `target/debug/web`, and Roamium
    trace setup as existing scenarios;
  - wait for the initial-open AppKit/Zig/Roamium correlation to pass;
  - inject Control-D to create the right split and wait for the same post-split
    proof used by prior split-right scenarios;
  - record the browser pane id, browser tab id, context id, split overlay frame,
    AppKit-presented pixel size, app-log line count, and Roamium trace line
    count as the browser-attached baseline;
  - click inside the right sibling terminal pane using deterministic mouse
    input;
  - prove the click does not route to the original browser context after the
    negative-hit boundary;
  - type a deterministic marker into the sibling terminal pane and prove the app
    log records post-click keyboard events with `overlay_frame=none`,
    `visible=false`, and `focused=true`;
  - prove Roamium does not receive browser key events for that sibling-terminal
    marker after the sibling-focus trace boundary;
  - prove Roamium receives a run-specific focus trace showing the original
    browser tab and pane became unfocused after the sibling-focus boundary;
  - prove the browser overlay remains presented for the original browser pane
    and context with the same split frame and AppKit-presented pixel size;
  - click back inside the original browser pane;
  - prove AppKit hit testing routes to the original browser context with
    `hit=true`, a current `web_point`, and an `overlay_frame` equal to the split
    baseline frame;
  - prove no fresh post-refocus AppKit presentation for the original browser
    pane/context reports a different frame or AppKit-presented pixel size than
    the split baseline, or require a fresh matching presentation/pixel record if
    the implementation re-presents on focus changes;
  - prove Roamium receives a run-specific focus trace showing the original
    browser tab and pane became focused again after the browser-refocus
    boundary;
  - type a deterministic browser marker and prove Roamium receives key events
    for the original browser tab and pane after the browser-refocus boundary;
  - capture a post-focus-back screenshot;
  - fail if any assertion accepts pre-focus-switch AppKit, Zig, Roamium, or
    hit-test records as post-switch proof.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - change only if the harness proves browser/sibling focus does not update
    Ghostty focus state or incorrectly forwards keyboard events across panes.
- `ghostboard/src/apprt/termsurf.zig`
  - change only if the harness proves Zig stale pane state or visibility state
    causes the browser to attach to the wrong focused pane.
- `roamium/src/dispatch.rs`
  - change only if existing run-specific trace output cannot prove browser-key
    forwarding resumed after refocus;
  - any such change must be trace-only under the existing run-specific trace
    mechanism.
- `issues/0809-ghostboard-viewport-geometry/11-same-tab-focus-switch.md`
  - record the design, implementation, verification, completion review, result,
    and conclusion.
- `issues/0809-ghostboard-viewport-geometry/README.md`
  - add Experiment 11 to the experiment index.

Reference files:

- `scripts/ghostboard-geometry-matrix.sh`
- `scripts/ghostty-app/inject.swift`
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
- `ghostboard/macos/Sources/Features/Terminal/BaseTerminalController.swift`
- `ghostboard/src/apprt/termsurf.zig`
- `roamium/src/dispatch.rs`
- `issues/0809-ghostboard-viewport-geometry/04-split-right-pane-attachment.md`
- `issues/0809-ghostboard-viewport-geometry/10-close-browser-pane.md`

## Verification

Pass criteria:

- Markdown is formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0809-ghostboard-viewport-geometry/README.md \
    issues/0809-ghostboard-viewport-geometry/11-same-tab-focus-switch.md
  ```

- Shell syntax is valid:

  ```bash
  bash -n scripts/ghostboard-geometry-matrix.sh
  ```

- If Swift files are changed:

  ```bash
  cd ghostboard
  swiftlint lint --strict --fix \
    "macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift"
  swiftlint lint --strict \
    "macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift"
  macos/build.nu --scheme Ghostty --configuration Debug --action build
  ```

- If Zig files are changed:

  ```bash
  cd ghostboard
  zig fmt src/apprt/termsurf.zig
  zig build -Demit-macos-app=false
  ```

- If Rust files are changed:

  ```bash
  cargo fmt -- roamium/src/dispatch.rs
  cargo check -p roamium
  ```

- If only the harness/docs change, the already-built app may be reused, but the
  final result must still state whether any product build was or was not needed.
- Existing adjacent scenarios still pass serially:

  ```bash
  scripts/ghostboard-geometry-matrix.sh initial-open
  scripts/ghostboard-geometry-matrix.sh split-right
  scripts/ghostboard-geometry-matrix.sh split-right-close-sibling
  scripts/ghostboard-geometry-matrix.sh split-right-close-browser-pane
  ```

- The new scenario passes:

  ```bash
  scripts/ghostboard-geometry-matrix.sh split-right-focus-switch
  ```

- The `split-right-focus-switch` passing run proves all of the following:
  - initial-open still correlates AppKit, Zig, Roamium, screenshot, and hit
    test;
  - the split action is triggered by the scenario-local `ctrl+d` keybinding;
  - after split, the browser overlay remains attached to the original browser
    pane id and context id;
  - clicking the sibling terminal pane does not route input to the original
    browser context;
  - keyboard input typed while the sibling terminal pane is focused is recorded
    as terminal-pane keyboard input, not browser input;
  - Roamium does not receive the sibling-terminal marker as browser key events;
  - Roamium records `focus-changed` with `focused=false` for the original
    browser tab id and pane id after the sibling-focus boundary;
  - while the sibling pane is focused, the original browser overlay keeps the
    same pane id, browser tab id, context id, split frame, and AppKit-presented
    pixel size;
  - clicking back into the browser pane produces a fresh `hit=true` hit-test for
    the original browser context with a current webview-relative point and the
    same `overlay_frame` as the split baseline;
  - after browser refocus, any fresh AppKit presentation/pixel record for the
    original browser pane and context either matches the split baseline frame
    and pixel size, or there is no fresh presentation because focus changed
    without requiring geometry re-presentation;
  - Roamium records `focus-changed` with `focused=true` for the original browser
    tab id and pane id after the browser-refocus boundary;
  - keyboard input typed after browser refocus reaches Roamium for the original
    browser tab id and pane id;
  - the post-focus-back screenshot shows the browser still visible in the
    original pane and not duplicated or shifted into the sibling pane.
- `git diff --check` passes.

Fail criteria:

- The harness focuses panes by calling private Ghostboard APIs instead of
  exercising real mouse input.
- The test accepts pre-focus-switch AppKit, Zig, Roamium, or hit-test records as
  proof of post-switch behavior.
- The browser hides, moves to the sibling pane, resizes unexpectedly, or changes
  browser tab/context identity merely because focus moved to the sibling pane.
- Keyboard input typed in the focused sibling terminal pane reaches Roamium as
  browser input for the original browser context.
- After focusing back to the browser pane, mouse or keyboard input no longer
  reaches the original browser context.
- After focusing back to the browser pane, the fresh hit-test frame, or any
  fresh presentation/pixel evidence, disagrees with the split baseline frame or
  AppKit-presented pixel size.
- Roamium focus trace shows no focus-away/focus-back transition for the original
  browser tab and pane even though pane focus changed.
- The experiment expands into tab switching, window switching, tab close, or
  multi-window behavior before same-tab focus switching is proven.

## Design Review

The first design review was performed by a fresh-context Codex adversarial
subagent.

Verdict: **Changes required**.

Findings:

- Required: focus-back verification could pass without proving the stated
  same-frame and same-pixel invariant after refocus. The design required the
  refocused browser overlay to keep the same pane id, browser tab id, context
  id, frame, and AppKit-presented pixel size, but the pass criteria only
  required a fresh hit-test, Roamium key events, and a screenshot.
- Optional: Roamium focus trace should be checked if same-tab pane focus is part
  of the intended contract.

Fixes:

- Added a requirement that the fresh post-refocus `hit=true` record include an
  `overlay_frame` equal to the split baseline.
- Added a requirement that post-refocus AppKit presentation/pixel evidence
  either matches the split baseline, or that no fresh re-presentation occurred
  because focus changed without needing geometry re-presentation.
- Added mandatory Roamium `focus-changed focused=false` and
  `focus-changed focused=true` checks after the focus-away and focus-back
  boundaries.

The fixed design was re-reviewed by the same fresh-context Codex adversarial
subagent.

Final verdict: **Approved**.

The reviewer confirmed the same-frame/same-pixel invariant is now explicitly
tested after browser refocus, the Roamium focus trace concern is addressed, and
no new Required findings were introduced.

## Result

**Result:** Pass

Implementation:

- `scripts/ghostboard-geometry-matrix.sh`
  - added `split-right-focus-switch`;
  - added negative Roamium trace assertions for sibling keyboard input;
  - added post-boundary AppKit frame and AppKit-presented pixel invariants;
  - added real mouse focus steps for the sibling pane and browser-owning pane;
  - added an explicit Browse-mode transition before requiring browser keyboard
    forwarding.
- `ghostboard/macos/Sources/App/macOS/ghostty-bridging-header.h`
  - exported `termsurf_pane_focus_changed`.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - reports per-surface focus changes from `focusDidChange`, the lower-level
    focus path that also receives first-responder transitions.
- `ghostboard/src/main_c.zig`
  - exports `termsurf_pane_focus_changed` to Swift.
- `ghostboard/src/apprt/termsurf.zig`
  - maps pane focus changes to the existing TermSurf `FocusChanged` protocol
    message for the owning browser tab, with `focused=true` gated on both
    terminal pane focus and webtui Browse mode.

Important learnings:

- The first focus bridge was too high-level when attached to
  `BaseTerminalController.syncFocusToSurfaceTree`. Some real mouse focus
  transitions reached `SurfaceView.focusDidChange` without producing the
  TermSurf focus notification. Moving the bridge call into
  `SurfaceView.focusDidChange` made Roamium focus traces deterministic.
- Clicking inside the browser overlay can route mouse events to Roamium without
  necessarily proving that the terminal surface focus changed. The harness now
  separates pane-focus clicks from overlay hit-test clicks.
- Browser keyboard forwarding is intentionally gated on `pane.browsing`.
  Refocusing the pane is not enough; the harness must press Enter in webtui
  Control mode, wait for `ModeChanged: ... browsing=true`, and only then require
  browser key events in Roamium.
- Browser focus forwarding must follow the same rule as keyboard forwarding:
  pane focus in webtui Control mode must not send `focused=true` to Roamium.
  Focus loss still sends `focused=false`, and entering Browse mode in the
  focused pane sends `focused=true`.

Verification performed:

- Markdown was formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0809-ghostboard-viewport-geometry/README.md \
    issues/0809-ghostboard-viewport-geometry/11-same-tab-focus-switch.md
  ```

- Shell syntax passed:

  ```bash
  bash -n scripts/ghostboard-geometry-matrix.sh
  ```

- Zig formatting and build passed:

  ```bash
  cd ghostboard
  zig fmt src/apprt/termsurf.zig src/main_c.zig
  zig build -Demit-macos-app=false
  ```

- Swift lint and app build passed:

  ```bash
  cd ghostboard
  swiftlint lint --strict --fix \
    "macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift" \
    "macos/Sources/Features/Terminal/BaseTerminalController.swift" \
    "macos/Sources/App/macOS/ghostty-bridging-header.h"
  swiftlint lint --strict \
    "macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift" \
    "macos/Sources/Features/Terminal/BaseTerminalController.swift" \
    "macos/Sources/App/macOS/ghostty-bridging-header.h"
  macos/build.nu --scheme Ghostty --configuration Debug --action build
  ```

- The new scenario passed:

  ```bash
  scripts/ghostboard-geometry-matrix.sh split-right-focus-switch
  ```

  Passing evidence:

  - app log:
    `logs/ghostboard-geometry-split-right-focus-switch-app-20260617-103845.log`
  - Roamium trace:
    `logs/ghostboard-geometry-split-right-focus-switch-roamium-20260617-103845.log`
  - harness log:
    `logs/ghostboard-geometry-split-right-focus-switch-harness-20260617-103845.log`
  - screenshot:
    `logs/ghostboard-geometry-split-right-focus-switch-split-screenshot-20260617-103845.png`

- Adjacent regression scenarios passed serially:

  ```bash
  scripts/ghostboard-geometry-matrix.sh initial-open
  scripts/ghostboard-geometry-matrix.sh split-right
  scripts/ghostboard-geometry-matrix.sh split-right-close-sibling
  scripts/ghostboard-geometry-matrix.sh split-right-close-browser-pane
  ```

  Passing evidence:

  - `initial-open`:
    `logs/ghostboard-geometry-initial-open-harness-20260617-103907.log`
  - `split-right`:
    `logs/ghostboard-geometry-split-right-harness-20260617-103913.log`
  - `split-right-close-sibling`:
    `logs/ghostboard-geometry-split-right-close-sibling-harness-20260617-103955.log`
  - `split-right-close-browser-pane`:
    `logs/ghostboard-geometry-split-right-close-browser-pane-harness-20260617-104005.log`

- Whitespace check passed:

  ```bash
  git diff --check
  ```

The final `split-right-focus-switch` run proved the sibling click did not route
to the original browser context, sibling keyboard input stayed terminal-side,
Roamium received `focused=false` for the original browser pane, the original
browser AppKit frame and AppKit-presented pixel size stayed unchanged while the
sibling pane was focused, browser refocus produced a fresh hit-test against the
original context with the split baseline frame, raw Control-mode refocus did not
send stale `focused=true` to Roamium, webtui entered Browse mode, Roamium then
received `focused=true`, and browser keyboard input reached the original Roamium
browser tab and pane.

## Completion Review

The first completion review was performed by a fresh-context Codex adversarial
subagent.

Verdict: **Changes required**.

Findings:

- Required: `paneFocusChanged` forwarded `focused=true` to Roamium even when the
  pane was focused in webtui Control mode, before `ModeChanged(browsing=true)`.
  This created stale or duplicate focus events and diverged from the Wezboard
  behavior where browser focus true is gated on Browse mode.
- Required: the harness encoded that stale pre-Browse `focused=true` as a pass
  condition instead of rejecting it.

Fixes:

- Added `focused` tracking to `PaneState`.
- Changed `paneFocusChanged` so focus loss sends `focused=false`, but focus gain
  sends `focused=true` only if the pane is already browsing.
- Changed `handleModeChanged` so entering Browse sends `focused=true` only when
  the pane is currently focused, while leaving Browse sends `focused=false`.
- Changed `split-right-focus-switch` to require no Roamium `focused=true` after
  raw Control-mode browser pane focus/refocus, then require `focused=true` only
  after the explicit Browse-mode transition.

The fixes were re-reviewed by the same fresh-context Codex adversarial subagent.

Final verdict: **Approved**.

The reviewer confirmed both Required findings were resolved: `focused=true` is
now gated on `focused && browsing`, the harness rejects pre-Browse browser focus
true events, the corrected passing logs support the fix, and no new Required
findings were introduced.

## Conclusion

Same-tab focus switching now passes for a split-right layout. Ghostboard reports
browser-pane focus changes to Roamium through the TermSurf protocol only when
the focused pane is in Browse mode, preserves the browser overlay geometry while
focus moves to a sibling terminal pane, and restores browser mouse and keyboard
interactivity for the original browser context after focus returns and webtui
enters Browse mode.

The next experiment should move to the next viewport matrix row: creating a new
terminal tab and proving the existing browser overlay is hidden when its owning
tab is no longer selected.
