+++
status = "open"
opened = "2026-04-15"
+++

# Issue 779: Native popups (date picker, select dropdown) render outside webview overlay

## Goal

Native/popup UI elements spawned by the webview — date pickers, `<select>`
dropdowns, and any other OS-level popup Chromium creates — should appear over
the webview where the user clicked, not detached from it in an unrelated screen
region.

## Background

While testing an app with a DaisyUI date input, clicking the date field causes
the picker to pop up in the wrong location. When the webview overlay is
positioned on the right side of the terminal window (e.g., a right split), the
date picker appears on the left — entirely outside the webview's bounds.

The same bug happens with **native `<select>` dropdown boxes**: clicking a
dropdown opens the menu at a detached screen position that doesn't match the
`<select>` element the user clicked. This confirms the problem is not specific
to date pickers — it affects every kind of native popup window Chromium creates.

This is surprising because the webview is composited into the terminal via
CALayerHost (zero-copy GPU texture sharing from Roamium's Chromium process into
Wezboard). Content rendered into that texture is necessarily clipped to the
overlay's rect. The fact that the picker renders outside the overlay strongly
implies it is **not** drawn into the webview's GPU texture at all — it must be a
separate OS-level window (a popup/child window) that Chromium positions using
screen coordinates it computed against its own internal notion of where the
webview lives, which does not match where Wezboard actually composites the
CALayerHost overlay.

In other words: Chromium thinks the webview is at screen coordinates (X, Y), but
Wezboard is actually displaying the layer at (X', Y'). Any popup window Chromium
spawns (date pickers, select dropdowns, autofill, color pickers, context menus
rendered as native windows, etc.) will be placed at the wrong absolute screen
position.

## Analysis

Possible root causes to investigate:

1. **Chromium's view bounds are stale or wrong.** The embedding API
   (`libtermsurf_chromium`) needs to tell Chromium the webview's real on-screen
   rect whenever the overlay moves or resizes, so that popup-positioning code
   inside Chromium uses the correct origin. If we're only updating the
   CALayerHost frame and not informing Chromium's `RenderWidgetHostView` of its
   new screen position, popups will anchor to stale coordinates (often 0,0 or
   the window origin).

2. **Popup windows are separate OS windows.** Chromium typically renders
   `<select>` dropdowns, date pickers, and autofill as platform popup widgets on
   macOS. These are real `NSWindow`s (or `NSPanel`s) positioned in screen
   coordinates. If Chromium's host view reports the wrong screen origin, the
   popup opens in the wrong place.

3. **Coordinate space mismatch.** Wezboard positions the overlay in its own
   window-local coordinates and converts to screen coordinates for CALayerHost.
   Roamium/Chromium may be using a different origin (top-left vs bottom-left, or
   main-screen vs window-local) when computing popup placement.

## Proposed Solutions

- Audit what view/window bounds Roamium reports into Chromium when the GUI sends
  `OverlayReposition` / `OverlayResize` protocol messages. Ensure the
  `RenderWidgetHostView`'s screen rect is updated, not just the compositor layer
  size.
- Add a protocol field (or reuse existing reposition messages) to carry the
  webview's **absolute screen rect**, not just a window-local rect, so Chromium
  can position popups correctly.
- Verify on a right-split pane: open a DaisyUI date input, confirm the picker
  opens aligned to the input field within the overlay.
- Check other popup-style UI while we're here: `<select>` dropdowns, autofill
  suggestions, context menus, color pickers, file chooser anchors.

## Reproduction

### Date picker

1. Build and run Wezboard with a right split hosting a webview.
2. Load a page with a DaisyUI date input (or any `<input type="date">`).
3. Click the date field.
4. Observe: picker appears on the left side of the window, outside the webview
   overlay's bounds.

### Native `<select>` dropdown

1. Build and run Wezboard with a right split hosting a webview.
2. Load a page with a native `<select>` element.
3. Click the dropdown.
4. Observe: the dropdown menu appears at a detached location, not anchored to
   the `<select>` element the user clicked.

