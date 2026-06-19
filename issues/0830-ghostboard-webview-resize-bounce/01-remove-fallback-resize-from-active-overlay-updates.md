# Experiment 1: Remove Fallback Resize from Active Overlay Updates

## Description

Prevent Ghostboard from sending a fallback-size browser `Resize` during active
overlay geometry updates.

The current path sends one resize based on `fallback_cell_width = 10` and
`fallback_cell_height = 20`, then sends a second corrective resize after AppKit
reports the real CALayerHost pixel size. This experiment will make AppKit's
presented-pixel callback the resize authority for existing active overlays, so
the browser should see only the real target size during pane resize.

The experiment intentionally keeps the scope narrow. It should not change
Roamium, Chromium, webtui, or the protocol.

## Changes

Planned code changes:

1. Update `ghostboard/src/apprt/termsurf.zig`.

   - Stop sending `snapshotResize` from the existing-pane `handleSetOverlay`
     update path when that snapshot would be based only on fallback cell
     constants.
   - Keep `presentOverlay` in the existing-pane path so Swift updates the
     CALayerHost frame immediately.
   - Keep `overlayPresentedPixels` sending a browser `Resize` when AppKit
     reports a non-zero pixel size that differs from the last sent resize.
   - Leave the existing devtools-overlay fallback resize path unchanged in this
     experiment. Current `handleSetDevtoolsOverlay` does not call
     `snapshotOverlay`/`presentOverlay` for existing-pane updates, so removing
     its fallback resize would risk no replacement AppKit-presented-pixel
     callback. Devtools can be addressed in a later experiment after adding or
     confirming an equivalent present-overlay update path.
   - Preserve create-tab dimensions for initial browser creation. If initial
     creation still depends on fallback dimensions, document that separately and
     do not block the active-resize fix on replacing initial open sizing.

2. Add targeted logging or test support only if needed.

   - Prefer using existing `TERMSURF_GEOMETRY_TRACE` records.
   - Avoid broad new logging unless the existing logs cannot prove the resize
     sequence.

3. Do not change Chromium or Roamium.

## Verification

Static verification:

```bash
zig fmt ghostboard/src/apprt/termsurf.zig
git diff --check
```

Build verification:

```bash
cd ghostboard
zig build
```

Automated geometry verification:

```bash
scripts/ghostboard-geometry-matrix.sh window-resize
scripts/ghostboard-geometry-matrix.sh split-right
scripts/ghostboard-geometry-matrix.sh split-down
```

Pass criteria:

- Existing geometry scenarios still pass.
- AppKit still reports presented pixels after window and split resizes.
- Roamium still receives `ts_set_view_size` for the AppKit-presented pixel
  dimensions.
- Logs do not show an extra `Resize` for the same pane/tab using
  `width * 10`/`height * 20` fallback dimensions immediately before the AppKit
  pixel resize during active pane resize.
- Devtools behavior is not changed by this experiment.

Manual verification:

1. Build and run Ghostboard from the repo.
2. Open a browser pane with `web https://example.com`.
3. Resize the containing pane repeatedly:
   - resize the whole window;
   - resize a split divider horizontally;
   - resize a split divider vertically if available.
4. Watch the webview during each resize.

Manual pass criteria:

- The webview resizes smoothly to the pane's new size.
- It does not visibly shrink to a small/default size and then expand back.
- Browser content remains aligned with the pane after resizing.
- Browser input still works after resizing.

If the automated checks pass but manual verification still shows a bounce, the
result should be **Partial** and the next experiment should capture a geometry
trace from the manual reproduction.

## Design Review

Fresh-context adversarial review returned **CHANGES REQUIRED**.

Required finding:

- The original design said to apply the same fallback-resize removal rule to
  devtools overlays, but current `handleSetDevtoolsOverlay` does not call
  `snapshotOverlay`/`presentOverlay` for existing-pane updates. Removing its
  fallback resize could therefore leave no AppKit-presented-pixel callback to
  send the replacement browser resize.

Fix applied:

- Narrowed Experiment 1 to normal browser overlay updates.
- Explicitly left devtools fallback resize behavior unchanged in this
  experiment.
