# Experiment 6: Split-right divider resize

## Description

Experiments 4 and 5 proved that creating right and down splits keeps the browser
attached to the original pane and resizes the browser overlay to the newly
smaller pane. The next viewport matrix row is resizing an existing split
boundary.

This experiment should start with a browser in the left pane of a split-right
layout, then move the vertical divider with normal Ghostty/Ghostboard keybinding
behavior. It must prove the browser follows the original left pane as that pane
changes width again:

- create a right-side split from the browser-owning pane;
- record the post-split browser frame/pixels as the baseline for divider
  movement;
- invoke a scenario-local `resize_split:right,20` keybinding;
- prove the original browser pane widens relative to the post-split baseline
  while height stays stable;
- prove Zig and Roamium receive the new AppKit-presented pixel size after the
  divider resize;
- prove positive hit testing still works inside the resized browser frame;
- prove a sibling-pane click outside the resized browser frame does not route to
  the original browser.

This experiment intentionally covers only the vertical divider resize in a
split-right layout. Split-down divider resize can be a later experiment if this
one exposes a different geometry or input failure mode.

If current Ghostboard already passes, the experiment should record that and
avoid product source changes. If it fails, the harness must first localize which
invariant failed before any Ghostboard fix is designed in this experiment.

## Changes

Planned files:

- `scripts/ghostboard-geometry-matrix.sh`
  - add a `split-right-resize` scenario;
  - add scenario-local keybindings:
    - `keybind = ctrl+d=new_split:right`;
    - `keybind = ctrl+l=resize_split:right,20`;
  - launch the same repo-built `TermSurf.app`, `target/debug/web`, and Roamium
    trace setup as the existing scenarios;
  - wait for the initial-open AppKit/Zig/Roamium correlation to pass;
  - inject Control-D to create the right split and wait for the same post-split
    proof used by Experiment 4;
  - record the post-split overlay frame, AppKit-presented pixel size, pane id,
    browser tab id, context id, app-log line count, and Roamium trace line count
    as the divider-resize baseline;
  - inject Control-L to move the vertical divider right;
  - wait for a new AppKit presentation record after Control-L for the original
    pane/context whose frame width is larger than the post-split baseline and
    whose height remains equal or within a small documented tolerance;
  - wait for a new AppKit-presented pixel record after Control-L whose pixel
    width is larger than the post-split baseline and whose pixel height remains
    stable;
  - require Zig to record the divider-resized AppKit-presented pixel size after
    the divider-resize phase;
  - require Roamium's run-specific trace to contain `ffi=ts_set_view_size` after
    the divider-resize trace boundary with the divider-resized AppKit-presented
    pixel size for the original pane id and browser tab id;
  - capture a post-divider-resize screenshot;
  - send deterministic mouse input inside the divider-resized browser frame and
    require a fresh `hit=true` / `web_point` hit-test record after the divider
    resize;
  - send deterministic mouse input in the right sibling pane area, outside the
    divider-resized overlay frame but inside the window/sibling region, and
    require it does not route as a hit to the original browser overlay/context.
- `ghostboard/src/apprt/termsurf.zig`
  - change only if the harness proves divider-resize updates fail;
  - likely candidate fixes should be localized from the geometry logs before
    implementation.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - change only if AppKit does not re-present or report the updated overlay
    frame/pixels for the original pane after divider resize.
- `issues/0809-ghostboard-viewport-geometry/06-split-right-divider-resize.md`
  - record the design, implementation, verification, completion review, result,
    and conclusion.
- `issues/0809-ghostboard-viewport-geometry/README.md`
  - add Experiment 6 to the experiment index.

Reference files:

- `scripts/ghostboard-geometry-matrix.sh`
- `scripts/ghostty-app/inject.swift`
- `ghostboard/src/input/Binding.zig:3378-3388`
- `ghostboard/src/config/Config.zig:6611-6629`
- `ghostboard/macos/Sources/Ghostty/Ghostty.App.swift:1262-1286`
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift:491-614`
- `ghostboard/src/apprt/termsurf.zig:892-944`
- `ghostboard/src/apprt/termsurf.zig:1241-1358`
- `ghostboard-legacy/src/datastruct/split_tree.zig:813-890`
- `issues/0809-ghostboard-viewport-geometry/04-split-right-pane-attachment.md`

## Verification

Pass criteria:

- Markdown is formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0809-ghostboard-viewport-geometry/README.md \
    issues/0809-ghostboard-viewport-geometry/06-split-right-divider-resize.md
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
- Existing scenarios still pass serially:

  ```bash
  scripts/ghostboard-geometry-matrix.sh initial-open
  scripts/ghostboard-geometry-matrix.sh window-resize
  scripts/ghostboard-geometry-matrix.sh split-right
  scripts/ghostboard-geometry-matrix.sh split-down
  ```

- The new scenario passes:

  ```bash
  scripts/ghostboard-geometry-matrix.sh split-right-resize
  ```

- The `split-right-resize` passing run proves all of the following:
  - initial-open still correlates AppKit, Zig, Roamium, screenshot, and hit
    test;
  - the split action is triggered by the scenario-local `ctrl+d` keybinding;
  - the divider-resize action is triggered by the scenario-local `ctrl+l`
    keybinding;
  - after divider resize, AppKit reports a new overlay frame for the original
    pane id and context id;
  - the divider-resized overlay frame width and AppKit-presented pixel width are
    larger than the post-split baseline;
  - the divider-resized overlay frame height and AppKit-presented pixel height
    remain equal to the post-split baseline or within a small documented
    tolerance;
  - Zig records the divider-resized AppKit-presented pixel size for the original
    pane id after the divider-resize phase;
  - Roamium's run-specific trace records `ffi=ts_set_view_size` after the
    divider-resize trace boundary on the same line as the divider-resized
    AppKit-presented pixel size for the original pane id and browser tab id;
  - the post-divider-resize screenshot shows browser content filling only the
    original pane's resized viewport;
  - hit testing inside the divider-resized browser frame reports `hit=true` and
    a current webview-relative coordinate after divider resize;
  - hit testing in the right sibling pane area outside the divider-resized
    overlay frame does not route to the original browser overlay/context.
- `git diff --check` passes.

Fail criteria:

- The harness resizes the split by calling a private Ghostboard API instead of
  exercising user-visible keybinding behavior.
- The test accepts pre-divider-resize AppKit, Zig, Roamium, or hit-test records
  as proof of divider-resize behavior.
- The test proves split creation but not a subsequent divider-induced pane size
  change.
- The browser remains at the post-split size, overlaps the sibling pane, or
  loses hit-test routing after the divider resize.
- The experiment expands into equalize/rebalance, zoom/maximize, pane close,
  tabs, fullscreen, or multi-window behavior before split-right divider resize
  is proven.

## Design Review

The design was reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

The reviewer reported no findings and verified that the README links Experiment
6 as `Designed`, the experiment has the required sections, no implementation
diff has started beyond README/design docs, and the plan is plausible against
the current `resize_split` parser, keybinding/action path, macOS resize
notification path, split-tree `.right` semantics, and existing geometry harness
evidence model.
