+++
status = "open"
opened = "2026-06-17"
+++

# Issue 809: Ghostboard Viewport Geometry

## Goal

Make Ghostboard browser overlays correctly fill and follow their owning terminal
pane across all pane, tab, window, resize, move, split, close, hide, show, and
input-routing transitions.

The issue is complete when the full viewport geometry matrix is tested and
proven to work simultaneously in the new `ghostboard/` implementation.

## Background

Issue 808 restored Ghostboard from Ghostty `v1.3.1` and proved the ordinary
TermSurf browsing path: `webtui` runs inside Ghostboard, Roamium launches, and
browser content is visibly composited into the terminal pane.

A follow-up screenshot captured on 2026-06-17 shows the next class of bugs: the
web browser webview renders, but it does not fill the whole viewport allocated
by `webtui`. The browser layer appears smaller than the terminal overlay frame,
leaving unused dark space to the right and below the rendered page.

This is broader than the initial-size bug. A browser webview belongs to exactly
one terminal pane in exactly one tab in exactly one window, and its native layer
must always match that pane's current viewport rectangle. All geometry and
visibility transitions must preserve that invariant.

Most or all of these behaviors were previously solved in `ghostboard-legacy/`.
The new implementation should use that legacy implementation as reference
evidence whenever a matrix item fails, but it should not blindly copy code. The
goal is to understand the legacy mechanism, adapt the design to the current
Ghostty `v1.3.1` codebase, and prove the new behavior with runtime evidence.

## Scope

In scope:

- Initial browser overlay size and position.
- Window resize larger and smaller.
- Horizontal and vertical pane splits.
- Closing sibling panes and browser-owning panes.
- Focus changes between panes.
- Tab creation, tab switching, and tab close.
- Window creation, window switching, and window close.
- Multiple browser overlays across tabs and windows.
- Terminal font-size or cell-metric changes when they affect overlay geometry.
- Fullscreen, unfullscreen, minimize, hide, restore, and display scale changes
  when practical to automate or manually verify.
- Mouse hit testing and input coordinate mapping after every geometry change.
- Cleanup of native layers when panes, tabs, windows, or browser sessions are
  removed.

Out of scope unless required for viewport correctness:

- Downloads.
- Bookmarks and history UI.
- JavaScript dialogs.
- HTTP authentication dialogs.
- General browser product polish.
- Changes to `webtui` or Roamium behavior unless a failure proves the protocol
  contract itself is incomplete.

## Strategy

The issue should start with observability, not fixes. Experiment 1 should define
the canonical geometry state, add durable logging or assertions for that state,
and create a reusable manual/automated harness that can run matrix scenarios and
collect comparable evidence.

The required geometry evidence is:

- identity tuple:
  `window_id + surface_id + selected_tab_id + pane_id + browser_tab_id`;
- terminal pane viewport rectangle;
- TUI overlay cell rectangle;
- native AppKit/CALayerHost frame;
- Roamium/browser viewport size;
- backing scale factor;
- visibility state;
- input hit-test frame and webview-relative coordinates.

After that harness exists, each matrix item should be handled in sequence:

1. Test the behavior against current `ghostboard/`.
2. Mark the item `Pass`, `Fail`, or `Not yet tested`.
3. Use the harness logs to localize which invariant failed.
4. Inspect the most relevant reference implementation or source:
   - current upstream Ghostty layout/rendering APIs for native pane/window
     behavior;
   - `ghostboard-legacy/` when it likely contains the corresponding TermSurf
     overlay mechanism or behavioral precedent;
   - Wezboard when the same TermSurf protocol geometry behavior is already
     solved there;
   - historical issue documents when they contain useful prior experiments.
5. Record what the reference implementation teaches.
6. Design the smallest fix for the new Ghostboard implementation.
7. Implement and test the fix.
8. Re-test adjacent matrix items that could regress.
9. After all targeted items pass, run a full matrix regression sweep.

The final experiment must re-test the complete matrix and prove the behaviors
work together, not just individually.

## Viewport Matrix

