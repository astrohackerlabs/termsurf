# Experiment 1: Wire Browser Cursor Updates Into AppKit

## Description

Ghostboard already has the browser-side cursor signal available: Roamium emits
`CursorChanged`, the protobuf message carries `tab_id` and `cursor_type`, and
Wezboard stores that cursor type per pane before mapping Chromium cursor values
to the visible terminal cursor. Ghostboard currently names `CursorChanged` for
logging but has no dispatcher case, so inbound browser cursor changes fall
through the ignored-message path.

This experiment will add the smallest Ghostboard-side runtime path that consumes
`CursorChanged`, routes it through the existing browser-fd/profile/browser/tab
lookup, and applies the cursor only while the mouse is over that pane's browser
overlay. The target user-visible behavior is:

- Chromium cursor type `2` shows AppKit's pointing-hand cursor over links;
- Chromium cursor type `3` shows AppKit's I-beam cursor over editable or
  selectable text;
- any other cursor type returns to AppKit's arrow cursor;
- cursor state is pane-scoped and does not leak outside the browser overlay or
  between panes.

## Changes

Planned source changes:

- `ghostboard/src/apprt/termsurf.zig`
  - Add an AppKit bridge declaration for a pane-scoped cursor update, likely
    `termsurf_set_cursor(pane_id, cursor_type)`.
  - Store the latest browser cursor type on `PaneState`.
  - Add a `CursorChanged` dispatcher case.
  - Implement `handleCursorChanged(fd, req)` by resolving:
    `fd -> ServerState -> profile/browser -> tab_id -> pane_id -> PaneState`.
  - Update pane cursor state while holding `state_mutex`, snapshot the pane id
    and cursor type, then call the AppKit bridge after releasing the mutex.
  - Log unknown browser fd, unknown tab, missing pane, and successful cursor
    updates with enough detail to diagnose routing failures.
- `ghostboard/macos/Sources/App/macOS/AppDelegate+TermSurf.swift`
  - Export a new C bridge function with `@_cdecl("termsurf_set_cursor")`.
  - Convert the C pane id into a UUID, find the matching `SurfaceView` through
    `AppDelegate.findSurface(forUUID:)`, and dispatch the update to the main
    queue just like overlay present/clear.
  - Log cursor requests and rejected requests to stderr/AppDelegate logger.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - Add pane-local TermSurf cursor state.
  - Map cursor type `2` to `NSCursor.pointingHand`, `3` to `NSCursor.iBeam`, and
    all other values to `NSCursor.arrow`, matching Wezboard's current
    `2 => Hand`, `3 => Text`, `_ => Arrow` behavior.
  - Apply the stored TermSurf cursor only when the mouse is over
    `termsurfOverlayFrame`; reset to arrow on overlay clear or mouse exit.
  - Re-apply the cursor after successful TermSurf mouse-move forwarding so hover
    changes become visible immediately without stealing normal terminal,
    split-divider, or drag cursors outside browser content.

Planned issue-doc changes:

- Update this experiment with the review result before implementation.
- After verification, append `## Result` and `## Conclusion` here and update the
  Issue 811 README status line.

## Verification

Baseline check before implementation:

1. Build the current code before source edits with the same build commands
   below.
2. Launch Ghostboard with local Roamium and open the cursor test page described
   below.
3. Hover the plain background, link, and text input/text region.
4. Record the current failure evidence:
   - `CursorChanged` is decoded but falls through the ignored-message path, or
     no Ghostboard cursor-routing log exists;
   - the visible cursor does not reliably change to pointing hand over the link
     and I-beam over text.

Static and build checks:

1. `prettier --write --prose-wrap always --print-width 80 issues/0811-ghostboard-cursor-feedback/README.md issues/0811-ghostboard-cursor-feedback/01-wire-browser-cursor-updates.md`
2. `zig fmt ghostboard/src/apprt/termsurf.zig`
3. `cd ghostboard && zig build -Demit-macos-app=false`
4. `cd ghostboard && macos/build.nu --scheme Ghostty --configuration Debug --action build`

