# Experiment 20: Font-Size Cell Metrics

## Description

Experiment 19 proved browser overlay visibility and input routing through
minimize, hide, and restore. The next matrix row is terminal font-size or
cell-metric changes.

This experiment should prove Ghostboard recomputes browser overlay geometry when
the owning terminal pane's cell metrics change. The browser should keep the same
identity, but the terminal grid, overlay frame, AppKit pixel size, Roamium
viewport size, hit-test mapping, and keyboard routing must all reflect the new
cell size after increasing and then decreasing or resetting font size.

This experiment intentionally covers one window with one browser overlay. It
does not test splits, tabs, multiple windows, fullscreen, display moves,
DevTools, scrollback, browser navigation, or final matrix regression.

If current Ghostboard already passes, the experiment should record that and
avoid product source changes. If it fails, the harness must first localize
whether the failure is keybind/action dispatch, terminal cell-size propagation,
TUI overlay grid recalculation, AppKit frame/pixel recomputation, Roamium resize
delivery, stale hit testing, or keyboard routing before any product fix is
designed.

## Changes

Planned files:

- `scripts/ghostboard-geometry-matrix.sh`
  - add a `font-size-cell-metrics` scenario;
  - add deterministic config keybinds for font-size changes, for example:
    `ctrl+equal=increase_font_size:2`, `ctrl+minus=decrease_font_size:2`, and
    optionally `ctrl+0=reset_font_size`;
  - launch one browser in one Ghostboard window using the repo-built `web` and
    Roamium binaries;
  - record the baseline canonical identity tuple:
    `window_id + surface_id + selected_tab_id + pane_id + browser_tab_id`, plus
    `context_id + grid + cell size + AppKit frame + AppKit pixels + backing_scale`;
  - invoke the user-visible font-size increase keybind rather than mutating
    private geometry state;
  - wait for fresh post-increase geometry records after the keybind boundary;
  - require the same canonical identity and context id after the font-size
    increase;
  - require the cell size and grid to change in the expected direction, or
    record the exact observed terminal behavior if Ghostty changes only one of
    those quantities for the fixed window size;
  - require the AppKit overlay frame and AppKit pixel size to be current
    post-increase values derived from the new cell metrics, not stale baseline
    records;
  - require Roamium to receive the post-increase AppKit pixel size through
    `ts_set_view_size` when pixels change;
  - click inside the post-increase overlay and prove hit testing uses the
    post-increase AppKit frame, surface id, selected tab id, context id, and
    web-relative coordinates;
  - enter Browse mode and prove keyboard input reaches the same browser after
    the font-size increase;
  - return to Control mode;
  - invoke the user-visible decrease or reset keybind;
  - wait for fresh post-decrease/reset geometry records after that keybind
    boundary;
  - require the same canonical identity and context id after decrease/reset;
  - require the cell size, grid, AppKit frame, AppKit pixels, and Roamium resize
    evidence to return to baseline or to an explicitly recorded current
    terminal-defined value;
  - re-prove hit testing and Browse-mode keyboard routing after decrease/reset;
  - capture screenshots before the font-size change, after increase, and after
    decrease/reset;
  - fail if assertions accept baseline records as post-increase proof or
    post-increase records as post-decrease/reset proof.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - change only if runtime evidence proves font/cell-size changes do not
    recompute native overlay frame/pixels or hit-test coordinates.
- `ghostboard/macos/Sources/Ghostty/Ghostty.App.swift`
  - change only if runtime evidence proves the existing font-size or cell-size
    action path does not notify the surface view correctly.
- `ghostboard/src/apprt/termsurf.zig`
  - change only if runtime evidence proves TermSurf resize/update messages omit
    data needed to track cell-metric changes.
- `roamium/src/dispatch.rs`
  - change only if existing trace evidence cannot prove resize/focus/key input
    after cell-metric changes. Any such change must be trace-only under the
    existing trace mechanism.
- `issues/0809-ghostboard-viewport-geometry/20-font-size-cell-metrics.md`
  - record the design review, implementation, verification, completion review,
    result, and conclusion.
- `issues/0809-ghostboard-viewport-geometry/README.md`
  - add Experiment 20 to the experiment index.

Reference files:

- `scripts/ghostboard-geometry-matrix.sh`
- `scripts/ghostty-app/inject.swift`
- `issues/0809-ghostboard-viewport-geometry/03-window-resize-follow.md`
- `issues/0809-ghostboard-viewport-geometry/18-fullscreen-unfullscreen.md`
- `issues/0809-ghostboard-viewport-geometry/19-minimize-hide-restore.md`
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
- `ghostboard/macos/Sources/Ghostty/Ghostty.App.swift`
- `ghostboard/src/apprt/termsurf.zig`
- `roamium/src/dispatch.rs`

## Verification

Pass criteria:

- Markdown is formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0809-ghostboard-viewport-geometry/README.md \
    issues/0809-ghostboard-viewport-geometry/20-font-size-cell-metrics.md
  ```

- Shell syntax is valid:

  ```bash
  bash -n scripts/ghostboard-geometry-matrix.sh
  ```

- If Swift files are changed:

  ```bash
  cd ghostboard
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
  cargo fmt
  cargo check -p roamium
  ```

- The new scenario passes:

  ```bash
  scripts/ghostboard-geometry-matrix.sh font-size-cell-metrics
  ```

- The passing run proves:
  - font size changes are invoked through user-visible keybind/action paths;
  - the browser keeps the same window id, surface id, selected tab id, pane id,
    browser tab id, and context id after increase and after decrease/reset;
  - fresh post-increase and post-decrease/reset geometry records exist after
    their respective keybind boundaries;
  - cell size and grid evidence reflects the terminal's current metrics after
    each change;
  - AppKit frame, AppKit pixels, and backing scale evidence is current and is
    not stale baseline or previous-phase evidence;
  - Roamium receives the current AppKit pixel size via `ts_set_view_size` when
    pixels change;
  - mouse hit-testing and Browse-mode keyboard input still route to the browser
    after increase and after decrease/reset;
  - screenshots show baseline, increased font size, and restored/decreased font
    size states.
- Adjacent geometry regressions still pass:

  ```bash
  scripts/ghostboard-geometry-matrix.sh window-resize
  scripts/ghostboard-geometry-matrix.sh minimize-hide-restore
  ```

- `git diff --check` passes.
- The design review is recorded in this experiment file and the plan is
  committed before implementation begins.
- After implementation, verification, and result recording, the completion
  review is recorded in this experiment file and the result commit is made
  before designing or implementing Experiment 21.

Fail criteria:

- The harness fakes a cell-metric change by directly mutating private TermSurf
  overlay state instead of invoking a terminal font-size action.
- The browser changes window id, surface id, selected tab id, pane id, browser
  tab id, or context id across font-size changes.
- Current post-change geometry is ambiguous: AppKit frame/pixels/backing scale
  evidence is missing or stale.
- Roamium resize evidence is missing when AppKit pixels change.
- Mouse or keyboard input after either font-size transition reaches the wrong
  browser or no browser.
- The experiment expands into TUI overlay resize commands, scrollback, browser
  navigation, DevTools, or final matrix regression before font/cell metrics are
  isolated.

## Design Review

Fresh-context adversarial review approved the design before implementation.

Verdict: **APPROVED**.

Findings: none.
