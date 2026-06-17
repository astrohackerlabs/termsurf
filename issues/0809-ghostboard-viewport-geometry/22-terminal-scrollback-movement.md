# Experiment 22: Terminal Scrollback Movement

## Description

Experiment 21 proved that the browser overlay follows a TUI-requested viewport
resize and reset. The next matrix row is terminal scrollback movement.

A browser overlay belongs to the live terminal pane viewport, not to historical
scrollback content. When the terminal is scrolled away from the bottom, the
overlay should remain attached to the pane's current viewport frame and should
not drift with older terminal content, duplicate itself, resize unexpectedly, or
route input through stale coordinates. When the terminal returns to the bottom,
the same browser identity, AppKit frame, AppKit pixels, Roamium size, and input
routing should still hold.

This experiment should isolate one window with one browser overlay. It must
exercise scrollback in the same active terminal screen that owns the browser
overlay. `webtui` currently enters the alternate screen, and Ghostboard's
alternate screen does not have scrollback history, so pre-launch primary-screen
history is not enough by itself. The harness may use pre-launch history only if
surface visible-rect or scrollbar evidence proves that history is actually
scrollable while the browser-owning `webtui` session is active. If that cannot
be proven, the experiment should add a narrow user-visible way to run `webtui`
on the primary screen for this scenario, then create and test real
primary-screen scrollback with the overlay active.

The scrollback movement itself should be invoked through public Ghostboard
scrollback actions and proven with concrete surface visible-rect or scrollbar
evidence. The overlay geometry and input routing must remain live-pane based
during scrollback movement and after returning to the bottom.

If current Ghostboard already passes, the experiment should record that and
avoid product changes. If it fails, the harness must first localize whether the
failure is scrollback action delivery, visible terminal content movement,
overlay frame/pixel recomputation, stale AppKit hit testing, or Roamium
focus/key routing before any product fix is designed.

## Changes

Planned files:

- `scripts/ghostboard-geometry-matrix.sh`
  - add a `terminal-scrollback-movement` scenario;
  - do not treat pre-launch primary-screen output as proof unless runtime
    visible-rect or scrollbar evidence proves it remains scrollable while the
    browser-owning `webtui` session is active;
  - generate real scrollback in the active screen that owns the browser overlay;
    if default `webtui` alternate-screen mode prevents this, run the scenario
    through a narrow user-visible primary-screen `webtui` mode added in this
    experiment;
  - add scenario-local keybinds for public Ghostboard scrollback actions, for
    example `ctrl+u=scroll_page_up`, `ctrl+y=scroll_page_down`, and
    `ctrl+b=scroll_to_bottom`;
  - launch one browser in one Ghostboard window using the repo-built `web` and
    Roamium binaries;
  - record the baseline canonical identity tuple:
    `window_id + surface_id + selected_tab_id + pane_id + browser_tab_id`, plus
    `context_id + grid + cell size + AppKit frame + AppKit pixels + backing_scale`;
  - invoke scrollback movement through the configured keybind, not by changing
    window, pane, split, font, TUI viewport, or private AppKit state;
  - prove scrollback movement actually happened with a concrete current-scroll
    signal: `SurfaceScrollView` `documentVisibleRect`, scrollbar `total`,
    `offset`, `len`, derived visible row, or equivalent surface visible-rect
    evidence;
  - require fresh phase boundaries showing baseline-at-bottom,
    scrolled-back/current-visible-rect-changed, and returned-to-bottom states;
  - wait after the scrollback action and require no stale/different AppKit
    presented frame or presented-pixels records for the browser context;
  - capture a scrolled-back screenshot;
  - click inside the current overlay frame and prove hit testing still uses the
    baseline AppKit frame, context id, surface id, selected tab id, and
    web-relative coordinates;
  - click outside the baseline overlay frame and fail if it routes to the
    browser context;
  - enter Browse mode and prove keyboard input reaches the same browser while
    the terminal is scrolled back;
  - return to Control mode;
  - invoke the scroll-to-bottom path through public keybinds or repeated
    `scroll_page_down` actions;
  - prove the same identity, frame, pixels, Roamium size, hit testing, and
    Browse-mode keyboard routing still hold after returning to the bottom;
  - capture a returned-to-bottom screenshot;
  - fail if assertions accept pre-scroll records as post-scroll proof or
    scrolled-back records as post-bottom proof.
- `webtui/src/main.rs`
  - change only if the default alternate screen prevents exercising terminal
    scrollback in the browser-owning active screen;
  - any change must be a narrow user-visible primary-screen mode, not a hidden
    test hook, and normal alternate-screen behavior must remain the default;
  - if such a Rust change is made, rebuild the `web` binary before running the
    runtime scenario.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceScrollView.swift`
  - change only if existing logs cannot prove concrete surface visible-rect or
    scrollbar movement;
  - any added trace must be narrow, scenario-gated by the existing geometry
    trace environment, and include enough data to prove current visible row/rect
    changes and return-to-bottom freshness.
- `ghostboard/src/apprt/termsurf.zig`
  - change only if runtime evidence proves Ghostboard needs an additional
    geometry or visibility trace to distinguish live-pane viewport geometry from
    scrollback movement.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - change only if runtime evidence proves AppKit hit-test/frame state changes
    incorrectly during valid terminal scrollback movement.
- `issues/0809-ghostboard-viewport-geometry/22-terminal-scrollback-movement.md`
  - record the design review, implementation, verification, completion review,
    result, and conclusion.
- `issues/0809-ghostboard-viewport-geometry/README.md`
  - add Experiment 22 to the experiment index.

Reference files:

- `scripts/ghostboard-geometry-matrix.sh`
- `scripts/ghostty-app/inject.swift`
- `ghostboard/src/input/command.zig`
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceScrollView.swift`
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
- `ghostboard/src/apprt/termsurf.zig`
- `issues/0809-ghostboard-viewport-geometry/21-tui-overlay-resize-command.md`
- `issues/0809-ghostboard-viewport-geometry/20-font-size-cell-metrics.md`