Runtime cursor checks:

1. Create or serve a small local test page with a plain background, a link, and
   a text input or selectable text region.
2. Launch the built Ghostboard app with local Roamium and the existing
   TermSurf/web flow.
3. Open the test page inside Ghostboard.
4. Move the mouse over the plain page background, the link, and the text
   input/text region.
5. Confirm logs show `CursorChanged` received, routed to the expected pane id,
   and sent through the AppKit bridge with cursor types `2`, `3`, and the
   default/arrow value.
6. Confirm the visible macOS cursor changes to arrow, pointing hand, and I-beam
   in the corresponding page regions.
7. Open a second Ghostboard browser pane or split with the same test page.
8. Hover the link in pane A, then switch focus to pane B and hover plain
   background/text/link in pane B.
9. Confirm pane B uses its own cursor state and pane A's prior hand cursor does
   not leak into pane B, the terminal area, or the split divider.
10. Switch focus back to pane A and confirm cursor updates still follow the
    hovered page region in pane A.

Pass criteria:

- Baseline evidence proves the pre-change behavior is missing or ignored.
- All static/build checks pass.
- `CursorChanged` is no longer logged as an ignored message.
- Link hover visibly shows a pointing-hand cursor.
- Text/input hover visibly shows an I-beam cursor.
- Plain page hover visibly returns to arrow.
- Leaving the browser overlay does not leave Ghostboard stuck in a browser
  cursor.
- Two-pane or tab/focus switching proves cursor state remains pane-scoped and
  does not leak between browser panes or into terminal chrome.

Partial criteria:

- Build passes and cursor routing logs prove `CursorChanged` reaches AppKit, but
  visual cursor verification is inconclusive because of VM or macOS cursor
  capture limitations.

Fail criteria:

- Ghostboard still ignores `CursorChanged`, cursor updates cannot be routed to
  the correct pane, source does not build, or cursor state leaks outside the
  browser overlay.

## Design Review

Fresh-context adversarial review by Codex subagent `Dirac`:

- **Verdict:** Changes required.
- **Required finding:** Multi-pane or pane-focus verification was optional and
  missing from pass criteria.
- **Required finding:** The design did not explicitly prove current behavior
  before implementation.
- **Resolution:** The verification plan now requires a baseline failure check
  before implementation and a mandatory two-pane or tab/focus-switch cursor
  isolation check with explicit pass criteria.
- **Re-review verdict:** Approved. The reviewer confirmed both required findings
  were resolved and found no new required issues.

## Result

**Result:** Pass

Implemented Ghostboard `CursorChanged` handling and AppKit cursor application.

Changed files:

- `ghostboard/src/apprt/termsurf.zig`
  - Added `termsurf_set_cursor` bridge declaration.
  - Added `cursor_type` to `PaneState`.
  - Added `CursorSnapshot`, a dispatcher case for `CursorChanged`, and
    `handleCursorChanged`.
  - Routes cursor updates by browser fd, profile/browser, tab id, and pane id
    before calling the AppKit bridge outside `state_mutex`.
- `ghostboard/macos/Sources/App/macOS/AppDelegate+TermSurf.swift`
  - Added exported `termsurf_set_cursor`.
  - Resolves pane id to `SurfaceView` on the main queue and forwards the cursor
    type.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - Added pane-local TermSurf cursor state.
  - Maps cursor type `2` to `.pointingHand`, `3` to `.iBeam`, and all other
    values to `.arrow`.
  - Applies the cursor only after a successful browser-overlay mouse move and
    resets to arrow when leaving or clearing the overlay.

Baseline evidence:

- Before implementation, the baseline cursor page built and ran with:
  - `cd ghostboard && zig build -Demit-macos-app=false`
  - `cd ghostboard && macos/build.nu --scheme Ghostty --configuration Debug --action build`
  - `TERMSURF_GEOMETRY_URL=file://... scripts/ghostboard-geometry-matrix.sh initial-open`
- Baseline log:
  - `logs/ghostboard-geometry-initial-open-app-20260617-192019.log`