## Experiments

### Experiment 1: Add a Native Popup Reproduction Page

#### Description

Create a focused reproduction page in `test-html` for native popup positioning.
Before changing Chromium, Roamium, or the TermSurf protocol, we need a stable
local page that demonstrates the bug using plain browser-native controls.

`test-html` is the right place because it already hosts manual browser behavior
checks at `http://localhost:9616`, including pages for mouse input, dialogs,
file upload/download, storage, and page zoom. This issue should get the same
kind of small deterministic test page.

The page should make the coordinate bug obvious without implying that the
control's position inside the HTML document is the root cause. The bug is about
the webview overlay's position on the screen: Chromium appears to place native
popup windows using a screen origin that does not match where Wezboard
composites the CALayerHost. The same control should behave differently when the
webview pane is moved to a different split position on the screen.

The page should therefore keep the controls simple and ordinary, and draw clear
visual bounds so the tester can tell whether a popup is attached to the webview
or appears detached elsewhere on the screen.

#### Changes

1. **Add `test-html/public/test-native-popups.html`.**

   The page should include:
   - `<input type="date">`
   - `<input type="time">`
   - `<input type="datetime-local">`
   - `<input type="color">`
   - a native single `<select>`
   - an `<input list="...">` with a `<datalist>` for suggestion-popup behavior

   Use plain HTML, CSS, and minimal JavaScript. No framework dependency is
   needed.

2. **Make the bug visually measurable.**

   The page should:
   - draw an obvious bordered test area representing the visible webview
     content;
   - label each control with the expected behavior: the native popup should
     appear anchored to that control, not far away elsewhere on the screen;
   - show the last focused/clicked control in an on-page log so screenshots make
     clear which element triggered the popup.

3. **Link the page from `test-html/public/index.html`.**

   Add a link under the existing input/browser-behavior section, for example:
   `Native Popups — date picker, select dropdown, color picker, datalist`.

4. **Do not fix the underlying bug in this experiment.**

   This experiment is only for reproduction. It should not modify Wezboard,
   Roamium, Chromium, protobufs, or overlay coordinate code.

#### Verification

1. Start the test server:

   ```bash
   bun test-html/server.ts
   ```

2. Open Wezboard and create a split layout where the browser pane is on the
   right side of the terminal window.

3. Open the reproduction page:

   ```bash
   web http://localhost:9616/test-native-popups.html
   ```

4. Click the native popup controls in the right-side browser pane:
   - date input;
   - select dropdown;
   - color input;
   - datalist input.

5. Confirm the issue is reproducible:
   - the clicked control is visibly inside the webview overlay;
   - the native popup appears detached from that control, outside the webview
     overlay, or at an obviously wrong screen coordinate;
   - the page log identifies which control was clicked.

6. Repeat with the browser pane on the left or as the only pane. The bug may be
   less visible there, but this comparison helps show that the bad offset is
   tied to the webview overlay's screen position, not the control's position
   inside the page.

**Result:** Pass

Added `test-html/public/test-native-popups.html`, a plain HTML reproduction page
for native popup positioning. The page includes native date, time,
datetime-local, color, select, and datalist controls; draws a visible webview
content boundary; and logs the last focused or clicked control so screenshots
can identify which native popup was triggered.

Added the page to `test-html/public/index.html` under Input.

Static serving verification passed using a temporary local server for
`test-html/public`:

```bash
curl -I http://localhost:9617/test-native-popups.html
curl -s http://localhost:9617/
curl -s http://localhost:9617/test-native-popups.html
```

Manual Wezboard reproduction of the native-popup mispositioning remains for the
next experiment.

#### Conclusion

Issue 779 now has a local, deterministic reproduction page. Future experiments
can use `http://localhost:9616/test-native-popups.html` to compare native popup
placement as the webview overlay moves between split positions on screen.
