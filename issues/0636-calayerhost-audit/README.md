+++
status = "closed"
opened = "2026-02-24"
closed = "2026-02-24"
+++

# Issue 636: CALayerHost Feature Audit (Resumed)

## Goal

Complete the CALayerHost feature audit that was paused in Issue 634. Issue 634
tested T1–T12, found a multi-pane regression, and deferred the remaining tests.
Issue 635 fixed the regression (per-tab persistent compositor). This issue
resumes the audit from where we left off.

## Background

[Issue 634](634-calayerhost-audit.md) systematically tested the browser overlay
pipeline after the CALayerHost migration (Issues 625–633). It passed 11 of 12
tests before discovering that the persistent compositor (Issue 633) broke
multi-tab isolation. [Issue 635](635-multi-pane-calayerhost.md) fixed this by
moving the compositor into per-tab `TabState`.

The audit now resumes with the Issue 635 fix in place.

## Prior results (from Issue 634)

| Test | Description             | Result |
| ---- | ----------------------- | ------ |
| T1   | Basic page load         | PASS   |
| T2   | Link navigation         | PASS   |
| T3   | Back/forward navigation | PASS   |
| T4   | Page reload             | PASS   |
| T5   | Resize (window)         | PASS   |
| T6   | Resize (pane split)     | PASS   |
| T7   | Multi-pane              | PASS   |
| T8   | Multi-profile           | PASS   |
| T9   | Mouse clicks            | PASS   |
| T10  | Mouse drag/selection    | PASS   |
| T11  | Mouse scroll            | PASS   |
| T12  | Cursor changes          | PASS   |
| T13  | Keyboard typing         | PASS   |
| T14  | Cmd+key bypass          | PASS   |
| T15  | Keyboard Tab            | PASS   |
| T16  | Focus lifecycle         | PASS   |
| T17  | Loading indicator       | PASS   |
| T18  | URL sync                | PASS   |
| T21  | Retina rendering        | PASS   |
| T22  | Overlay positioning     | PASS   |

Known bug from T2: refocusing the TermSurf window eats the first click even
though the browser pane is already focused. Cosmetic — not blocking.

## Resumed tests

Build: release (`./build-release.sh`)

### T4: Page reload

Reload the current page (Cmd+R). Content re-renders without blank frame.

**Result: PASS**

### T8: Multi-profile

Open browser panes with different profiles. Each gets its own Chromium Profile
Server process. Content renders independently in each.

**Result: PASS**

### T13: Keyboard input (typing)

Click a text input or search field. Type characters. Text appears in the field.

**Result: PASS**

### T14: Keyboard input (Cmd+key bypass)

Cmd+C, Cmd+V, Cmd+A, Cmd+X bypass the browser and work as expected (copy, paste,
select all, cut).

**Result: PASS**

### T15: Keyboard input (Tab)

Press Tab to move between form fields. Focus advances correctly.

**Result: PASS**

### T16: Focus lifecycle

Click between a terminal pane and a browser pane. Focus follows correctly.
Keyboard input routes to the focused pane.

**Result: PASS**

### T17: Loading indicator

Navigate to a page. Progress indicator appears during load and disappears when
the page finishes loading.

**Result: PASS**

### T18: URL sync

Navigate to a new page (by clicking a link or entering a URL). The URL bar in
the TUI updates to reflect the current page.

**Result: PASS**

### T21: Retina rendering

On a Retina display, content renders at physical pixel resolution (not blurry).
Text is sharp. Compare with native Chrome side-by-side if needed.

**Result: PASS**

### T22: Overlay positioning (pixel-perfect)

Compare the browser overlay position with the TUI viewport border. Content
should align to the pixel — no gap, no overlap, no offset.

**Result: PASS**

## Conclusion

All 20 implemented features pass. The CALayerHost migration (Issues 625–633)
plus the per-tab persistent compositor fix (Issue 635) produce a fully working
browser overlay pipeline. Page loads, navigation, resize, multi-pane,
multi-profile, mouse input, keyboard input, focus lifecycle, loading indicators,
URL sync, Retina rendering, and pixel-perfect overlay positioning all work
correctly.

T19 (tab creation) and T20 (tab close) were removed — browser tab
creation/closing has not been implemented yet. These are future features, not
regressions.

The only known cosmetic bug is from T2: refocusing the TermSurf window eats the
first click even though the browser pane is already focused. Not blocking.
