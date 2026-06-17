# Experiment 9: Close split-right sibling

## Description

Experiment 8 proved that a browser attached to the left pane of a split-right
layout expands with split zoom and restores to the split baseline after unzoom.
The next viewport matrix row is closing a sibling pane while keeping the
browser-owning pane alive.

This experiment should start with a browser in the left pane of a split-right
layout. It should close the right sibling pane through normal Ghostty/Ghostboard
keybinding behavior, then prove the original browser pane expands back to the
single-pane viewport:

- open a browser in a single pane;
- create a right-side split from the browser-owning pane;
- record the post-split frame and AppKit-presented pixels as the sibling-open
  baseline;
- use the fact learned in Experiment 8 that `new_split:right` leaves focus on
  the newly created sibling pane;
- invoke a scenario-local `close_surface` keybinding while that sibling pane is
  focused;
- prove the original browser pane expands back to the initial single-pane frame
  within a small documented tolerance;
- prove Zig and Roamium receive the expanded AppKit-presented pixel size after
  the close-sibling action;
- prove positive hit testing still works inside the expanded browser frame;
- prove the former sibling-pane area now routes as part of the expanded browser
  instead of remaining a dead or separate pane area.

This experiment intentionally covers only closing a non-browser sibling pane in
a two-pane split-right layout. Closing the browser-owning pane, cleanup of a
browser overlay, tab close, window close, fullscreen, and multi-window behavior
remain later matrix rows.

If current Ghostboard already passes, the experiment should record that and
avoid product source changes. If it fails, the harness must first localize which
invariant failed before any Ghostboard fix is designed in this experiment.

## Changes

Planned files:

- `scripts/ghostboard-geometry-matrix.sh`
  - add a `split-right-close-sibling` scenario;
  - add scenario-local close behavior:
    - `confirm-close-surface = false`;
  - add scenario-local keybindings:
    - `keybind = ctrl+d=new_split:right`;
    - `keybind = ctrl+k=close_surface`;
  - launch the same repo-built `TermSurf.app`, `target/debug/web`, and Roamium
    trace setup as the existing scenarios;
  - wait for the initial-open AppKit/Zig/Roamium correlation to pass;
  - inject Control-D to create the right split and wait for the same post-split
    proof used by Experiments 4, 6, 7, and 8;
  - record the post-split frame, AppKit-presented pixel size, pane id, browser
    tab id, context id, app-log line count, and Roamium trace line count as the
    sibling-open baseline;
  - inject Control-K to invoke `close_surface` on the focused right sibling
    pane;
  - wait for a new AppKit presentation record after Control-K for the original
    pane/context whose frame width grows from the split baseline and returns to
    the initial single-pane frame width within a documented tolerance;
  - wait for a new AppKit-presented pixel record after Control-K whose pixel
    width grows from the split baseline and returns to the initial single-pane
    AppKit pixel width within a documented tolerance;
  - require Zig to record the expanded AppKit-presented pixel size after the
    sibling-close phase;
  - require Roamium's run-specific trace to contain `ffi=ts_set_view_size` after
    the sibling-close trace boundary with the expanded AppKit-presented pixel
    size for the original pane id and browser tab id;
  - capture a post-close-sibling screenshot;
  - send deterministic mouse input inside the expanded browser frame and require
    a fresh `hit=true` / `web_point` hit-test record after sibling close;
  - send deterministic mouse input in the former right sibling area and require
    a fresh `hit=true` record for the original browser overlay/context, proving
    the browser now occupies that area.
- `ghostboard/src/apprt/termsurf.zig`
  - change only if the harness proves sibling close updates fail;
  - likely candidate fixes should be localized from the geometry logs before
    implementation.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - change only if AppKit does not re-present or report the updated overlay
    frame/pixels for the original pane after sibling close.
- `issues/0809-ghostboard-viewport-geometry/09-close-split-right-sibling.md`
  - record the design, implementation, verification, completion review, result,
    and conclusion.
- `issues/0809-ghostboard-viewport-geometry/README.md`
  - add Experiment 9 to the experiment index.

Reference files:

- `scripts/ghostboard-geometry-matrix.sh`
- `scripts/ghostty-app/inject.swift`
- `ghostboard/src/input/Binding.zig`
- `ghostboard/src/config/Config.zig:6514`
- `ghostboard/src/config/Config.zig:6880`
- `ghostboard/src/Surface.zig:5807`
- `ghostboard/macos/Sources/App/macOS/AppDelegate.swift:1176`
- `ghostboard/macos/Sources/Features/Terminal/BaseTerminalController.swift:410-450`
- `ghostboard/macos/Sources/Features/Terminal/BaseTerminalController.swift:753-765`
- `ghostboard/macos/Sources/Features/Splits/SplitTree.swift:139-155`
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift:491-614`
- `ghostboard/src/apprt/termsurf.zig:1241-1358`
- `issues/0809-ghostboard-viewport-geometry/08-split-right-zoom-restore.md`

## Verification

Pass criteria:

- Markdown is formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0809-ghostboard-viewport-geometry/README.md \
    issues/0809-ghostboard-viewport-geometry/09-close-split-right-sibling.md
  ```

- Shell syntax is valid:

  ```bash
  bash -n scripts/ghostboard-geometry-matrix.sh
  ```

- If Zig files are changed:

  ```bash
  cd ghostboard
  zig fmt src/apprt/termsurf.zig
  zig build -Demit-macos-app=false
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

- If only the harness/docs change, the already-built app may be reused, but the
  final result must still state whether any product build was or was not needed.
- Existing adjacent scenarios still pass serially:

  ```bash
  scripts/ghostboard-geometry-matrix.sh initial-open
  scripts/ghostboard-geometry-matrix.sh split-right
  scripts/ghostboard-geometry-matrix.sh split-right-resize
  scripts/ghostboard-geometry-matrix.sh split-right-equalize
  scripts/ghostboard-geometry-matrix.sh split-right-zoom
  ```

- The new scenario passes:

  ```bash
  scripts/ghostboard-geometry-matrix.sh split-right-close-sibling
  ```

- The `split-right-close-sibling` passing run proves all of the following:
  - initial-open still correlates AppKit, Zig, Roamium, screenshot, and hit
    test;
  - close confirmation is disabled by scenario-local config
    `confirm-close-surface = false`, so Control-K closes the focused sibling
    directly instead of opening a confirmation dialog;
  - the split action is triggered by the scenario-local `ctrl+d` keybinding;
  - the close-sibling action is triggered by the scenario-local `ctrl+k`
    keybinding;
  - after sibling close, AppKit reports a new overlay frame for the original
    pane id and context id;
  - the expanded overlay frame and AppKit-presented pixel width grow from the
    split baseline and return to the initial single-pane width within a small
    documented tolerance;
  - Zig records the expanded AppKit-presented pixel size for the original pane
    id after the sibling-close phase;
  - Roamium's run-specific trace records `ffi=ts_set_view_size` after the
    sibling-close trace boundary on the same line as the expanded
    AppKit-presented pixel size for the original pane id and browser tab id;
  - the post-close-sibling screenshot shows browser content filling the expanded
    remaining pane;
  - hit testing inside the expanded browser frame reports `hit=true` and a
    current webview-relative coordinate after sibling close;
  - hit testing in the former sibling-pane area reports `hit=true` for the
    original browser overlay/context after sibling close.
- `git diff --check` passes.

Fail criteria:

- The harness closes the sibling by calling a private Ghostboard API instead of
  exercising user-visible keybinding behavior.
- The harness leaves close confirmation enabled and therefore risks proving a
  confirmation dialog rather than a pane close.
- The test accepts pre-close AppKit, Zig, Roamium, or hit-test records as proof
  of sibling-close behavior.
- The browser remains at the split baseline, overlaps stale sibling geometry, or
  loses hit-test routing after the sibling closes.
- The former sibling-pane area remains non-browser input space after the browser
  pane expands.
- The experiment expands into closing the browser-owning pane, tab close, window
  close, fullscreen, or multi-window behavior before close-sibling geometry is
  proven.

## Design Review

The first design review was performed by a fresh-context Codex adversarial
subagent.

Verdict: **Changes required**.

Finding:

- Required: the plan did not disable close confirmation, so
  `ctrl+k=close_surface` could open a confirmation dialog instead of closing the
  focused sibling pane. Evidence cited by the reviewer: `confirm-close-surface`
  defaults to enabled, `close_surface` calls `Surface.close()`, and the macOS
  close path can gate removal behind confirmation.

Fix:

- Added scenario-local `confirm-close-surface = false` to the planned harness
  config and made it part of the pass/fail criteria.

The fixed design was re-reviewed by a fresh-context Codex adversarial subagent.

Final verdict: **Approved**.

The reviewer confirmed the prior Required finding was resolved and reported no
new findings.
