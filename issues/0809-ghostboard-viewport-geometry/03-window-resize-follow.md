# Experiment 3: Window resize follow

## Description

Experiments 1 and 2 established geometry observability and fixed the initial
browser-open viewport size. The next viewport matrix rows are window resize
larger and window resize smaller:

- when the owning window grows, the browser overlay must grow with the terminal
  pane;
- when the owning window shrinks, the browser overlay must shrink or clip
  correctly within the pane;
- hit testing must use the new overlay frame after each resize.

This experiment will extend the reusable harness with a `window-resize`
scenario, test the current behavior, and fix only the window-resize follow path
if the test fails. It must keep using the evidence pattern from Experiment 2:
AppKit-presented pixels are the GUI-side source of truth, and Roamium's
run-specific trace file is the browser-side proof that the resize was applied.

Likely paths involved:

- `webtui/src/main.rs` redraws on terminal `Event::Resize` and sends
  `SetOverlay` when `viewport_rect` changes.
- `ghostboard/src/apprt/termsurf.zig` handles `SetOverlay` updates, sends
  browser `Resize`, and calls `presentOverlay`.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  receives Ghostty/AppKit size changes, presents the overlay frame, reports
  AppKit-presented pixels, and forwards input hit tests.
- `scripts/ghostboard-geometry-matrix.sh` already proves the initial-open
  AppKit/Zig/Roamium correlation and should become the reusable entry point for
  this dynamic resize row.

If current Ghostboard already passes after Experiment 2, this experiment should
record that and avoid product code changes. If it fails, the failure must be
localized with the harness before any fix is designed inside this experiment.

## Changes

Planned files:

- `scripts/ghostboard-geometry-matrix.sh`
  - add a `window-resize` scenario;
  - launch the same repo-built `TermSurf.app`, `target/debug/web`, and Roamium
    trace setup as `initial-open`;
  - after the initial-open checks pass, resize the owning app window larger via
    automation targeted at the running process/window;
  - wait for a new stable AppKit `presented` / `presented_pixels` record with a
    larger overlay frame and AppKit pixel size;
  - require Zig to record the new AppKit-presented pixel size and send a resize
    if needed;
  - require Roamium's trace file to contain `ffi=ts_set_view_size` on the same
    line as the larger AppKit-presented pixel size;
  - capture an after-grow screenshot;
  - send deterministic mouse input after the grow resize and require `hit=true`
    with webview-relative coordinates correlated to the grown AppKit frame;
  - resize the same window smaller;
  - wait for a new stable AppKit `presented` / `presented_pixels` record with a
    smaller overlay frame and AppKit pixel size;
  - require Zig and Roamium to converge to the smaller AppKit-presented pixel
    size;
  - capture an after-shrink screenshot;
  - send deterministic mouse input after the shrink resize and require
    `hit=true` with webview-relative coordinates correlated to the shrunken
    AppKit frame.
- `ghostboard/src/apprt/termsurf.zig`
  - change only if the harness proves the resize path fails;
  - likely candidate fixes include improving `SetOverlay` update snapshots,
    avoiding stale fallback resize sends, or reusing the Experiment 2
    AppKit-presented pixel correction for repeated dynamic presentation.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - change only if the harness proves AppKit is not re-presenting/reporting
    updated overlay pixels after window resize.
- `issues/0809-ghostboard-viewport-geometry/03-window-resize-follow.md`
  - record the design, implementation, verification, completion review, result,
    and conclusion.
- `issues/0809-ghostboard-viewport-geometry/README.md`
  - add Experiment 3 to the experiment index.

Reference files:

- `scripts/ghostboard-geometry-matrix.sh`
- `webtui/src/main.rs:535-563`
- `ghostboard/src/apprt/termsurf.zig:892-944`
- `ghostboard/src/apprt/termsurf.zig:1241-1358`
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift:491-614`
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift:672-678`
- `roamium/src/dispatch.rs:174-199`
- `ghostboard-legacy/src/apprt/xpc.zig:280-307`
- `ghostboard-legacy/src/Surface.zig:2492-2515`
- `ghostboard-legacy/src/renderer/generic.zig:849-862`

## Verification

Pass criteria:

- Markdown is formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0809-ghostboard-viewport-geometry/README.md \
    issues/0809-ghostboard-viewport-geometry/03-window-resize-follow.md
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
- The baseline `initial-open` scenario still passes:

  ```bash
  scripts/ghostboard-geometry-matrix.sh initial-open
  ```

- The new scenario passes:

  ```bash
  scripts/ghostboard-geometry-matrix.sh window-resize
  ```

- The `window-resize` passing run proves all of the following:
  - initial-open still correlates AppKit, Zig, Roamium, screenshot, and hit
    test;
  - after growing the window, AppKit reports a larger overlay frame and
    AppKit-presented pixel size than the initial frame;
  - Roamium's run-specific trace records `ffi=ts_set_view_size` on the same line
    as the larger AppKit-presented pixel size;
  - the after-grow screenshot shows browser content filling the resized browser
    viewport;
  - hit testing after the grow resize uses the grown AppKit overlay frame and
    reports `hit=true` with current webview-relative coordinates;
  - after shrinking the window, AppKit reports a smaller overlay frame and
    AppKit-presented pixel size than the grown frame;
  - Roamium's run-specific trace records `ffi=ts_set_view_size` on the same line
    as the smaller AppKit-presented pixel size;
  - the after-shrink screenshot shows browser content fitting the resized
    browser viewport with no stale overflow into surrounding terminal UI;
  - hit testing after the shrink resize uses the shrunken AppKit overlay frame
    and reports `hit=true` with current webview-relative coordinates.
- `git diff --check` passes.

Fail criteria:

- The harness changes only window dimensions without proving the owning pane's
  overlay frame changed.
- The test accepts stale AppKit, Zig, or Roamium records from the initial-open
  phase as resize proof.
- The test relies only on screenshot appearance without Roamium trace proof.
- The fix weakens Experiment 1/2 correlation or initial-open regression
  coverage.
- The experiment expands into split, tab, fullscreen, or multi-window behavior
  before the window-resize rows are proven.

## Design Review

The design was reviewed by a fresh-context Codex adversarial subagent.

Initial verdict: **Changes required**.

- Required finding: the original design only required hit-test proof after the
  final shrink resize, which could miss a broken hit-test frame after the grow
  resize.

Fix:

- The planned harness changes now require deterministic mouse input and
  `hit=true` / webview-relative coordinate evidence after both the grow and
  shrink resize phases, each correlated to the corresponding current AppKit
  overlay frame.

Re-review verdict: **Approved**. The reviewer confirmed the required finding was
resolved and reported no new required findings.