- Removed `devtools-split-geometry` from the automated verification list.
- Added a pass criterion that devtools behavior is not changed.

Re-review verdict: **APPROVED**.

The reviewer confirmed that the prior Required finding is resolved and no new
Required findings were introduced.

## Result

**Result:** Partial

Implemented the normal browser-overlay part of the experiment. The existing-pane
`handleSetOverlay` update path now updates Ghostboard state and calls
`presentOverlay`, but it no longer sends the fallback `snapshotResize` browser
resize before AppKit has reported the real presented pixel size. Devtools
overlay handling was intentionally left unchanged.

Verification performed:

```bash
zig fmt ghostboard/src/apprt/termsurf.zig
git diff --check
cd ghostboard && zig build
scripts/ghostboard-geometry-matrix.sh window-resize
scripts/ghostboard-geometry-matrix.sh split-right
scripts/ghostboard-geometry-matrix.sh split-down
```

Results:

- `zig fmt ghostboard/src/apprt/termsurf.zig` passed.
- `git diff --check` passed.
- `cd ghostboard && zig build` passed.
- `scripts/ghostboard-geometry-matrix.sh window-resize` passed with logs at
  `logs/ghostboard-geometry-window-resize-app-20260619-181221.log` and
  `logs/ghostboard-geometry-window-resize-roamium-20260619-181221.log`.
- `scripts/ghostboard-geometry-matrix.sh split-right` passed with logs at
  `logs/ghostboard-geometry-split-right-app-20260619-181240.log` and
  `logs/ghostboard-geometry-split-right-roamium-20260619-181240.log`.
- `scripts/ghostboard-geometry-matrix.sh split-down` passed with logs at
  `logs/ghostboard-geometry-split-down-app-20260619-181422.log` and
  `logs/ghostboard-geometry-split-down-roamium-20260619-181422.log`.

The sequential geometry logs show the intended active-update sequence:

- `set_overlay_update`
- `present_overlay_call`
- AppKit `presented_pixels`
- Zig `appkit_presented_pixels`
- Zig `appkit_corrective_resize`
- Zig `resize` with the AppKit pixel size

For split-right, the active update changed from grid `87x48+1+1` to AppKit
pixels `1392x1632`; there was no intervening browser `Resize` for the fallback
grid-derived `870x960` size. For split-down, the active update changed from grid
`177x21+1+1` to AppKit pixels `2832x714`; there was no intervening browser
`Resize` for the fallback grid-derived `1770x420` size. For window shrink, the
active update changed from grid `177x48+1+1` to AppKit pixels `2832x1632`; there
was no intervening browser `Resize` for the fallback grid-derived `1770x960`
size.

An initial parallel run of the three geometry scenarios produced pointer
hit-test failures in the window-resize and split-right harnesses before those
runs reached their resize assertions. Re-running the scenarios sequentially
passed, so the recorded verification uses the sequential runs.

Manual visual verification is still pending. The primary bug is a short-lived
visual bounce, so this experiment should stay Partial until a human confirms
that resizing an active browser pane no longer visibly shrinks the webview to a
small/default size before returning to the pane size.

## Conclusion

The likely code-level cause has been removed from normal browser overlay
updates, and the automated geometry matrix still proves AppKit pixel resize
delivery for window, horizontal split, and vertical split changes. If manual
testing still shows a visible bounce, the next experiment should capture a
geometry trace from the manual reproduction and look for a remaining resize
source outside the normal `handleSetOverlay` path.

## Completion Review

Fresh-context adversarial completion review returned **APPROVED** with no
findings.

The reviewer independently confirmed:

- the implementation removes only the normal `handleSetOverlay` fallback
  `snapshotResize` send;
- `handleSetDevtoolsOverlay` still keeps `snapshotResize` and `sendResize`;
- existing normal browser overlay updates still call `presentOverlay`;
- `overlayPresentedPixels` still sends corrective browser resizes;
- the README status matches the experiment result;
- no result commit existed before the completion review;
- the recorded logs show the expected
  `set_overlay_update -> present_overlay_call -> presented_pixels -> appkit_corrective_resize -> resize`
  sequence without the old fallback resize dimensions.