## Verification

Pass criteria:

- Markdown is formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0809-ghostboard-viewport-geometry/README.md \
    issues/0809-ghostboard-viewport-geometry/22-terminal-scrollback-movement.md
  ```

- Shell syntax is valid:

  ```bash
  bash -n scripts/ghostboard-geometry-matrix.sh
  ```

- If Rust files are changed:

  ```bash
  cargo fmt
  cargo check -p webtui
  cargo build -p webtui
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

- The new scenario passes:

  ```bash
  scripts/ghostboard-geometry-matrix.sh terminal-scrollback-movement
  ```

- The passing run proves:
  - real terminal scrollback content exists in the same active screen that owns
    the browser overlay;
  - if pre-launch primary-screen history is used, surface visible-rect or
    scrollbar evidence proves it is actually scrollable while `webtui` and the
    browser overlay are active;
  - scrollback movement is invoked through public Ghostboard scrollback actions;
  - scrollback movement is observed with concrete surface visible-rect or
    scrollbar evidence, including fresh baseline, scrolled-back, and
    returned-to-bottom records;
  - the evidence is not confused with browser page scrolling or with stale
    pre-scroll records;
  - the browser keeps the same window id, surface id, selected tab id, pane id,
    browser tab id, and context id after scrollback movement and after returning
    to the bottom;
  - AppKit frame, AppKit pixels, backing scale, and Roamium view size do not
    drift, resize, or become stale during scrollback movement;
  - mouse hit testing inside the current overlay frame still routes to the
    browser context with web-relative coordinates;
  - mouse hit testing outside the overlay frame does not route to the browser
    context;
  - Browse-mode keyboard input reaches the same browser while scrolled back and
    after returning to the bottom;
  - screenshots show baseline, scrolled-back, and returned-to-bottom states.
- Adjacent geometry regressions still pass:

  ```bash
  scripts/ghostboard-geometry-matrix.sh tui-overlay-resize-command
  scripts/ghostboard-geometry-matrix.sh window-resize
  ```

- `git diff --check` passes.
- The design review is recorded in this experiment file and the plan is
  committed before implementation begins.
- After implementation, verification, and result recording, the completion
  review is recorded in this experiment file and the result commit is made
  before designing or implementing Experiment 23.

Fail criteria:

- The harness fakes scrollback by changing window, pane, split, font-size, TUI
  viewport height, browser page scroll, or private AppKit state.
- The test cannot prove terminal scrollback movement actually happened.
- The test relies on pre-launch primary-screen history while `webtui` is on an
  alternate screen, without visible-rect or scrollbar evidence proving that
  history is scrollable while the overlay is active.
- The browser changes window id, surface id, selected tab id, pane id, browser
  tab id, or context id across scrollback movement.
- AppKit frame, AppKit pixels, backing scale, or Roamium size drift during
  scrollback movement.
- Mouse or keyboard input while scrolled back reaches the wrong browser, no
  browser, or stale coordinates.
- The experiment expands into browser navigation, DevTools, tab/window switch,
  or final matrix regression before scrollback movement is isolated.

## Design Review

Fresh-context adversarial design review initially returned **CHANGES REQUIRED**.

Required findings:

- The design suggested creating scrollback by printing lines before `exec`ing
  `webtui`, but `webtui` enters the alternate screen and Ghostboard's alternate
  screen does not have scrollback history. The design could therefore test the
  wrong screen unless it proved pre-launch primary history remained scrollable
  while the browser-owning `webtui` session was active.
- The design did not require the issue README's concrete surface visible-rect
  evidence. It allowed ambiguous "scrollback logs" instead of requiring a
  current-scroll signal with fresh scrolled-back and returned-to-bottom phase
  boundaries.

Fixes:

- The design now requires scrollback in the same active screen that owns the
  overlay, rejects pre-launch history unless visible-rect or scrollbar evidence
  proves it is scrollable while `webtui` is active, and allows a narrow
  user-visible primary-screen `webtui` mode if that is the only way to exercise
  the matrix row.
- The design now requires concrete surface visible-rect or scrollbar evidence,
  including `documentVisibleRect`, scrollbar `total`, `offset`, `len`, derived
  visible row, or equivalent evidence with fresh baseline, scrolled-back, and
  returned-to-bottom boundaries.

Fresh-context adversarial re-review approved the design before implementation.

Verdict: **APPROVED**.

Findings: none.
