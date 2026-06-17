# Experiment 8: Split-right zoom restore

## Description

Experiment 7 proved that a browser attached to the left pane of a split-right
layout follows equalize/rebalance after the divider has been moved. The next
viewport matrix row is zooming or maximizing a pane, then restoring the original
split layout.

This experiment should start with a browser in the left pane of a split-right
layout. It must toggle split zoom through normal Ghostty/Ghostboard keybinding
behavior, prove the browser expands with the zoomed pane, toggle zoom again, and
prove the browser returns to the original split-pane geometry:

- open a browser in a single pane;
- create a right-side split from the browser-owning pane;
- record the post-split frame and AppKit-presented pixels as the restore
  baseline;
- invoke a scenario-local `toggle_split_zoom` keybinding;
- prove the original browser pane expands back to the single-pane viewport size
  while zoomed;
- prove Zig and Roamium receive the zoomed AppKit-presented pixel size after the
  zoom action;
- prove positive hit testing still works inside the zoomed browser frame;
- invoke the same `toggle_split_zoom` keybinding again to unzoom;
- prove the original browser pane returns to the post-split restore baseline
  within a small documented tolerance;
- prove Zig and Roamium receive the restored AppKit-presented pixel size after
  the unzoom action;
- prove a sibling-pane click outside the restored browser frame does not route
  to the original browser.

This experiment intentionally covers only zooming and unzooming the
browser-owning pane in a two-pane split-right layout. Pane close, tab, window,
fullscreen, minimize, display-scale, and multi-window behavior remain later
matrix rows.

If current Ghostboard already passes, the experiment should record that and
avoid product source changes. If it fails, the harness must first localize which
invariant failed before any Ghostboard fix is designed in this experiment.

## Changes

Planned files:

- `scripts/ghostboard-geometry-matrix.sh`
  - add a `split-right-zoom` scenario;
  - add scenario-local keybindings:
    - `keybind = ctrl+d=new_split:right`;
    - `keybind = ctrl+z=toggle_split_zoom`;
  - launch the same repo-built `TermSurf.app`, `target/debug/web`, and Roamium
    trace setup as the existing scenarios;
  - wait for the initial-open AppKit/Zig/Roamium correlation to pass;
  - inject Control-D to create the right split and wait for the same post-split
    proof used by Experiments 4, 6, and 7;
  - record the post-split frame, AppKit-presented pixel size, pane id, browser
    tab id, context id, app-log line count, and Roamium trace line count as the
    restore baseline;
  - inject Control-Z to invoke `toggle_split_zoom`;
  - wait for a new AppKit presentation record after Control-Z for the original
    pane/context whose frame width is larger than the split baseline and returns
    to the initial single-pane frame width within a documented tolerance;
  - wait for a new AppKit-presented pixel record after Control-Z whose pixel
    width is larger than the split baseline and returns to the initial
    single-pane AppKit pixel width within a documented tolerance;
  - require Zig to record the zoomed AppKit-presented pixel size after the zoom
    phase;
  - require Roamium's run-specific trace to contain `ffi=ts_set_view_size` after
    the zoom trace boundary with the zoomed AppKit-presented pixel size for the
    original pane id and browser tab id;
  - capture a post-zoom screenshot;
  - send deterministic mouse input inside the zoomed browser frame and require a
    fresh `hit=true` / `web_point` hit-test record after zoom;
  - inject Control-Z again to unzoom;
  - wait for a new AppKit presentation record after the second Control-Z whose
    frame returns to the post-split restore baseline within tolerance;
  - wait for a new AppKit-presented pixel record after the second Control-Z
    whose pixel size returns to the post-split restore baseline within
    tolerance;
  - require Zig and Roamium to record the restored AppKit-presented pixel size
    after the unzoom phase;
  - capture a post-unzoom screenshot;
  - send deterministic mouse input inside the restored browser frame and require
    a fresh positive hit-test record;
  - send deterministic mouse input in the right sibling pane area, outside the
    restored overlay frame but inside the window/sibling region, and require it
    does not route as a hit to the original browser overlay/context.
- `ghostboard/src/apprt/termsurf.zig`
  - change only if the harness proves zoom/unzoom updates fail;
  - likely candidate fixes should be localized from the geometry logs before
    implementation.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - change only if AppKit does not re-present or report the updated overlay
    frame/pixels for the original pane after zoom or unzoom.
- `issues/0809-ghostboard-viewport-geometry/08-split-right-zoom-restore.md`
  - record the design, implementation, verification, completion review, result,
    and conclusion.
- `issues/0809-ghostboard-viewport-geometry/README.md`
  - add Experiment 8 to the experiment index.

Reference files:

