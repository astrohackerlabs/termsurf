# Experiment 16: Multiple Windows With Browsers

## Description

Experiment 15 proved the first multi-window case: browser A in window A, browser
B in window B, and keyboard/mouse routing between those two native windows. The
next matrix row is broader multiple-window behavior.

This experiment should prove Ghostboard can keep more than one native window
with browser overlays alive at the same time while switching among them. The
minimum useful shape is three native windows, each with exactly one browser
overlay launched through a real repo-built `web --browser ...` command. The
harness should prove that each window shows only its own browser overlay, each
browser has its own identity tuple, and mouse/keyboard input routes only to the
browser in the currently targeted window after cycling through all three
windows.

This experiment intentionally covers one browser overlay per native window. It
does not test multiple browser panes inside one window, split panes across
multiple windows, closing windows, moving windows between displays, fullscreen,
minimize/hide, DevTools, or final matrix regression.

If current Ghostboard already passes, the experiment should record that and
avoid product source changes. If it fails, the harness must first localize
whether the failure is third-window creation, inherited first-run command state,
window identity mapping, overlay visibility gating, stale hit testing, keyboard
focus routing, or Roamium tab/context state before any product fix is designed.

## Changes

Planned files:

- `scripts/ghostboard-geometry-matrix.sh`
  - add a `multiple-windows-with-browsers` scenario;
  - reuse the Experiment 15 window enumeration, window focusing, and
    content-view coordinate conversion helpers;
  - keep the first-run wrapper limited to browser A in window A and fail if
    later windows inherit and run that wrapper;
  - use a scenario-local `ctrl+b=new_window` keybinding;
  - launch browser A in window A from the harness initial command;
  - create window B through the public `new_window` action and type a real
    repo-built `web --browser ... https://example.org` command into window B;
  - create window C through the public `new_window` action and type a real
    repo-built `web --browser ... https://example.net` command into window C;
  - record each browser's canonical identity tuple:
    `window_id + surface_id + selected_tab_id + pane_id + browser_tab_id`, plus
    `context_id + AppKit frame + AppKit pixels`;
  - require window A, window B, and window C to have distinct native window ids;
  - require browser A, browser B, and browser C to have distinct surface ids,
    pane ids, browser tab ids, and context ids;
  - prove each browser's AppKit presented record and Roamium resize trace are
    tied to that browser's own window and pixel size;
  - fail if browser A or browser B is freshly presented as visible under window
    C's window id after browser C opens;
  - capture screenshots for all three windows while all three are alive;
  - cycle input through window C, then window B, then window A, using visible
    non-overlapped overlay points when needed;
  - in each targeted window, prove hit testing uses that browser's own
    `window_id`, `surface_id`, selected tab id, context id, and AppKit frame;
  - enter Browse mode in each targeted browser and prove a keyboard marker
    reaches only that browser's Roamium tab/pane and not the other two;
  - return each browser to Control mode before switching to the next window;
  - fail if assertions accept pre-window, wrong-window, or wrong-browser records
    as proof of the current window.
- `ghostboard/src/apprt/termsurf.zig`
  - change only if runtime evidence proves TermSurf pane/browser state is not
    window-scoped across three live windows.
- `ghostboard/macos/Sources/Features/Terminal/TerminalController.swift`
  - change only if runtime evidence proves third-window creation or focus cannot
    produce independently identifiable terminal surfaces.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - change only if runtime evidence proves native overlay visibility, layer
    lookup, hit testing, or keyboard focus leaks across multiple active windows.
- `roamium/src/dispatch.rs`
  - change only if existing run-specific tracing cannot prove browser C resize,
    focus, and keyboard input. Any such change must be trace-only under the
    existing trace mechanism.
- `issues/0809-ghostboard-viewport-geometry/16-multiple-windows-with-browsers.md`
  - record the design review, implementation, verification, completion review,
    result, and conclusion.
- `issues/0809-ghostboard-viewport-geometry/README.md`
  - add Experiment 16 to the experiment index.

Reference files:

- `scripts/ghostboard-geometry-matrix.sh`
- `scripts/ghostty-app/inject.swift`
- `scripts/ghostty-app/winid.swift`
- `issues/0809-ghostboard-viewport-geometry/15-open-browser-in-new-window.md`
- `issues/0809-ghostboard-viewport-geometry/13-open-browser-in-new-tab.md`
- `ghostboard/src/apprt/termsurf.zig`
- `ghostboard/macos/Sources/Features/Terminal/TerminalController.swift`
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
- `roamium/src/dispatch.rs`

## Verification

Pass criteria:

- Markdown is formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0809-ghostboard-viewport-geometry/README.md \
    issues/0809-ghostboard-viewport-geometry/16-multiple-windows-with-browsers.md
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
  scripts/ghostboard-geometry-matrix.sh multiple-windows-with-browsers
  ```

- Adjacent window and tab regressions still pass serially:

  ```bash
  scripts/ghostboard-geometry-matrix.sh open-browser-in-new-window
  scripts/ghostboard-geometry-matrix.sh open-browser-in-new-tab
  ```

- The passing `multiple-windows-with-browsers` run proves:
  - windows A, B, and C are created through user-visible `new_window` actions
    and real typed `web --browser ...` commands for B and C;
  - windows B and C do not inherit the first-run browser command automatically;
  - browser A, browser B, and browser C have distinct native window ids, surface
    ids, pane ids, browser tab ids, and context ids;
  - each browser appears only in its own window, with AppKit frame, AppKit
    pixels, Roamium resize, screenshot, and hit-test evidence tied to that
    window;
  - targeting window C routes mouse and keyboard input only to browser C;
  - targeting window B after window C routes mouse and keyboard input only to
    browser B;
  - targeting window A after window B routes mouse and keyboard input only to
    browser A;
  - no assertion accepts pre-third-window records, browser A records as browser
    B/C proof, or browser B records as browser C proof.
- `git diff --check` passes.
- The design review is recorded in this experiment file and the plan is
  committed before implementation begins.
- After implementation, verification, and result recording, the completion
  review is recorded in this experiment file and the result commit is made
  before designing or implementing Experiment 17.

Fail criteria:

- The harness creates browser windows through private state mutation instead of
  public new-window actions plus real typed `web` commands.
- A later window inherits the harness initial `web` command automatically.
- Any two browsers reuse a native window id, surface id, pane id, browser tab
  id, or context id.
- Any browser is visible, hit-testable, focused, or keyboard-routable while a
  different window is the active target.
- The harness accepts pre-window AppKit, Zig, Roamium, or hit-test records as
  proof of later-window behavior.
- The experiment expands into closing windows, split panes across windows,
  display moves, fullscreen, minimize/hide, DevTools, or final matrix regression
  before the three-window one-browser-per-window case is proven.

## Design Review

Fresh-context adversarial review requested one design fix before implementation.

Initial verdict: **CHANGES REQUIRED**.

Required finding:

- The planned identity tuple omitted `surface_id`, but the issue defines the
  canonical tuple as
  `window_id + surface_id + selected_tab_id + pane_id + browser_tab_id`.

Fix:

- Added `surface_id` to the recorded browser identity tuple, distinctness
  assertions, per-window hit-test expectations, pass criteria, and fail
  criteria.

Final verdict after re-review: **APPROVED**.

Findings after re-review: none.
