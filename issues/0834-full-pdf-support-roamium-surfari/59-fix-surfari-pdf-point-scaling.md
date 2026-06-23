# Experiment 59: Fix Surfari PDF Point Scaling

## Description

Experiment 58 failed to obtain WebKit-internal PDF selection trace records, but
it exposed a more direct Surfari integration hypothesis: Surfari receives
browser geometry from Ghostboard in pixels, while its hidden `WKWebView` and
synthetic AppKit mouse events operate in points.

The current Surfari code path appears to apply pixel-space values directly as
AppKit point-space values:

- `ts_set_view_size` assigns `width` and `height` directly to an `NSWindow`
  frame and `WKWebView` frame;
- `ts_forward_mouse_event` and `ts_forward_mouse_move` pass integer `x`/`y`
  values through `eventLocationInWindow` into `NSEvent.locationInWindow`;
- the `screen_scale` argument to `ts_set_view_size` is currently ignored, and
  Ghostboard currently sends it as `0.0`.

On a 2x Retina display this can make the hidden WebKit view twice the intended
point size while TermSurf's visible overlay and automation gestures remain in
pixel space. That mismatch fits the observed symptom: a visually full-width PDF
drag selects only the left-side token (`LEFT834`) in embedded Surfari.

This experiment should prove or disprove that hypothesis and, if proven, fix
Surfari's point/pixel conversion for WebKit view sizing and input forwarding.

## Changes

- `surfari/libtermsurf_webkit/src/libtermsurf_webkit.mm`
  - Add an effective-scale helper with this precedence:
    - use `screen_scale` from `ts_set_view_size` when it is greater than `0`;
    - otherwise use `contents->window.backingScaleFactor` when available;
    - otherwise use `contents->window.screen.backingScaleFactor`;
    - otherwise use `NSScreen.mainScreen.backingScaleFactor`;
    - finally fall back to `1.0`.
  - Store the latest effective scale on `WebContents`.
  - Apply point sizing at create time for both normal web contents and devtools
    contents, using the fallback scale because Ghostboard has not sent a resize
    yet.
  - Convert pixel dimensions to AppKit points before assigning the host window
    and `WKWebView` frame.
  - Convert pixel mouse coordinates to AppKit points before creating synthetic
    `NSEvent` instances.
  - Convert scroll event hit coordinates through the same point conversion path.
  - Keep exported CAContext pixel dimensions and snapshot-layer dimensions in
    pixel space so Ghostboard compositing remains unchanged.
  - Extend existing PDF copy/geometry traces to report:
    - raw pixel input;
    - effective scale;
    - converted point coordinates;
    - `WKWebView` point frame/bounds;
    - exported pixel dimensions.
- `scripts/test-issue-834-surfari-pdf-selection-copy.sh`
  - Reuse the existing separated-token PDF selection harness.
  - Add summary fields, if needed, that make the point/pixel proof explicit.

## Verification

The experiment passes only if all of the following are true:

- before the fix, trace evidence shows the mismatch:
  - `ts_set_view_size` receives pixel dimensions;
  - the hidden `WKWebView` is sized in those same numeric point dimensions;
  - Ghostboard supplied `screen_scale` is `0.0`;
  - Surfari's fallback display scale is greater than `1`;
  - mouse coordinates are injected without dividing by scale.
- after the fix, trace evidence shows:
  - the `WKWebView` point size is pixel size divided by scale;
  - synthetic mouse event point coordinates are pixel coordinates divided by
    scale;
  - scroll event hit coordinates are pixel coordinates divided by scale;
  - create-time host frames and first resize frames use the same point
    conversion;
  - exported CAContext pixel dimensions still match the requested browser pixel
    dimensions.
- the calibrated embedded Surfari separated-token PDF selection/copy harness
  copies all expected tokens:
  - `LEFT834`
  - `MID834`
  - `RIGHT834`
- the standalone oracle and calibration gates still match the cells being used.
- `scripts/test-issue-756-surfari-input-regression.sh` passes, proving ordinary
  Surfari page input still reaches the real app after the coordinate conversion
  change.

Required commands:

```bash
surfari/libtermsurf_webkit/build.sh
cargo build -p surfari
bash -n scripts/test-issue-834-surfari-pdf-selection-copy.sh
scripts/test-issue-834-surfari-pdf-selection-copy.sh
scripts/test-issue-756-surfari-input-regression.sh
git diff --check
```

If the scaling fix makes the PDF selection/copy harness pass, the result should
record the exact geometry trace lines proving the point conversion and classify
the bug as a Surfari point/pixel conversion gap.

If the trace does not show a scale mismatch, or if the mismatch is fixed but PDF
copy still returns only `LEFT834`, the result should be recorded as partial or
fail and the next experiment should follow the new evidence rather than
continuing this hypothesis.

## Design Review

Codex reviewed the Experiment 59 design before implementation and agreed that
the point/pixel hypothesis is coherent and is the right next direction after
Experiments 56 through 58.

The review required four plan fixes before implementation:

- `screen_scale` cannot be the only scale source because Ghostboard currently
  sends `0.0`;
- scroll coordinates use the same coordinate path and must be handled or
  explicitly deferred;
- create-time normal and devtools WebKit view sizing must be covered, not only
  later resize messages;
- non-PDF Surfari regression verification must name a concrete command.

The design was updated to use a Surfari effective-scale fallback chain, include
scroll coordinate conversion, include create-time sizing, and require
`scripts/test-issue-756-surfari-input-regression.sh` as the concrete non-PDF
input regression guard. The plan is approved for implementation after the plan
commit.