| Scenario                               | Expected behavior                                                                    | Evidence required                                                                              |
| -------------------------------------- | ------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------- |
| Initial browser open                   | Webview fills the browser viewport area inside the pane                              | Screenshot plus logs proving overlay rect, native layer frame, and browser viewport size agree |
| Window resize larger                   | Webview grows with the owning pane                                                   | Before/after screenshot and resize logs                                                        |
| Window resize smaller                  | Webview shrinks or clips correctly within the owning pane                            | Before/after screenshot and resize logs                                                        |
| Horizontal pane split                  | Browser remains attached to its original pane and resizes to the new pane rectangle  | Screenshot plus pane/browser mapping logs                                                      |
| Vertical pane split                    | Browser moves/resizes to the new pane rectangle without overlapping the sibling pane | Screenshot plus pane/browser mapping logs                                                      |
| Resize existing split boundary         | Browser moves/resizes with the pane as the divider moves                             | Before/after screenshots plus rect and hit-test logs                                           |
| Equalize or rebalance panes            | Browser follows the owning pane after layout reflow                                  | Screenshot plus pane/browser mapping and rect logs                                             |
| Zoom or maximize pane                  | Browser expands with the maximized pane and restores with original pane layout       | Before/after screenshots plus visibility and rect logs                                         |
| Close sibling pane                     | Browser expands or moves with the remaining owning pane                              | Screenshot plus cleanup/layout logs                                                            |
| Close browser pane                     | Webview disappears and native layer/browser state is cleaned up                      | Screenshot plus no stale layer/mapping evidence                                                |
| Focus different pane in same tab       | Browser remains attached to the original pane only                                   | Screenshot plus input-routing logs                                                             |
| Focus back to browser pane             | Browser remains visible and interactive in its original pane                         | Screenshot plus input-routing logs                                                             |
| New terminal tab                       | Existing browser is hidden when its tab is not selected                              | Tab-switch screenshots plus visibility logs                                                    |
| Switch back to browser tab             | Existing browser reappears in the same pane with current geometry                    | Tab-switch screenshot plus visibility logs                                                     |
| Open browser in new tab                | New browser attaches only to the pane in the new tab                                 | Screenshot and independent tab/browser ids                                                     |
| Close browser tab                      | Browser layer is removed and browser state is cleaned or detached                    | Cleanup logs and no stale layer evidence                                                       |
| Open browser in new window             | Browser appears only in the new window                                               | Window-specific screenshot and ids                                                             |
| Multiple windows with browsers         | Each window shows only its own panes' browsers                                       | Screenshots of both windows plus window-scoped ids                                             |
| Move window between displays           | Webview remains aligned and scaled                                                   | Screenshot plus backing-scale evidence                                                         |
| Fullscreen and unfullscreen            | Webview remains aligned through the size transition                                  | Before/after screenshots                                                                       |
| Minimize, hide, and restore            | Webview hides and restores with the owning window                                    | Before/after screenshots or logs                                                               |
| Terminal font-size/cell-metric change  | Overlay rectangle and browser viewport recompute from the new cell metrics           | Screenshot plus cell-size/overlay logs                                                         |
| TUI overlay resize command             | Webview follows the TUI-requested viewport dimensions                                | Protocol logs plus screenshot                                                                  |
| Terminal scrollback movement           | Webview stays attached to the live pane viewport, not scrollback content             | Screenshot and surface visible-rect evidence                                                   |
| Browser navigation                     | Page changes do not reset or corrupt overlay geometry                                | Screenshot before/after navigation                                                             |
| DevTools split or tab                  | DevTools overlay follows the same pane/tab/window geometry rules                     | Screenshot plus normal/devtools tab ids                                                        |
| Mouse input after geometry change      | Hit testing uses the current webview-relative frame                                  | Click/scroll logs with current coordinates                                                     |
| Keyboard input after tab/window switch | Keyboard reaches only the visible active browser pane                                | Input logs with active window/tab/pane ids                                                     |

## State Invariants

- A native overlay layer is keyed by the canonical identity tuple:
  `window_id + surface_id + selected_tab_id + pane_id + browser_tab_id`.
- Browser geometry is derived from the current pane viewport rectangle, overlay
  cell rectangle, and backing scale factor.
- Visibility is gated by the selected window, selected tab, pane existence, and
  browser session state.
- Input routing is gated by the active window, active tab, and current hit-test
  frame.
- Cleanup removes native layers and state when the owning pane, tab, window, or
  browser session is removed.

## Acceptance Criteria

- The 2026-06-17 screenshot failure is reproduced and fixed.
- Experiment 1 establishes reusable geometry instrumentation and a repeatable
  matrix harness before individual fixes begin.
- Every viewport matrix item is tested against current `ghostboard/`.
- Every failing item is localized with geometry evidence before a fix is
  designed.
- Every failing item includes a relevant reference audit before implementation;
  acceptable references include current upstream Ghostty, `ghostboard-legacy/`,
  Wezboard, or historical issue documents, depending on the localized failure.
- Fixes are implemented in `ghostboard/` only unless evidence proves another
  component must change.
- Runtime evidence proves overlay rect, native layer frame, browser viewport
  size, and input hit-test coordinates agree after relevant geometry changes.
- A final full-matrix regression run passes with no known geometry, visibility,
  cleanup, or input-coordinate regressions.
- The final regression result includes a per-row table with scenario, status,
  screenshot path, log path, identity tuple, rect comparisons, backing scale,
  and pass/fail notes.

## Experiments

- [Experiment 1: Geometry observability harness](01-geometry-observability-harness.md)
  — **Pass**
- [Experiment 2: Initial viewport fill](02-initial-viewport-fill.md) — **Pass**
- [Experiment 3: Window resize follow](03-window-resize-follow.md) — **Pass**
- [Experiment 4: Split-right pane attachment](04-split-right-pane-attachment.md)
  — **Pass**