- The baseline decoded but ignored the cursor message:
  - `TermSurf message decoded type=CursorChanged`
  - `TermSurf message ignored type=CursorChanged`

Verification:

- `zig fmt ghostboard/src/apprt/termsurf.zig` passed.
- `cd ghostboard && zig build -Demit-macos-app=false` passed.
- `cd ghostboard && macos/build.nu --scheme Ghostty --configuration Debug --action build`
  passed.
- `git diff --check` passed.
- Direct `swiftlint` was not available in this VM (`command not found`), but the
  macOS app build ran its SwiftLint build phase and completed successfully.

Runtime cursor checks:

- Link hover page:
  - Harness passed:
    `logs/ghostboard-geometry-initial-open-harness-20260617-192326.log`
  - App log: `logs/ghostboard-geometry-initial-open-app-20260617-192326.log`
  - Evidence:
    `CursorChanged: pane_id=8D426C37-1EA3-4AB4-9DE6-74B5F76141E9 tab_id=1 cursor_type=2`
    and
    `TermSurf cursor set pane_id=8D426C37-1EA3-4AB4-9DE6-74B5F76141E9 cursor_type=2 over_overlay=true`.
- Text/input hover page:
  - Harness passed:
    `logs/ghostboard-geometry-initial-open-harness-20260617-192332.log`
  - App log: `logs/ghostboard-geometry-initial-open-app-20260617-192332.log`
  - Evidence:
    `CursorChanged: pane_id=D08C5426-6BFF-4EE8-AF0C-1C42CAFDE551 tab_id=1 cursor_type=3`
    and
    `TermSurf cursor set pane_id=D08C5426-6BFF-4EE8-AF0C-1C42CAFDE551 cursor_type=3 over_overlay=true`.
- Blank-background hover page:
  - Harness passed:
    `logs/ghostboard-geometry-initial-open-harness-20260617-192402.log`
  - App log: `logs/ghostboard-geometry-initial-open-app-20260617-192402.log`
  - Evidence:
    `CursorChanged: pane_id=1BB390AD-F6B2-4DDA-AD39-3DBFD0223383 tab_id=1 cursor_type=0`
    and
    `TermSurf cursor set pane_id=1BB390AD-F6B2-4DDA-AD39-3DBFD0223383 cursor_type=0 over_overlay=true`.
- Two-browser-tab isolation:
  - Harness passed:
    `logs/ghostboard-geometry-open-browser-in-new-tab-harness-20260617-192430.log`
  - App log:
    `logs/ghostboard-geometry-open-browser-in-new-tab-app-20260617-192430.log`
  - Roamium trace:
    `logs/ghostboard-geometry-open-browser-in-new-tab-roamium-20260617-192430.log`
  - Pane A evidence:
    `CursorChanged: pane_id=98FC3F99-6089-413A-8804-F8864B8F0FDC tab_id=1 cursor_type=2`.
  - Pane B evidence:
    `CursorChanged: pane_id=69ACDB68-4DF6-43C3-BDF3-6DB3671EC7C1 tab_id=2 cursor_type=3`.
  - The harness also proved tab/focus isolation: browser B keyboard did not
    reach browser A, browser A keyboard did not reach browser B, and restored
    hit tests used the correct pane/tab frames after switching.
- All post-change cursor runtime app logs checked had no
  `ignored type=CursorChanged` entries.

Completion review:

- Fresh-context adversarial review by Codex subagent `Poincare`.
- **Verdict:** Approved.
- **Findings:** None.
- The reviewer independently checked `git diff --check`,
  `cd ghostboard && zig build -Demit-macos-app=false`, the working-tree diff,
  the baseline ignored-message evidence, post-change cursor routing logs, and
  the two-tab isolation harness evidence.

## Conclusion

Ghostboard now consumes Roamium `CursorChanged` messages and applies browser
cursor feedback through AppKit. Link hover routes to the pointing-hand cursor,
text/input hover routes to I-beam, blank page hover routes to arrow, and the
two-browser-tab run proves cursor updates are keyed to the correct pane/tab
instead of leaking through focus switches.