- `scripts/ghostboard-geometry-matrix.sh`
- `scripts/ghostty-app/inject.swift`
- `ghostboard/src/input/Binding.zig:616-619`
- `ghostboard/src/input/Binding.zig:1389`
- `ghostboard/src/config/Config.zig:6778-6784`
- `ghostboard/src/apprt/action.zig:151-153`
- `ghostboard/macos/Sources/Ghostty/Ghostty.App.swift:1317-1337`
- `ghostboard/macos/Sources/Features/Terminal/BaseTerminalController.swift:663-682`
- `ghostboard/macos/Sources/Features/Splits/SplitTree.swift`
- `ghostboard/macos/Sources/Features/Splits/TerminalSplitTreeView.swift:33`
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift:491-614`
- `ghostboard/src/apprt/termsurf.zig:892-944`
- `ghostboard/src/apprt/termsurf.zig:1241-1358`
- `issues/0809-ghostboard-viewport-geometry/07-split-right-equalize-rebalance.md`

## Verification

Pass criteria:

- Markdown is formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0809-ghostboard-viewport-geometry/README.md \
    issues/0809-ghostboard-viewport-geometry/08-split-right-zoom-restore.md
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
  ```

- The new scenario passes:

  ```bash
  scripts/ghostboard-geometry-matrix.sh split-right-zoom
  ```

- The `split-right-zoom` passing run proves all of the following:
  - initial-open still correlates AppKit, Zig, Roamium, screenshot, and hit
    test;
  - the split action is triggered by the scenario-local `ctrl+d` keybinding;
  - the zoom action is triggered by the scenario-local `ctrl+z` keybinding;
  - after zoom, AppKit reports a new overlay frame for the original pane id and
    context id;
  - the zoomed overlay frame and AppKit-presented pixel width grow from the
    split baseline and return to the initial single-pane width within a small
    documented tolerance;
  - Zig records the zoomed AppKit-presented pixel size for the original pane id
    after the zoom phase;
  - Roamium's run-specific trace records `ffi=ts_set_view_size` after the zoom
    trace boundary on the same line as the zoomed AppKit-presented pixel size
    for the original pane id and browser tab id;
  - the post-zoom screenshot shows browser content filling the zoomed pane;
  - hit testing inside the zoomed browser frame reports `hit=true` and a current
    webview-relative coordinate after zoom;
  - the unzoom action is triggered by the second scenario-local `ctrl+z`
    keybinding;
  - after unzoom, AppKit reports a new overlay frame for the original pane id
    and context id;
  - the restored overlay frame and AppKit-presented pixel size return to the
    post-split baseline within a small documented tolerance;
  - Zig and Roamium record the restored AppKit-presented pixel size after the
    unzoom phase;
  - the post-unzoom screenshot shows browser content filling only the original
    pane's restored viewport;
  - hit testing inside the restored browser frame reports `hit=true` after
    unzoom;
  - hit testing in the right sibling pane area outside the restored overlay
    frame does not route to the original browser overlay/context.
- `git diff --check` passes.

Fail criteria:

- The harness toggles zoom by calling a private Ghostboard API instead of
  exercising user-visible keybinding behavior.
- The test accepts pre-zoom AppKit, Zig, Roamium, or hit-test records as proof
  of zoom behavior.
- The test proves zoom but not restoration after unzoom.
- The browser remains at the split baseline after zoom, remains at zoomed size
  after unzoom, overlaps the sibling pane after restore, or loses hit-test
  routing after either transition.
- The experiment expands into pane close, tabs, fullscreen, or multi-window
  behavior before split-right zoom/unzoom is proven.

## Design Review

The design was reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

The reviewer reported no findings.

## Result

**Result:** Pass.

The `split-right-zoom` scenario is implemented in
`scripts/ghostboard-geometry-matrix.sh`. The harness now:

- accepts `split-right-zoom` in addition to the prior scenarios;
- adds scenario-local keybindings:
  - `keybind = ctrl+d=new_split:right`;
  - `keybind = ctrl+z=toggle_split_zoom`;
- creates a right-side split from the browser-owning pane;
- records the post-split frame/pixels as the restore baseline;
- focuses the browser-owning pane with real mouse input before invoking zoom;
- toggles split zoom through Control-Z;
- verifies that AppKit, Zig, and Roamium all resize the original browser pane
  back to the full single-pane geometry;
- verifies a positive hit test inside the zoomed browser overlay;
- toggles split zoom again through Control-Z;
- verifies that AppKit, Zig, and Roamium all resize the original browser pane
  back to the split baseline;
- captures post-zoom and post-unzoom screenshots;
- verifies a positive hit test inside the restored browser overlay;
- verifies the right sibling pane area does not route input to the original
  browser context after unzoom.

This experiment required a small Ghostboard product fix in
`ghostboard/src/apprt/termsurf.zig`. The first passing zoom attempt exposed that
unzoom restored the AppKit and Zig geometry to `912x986`, but Roamium only
received the logical fallback resize `570x580`. The root cause was that
`overlayPresentedPixels` used the stale CA-context dimensions
`ca_pixel_width/ca_pixel_height` as proof that the browser was already sized to
the AppKit-presented pixel size. Those CA-context dimensions are the size
reported when the context was created; after pane layout changes, the browser's
actual size is the last resize sent to Roamium. The fix makes corrective-resize
deduplication rely on `last_resize_pixel_width/last_resize_pixel_height`
instead.

