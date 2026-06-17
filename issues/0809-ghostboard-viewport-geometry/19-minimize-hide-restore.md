# Experiment 19: Minimize, Hide, and Restore

## Description

Experiment 18 proved browser overlay geometry and input routing through native
fullscreen and unfullscreen transitions. The next matrix row is minimize, hide,
and restore.

This experiment should prove a browser overlay follows the owning native window
visibility state: hidden/minimized windows should not leave an interactive
browser overlay behind, and restoring the window should bring back the same
browser identity, geometry, hit-testing, and keyboard routing.

This experiment intentionally covers one window with one browser overlay. It
does not test fullscreen, display moves, split panes, multiple windows,
DevTools, or final matrix regression.

If current Ghostboard already passes, the experiment should record that and
avoid product source changes. If it fails, the harness must first localize
whether the failure is minimize action dispatch, app hide/show behavior,
on-screen window detection, AppKit overlay visibility, stale hit testing, or
keyboard routing before any product fix is designed.

## Changes

Planned files:

- `scripts/ghostboard-geometry-matrix.sh`
  - add a `minimize-hide-restore` scenario;
  - add generated Swift helpers, or extend existing helpers, to:
    - minimize and deminimize the target native window through accessibility;
    - hide and unhide/activate the app through public AppKit APIs;
    - read whether the target window is minimized and whether it is visible in
      the CG window list;
  - launch one browser in one Ghostboard window using the repo-built `web` and
    Roamium binaries;
  - record the baseline canonical identity tuple:
    `window_id + surface_id + selected_tab_id + pane_id + browser_tab_id`, plus
    `context_id + AppKit frame + AppKit pixels + backing_scale`;
  - minimize the native window through the public/accessibility path;
  - independently prove the window is minimized or no longer present as an
    onscreen layer-0 CG window;
  - fail if a click at the former browser overlay area produces a fresh hit-test
    routed to the hidden/minimized browser context;
  - restore/deminimize the window through the public/accessibility path;
  - wait for current AppKit presentation/pixel records after restore;
  - require the same canonical browser identity, or if macOS changes the native
    CG window id, record the current window id and prove AppKit/hit-test
    evidence is tied to that current id without accepting stale records;
  - prove restored hit testing uses the restored AppKit frame, surface id,
    selected tab id, context id, and web-relative coordinates;
  - enter Browse mode and prove keyboard input reaches the same browser after
    minimize/restore;
  - return to Control mode;
  - hide the app through a public AppKit path;
  - independently prove the target window is not visible in the onscreen CG
    window list while hidden;
  - fail if a click at the former browser overlay area produces a fresh hit-test
    routed to the hidden browser context;
  - activate/unhide the app through a public AppKit path;
  - wait for current AppKit presentation/pixel records after unhide;
  - re-prove canonical identity, current window id behavior, hit testing, and
    Browse-mode keyboard routing;
  - capture screenshots before minimize, after restore, and after unhide;
  - fail if assertions accept baseline records as restore/unhide proof or accept
    hidden/minimized stale hit-test records.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - change only if runtime evidence proves minimized/hidden windows leave
    browser overlays visible or hit-testable, or fail to restore overlay
    frame/pixels after being shown.
- `ghostboard/macos/Sources/Features/Terminal/TerminalController.swift`
  - change only if runtime evidence proves native minimize/hide/show transitions
    do not update terminal surface/window visibility state.
- `ghostboard/src/apprt/termsurf.zig`
  - change only if runtime evidence proves TermSurf lifecycle or visibility
    messages are incomplete for minimize/hide/show.
- `roamium/src/dispatch.rs`
  - change only if existing trace evidence cannot prove focus/key input after
    restore/unhide. Any such change must be trace-only under the existing trace
    mechanism.
- `issues/0809-ghostboard-viewport-geometry/19-minimize-hide-restore.md`
  - record the design review, implementation, verification, completion review,
    result, and conclusion.
- `issues/0809-ghostboard-viewport-geometry/README.md`
  - add Experiment 19 to the experiment index.

Reference files:

- `scripts/ghostboard-geometry-matrix.sh`
- `scripts/ghostty-app/inject.swift`
- `scripts/ghostty-app/winid.swift`
- `issues/0809-ghostboard-viewport-geometry/15-open-browser-in-new-window.md`
- `issues/0809-ghostboard-viewport-geometry/18-fullscreen-unfullscreen.md`
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
- `ghostboard/macos/Sources/Features/Terminal/TerminalController.swift`
- `ghostboard/src/apprt/termsurf.zig`
- `roamium/src/dispatch.rs`

## Verification

Pass criteria:

- Markdown is formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0809-ghostboard-viewport-geometry/README.md \
    issues/0809-ghostboard-viewport-geometry/19-minimize-hide-restore.md
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
  macos/build.nu --scheme Ghostty --configuration Debug --action build
  ```

- If Rust files are changed:

  ```bash
  cargo fmt
  cargo check -p roamium
  ```

- The new scenario passes:

  ```bash
  scripts/ghostboard-geometry-matrix.sh minimize-hide-restore
  ```

- The passing run proves:
  - minimize is invoked through a public/accessibility path;
  - the window is independently proven minimized or absent from the onscreen CG
    window list;
  - clicking the former browser area while minimized does not create a fresh
    hit-test for the browser context;
  - after deminimize/restore, the browser proves the expected current-window
    identity behavior and keeps the same surface id, selected tab id, pane id,
    browser tab id, and context id;
  - restored AppKit frame/pixels/backing scale are current, not stale baseline
    records;
  - restored mouse hit-testing and Browse-mode keyboard input route to the
    browser;
  - hide is invoked through a public AppKit path;
  - the window is independently proven absent from the onscreen CG window list
    while hidden;
  - clicking the former browser area while hidden does not create a fresh
    hit-test for the browser context;
  - after unhide/activate, the browser again proves current-window identity,
    geometry, hit-testing, and Browse-mode keyboard routing;
  - screenshots show baseline, restored-after-minimize, and restored-after-hide
    states.
- Adjacent geometry regressions still pass:

  ```bash
  scripts/ghostboard-geometry-matrix.sh fullscreen-unfullscreen
  scripts/ghostboard-geometry-matrix.sh open-browser-in-new-window
  ```

- `git diff --check` passes.
- The design review is recorded in this experiment file and the plan is
  committed before implementation begins.
- After implementation, verification, and result recording, the completion
  review is recorded in this experiment file and the result commit is made
  before designing or implementing Experiment 20.

Fail criteria:

- The harness fakes hide/minimize by changing private TermSurf state instead of
  invoking native/public window or app APIs.
- A minimized or hidden browser remains hit-testable from the former visible
  overlay area.
- Restore/unhide changes surface id, selected tab id, pane id, browser tab id,
  or context id.
- Current window id behavior is ambiguous: post-restore/unhide evidence is not
  tied either to the preserved window id or to an explicitly recorded current
  replacement window id.
- AppKit frame/pixels/backing scale evidence is missing or stale after restore
  or unhide.
- Keyboard input after restore/unhide reaches the wrong browser or no browser.
- The experiment expands into fullscreen, display moves, DevTools, split panes,
  multiple windows, or final matrix regression before minimize/hide/restore
  behavior is isolated.

## Design Review

Fresh-context adversarial review approved the design before implementation.

Verdict: **APPROVED**.

Findings: none.
