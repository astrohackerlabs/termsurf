# Experiment 4: Split-right pane attachment

## Description

Experiment 3 proved that a browser overlay follows ordinary window grow/shrink
events. The next viewport matrix row is a horizontal pane split:

- start with one pane running `web`;
- create a right-side split from that browser-owning pane;
- prove the browser remains attached to the original pane, which should become
  the left pane;
- prove the browser resizes to the original pane's new narrower viewport instead
  of staying at the pre-split width or moving into the new sibling pane;
- prove mouse hit testing uses the post-split browser frame.

This experiment should extend the existing geometry harness with a `split-right`
scenario. It should use normal Ghostty/Ghostboard user behavior to create the
split, not a private test-only code path. The intended automation is to add a
scenario-local keybinding to the generated config:

```text
keybind = ctrl+d=new_split:right
```

Then the harness can inject Control-D with `scripts/ghostty-app/inject.swift`
after the initial browser-open correlation has passed.

If current Ghostboard already passes, the experiment should record that and
avoid product source changes. If it fails, the harness must first localize which
invariant failed before any Ghostboard fix is designed in this experiment.

## Changes

Planned files:

- `scripts/ghostboard-geometry-matrix.sh`
  - add a `split-right` scenario;
  - for this scenario, add `keybind = ctrl+d=new_split:right` to the generated
    Ghostboard config;
  - launch the same repo-built `TermSurf.app`, `target/debug/web`, and Roamium
    trace setup as `initial-open`;
  - wait for the same initial-open AppKit/Zig/Roamium correlation to pass;
  - record the pre-split identity tuple, pane id, browser tab id, AppKit overlay
    frame, AppKit-presented pixel size, and window bounds;
  - inject Control-D into the focused app window to create a split on the right;
  - wait for a new AppKit presentation record after the key injection whose
    identity still contains the original pane id and browser tab id;
  - require the post-split AppKit overlay frame width and AppKit-presented pixel
    width to be smaller than the pre-split width, while the frame/pixel height
    remains equal or within a small tolerance expected from split-divider or
    terminal-layout rounding;
  - require Zig to record the post-split AppKit-presented pixel size for the
    original pane id after the split phase;
  - require Roamium's run-specific trace to contain `ffi=ts_set_view_size` with
    the post-split AppKit-presented pixel size for the original pane id and
    browser tab id;
  - capture a post-split screenshot;
  - send deterministic mouse input inside the post-split browser frame and
    require a fresh `hit=true` / `web_point` hit-test record after the split;
  - send deterministic mouse input in the right sibling pane area, at a point
    outside the post-split overlay frame but inside the old pre-split browser
    width/window area, and require it does not route as a hit to the original
    browser overlay/context.
- `ghostboard/src/apprt/termsurf.zig`
  - change only if the harness proves the split update path fails;
  - likely candidate fixes include pane-id keyed overlay updates after split
    layout changes, stale resize suppression, or AppKit-presented pixel
    correction after split-induced `SetOverlay` updates.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - change only if AppKit does not re-present or report the updated overlay
    frame/pixels for the original pane after the split.
- `issues/0809-ghostboard-viewport-geometry/04-split-right-pane-attachment.md`
  - record the design, implementation, verification, completion review, result,
    and conclusion.
- `issues/0809-ghostboard-viewport-geometry/README.md`
  - add Experiment 4 to the experiment index.

Reference files:

- `scripts/ghostboard-geometry-matrix.sh`
- `scripts/ghostty-app/inject.swift`
- `ghostboard/src/build/mdgen/ghostty_5_header.md`
- `ghostboard/macos/Sources/Ghostty/Ghostty.App.swift:846-864`
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift:491-614`
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift:2152-2169`
- `ghostboard/src/apprt/termsurf.zig:892-944`
- `ghostboard/src/apprt/termsurf.zig:1241-1358`
- `ghostboard-legacy/src/datastruct/split_tree.zig:505-570`
- `ghostboard-legacy/src/Surface.zig:2492-2515`
- `ghostboard-legacy/src/renderer/generic.zig:849-862`