The scenario also learned that `new_split:right` leaves focus on the newly
created sibling pane. To make `toggle_split_zoom` target the browser-owning pane
instead of the sibling pane, the harness focuses the browser overlay with a real
mouse click after split creation and before Control-Z.

Final passing artifacts:

- split-right-zoom app log:
  `logs/ghostboard-geometry-split-right-zoom-app-20260617-085230.log`
- split-right-zoom harness log:
  `logs/ghostboard-geometry-split-right-zoom-harness-20260617-085230.log`
- split-right-zoom initial screenshot:
  `logs/ghostboard-geometry-split-right-zoom-screenshot-20260617-085230.png`
- split-right-zoom post-zoom screenshot:
  `logs/ghostboard-geometry-split-right-zoom-zoom-screenshot-20260617-085230.png`
- split-right-zoom post-unzoom screenshot:
  `logs/ghostboard-geometry-split-right-zoom-unzoom-screenshot-20260617-085230.png`
- split-right-zoom Roamium trace:
  `logs/ghostboard-geometry-split-right-zoom-roamium-20260617-085230.log`
- initial-open regression app log:
  `logs/ghostboard-geometry-initial-open-app-20260617-085253.log`
- initial-open regression harness log:
  `logs/ghostboard-geometry-initial-open-harness-20260617-085253.log`
- split-right regression app log:
  `logs/ghostboard-geometry-split-right-app-20260617-085259.log`
- split-right regression harness log:
  `logs/ghostboard-geometry-split-right-harness-20260617-085259.log`
- split-right-resize regression app log:
  `logs/ghostboard-geometry-split-right-resize-app-20260617-085340.log`
- split-right-resize regression harness log:
  `logs/ghostboard-geometry-split-right-resize-harness-20260617-085340.log`
- split-right-equalize regression app log:
  `logs/ghostboard-geometry-split-right-equalize-app-20260617-085423.log`
- split-right-equalize regression harness log:
  `logs/ghostboard-geometry-split-right-equalize-harness-20260617-085423.log`
- window-resize regression app log:
  `logs/ghostboard-geometry-window-resize-app-20260617-085514.log`
- window-resize regression harness log:
  `logs/ghostboard-geometry-window-resize-harness-20260617-085514.log`
- split-down regression app log:
  `logs/ghostboard-geometry-split-down-app-20260617-085526.log`
- split-down regression harness log:
  `logs/ghostboard-geometry-split-down-harness-20260617-085526.log`

Key runtime evidence from the passing `split-right-zoom` run:

- initial frame: `944x493`, AppKit pixel size `1888x986`;
- post-split restore baseline: `456x493`, AppKit pixel size `912x986`;
- post-zoom frame: `944x493`, AppKit pixel size `1888x986`;
- post-unzoom frame: `456x493`, AppKit pixel size `912x986`;
- pane id: `252677D9-C383-4202-A201-B2961E35F923`;
- browser tab id: `1`;
- context id: `522695247`;
- browser-pane focus point before zoom: `276,414`;
- positive post-zoom hit point: `520,414`;
- positive post-unzoom hit point: `276,414`;
- sibling negative post-unzoom point: `756,414`.

Verification commands run:

```bash
cd ghostboard
zig fmt src/apprt/termsurf.zig
zig build -Demit-macos-app=false
macos/build.nu --scheme Ghostty --configuration Debug --action build
cd ..
bash -n scripts/ghostboard-geometry-matrix.sh
git diff --check
scripts/ghostboard-geometry-matrix.sh split-right-zoom
scripts/ghostboard-geometry-matrix.sh initial-open
scripts/ghostboard-geometry-matrix.sh split-right
scripts/ghostboard-geometry-matrix.sh split-right-resize
scripts/ghostboard-geometry-matrix.sh split-right-equalize
scripts/ghostboard-geometry-matrix.sh window-resize
scripts/ghostboard-geometry-matrix.sh split-down
```

The required adjacent regression sweep was run serially, and `window-resize` and
`split-down` were also run because the product fix touched shared resize
bookkeeping.

## Completion Review

The completed experiment was reviewed by a fresh-context Codex adversarial
subagent.

Verdict: **Approved**.

The reviewer reported no findings.

## Conclusion

The zoom/maximize matrix row passes for a two-pane split-right layout. A browser
in the original left pane expands with the zoomed pane, restores to the split
baseline after unzoom, and keeps AppKit, Zig, Roamium, screenshot, and input-hit
evidence aligned through both transitions.

The next experiment should move to the next untested matrix row, most likely
closing a sibling pane and proving the browser expands with the remaining owning
pane.