## Verification

Pass criteria:

- Markdown is formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0809-ghostboard-viewport-geometry/README.md \
    issues/0809-ghostboard-viewport-geometry/04-split-right-pane-attachment.md
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
- Existing scenarios still pass:

  ```bash
  scripts/ghostboard-geometry-matrix.sh initial-open
  scripts/ghostboard-geometry-matrix.sh window-resize
  ```

- The new scenario passes:

  ```bash
  scripts/ghostboard-geometry-matrix.sh split-right
  ```

- The `split-right` passing run proves all of the following:
  - initial-open still correlates AppKit, Zig, Roamium, screenshot, and hit
    test;
  - the split action is triggered by the scenario-local `ctrl+d` keybinding;
  - after the split, AppKit reports a new overlay frame for the original pane id
    and browser tab id;
  - the post-split overlay frame width and AppKit-presented pixel width are
    smaller than the pre-split width;
  - the post-split overlay frame height and AppKit-presented pixel height remain
    equal to the pre-split height or within a small documented tolerance;
  - Zig records the post-split AppKit-presented pixel size for the original pane
    id after the split phase;
  - Roamium's run-specific trace records `ffi=ts_set_view_size` on the same line
    as the post-split AppKit-presented pixel size for the original pane id and
    browser tab id;
  - the post-split screenshot shows the browser content filling only the
    original left pane's resized viewport;
  - hit testing inside the resized browser frame reports `hit=true` and a
    current webview-relative coordinate after the split;
  - hit testing in the right sibling pane area, outside the post-split overlay
    frame but inside the old pre-split browser width/window area, does not route
    to the original browser overlay/context.
- `git diff --check` passes.

Fail criteria:

- The harness creates a split by calling a private Ghostboard API instead of
  exercising user-visible keybinding behavior.
- The test accepts pre-split AppKit, Zig, Roamium, or hit-test records as proof
  of post-split behavior.
- The test proves the window size changed but not that the original
  browser-owning pane changed.
- The browser moves to the new sibling pane, remains at the old full-window
  size, overlaps the sibling pane, or loses hit-test routing after the split.
- The experiment expands into vertical splits, split-boundary dragging, pane
  close, tabs, fullscreen, or multi-window behavior before split-right is
  proven.

## Design Review

The design was reviewed by a fresh-context Codex adversarial subagent.

Initial verdict: **Changes required**.

- Required finding: the original design incorrectly required both post-split
  width and height to shrink. A right-side split should make the original pane
  narrower while leaving height equal or within a small layout tolerance.
- Required finding: the original design made sibling-pane negative hit testing
  conditional. A stale pre-split full-width browser frame could still pass a
  positive click inside the resized browser area, so the design must require a
  negative click in the right sibling area.

Fixes:

- The design now requires post-split frame/pixel width to shrink while
  frame/pixel height remains equal or within a documented tolerance.
- The design now requires a deterministic post-split negative hit test in the
  right sibling pane area, outside the post-split overlay frame but inside the
  old pre-split browser width/window area.

Re-review verdict: **Approved**. The reviewer confirmed both required findings
were resolved and reported no new required findings.

## Result

**Result:** Pass.

The `split-right` scenario is implemented in
`scripts/ghostboard-geometry-matrix.sh`. The harness now:

- accepts `split-right` in addition to `initial-open` and `window-resize`;
- adds the scenario-local config line `keybind = ctrl+d=new_split:right`;
- injects Control-D with `scripts/ghostty-app/inject.swift`;
- waits for a post-split AppKit presentation for the original pane/context;
- verifies that the original browser overlay width shrinks while height remains
  stable;
- verifies that AppKit-presented pixel width shrinks while pixel height remains
  stable;
- verifies that Zig records the post-split AppKit-presented pixel size for the
  original pane;
- verifies that Roamium applies the post-split AppKit pixel size with
  `ffi=ts_set_view_size` after the split key injection;
- captures a post-split screenshot;
- verifies a positive hit test inside the resized browser overlay;
- verifies a negative hit test in the right sibling pane area, proving the old
  full-width browser frame is no longer routing input to the original browser
  context.

No Ghostboard product source changes were needed for this row. Current
Ghostboard already keeps the browser attached to the original pane and resizes
it correctly after a right-side split.

Final passing artifacts:

- split-right app log:
  `logs/ghostboard-geometry-split-right-app-20260617-075653.log`
- split-right harness log:
  `logs/ghostboard-geometry-split-right-harness-20260617-075653.log`
- split-right initial screenshot:
  `logs/ghostboard-geometry-split-right-screenshot-20260617-075653.png`
- split-right post-split screenshot:
  `logs/ghostboard-geometry-split-right-split-screenshot-20260617-075653.png`
- split-right Roamium trace:
  `logs/ghostboard-geometry-split-right-roamium-20260617-075653.log`
- initial-open regression app log:
  `logs/ghostboard-geometry-initial-open-app-20260617-075316.log`
- initial-open regression harness log:
  `logs/ghostboard-geometry-initial-open-harness-20260617-075316.log`
- initial-open regression screenshot:
  `logs/ghostboard-geometry-initial-open-screenshot-20260617-075316.png`
- initial-open regression Roamium trace:
  `logs/ghostboard-geometry-initial-open-roamium-20260617-075316.log`
- window-resize regression app log:
  `logs/ghostboard-geometry-window-resize-app-20260617-075254.log`
- window-resize regression harness log:
  `logs/ghostboard-geometry-window-resize-harness-20260617-075254.log`
- window-resize regression Roamium trace:
  `logs/ghostboard-geometry-window-resize-roamium-20260617-075254.log`

Key passing evidence from the `split-right` run:

- pre-split AppKit overlay frame: `624x272`, AppKit pixel size: `1248x544`;
- post-split AppKit overlay frame: `296x272`, AppKit pixel size: `592x544`;
- the original pane id remained `A59654BE-BAF0-42B7-9068-FC9C476EC3F1`;
- the original browser tab id remained `1`;
- Roamium applied the post-split resize with `ffi=ts_set_view_size`;
- the post-split positive click reported `hit=true` with a current `web_point`;
- the right-sibling negative click did not route to the original browser
  context.

Verification commands run:

```bash
bash -n scripts/ghostboard-geometry-matrix.sh
git diff --check
scripts/ghostboard-geometry-matrix.sh split-right
scripts/ghostboard-geometry-matrix.sh window-resize
scripts/ghostboard-geometry-matrix.sh initial-open
```

The post-split screenshot was visually inspected. It shows the browser filling
only the left pane while the new shell split occupies the right pane.

One attempted parallel regression run launched `initial-open` and
`window-resize` simultaneously. `window-resize` passed, but `initial-open`
failed to observe its click hit-test because GUI focus/input can be stolen when
two TermSurf app windows are automated at once. Re-running `initial-open`
serially passed. Geometry/input matrix scenarios should be run serially unless
the harness is explicitly changed to isolate focus and input per window.

## Conclusion

The horizontal split-right matrix row passes in current Ghostboard. The browser
stays attached to its original pane, resizes to the left pane's narrower
viewport, forwards the corrected size to Roamium, and stops accepting input in
the new right sibling pane.

## Completion Review

The completed experiment was reviewed by a fresh-context Codex adversarial
subagent.

Initial verdict: **Changes required**.

- Required finding: the post-split Roamium resize assertion used
  `require_trace`, which searched the whole Roamium trace and could accept stale
  pre-split trace evidence if the same pixel size appeared earlier.

Fix:

- The harness now records the Roamium trace line count before injecting the
  split keybinding and uses `require_trace_after` for the post-split Roamium
  resize assertion.

Re-review verdict: **Approved**. The reviewer confirmed the stale Roamium trace
finding was resolved, verified `bash -n scripts/ghostboard-geometry-matrix.sh`
and `git diff --check`, and reported no new required findings.
