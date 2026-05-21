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

### Experiment 2: Send and Apply Webview Screen Bounds

#### Description

Fix native popup positioning by teaching Chromium the webview's real screen
rect. If the fix is incomplete, leave enough logs in the result to identify
which coordinate system is still wrong.

Experiment 1 proved the bug is tied to the webview overlay's screen position,
not to the HTML control's position inside the page. The current protocol only
sends webview size to Roamium/Chromium. Wezboard separately moves the
CALayerHost to the correct terminal-pane location, but Chromium's native view
still appears to believe it lives somewhere else. Native popup windows use that
Chromium-side screen rect, so they open far away from the webview.

This experiment should make the smallest end-to-end change that can plausibly
fix the issue:

1. compute the overlay's absolute screen rect in Wezboard;
2. send that rect to Roamium with resize/update messages;
3. pass the rect through Roamium's FFI to `libtermsurf_chromium`;
4. update Chromium's WebContents / RenderWidgetHostView host bounds from that
   rect;
5. log both the Wezboard rect and the Chromium rect so a failed attempt reveals
   the remaining mismatch.

#### Changes

1. **Extend the TermSurf resize message with screen bounds.**

   In `proto/termsurf.proto`, add optional-compatible fields to `Resize`:
   - `double screen_x`
   - `double screen_y`
   - `double screen_width`
   - `double screen_height`
   - `double screen_scale`

   These should represent the webview overlay rect in Chromium-style screen DIP:
   top-left origin, device-independent points, not terminal cells and not
   backing pixels. Keep the existing `pixel_width` and `pixel_height` fields
   unchanged for content size.

   Regenerate Rust protobuf bindings using the repo's existing protobuf build
   path.

2. **Compute the overlay screen rect in Wezboard.**

   In `wezboard/wezboard-gui/src/termsurf/conn.rs`, where `set_overlay_frame`
   already receives the CA layer frame in backing pixels, compute the matching
   screen-space rect for the webview:
   - convert the backing-pixel overlay frame into the overlay NSView's logical
     coordinate system using the same scale logic already used by
     `set_overlay_frame`;
   - convert that rect from the overlay NSView/window coordinate system to
     screen coordinates using Cocoa APIs, not hand-rolled origin math;
   - convert the Cocoa/AppKit screen rect into the protocol's Chromium-style
     top-left screen DIP convention before sending;
   - record the resulting `screen_x`, `screen_y`, `screen_width`,
     `screen_height`, and `screen_scale` on the TermSurf pane state.

   Add targeted debug logs showing:
   - pane id;
   - backing frame;
   - logical/view frame;
   - Cocoa/AppKit screen rect;
   - protocol screen rect;
   - scale/dpi used.

3. **Send screen bounds to Roamium on resize/update.**

   In `wezboard/wezboard-gui/src/termsurf/conn.rs`, when sending `Resize` to the
   browser process, include the latest screen bounds stored for that pane. This
   applies to:
   - existing-pane `SetOverlay` resize path;
   - the frame update path after `set_overlay_frame` changes the overlay
     position.

   Do not send a `Resize` on every paint. Store the last screen-bounds message
   sent for each pane and only send when content size or screen bounds changed
   beyond a small tolerance, for example 0.5 DIP for position/size and exact
   change for pixel dimensions.

   If the tab id is not known yet, store the bounds and send them as soon as the
   tab id becomes available.

4. **Pass bounds through Roamium FFI.**

   In `roamium/src/dispatch.rs` and `roamium/src/ffi.rs`, extend the resize path
   so `Resize` calls a new or expanded FFI function with both size and screen
   bounds, for example:
   `ts_set_view_bounds(handle, pixel_width, pixel_height, screen_x, screen_y, screen_width, screen_height, screen_scale)`.

   Keep the old size-only behavior available as a fallback if bounds are zero.

5. **Apply bounds inside Chromium.**

   In the Chromium fork, create a new issue branch for Issue 779 following the
   Chromium branch policy.

   Add the `libtermsurf_chromium` API needed by Roamium. In the implementation,
   update the WebContents view / RenderWidgetHostView host so Chromium's screen
   rect matches the rect sent by Wezboard.

   The first candidate implementation should update the per-tab WebContents /
   RenderWidgetHostView native view bounds, not move the shared host `NSWindow`.
   Moving the whole host window can break multiple tabs/webviews in the same
   Roamium process. If the native view frame is relative to its host `NSWindow`,
   normalize the incoming absolute screen rect by subtracting the host window's
   screen origin before calling `setFrame:` or the equivalent Chromium view
   bounds API.

   If updating the per-tab native view does not affect popup placement, keep the
   logs and record that result. The next candidate would be a more direct
   RenderWidgetHostViewMac screen-coordinate override, such as the path used by
   popup positioning / `GetBoundsInRootWindow` / screen-info conversion.

   Add Chromium-side logs showing, for each resize/bounds update:
   - incoming screen rect;
   - host window screen rect/origin;
   - normalized native-view frame;
   - WebContents native view bounds;
   - RenderWidgetHostView bounds;
   - any available window/screen bounds returned by Chromium after the update.

6. **Keep diagnostics if the fix fails.**

   If native popups still open in the wrong place, do not remove the logs before
   recording the result. The result should include enough log excerpts to answer
   which of these is true:
   - Wezboard computed the wrong screen rect;
   - Wezboard computed the right rect but sent the wrong values;
   - Roamium received the right values but passed wrong values through FFI;
   - Chromium received the right values but updated the wrong view/window;
   - Chromium reports the right view bounds but native popups use another
     coordinate path.

#### Verification

1. Build all affected components:

   ```bash
   scripts/build.sh wezboard
   scripts/build.sh roamium
   scripts/build.sh chromium
   ```

2. Start `test-html`:

   ```bash
   bun test-html/server.ts
   ```

3. Open the reproduction page in a browser pane:

   ```bash
   web http://localhost:9616/test-native-popups.html
   ```

4. Put the browser pane in at least three screen positions:
   - only pane;
   - left split;
   - right split or top-right split.

5. In each position, click:
   - date input;
   - select dropdown;
   - datalist input.

   Also click the color input and record what happens, but do not treat color
   picker anchoring as required for this experiment. On macOS Chromium may use
   the global `NSColorPanel`, which is not necessarily anchored to the webview.

6. Pass criteria:
   - each native popup opens anchored to the clicked control;
   - moving the webview pane to another split position does not detach the
     popup;
   - Wezboard logs and Chromium logs agree on the webview screen rect within 1
     DIP.
   - continuous resize or pane movement for several seconds produces a small
     finite number of screen-bounds resize sends, not one send per paint frame.

7. Fail criteria:
   - any native popup still opens far outside the webview;
   - logs do not clearly show where the coordinate mismatch occurs.

8. If the result is Fail or Partial, include the relevant Wezboard, Roamium, and
   Chromium log excerpts in the result before designing the next experiment.

**Result:** Fail

Implemented the screen-bounds path through the stack:

- `Resize` now carries absolute webview bounds in Chromium-style screen DIPs
  (`screen_x`, `screen_y`, `screen_width`, `screen_height`, `screen_scale`).
- Wezboard computes the overlay's Cocoa screen rect from the CALayerHost view,
  converts it to top-left DIP coordinates, stores it on the pane, and sends
  throttled resize messages only when size or screen bounds change.
- Roamium forwards bounded resize messages through the new `ts_set_view_bounds`
  FFI call, falling back to `ts_set_view_size` when bounds are unavailable.
- Chromium branch `148.0.7778.97-issue-779` adds `ts_set_view_bounds` and
  applies the incoming screen rect to the per-tab `RenderWidgetHostView` while
  preserving the existing content-size and compositor resize path.

Build verification passed:

```bash
scripts/build.sh wezboard
scripts/build.sh roamium
scripts/build.sh chromium
```

Manual verification failed. Running the latest local Wezboard, Roamium,
Chromium, and `web` TUI builds produced the exact same visible behavior as
before: native inputs still open completely outside the Wezboard window, far
outside the webview bounds.

#### Conclusion

This disproves the first fix candidate. Passing the absolute webview rect
through `Resize` and applying it to `RenderWidgetHostView::SetBounds` does not
affect the coordinate path used by macOS native controls. The next experiment
must move deeper into Chromium's macOS view/root-window coordinate plumbing,
likely `RenderWidgetHostViewMac` popup positioning, root-window bounds, or
screen-info conversion.

### Experiment 3: Apply Synthetic Window Bounds

#### Description

Experiment 2 failed because the webview screen rect reached Chromium but native
popups still opened completely outside the Wezboard window. That means the
implementation updated the wrong half of Chromium's macOS bounds model.

In Chromium's `RenderWidgetHostViewMac`, `GetViewBounds()` is computed from two
cached rectangles:

```cpp
return view_bounds_in_window_dip_ +
       window_frame_in_screen_dip_.OffsetFromOrigin();
```

Experiment 2 passed the absolute TermSurf webview screen rect to
`RenderWidgetHostView::SetBounds()`. On macOS, `SetBounds()` updates the
view-in-window bounds. It only updates the window-frame-in-screen bounds when
`IsHeadless()` is true:

```cpp
ns_view_->SetBounds(rect);
if (IsHeadless()) {
  OnWindowFrameInScreenChanged(rect);
}
```

Roamium is not Chromium-headless; it is an offscreen/CALayerHost embedding.
Therefore Experiment 2 likely put the absolute screen origin into
`view_bounds_in_window_dip_` while `window_frame_in_screen_dip_` stayed tied to
Roamium's hidden host window. Native popup positioning then still saw the wrong
screen rect.

This experiment directly tests the likely fix while logging enough state to
prove or disprove it. Treat the TermSurf webview screen rect as a synthetic
host-window frame for the embedded tab, and keep the WebContents view local to
that synthetic window.

#### Changes

1. **Expose a TermSurf synthetic-window update on macOS.**

   On the Issue 779 Chromium branch, add a small TermSurf-specific helper around
   `RenderWidgetHostViewMac` state. The helper should take:
   - local view bounds, expected to be `(0, 0, width_dip, height_dip)`;
   - synthetic window frame, expected to be
     `(screen_x, screen_y, screen_width, screen_height)`.

   The helper must update the same state that `GetViewBounds()` uses:
   - call the existing `SetBounds()` path with local view bounds;
   - call or expose `OnWindowFrameInScreenChanged()` with the synthetic window
     frame.

   Do not pass the absolute screen rect to `SetBounds()` again. That was the
   Experiment 2 mistake.

2. **Apply the synthetic model from `ts_set_view_bounds`.**

   In `content/libtermsurf_chromium/ts_browser_main_parts.cc`, update the
   bounded resize path so it computes:

   ```text
   local_view_bounds = (0, 0, logical_width, logical_height)
   synthetic_window_frame = (screen_x, screen_y, screen_width, screen_height)
   ```

   Then apply those to the `RenderWidgetHostViewMac` helper from step 1.

   The expected invariant after the update is:

   ```text
   view_bounds_in_window_dip_ = (0, 0, w, h)
   window_frame_in_screen_dip_ = (screen_x, screen_y, w, h)
   GetViewBounds() = (screen_x, screen_y, w, h)
   ```

3. **Add targeted Chromium logs.**

   Add temporary logs with a consistent prefix, for example
   `[termsurf-popup-trace]`, when `ts_set_view_bounds` applies the synthetic
   model.

   Log:
   - incoming TermSurf screen rect;
   - local view bounds sent to `SetBounds()`;
   - synthetic window frame sent to `OnWindowFrameInScreenChanged()`;
   - resulting `GetViewBounds()`;
   - device scale factor;
   - content pixel size and logical size.

   If needed, add one popup-path log near `<select>` popup handling to record
   the anchor or view bounds consumed by the popup path. Do not broaden this
   into a full Chromium coordinate audit unless the `GetViewBounds()` invariant
   is correct and popups still fail.

4. **Keep the existing TermSurf bounds logs.**

   Preserve the Experiment 2 logs in Wezboard and Chromium that show:
   - Wezboard's computed webview screen rect;
   - the `Resize` payload sent to Roamium;
   - Chromium's received `ts_set_view_bounds` values.

   The important comparison is between TermSurf's known intended webview rect
   and Chromium's resulting `GetViewBounds()`.

5. **Run the Roamium reproduction with logs enabled.**

   Run the local debug builds, open the reproduction page in Wezboard, and put
   the browser pane in a position where the bug is obvious, such as the
   top-right split.

   Click at least:
   - select dropdown;
   - datalist input;
   - date input.

   Color input can be clicked and recorded, but it remains a known exception if
   Chromium delegates it to the global `NSColorPanel`.

6. **Collect logs in deterministic files.**

   Do not rely on default app logging locations for this experiment. Put all
   logs under the repo's `logs/` directory so the result can quote exact files.

   Start Wezboard from the repo root with an explicit `XDG_STATE_HOME` and Rust
   logging enabled:

   ```bash
   mkdir -p logs/state/termsurf
   XDG_STATE_HOME="$PWD/logs/state" \
   RUST_LOG=termsurf=info,wezboard_gui::termsurf=info \
     ./wezboard/target/debug/wezboard-gui \
     2>&1 | tee logs/wezboard-issue-779-exp3.log
   ```

   Because Wezboard spawns Roamium with `--enable-logging` and
   `--log-file=$XDG_STATE_HOME/termsurf/chromium-server.log`, Chromium/Roamium
   logs should appear at:

   ```bash
   logs/state/termsurf/chromium-server.log
   ```

   Tail the Chromium/Roamium log while testing:

   ```bash
   tail -f logs/state/termsurf/chromium-server.log
   ```

   If the log file does not appear after opening the first browser pane, record
   that as a failure of the logging setup before continuing.

   After clicking the select, datalist, and date controls, extract the relevant
   log lines with:

   ```bash
   rg "termsurf-popup-trace|Resize:|overlay screen rect|ResizeTab bounds|GetViewBounds" \
     logs/wezboard-issue-779-exp3.log \
     logs/state/termsurf/chromium-server.log
   ```

   The result must quote or summarize the extracted lines for:
   - the Wezboard overlay screen rect;
   - the resize message sent to Roamium;
   - Chromium's incoming `ts_set_view_bounds` values;
   - the local view bounds applied in Chromium;
   - the synthetic window frame applied in Chromium;
   - Chromium's resulting `GetViewBounds()`.

7. **Analyze the result.**

   The result must answer these questions:
   - Does Wezboard compute the correct visible webview screen rect?
   - Does Chromium receive that same rect through `ts_set_view_bounds`?
   - Does Chromium's `GetViewBounds()` become the same rect after applying the
     synthetic-window update?
   - Do select, datalist, and date popups anchor inside the Wezboard webview?
   - If popups still fail while `GetViewBounds()` is correct, which popup path
     should be logged next?

#### Verification

1. Build the affected targets:

   ```bash
   scripts/build.sh chromium
   scripts/build.sh roamium
   scripts/build.sh wezboard
   ```

2. Start the reproduction server:

   ```bash
   bun test-html/server.ts
   ```

3. Start local Wezboard with deterministic logging:

   ```bash
   mkdir -p logs/state/termsurf
   XDG_STATE_HOME="$PWD/logs/state" \
   RUST_LOG=termsurf=info,wezboard_gui::termsurf=info \
     ./wezboard/target/debug/wezboard-gui \
     2>&1 | tee logs/wezboard-issue-779-exp3.log
   ```

4. Tail Chromium/Roamium logs in another terminal:

   ```bash
   tail -f logs/state/termsurf/chromium-server.log
   ```

5. Run the reproduction:
   - run local `web` with `--browser` pointing at
     `chromium/src/out/Default/roamium`;
   - open the reproduction page;
   - move the browser pane to the top-right or another visibly offset split;
   - click select, datalist, and date controls;
   - record whether each popup is anchored correctly.

6. Extract the relevant log lines:

   ```bash
   rg "termsurf-popup-trace|Resize:|overlay screen rect|ResizeTab bounds|GetViewBounds" \
     logs/wezboard-issue-779-exp3.log \
     logs/state/termsurf/chromium-server.log
   ```

7. Pass criteria:
   - Chromium logs show `GetViewBounds()` equals the visible Wezboard webview
     rect within 1 DIP;
   - select, datalist, and date popups open anchored to their controls inside
     the webview;
   - moving the browser pane to another split position updates `GetViewBounds()`
     and popup anchoring follows it.

8. Partial criteria:
   - `GetViewBounds()` is correct, but one or more native popups still open in
     the wrong place. In that case, keep the logs and design the next experiment
     around the specific popup path that ignores `GetViewBounds()`.

9. Fail criteria:
   - `GetViewBounds()` does not equal the visible Wezboard webview rect after
     applying the synthetic-window update;
   - popups still open completely outside the Wezboard window and the logs do
     not explain whether `view_bounds_in_window_dip_` or
     `window_frame_in_screen_dip_` is wrong.

**Result:** Fail

Implemented the synthetic-window bounds path in Chromium:

- added `RenderWidgetHostViewMac::SetTermSurfSyntheticWindowBounds()`;
- added an Objective-C++ bridge helper so `libtermsurf_chromium` can call it
  from C++;
- changed bounded resize handling so the WebContents view receives local bounds
  `(0, 0, logical_width, logical_height)`;
- changed bounded resize handling so the TermSurf screen rect is applied as the
  synthetic window frame via `OnWindowFrameInScreenChanged()`;
- added `[termsurf-popup-trace]` logs showing the incoming rect, local view
  bounds, synthetic window frame, and resulting `GetViewBounds()`.

Build verification passed:

```bash
scripts/build.sh chromium
scripts/build.sh roamium
scripts/build.sh wezboard
```

Manual verification failed before popup anchoring could be tested. Running the
local stack with this Chromium change launches/logs the browser process, but the
`web` TUI no longer appears. That leaves the browser visible without the TUI
chrome or a usable close/control path from `web`.

#### Conclusion

This experiment introduced a more severe regression than the original popup
positioning bug: browser startup can progress far enough to show/log the
browser, but the `web` TUI is absent. The synthetic-window approach is not
acceptable in its current form. Before any further popup-coordinate work, the
next step must either revert this Chromium change or explain exactly why
changing `RenderWidgetHostViewMac` bounds breaks the TUI/browser lifecycle.

### Experiment 4: Passive Popup Coordinate Trace

#### Description

Experiment 3 proved that mutating global `RenderWidgetHostViewMac` geometry is
too dangerous: it restored no confidence in popup positioning and broke the
basic `web` TUI experience. The next experiment must be logging-only.

The goal is to collect enough precise data to identify the native popup
coordinate source and the correct fix location without changing behavior. This
experiment must preserve the current baseline:

- `web` TUI appears and works;
- browser pane opens;
- native popups still reproduce the off-window bug.

No code in this experiment may change view bounds, window frames, screen info,
input coordinates, focus behavior, tab lifecycle, renderer lifecycle, or overlay
placement. Any helper added for this experiment must only read existing state
and log it.

#### Changes

1. **Keep the Experiment 3 revert as the code baseline.**

   Use the current Issue 779 Chromium branch after the revert of
   `Apply synthetic popup bounds`. Do not reintroduce
   `SetTermSurfSyntheticWindowBounds()` or any equivalent behavior.

2. **Add passive Chromium logs for `RenderWidgetHostViewMac` geometry.**

   Add `[termsurf-popup-trace]` logs that only read and print state in
   `content/browser/renderer_host/render_widget_host_view_mac.mm`.

   Log these methods:
   - `SetBounds(const gfx::Rect& rect)`;
   - `GetViewBounds()`;
   - `OnBoundsInWindowChanged(...)`;
   - `OnWindowFrameInScreenChanged(...)`;
   - `SetWindowFrameInScreen(...)` if it is called in this path.

   Each log line must include:
   - method name;
   - input rect, if any;
   - `view_bounds_in_window_dip_`;
   - `window_frame_in_screen_dip_`;
   - computed `GetViewBounds()`;
   - whether `IsHeadless()` is true;
   - whether the view is attached to a window when that is known;
   - enough tab/view identity to correlate events for the same WebContents
     without logging private user data.

3. **Add passive Chromium logs for the native popup path.**

   Add `[termsurf-popup-trace]` logs at the macOS popup entry point for
   `<select>` controls. Start with:
   - `content/browser/renderer_host/render_frame_host_impl.cc`
     `ShowPopupMenu(...)`;
   - `content/browser/renderer_host/popup_menu_helper_mac.mm`
     `PopupMenuHelper::ShowPopupMenu(...)`.

   For each popup log, print:
   - function name;
   - popup anchor bounds passed into the function;
   - the owning `RenderWidgetHostView` `GetViewBounds()` result, when available;
   - the final rect passed to Cocoa/AppKit, when visible in the function;
   - the selected item count or item list size only if already available and
     cheap to log.

   Do not alter popup positioning. Do not normalize coordinates. Do not call
   `SetBounds()`, `OnWindowFrameInScreenChanged()`, or any screen-info update
   from these popup logs.

4. **Add passive logs for datalist/autofill and date input if identifiable
   quickly.**

   Search the local Chromium source for the macOS-specific paths for:
   - datalist/autofill popup display;
   - date/time chooser display.

   If the entry points are clear within a short source search, add the same
   passive log shape there. If they are not clear, do not guess and do not
   broaden the patch. Record in the result that Experiment 4 traced `<select>`
   first and that datalist/date need a separate target.

5. **Preserve existing TermSurf-side logs.**

   Keep the existing Wezboard logs from Experiment 2:
   - `overlay screen rect`;
   - `Resize: pane_id=... screen=(...)`;
   - Chromium `ResizeTab bounds` / `ts_set_view_bounds` received values.

   These are the ground truth for the visible Wezboard webview rect. The
   Chromium popup logs must be compared against these values.

6. **Use deterministic log files under `logs/`.**

   Run Wezboard from the repo root with:

   ```bash
   mkdir -p logs/issue-779-exp4-state/termsurf
   XDG_STATE_HOME="$PWD/logs/issue-779-exp4-state" \
   RUST_LOG=termsurf=info,wezboard_gui::termsurf=info \
     ./wezboard/target/debug/wezboard-gui \
     2>&1 | tee logs/issue-779-exp4-wezboard.log
   ```

   Chromium/Roamium logs should appear at:

   ```bash
   logs/issue-779-exp4-state/termsurf/chromium-server.log
   ```

   Tail Chromium/Roamium logs while testing:

   ```bash
   tail -f logs/issue-779-exp4-state/termsurf/chromium-server.log
   ```

   Extract relevant lines after testing:

   ```bash
   rg "termsurf-popup-trace|Resize:|overlay screen rect|ResizeTab bounds|GetViewBounds|ShowPopupMenu" \
     logs/issue-779-exp4-wezboard.log \
     logs/issue-779-exp4-state/termsurf/chromium-server.log
   ```

7. **Analyze root cause from the logs.**

   The result must answer these exact questions:
   - What is Wezboard's visible webview screen rect?
   - What `screen_x/screen_y/screen_width/screen_height` does Chromium receive
     through `ts_set_view_bounds`?
   - What are `view_bounds_in_window_dip_` and `window_frame_in_screen_dip_`
     immediately before opening the popup?
   - What does `GetViewBounds()` return immediately before opening the popup?
   - What anchor rect does `ShowPopupMenu(...)` receive?
   - Does the popup path add `GetViewBounds()` to the anchor rect, use a native
     `NSView`/`NSWindow` conversion, or use some other coordinate source?
   - Which exact value first diverges from the visible Wezboard webview rect?
   - Based on that divergence, where is the next fix location?

   The conclusion must name one of these fix targets:
   - popup-specific anchor conversion;
   - `PopupMenuHelper` / Cocoa popup wrapper;
   - `RenderWidgetHostViewMac::GetViewBounds()` caller-side use;
   - native `NSView`/`NSWindow` conversion path;
   - TermSurf `Resize` / screen rect computation;
   - another named function discovered in the logs.

#### Verification

1. Build affected components:

   ```bash
   scripts/build.sh chromium
   scripts/build.sh roamium
   scripts/build.sh wezboard
   scripts/build.sh webtui
   ```

2. Start the reproduction server:

   ```bash
   bun test-html/server.ts
   ```

3. Start local Wezboard with deterministic logs:

   ```bash
   mkdir -p logs/issue-779-exp4-state/termsurf
   XDG_STATE_HOME="$PWD/logs/issue-779-exp4-state" \
   RUST_LOG=termsurf=info,wezboard_gui::termsurf=info \
     ./wezboard/target/debug/wezboard-gui \
     2>&1 | tee logs/issue-779-exp4-wezboard.log
   ```

4. In local Wezboard, run local `web` with local Roamium:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/debug/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     http://localhost:9616/test-native-popups.html
   ```

5. Confirm the baseline still works:
   - `web` TUI is visible;
   - browser pane opens;
   - browser controls can close or navigate the browser.

   If the TUI is missing again, stop immediately and mark the experiment Fail.

6. Put the browser pane in a visibly offset location, preferably top-right.

7. Click the `<select>` control. If datalist/date logs were added, click those
   too.

8. Extract logs:

   ```bash
   rg "termsurf-popup-trace|Resize:|overlay screen rect|ResizeTab bounds|GetViewBounds|ShowPopupMenu" \
     logs/issue-779-exp4-wezboard.log \
     logs/issue-779-exp4-state/termsurf/chromium-server.log
   ```

9. Pass criteria:
   - `web` TUI remains fully usable;
   - no behavior changes are introduced;
   - logs identify the first coordinate value that diverges from the visible
     Wezboard webview rect;
   - the result names one concrete next fix location.

10. Fail criteria:

- `web` TUI fails to appear or becomes unusable;
- the browser pane fails to open;
- logs are missing for `SetBounds`, `GetViewBounds`, `OnBoundsInWindowChanged`,
  `OnWindowFrameInScreenChanged`, or `ShowPopupMenu`;
- the result cannot identify the coordinate source used by the `<select>` popup.

**Result:** Fail

Experiment 4 was abandoned and its code was reverted. The Chromium trace logs
and the earlier resize-screen-bounds changes did not produce a safe debugging
path: local testing showed the browser overlay opening while the `web` TUI did
not render as a usable interface.

The Issue 779 Chromium patch archive was deleted, and the active Chromium branch
now contains revert commits for the Issue 779 Chromium changes. The main repo
also reverted the protocol, Roamium, Wezboard, and `test-html` code changes from
this issue.

#### Conclusion

Issue 779 is back to documentation-only state. Any future attempt must start
from the pre-Issue-779 code path and avoid reusing the discarded patch archive
or the failed Chromium trace approach.

### Experiment 5: Restore Native Popup Proof Page

#### Description

Restore the `test-html` proof-of-bug page only. This experiment must not touch
Chromium, Roamium, Wezboard, protobufs, overlay geometry, popup positioning, or
debug logging.

The goal is to bring back a deterministic local page that proves Issue 779
exists: native Chromium/macOS popup UI opens at the wrong screen coordinates
when the webview overlay is not located where Chromium thinks it is.

This page should avoid implying that the HTML control's position inside the page
matters. The controls can be listed plainly. The bug is about the webview
overlay's position on the screen.

#### Changes

1. **Add `test-html/public/test-native-popups.html`.**

   Include native controls that trigger OS/Chromium popup UI:
   - `<select>`;
   - `<input type="date">`;
   - `<input type="time">`;
   - `<input type="datetime-local">`;
   - `<input type="color">`;
   - `<input list>` with `<datalist>`.

2. **Add a simple on-page event log.**

   It should show:
   - last focused control;
   - last clicked control;
   - timestamp;
   - expected behavior: popup should anchor to the clicked control and stay
     associated with the visible webview.

3. **Add a visible page boundary.**

   The boundary is only for screenshots and visual proof that the popup appears
   outside the webview. Do not describe it as testing right, bottom, edge, or
   in-page positioning.

4. **Link the page from `test-html/public/index.html`.**

   Add it under `Input` as:

   `Native Popups — select, date, time, color, datalist`

5. **Do not fix the underlying bug in this experiment.**

   This experiment is only for restoring the reproduction. It should not add
   logs or modify application behavior.

#### Verification

1. Run the test server:

   ```bash
   bun test-html/server.ts
   ```

2. Confirm the page serves:

   ```bash
   curl -I http://localhost:9616/test-native-popups.html
   ```

3. Open the page in a normal browser or Roamium to confirm it renders.

4. In Wezboard, open:

   ```bash
   web http://localhost:9616/test-native-popups.html
   ```

5. Put the browser pane somewhere visibly offset on screen, such as a split
   pane.

6. Click `<select>` first.

   Pass if the native popup appears detached from the visible webview/control,
   proving the bug.

7. Click date, time, color, and datalist controls.

   Pass if any native popup appears at the wrong absolute screen position.

8. Pass criteria:
   - the page exists and is linked from the `test-html` index;
   - it proves the native-popup bug without changing application code;
   - it gives enough visual context for screenshots;
   - it does not add debug logs or attempt a fix.

**Result:** Pass

Restored `test-html/public/test-native-popups.html` with native select, date,
time, datetime-local, color, and datalist controls. The page includes a visible
webview content boundary and an on-page event log with timestamps for the last
focused or clicked control.

Linked the page from `test-html/public/index.html` under Input as
`Native Popups — select, date, time, color, datalist`.

Verification passed:

```bash
curl -I http://localhost:9616/test-native-popups.html
curl -v http://localhost:9616/
curl -v http://localhost:9616/test-native-popups.html
```

Manual verification also passed: the restored page worked as the proof-of-bug
reproduction in Wezboard.

No Chromium, Roamium, Wezboard, protobuf, overlay geometry, popup positioning,
or debug logging code was changed.

#### Conclusion

Issue 779 again has a local proof-of-bug page without restoring the failed fix
or debug patches. Future experiments can use
`http://localhost:9616/test-native-popups.html` to reproduce native popup
mispositioning.

### Experiment 6: Trace TUI, Overlay, and Popup Coordinates Safely

#### Description

Add narrow, low-frequency logs that answer two questions without changing
behavior:

1. What exact coordinate value causes native popups to open away from the
   visible webview?
2. If the `web` TUI fails to render or becomes hidden again, where did that
   happen?

This experiment must avoid the failed Experiment 4 approach. Do not log inside
Chromium hot geometry getters such as `RenderWidgetHostViewMac::GetViewBounds()`
and do not log from repeated Chromium layout or bounds callbacks. Logging must
stay at ownership boundaries where TermSurf already sends, receives, creates, or
positions something.

Use a single log prefix across all components:

```text
[issue-779-trace]
```

Every log line should include the pane id or tab id when available so lines from
Wezboard, `web`, Roamium, and Chromium can be joined manually.

Each component must read `TERMSURF_ISSUE_779_TRACE` once and cache the result:
use `OnceLock<bool>` or equivalent on the Rust side and a function-local static
or equivalent on the Chromium side. Do not read environment variables from paint
loops, overlay placement loops, or popup callbacks after the cached value has
been initialized.

This experiment diagnoses `<select>` first. Date/time inputs, datalist
suggestions, and color pickers may travel through different Chromium or AppKit
paths; if the `<select>` findings do not explain those controls, follow-up
experiments should add similarly narrow logs for those specific subsystems.

#### Changes

1. **Add `webtui` baseline draw logs.**

   In `webtui/src/main.rs`, log only when one of these low-frequency events
   happens:
   - raw/alternate screen setup completes;
   - `terminal.draw(...)` completes for the first frame;
   - `viewport_rect` changes;
   - `SetOverlay` or `SetDevtoolsOverlay` is sent;
   - `BrowserReady` is received;
   - the event loop exits.

   Include:
   - pane id;
   - draw count;
   - terminal frame area;
   - returned viewport rect in cells;
   - current mode;
   - browser URL;
   - whether the overlay message was sent.

   This tells us whether `web` actually drew and what viewport rect it asked
   Wezboard to cover.

   These logs must not use stdout or stderr, because `web` owns the terminal
   screen. Write them directly to:

   ```text
   $XDG_STATE_HOME/termsurf/webtui-trace.log
   ```

   If `XDG_STATE_HOME` is unset, use
   `$HOME/.local/state/termsurf/webtui-trace.log`.

   Also add a `webtui_chrome_overlap` log at `SetOverlay` send time. It should
   compare the viewport rect returned by `ui(...)` against the full terminal
   frame area and classify whether `web` itself is about to request an overlay
   that covers its URL or status chrome.

2. **Add Wezboard overlay receive and placement logs.**

   In `wezboard/wezboard-gui/src/termsurf/conn.rs`, log when Wezboard receives
   `SetOverlay` or `SetDevtoolsOverlay` and when it creates or positions the
   CALayerHost.

   Include:
   - pane id and tab id if known;
   - received overlay cell rect `(col,row,width,height)`;
   - current `cell_width_px` and `cell_height_px`;
   - computed pixel size from current cell metrics;
   - CALayerHost frame in view points;
   - root overlay view bounds;
   - flipped layer frame;
   - positioning layer frame;
   - final host layer frame if available;
   - absolute screen rect from `update_overlay_screen_rect`.

   Add an explicit layer-frame check log:

   ```text
   [issue-779-trace] wezboard_layer_frame_check pane_id=... matches_expected=... positioning_frame=... expected_frame=... reason=...
   ```

   `matches_expected` should be `false` if Cocoa reports a positioning-layer
   frame that differs from the frame Wezboard just assigned.

   Use one of these reason values:
   - `none`;
   - `frame_extends_above_viewport`;
   - `frame_extends_below_viewport`;
   - `frame_extends_left_of_viewport`;
   - `frame_extends_right_of_viewport`;
   - `frame_wider_than_viewport`;
   - `frame_taller_than_viewport`;
   - `missing_viewport`;
   - `unknown`.

   Because `set_overlay_frame` runs on the paint path, only log placement when a
   pane's backing frame changes. Do not emit per-frame steady-state placement
   logs.

3. **Add one Roamium boundary log.**

   In `roamium/src/dispatch.rs`, log only when a resize/create message is
   received from Wezboard and passed through to the FFI.

   Include:
   - tab id;
   - pixel width and height;
   - whether this is initial create or resize.

   Write Roamium trace lines directly to:

   ```text
   $XDG_STATE_HOME/termsurf/roamium-trace.log
   ```

   If `XDG_STATE_HOME` is unset, use
   `$HOME/.local/state/termsurf/roamium-trace.log`.

   Do not add new protocol fields and do not change FFI signatures.

4. **Add one Chromium popup-entry log.**

   In Chromium, add one tab mapping log in `libtermsurf_chromium` where a
   WebContents is created or registered for a TermSurf tab:

   ```text
   [issue-779-trace] chromium_tab_map tab_id=... webcontents=...
   ```

   Then log only at `PopupMenuHelper::ShowPopupMenu(...)` for native `<select>`
   popup positioning. Do not log in `GetViewBounds()` or repeated bounds
   callbacks.

   Include:
   - `webcontents` pointer, so it can be joined to `chromium_tab_map`;
   - input `bounds`;
   - `web_contents->GetContainerBounds()`;
   - computed `bounds_in_screen`;
   - the current `RenderWidgetHostViewMac` pointer;
   - `rwhvm->GetViewBounds()` result read once inside this popup entry log;
   - item count and selected item.

   This single log line should identify whether the popup misplacement comes
   from the element anchor, the WebContents container bounds, or the
   RenderWidgetHostView screen bounds.

5. **Keep all logs opt-in.**

   Logs should only emit when an environment variable is set, for example:

   ```bash
   TERMSURF_ISSUE_779_TRACE=1
   ```

   If the variable is unset, behavior and log volume should be unchanged.

   All log extraction should use only the `[issue-779-trace]` prefix. Each
   component should emit one `trace_enabled component=...` line the first time
   its trace gate fires.

6. **No behavior changes.**

   Do not change overlay geometry, protocol fields, Chromium bounds,
   `RenderWidgetHostViewMac` state, popup placement, focus, input, or lifecycle.

#### Verification

1. Build affected components:

   ```bash
   scripts/build.sh chromium
   scripts/build.sh roamium
   scripts/build.sh wezboard
   scripts/build.sh webtui
   ```

2. Start the test server:

   ```bash
   bun test-html/server.ts
   ```

3. Run a trace-off baseline before enabling any trace.

   Start local Wezboard without `TERMSURF_ISSUE_779_TRACE` and open:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/debug/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     http://localhost:9616/test-native-popups.html
   ```

   Confirm the `web` TUI is visible and usable, and that no `[issue-779-trace]`
   lines appear in any log path. If trace-off is already broken, stop: the
   failure is not caused by the trace experiment.

4. Start local Wezboard with trace enabled and logs in the repo log directory:

   ```bash
   mkdir -p logs/issue-779-exp6-state/termsurf
   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-779-exp6-state" \
   RUST_LOG=info \
     ./wezboard/target/debug/wezboard-gui \
     2>&1 | tee logs/issue-779-exp6-wezboard.log
   ```

5. In local Wezboard, run local `web` with local Roamium:

   ```bash
   TERMSURF_ISSUE_779_TRACE=1 \
   /Users/ryan/dev/termsurf/webtui/target/debug/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     http://localhost:9616/test-native-popups.html
   ```

6. Confirm trace files exist:

   ```bash
   test -f logs/issue-779-exp6-state/termsurf/webtui-trace.log
   test -f logs/issue-779-exp6-state/termsurf/roamium-trace.log
   test -f logs/issue-779-exp6-state/termsurf/chromium-server.log
   ```

7. Confirm baseline TUI behavior first:
   - the `web` TUI is visible;
   - the URL/status chrome is visible;
   - the browser overlay is confined to the viewport area;
   - logs contain `webtui` draw, `webtui_chrome_overlap`, `SetOverlay`, Wezboard
     receive, and Wezboard placement lines;
   - `webtui_chrome_overlap` is `false`;
   - `wezboard_layer_frame_check` reports `matches_expected=true`.

   If the TUI fails to render or is hidden again, stop before clicking native
   controls. The result must use the logs to classify the failure as one of:
   - `webtui` never completed its first draw;
   - `webtui` drew but sent a bad viewport rect;
   - Wezboard received a good viewport but computed a bad overlay frame;
   - Wezboard positioned the overlay correctly but the root/layer frame covered
     more than the viewport;
   - another named point shown by the trace.

8. If the baseline is good, click the `<select>` control on the proof page.

9. Extract logs:

   ```bash
   rg "\\[issue-779-trace\\]" \
     logs/issue-779-exp6-wezboard.log \
     logs/issue-779-exp6-state/termsurf/webtui-trace.log \
     logs/issue-779-exp6-state/termsurf/roamium-trace.log \
     logs/issue-779-exp6-state/termsurf/chromium-server.log
   ```

10. Pass criteria:

- trace is opt-in and quiet when `TERMSURF_ISSUE_779_TRACE` is unset;
- `web` remains visible and usable with trace enabled;
- if `web` is not visible, the logs identify the exact failing stage;
- the `<select>` popup log names the first divergent coordinate among webview
  overlay screen rect, Chromium view bounds, container bounds, and popup
  `bounds_in_screen`;
- no application behavior changes are made.

11. Fail criteria:

- logs emit without `TERMSURF_ISSUE_779_TRACE=1`;
- `web` disappears again and the logs do not identify why;
- native popup misplacement occurs but the logs cannot identify the coordinate
  source;
- any geometry, protocol, FFI, popup, focus, input, or lifecycle behavior is
  changed.

**Result:** Fail

Implemented the opt-in trace points without intentionally changing protocol
fields, FFI signatures, overlay geometry, Chromium bounds, popup placement,
focus, input, or lifecycle behavior.

Build verification passed:

```bash
scripts/build.sh webtui
scripts/build.sh roamium
scripts/build.sh wezboard
scripts/build.sh chromium
```

Manual trace collection showed that the experiment failed to log the only event
that mattered: the native popup placement itself. The native popup was rendered
in the wrong place, but the logs contain no `PopupMenuHelper::ShowPopupMenu`
line and no equivalent native-popup coordinate line.

#### Conclusion

Experiment 6 proved some surrounding paths but failed its diagnostic goal.

What the logs did show:

- `webtui` drew normally, computed viewport `(1,1 158x66)`, and reported no
  chrome overlap;
- Wezboard received the same viewport and computed `2212x2112` backing pixels;
- CALayerHost creation matched the expected frame;
- Roamium received only the webview size;
- Chromium created a `WebContents` and logged its TermSurf tab mapping.

What the logs did not show:

- no `PopupMenuHelper::ShowPopupMenu` line;
- no date/time picker coordinate line;
- no datalist/autofill popup coordinate line;
- no AppKit/native popup window coordinate line.

That means the experiment did not identify the divergent coordinate source. The
next experiment must trace the actual native popup path that fires for the
reproduction page controls, not just the surrounding overlay and tab lifecycle
boundaries.

### Experiment 7: Trace Actual Native Popup Paths

#### Description

Add the Chromium logs Experiment 6 failed to capture: the logs at the exact
native-popup creation and display paths.

Experiment 6 proved that the `web` TUI, Wezboard overlay placement, Roamium tab
creation, and Chromium `WebContents` creation can all be correlated. It failed
because no log fired when the native popup actually opened. That means the
previous hook was either in the wrong popup path or too high-level to observe
the control used in the reproduction page.

This experiment should keep the working Experiment 6 boundary logs, but add
Chromium-only popup-path logs at the places that decide popup bounds and hand
them to AppKit. It must not change popup geometry, Chromium view bounds,
protocol fields, FFI signatures, webtui behavior, overlay placement, focus, or
input behavior.

The goal is not to fix the popup yet. The goal is to produce one trace that
answers these questions:

1. Which Chromium popup path fires for the reproduction control?
2. What bounds does Chromium compute before AppKit receives the popup?
3. What `NSView` / `NSWindow` frame does AppKit use when placing the popup?
4. Where is the first coordinate mismatch relative to the TermSurf overlay
   frame?

#### Changes

1. **Keep the existing opt-in trace gate.**

   Reuse `TERMSURF_ISSUE_779_TRACE=1` and the `[issue-779-trace]` prefix. Do not
   emit any new log line unless the env var is set. Cache the env-var check once
   per process, as in Experiment 6.

2. **Log the renderer-to-browser `<select>` popup request.**

   In `chromium/src/content/browser/renderer_host/render_frame_host_impl.cc`,
   add one gated log in `RenderFrameHostImpl::ShowPopupMenu`.

   The log should include:
   - `RenderFrameHostImpl*`;
   - `WebContents*`, when available;
   - original renderer `bounds`;
   - transformed browser-side bounds;
   - menu item count;
   - selected item;
   - the current `RenderWidgetHostView` bounds, when available.

   This tells us whether Blink sent a native select popup request and whether
   the first browser-process transform is already wrong.

3. **Log the Mac WebContents popup handoff.**

   In `chromium/src/content/browser/web_contents/web_contents_view_mac.mm`, add
   one gated log in `WebContentsViewMac::ShowPopupMenu`.

   The log should include:
   - `WebContents*`;
   - incoming popup bounds;
   - menu item count;
   - selected item.

   This confirms whether the Mac `WebContentsView` path is used before the popup
   reaches the remote Cocoa bridge.

4. **Log the remote Cocoa popup bridge.**

   In
   `chromium/src/content/app_shim_remote_cocoa/render_widget_host_ns_view_bridge.mm`,
   add one gated log in `RenderWidgetHostNSViewBridge::DisplayPopupMenu`.

   The log should include:
   - `menu->bounds`;
   - `menu->selected_item`;
   - item count;
   - `cocoa_view_` pointer;
   - `cocoa_view_.frame`;
   - `cocoa_view_.bounds`;
   - `cocoa_view_.window.frame`;
   - the result of `flipRectToNSRect(menu->bounds)`.

   This is likely the real display path for native `<select>` menus on macOS. If
   this fires, it gives us the Cocoa view/window coordinate context used for
   popup placement.

5. **Log the actual AppKit menu runner placement.**

   In `chromium/src/content/app_shim_remote_cocoa/web_menu_runner_mac.mm`, add
   one gated log in `-[WebMenuRunner runMenuInView:withBounds:initialIndex:]`.

   The log should include:
   - the `view` pointer passed to the runner;
   - input `bounds`;
   - `view.frame`;
   - `view.bounds`;
   - `view.window.frame`;
   - `bounds` converted to window coordinates;
   - those window coordinates converted to screen coordinates;
   - `fakeControlView.frame` after it is added;
   - `fakeControlView.bounds`;
   - initial index;
   - item count.

   This is the key log. It records the native AppKit placement data at the point
   where Chromium creates the fake control view that anchors the native menu.

6. **Log the generic date/time chooser entry if it fires.**

   In `chromium/src/content/browser/date_time_chooser/date_time_chooser.cc`, add
   one gated log in `DateTimeChooser::OpenDateTimeDialog`.

   The log should include:
   - `WebContents*`, when available;
   - dialog type;
   - current value;
   - min;
   - max;
   - step;
   - suggestion count.

   If clicking `date`, `time`, or `datetime-local` produces no line here, the
   result should explicitly say this Chromium path is not used for the macOS
   native control in our embedding.

7. **Log the datalist/autofill popup request if it fires.**

   In the Chromium autofill popup path, add one gated log at the point where the
   popup open arguments are created or sent. Start with
   `components/autofill/core/browser/ui/autofill_external_delegate.cc` and log
   `AutofillClient::PopupOpenArgs`.

   The log should include:
   - element bounds;
   - suggestion count;
   - trigger/source if available;
   - `WebContents*` or another stable pointer that can be joined to the existing
     `chromium_tab_map` line, when available.

   If the exact platform show controller is easy to identify while implementing,
   add one more gated log there. Do not broaden this into a large autofill
   investigation.

8. **Do not add new webtui, Wezboard, Roamium, protocol, or FFI logs.**

   Experiment 6 already logs the surrounding TermSurf boundary state. This
   experiment is only for the native popup paths that were missing.

9. **Keep color picker out of the pass criteria.**

   `<input type="color">` may use `NSColorPanel`, which is a global AppKit panel
   with different placement behavior. It can be clicked during manual testing,
   but this experiment should not require color picker logs to pass.

#### Verification

1. Build Chromium and the local binaries using the normal project scripts.

   ```bash
   scripts/build.sh chromium
   scripts/build.sh roamium
   scripts/build.sh webtui
   scripts/build.sh wezboard
   ```

2. Start the reproduction server:

   ```bash
   bun test-html/server.ts
   ```

3. Start local Wezboard with trace enabled and logs in the repo log directory:

   ```bash
   mkdir -p logs/issue-779-exp7-state/termsurf
   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-779-exp7-state" \
   RUST_LOG=info \
     ./wezboard/target/debug/wezboard-gui \
     2>&1 | tee logs/issue-779-exp7-wezboard.log
   ```

4. In local Wezboard, run local `web` with local Roamium. Start from a normal
   remote page first, then navigate to the reproduction page inside the working
   `web` TUI:

   ```bash
   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-779-exp7-state" \
   /Users/ryan/dev/termsurf/webtui/target/debug/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     ryanxcharles.com
   ```

   Then navigate to:

   ```text
   http://localhost:9616/test-native-popups.html
   ```

5. Click the native `<select>` control first.

   The trace must include at least one of these lines:
   - `RenderFrameHostImpl::ShowPopupMenu`;
   - `WebContentsViewMac::ShowPopupMenu`;
   - `RenderWidgetHostNSViewBridge::DisplayPopupMenu`;
   - `WebMenuRunner::runMenuInView`.

   A useful trace should include all four. If only the lower Cocoa/AppKit logs
   fire, that is still enough to identify the real display path.

6. Click the `date`, `time`, `datetime-local`, and datalist controls.

   For each control, record whether a date/time chooser log, autofill log,
   select-menu log, or no native-popup log fires.

7. Extract the trace:

   ```bash
   rg -a "\\[issue-779-trace\\]" \
     logs/issue-779-exp7-wezboard.log \
     logs/issue-779-exp7-state/termsurf/webtui-trace.log \
     logs/issue-779-exp7-state/termsurf/roamium-trace.log \
     logs/issue-779-exp7-state/termsurf/chromium-server.log
   ```

8. Pass criteria:
   - trace remains opt-in;
   - `web` remains visible and usable;
   - clicking `<select>` emits native-popup placement logs at the actual AppKit
     display path;
   - the logs include both Chromium popup bounds and Cocoa window/screen
     coordinates;
   - the result names the first coordinate stage that diverges from the TermSurf
     overlay frame.

9. Fail criteria:
   - `web` breaks or disappears;
   - logs emit when `TERMSURF_ISSUE_779_TRACE` is unset;
   - a native popup opens but none of the new popup-path logs fire;
   - logs fire but omit the Cocoa view/window/screen coordinates needed to
     identify the bad coordinate source;
   - any behavior changes are introduced.

**Result:** Fail

Implemented the Chromium-only popup-path trace hooks and verified that Chromium
still builds:

```bash
scripts/build.sh chromium
```

The manual run reproduced the native popup placement bug, but none of the new
native-popup trace points fired.

The trace did show:

- Wezboard received the TUI overlay request for pane `0`;
- Wezboard computed the browser viewport as cell rect `(1,1 158x66)` and backing
  size `2212x2112`;
- Wezboard created the CALayerHost at
  `positioning_frame=(x=14.0 y=40.0 width=1106.0 height=1056.0)`;
- Wezboard reported `matches_expected=true` for the layer frame;
- Roamium created a tab for pane `0`;
- Chromium mapped `tab_id=1` to `webcontents=0xc66c3a000`;
- Chromium tracing was enabled.

The trace did not show:

- `RenderFrameHostImpl::ShowPopupMenu`;
- `WebContentsViewMac::ShowPopupMenu`;
- `PopupMenuHelper::ShowPopupMenu`;
- `RenderWidgetHostNSViewBridge::DisplayPopupMenu`;
- `WebMenuRunner::runMenuInView`;
- `WebMenuRunner::fakeControlView`;
- `DateTimeChooser::OpenDateTimeDialog`;
- `AutofillExternalDelegate::ShowSuggestions`.

The Experiment 7 strings were present in the built Chromium output
`chromium/src/out/Default/libcontent.dylib`, so the missing popup logs were not
caused by running a stale Chromium build for the instrumented content hooks.

#### Conclusion

Experiment 7 failed to identify the root cause. It proved that the TermSurf
overlay path and Chromium tab creation path were active, but it did not capture
the native popup creation path. The instrumented Chromium popup APIs were not
the path used by the reproduced native popup, or they were bypassed before those
hooks could run.

The next diagnostic experiment should instrument lower-level Cocoa/AppKit window
creation, frame changes, and ordering in the Chromium/Roamium process so that
any native popup window is logged regardless of which Chromium subsystem creates
it. It should also log the browser-side click/control activation path closely
enough to prove that the click that opens the native popup reached Chromium.

### Experiment 8: Trace Native Widget Windows and Shell Bounds

#### Description

Stop guessing Chromium popup subsystems and log the shared native-window
placement boundary.

Experiments 6 and 7 proved that the surrounding TermSurf overlay path is active,
but they did not capture the native popup path. Experiment 7 also proved that
hooking expected Chromium popup APIs is not reliable enough: the bug reproduced
without hitting `PopupMenuHelper`, `WebMenuRunner`, `DateTimeChooser`, or the
Autofill trace hook.

Further source research showed that the macOS controls split across different
paths:

- native `<select>` uses `PopupMenuHelper` / `WebMenuRunner` /
  `NSPopUpButtonCell`;
- `date`, `time`, and `datetime-local` do not use
  `DateTimeChooser::OpenDateTimeDialog` on macOS Chromium 148; they route
  through Blink page popup / Views widget code;
- datalist/autofill starts in `AutofillExternalDelegate`, but final placement is
  a Views widget;
- color picker is not a useful pass criterion on this build because Chromium
  does not provide the same macOS native color chooser path.

For everything except native `<select>`, the common placement boundary is the
Views native widget bridge:

```text
components/remote_cocoa/app_shim/native_widget_ns_window_bridge.mm
NativeWidgetNSWindowBridge::SetBounds
NativeWidgetNSWindowBridge::SetVisibilityState
```

Those methods set and show the actual `NSWindow` for Views-backed popups. The
root hypothesis is:

```text
Popup windows are anchored to Chromium's hidden Shell NSWindow,
but Wezboard displays the CALayerHost somewhere else.
```

This experiment should therefore log four coordinate sources:

1. the **Wezboard webview screen rect** where the CALayerHost is actually
   visible;
2. Chromium's **believed webview screen rect** from `RenderWidgetHostViewMac`;
3. Chromium content shell's **host `NSWindow` frame**;
4. the **actual popup `NSWindow` frame** before and after
   `NativeWidgetNSWindowBridge` applies bounds and shows it.

The output must be enough to answer:

```text
native_popup_window_frame inside wezboard_webview_screen_rect?
chromium_view_bounds == wezboard_webview_screen_rect?
shell_window_frame == wezboard_webview_screen_rect?
native_popup_window anchored to shell_window_frame?
```

This remains a diagnostic experiment. It must not change popup placement,
Chromium bounds, host-window position, protocol fields, FFI signatures, focus,
input routing, overlay geometry, or TUI behavior.

#### Changes

1. **Keep the existing opt-in trace gate.**

   Reuse `TERMSURF_ISSUE_779_TRACE=1` and `[issue-779-trace]`. Every new log
   must be gated. Cache the env-var check once per process.

2. **Log Wezboard's authoritative webview screen rect.**

   In `wezboard/wezboard-gui/src/termsurf/conn.rs`, extend the existing
   Experiment 6 overlay trace in `create_pending_ca_layer_host` and
   `set_overlay_frame`.

   Log a line named `wezboard_webview_screen_rect` with:
   - `pane_id`;
   - local overlay frame in the Wezboard window;
   - backing-pixel overlay rect;
   - root view/window frame;
   - converted screen rect in Cocoa screen coordinates;
   - scale/dpi.

   The screen rect should be computed with Cocoa conversion APIs from the actual
   view/layer host context, not by hand-rolling origin math.

3. **Log Chromium's believed webview rect on changes.**

   In
   `chromium/src/content/browser/renderer_host/render_widget_host_view_mac.mm`,
   add gated, change-only logs in:
   - `SetBounds`;
   - `OnBoundsInWindowChanged`;
   - `OnWindowFrameInScreenChanged`.

   Log a line named `chromium_webview_bounds` with:
   - `RenderWidgetHostViewMac*`;
   - `WebContents*`, when available;
   - `input_bounds`;
   - `view_bounds_in_window_dip_`;
   - `window_frame_in_screen_dip_`;
   - computed `GetViewBounds()`;
   - `IsHeadless()`;
   - whether the view is attached to an `NSWindow`.

   Do not log from `GetViewBounds()` itself on every call. Logging hot getters
   caused noise in earlier experiments.

4. **Add a Chromium AppKit trace helper.**

   Add a small macOS-only helper in Chromium, preferably under
   `content/libtermsurf_chromium/`, that can:
   - test the trace env var;
   - log `trace_enabled component=chromium-appkit`;
   - describe an `NSRect`;
   - describe an `NSWindow`;
   - install trace observers exactly once.

   The helper should produce stable, grep-friendly lines with one logical event
   per line.

5. **Log the content Shell window frame.**

   In `chromium/src/content/shell/browser/shell_mac.mm`, add gated logs at the
   Shell window lifecycle points that create, resize, or attach WebContents to
   the host window.

   Log a line named `shell_window_frame` with:
   - `Shell*`;
   - `WebContents*`, when available;
   - `NSWindow*`;
   - class name;
   - `frame`;
   - `contentView.frame`;
   - `visible`;
   - `key`;
   - `main`;
   - `level`;
   - `windowNumber`;
   - `screen.frame`, if available;
   - reason (`constructor`, `PlatformSetContents`, `PlatformResizeSubViews`, or
     equivalent actual method names in this Chromium version).

   This proves whether Chromium's hidden Shell host window is frozen at a
   default origin while Wezboard composites the CALayerHost elsewhere.

6. **Log Views popup window bounds at the native bridge.**

   In
   `chromium/src/components/remote_cocoa/app_shim/native_widget_ns_window_bridge.mm`,
   add gated logs in `NativeWidgetNSWindowBridge::SetBounds`.

   Before the existing `setFrame:` call, log:
   - `NativeWidgetNSWindowBridge*`;
   - `NSWindow*`;
   - `new_bounds` in Chromium screen DIP;
   - `actual_new_bounds` after minimum/maximum size adjustment;
   - `window_.frame` before;
   - `window_.screen.frame`;
   - `window_.parentWindow`;
   - `window_.parentWindow.frame`;
   - `window_.level`;
   - `window_.isVisible`;
   - `modal_type_`;
   - `parent_` pointer, if present.

   Immediately after the existing `setFrame:` call, log:
   - `window_.frame` after;
   - `window_.screen.frame` after;
   - whether AppKit changed the requested frame.

   This is the primary hook for date/time page popups, datalist/autofill popups,
   and other Views-backed native popup windows.

7. **Log popup window visibility/order.**

   In the same file, add gated logs in
   `NativeWidgetNSWindowBridge::SetVisibilityState`.

   Log a line named `native_widget_visibility` with:
   - requested `WindowVisibilityState`;
   - `NSWindow*`;
   - `window_.frame`;
   - `window_.screen.frame`;
   - `window_.isVisible`;
   - `window_.isKeyWindow`;
   - `window_.isMainWindow`;
   - parent window pointer/frame;
   - whether the method is about to call `makeKeyAndOrderFront:`,
     `orderWindow:relativeTo:`, `orderFrontKeepWindowKeyState`, or `orderOut:`.

   This is the last boundary before the popup becomes visible.

8. **Keep the Experiment 7 `<select>` logs.**

   Keep `WebMenuRunner::runMenuInView` and
   `RenderWidgetHostNSViewBridge::DisplayPopupMenu`. These cover the native
   `<select>` path, which does not use `NativeWidgetNSWindowBridge::SetBounds`.

   The `<select>` logs should be compared against:
   - Wezboard's screen rect;
   - Chromium's `RenderWidgetHostViewMac` bounds;
   - the Shell `NSWindow` frame;
   - the `WebMenuRunner` `bounds_in_screen`.

9. **Do not rely on dead or too-high hooks.**

   Leave these logs if already present, but do not count on them for pass/fail:
   - `DateTimeChooser::OpenDateTimeDialog`, which is not the macOS path for
     date/time controls in Chromium 148;
   - `AutofillExternalDelegate::ShowSuggestions`, which sees renderer-side
     element bounds but not the final native popup window;
   - `PopupMenuHelper::ShowPopupMenu`, which is redundant for `<select>` once
     `WebMenuRunner` is logged.

10. **Optional fallback: snapshot AppKit/CGWindow state.**

    If the Shell frame logs and `NativeWidgetNSWindowBridge` logs are
    inconclusive, add a fallback snapshot helper that enumerates
    `[NSApp windows]` and `CGWindowListCopyWindowInfo` for the Chromium/Roamium
    process. This should be used only as supporting evidence, not as the primary
    diagnostic path.

    If implemented, the helper should emit `appkit_nsapp_window` and
    `appkit_cgwindow` lines with class/name/window-number/frame/visibility
    fields.

#### Verification

1. Build local components:

   ```bash
   scripts/build.sh chromium
   scripts/build.sh roamium
   scripts/build.sh webtui
   scripts/build.sh wezboard
   ```

2. Start the reproduction server:

   ```bash
   bun test-html/server.ts
   ```

3. Start Wezboard with trace enabled:

   ```bash
   mkdir -p logs/issue-779-exp8-state/termsurf
   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-779-exp8-state" \
   RUST_LOG=info \
     ./wezboard/target/debug/wezboard-gui \
     2>&1 | tee logs/issue-779-exp8-wezboard.log
   ```

4. Inside Wezboard, run local `web` with local Roamium. Start from a normal
   remote page first:

   ```bash
   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-779-exp8-state" \
   /Users/ryan/dev/termsurf/webtui/target/debug/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     ryanxcharles.com
   ```

5. Navigate inside the TUI to:

   ```text
   http://localhost:9616/test-native-popups.html
   ```

6. Click only the native `<select>` first.

   Confirm logs include:
   - `wezboard_webview_screen_rect`;
   - `chromium_webview_bounds`;
   - `shell_window_frame`;
   - `RenderWidgetHostNSViewBridge::DisplayPopupMenu` or
     `WebMenuRunner::runMenuInView`;
   - `WebMenuRunner::fakeControlView`, if the fake-control path is reached.

   This verifies the `<select>` path and gives a direct comparison between the
   Shell window frame, Chromium view bounds, Wezboard webview screen rect, and
   AppKit menu anchor.

7. Click only the `date` input next.

   Confirm logs include:
   - `wezboard_webview_screen_rect`;
   - `chromium_webview_bounds`;
   - `shell_window_frame`;
   - `NativeWidgetNSWindowBridge::SetBounds`;
   - `NativeWidgetNSWindowBridge::SetBounds applied`;
   - `native_widget_visibility`.

   This verifies the Views-backed popup path and should prove whether the popup
   window is positioned relative to the hidden Shell window.

8. Only after the `<select>` and `date` traces are useful, click datalist,
   `time`, and `datetime-local`.

   These are secondary checks. Do not let them obscure the two primary cases.

9. Extract trace logs:

   ```bash
   rg -a "\\[issue-779-trace\\]" \
     logs/issue-779-exp8-wezboard.log \
     logs/issue-779-exp8-state/termsurf/webtui-trace.log \
     logs/issue-779-exp8-state/termsurf/roamium-trace.log \
     logs/issue-779-exp8-state/termsurf/chromium-server.log
   ```

10. Pass criteria:
    - trace remains opt-in;
    - `web` remains visible and usable;
    - the logs include Wezboard's webview screen rect;
    - the logs include Chromium's believed webview rect;
    - the logs include the Shell host `NSWindow` frame;
    - `<select>` logs show the AppKit menu anchor or final menu screen bounds;
    - `date` logs show the Views popup `NSWindow` frame before and after
      `SetBounds`;
    - the result can say whether native popup coordinates are anchored to the
      Shell window frame instead of the Wezboard webview screen rect.

11. Fail criteria:
    - `web` breaks or disappears;
    - logs emit without `TERMSURF_ISSUE_779_TRACE=1`;
    - the `<select>` click produces no `WebMenuRunner` or display-menu log;
    - the `date` click produces no `NativeWidgetNSWindowBridge::SetBounds` or
      visibility log;
    - logs omit either the Wezboard webview screen rect, Chromium view bounds,
      or Shell window frame;
    - the logs still cannot compare popup position against both the webview
      screen rect and Shell window frame;
    - behavior changes are introduced.

**Result:** Partial

The trace run reproduced the bug and identified the coordinate mismatch that
explains the misplaced native popups.

Wezboard's authoritative CALayerHost screen rect was:

```text
wezboard_webview_screen_rect
screen_rect=(x=1653.0 y=307.0 width=1106.0 height=1056.0)
window_frame=(x=1639.0 y=267.0 width=1134.0 height=1216.0)
```

Chromium's hidden content Shell window stayed near the screen origin:

```text
content_shell_window
window_frame={{0, 90}, {800, 656}}
```

Chromium's main webview bounds were then computed from that hidden Shell window
coordinate space:

```text
chromium_webview_bounds
computed_view_bounds=0,654 1106x1056
```

Popup-like Chromium views later appeared in the same wrong coordinate space:

```text
chromium_webview_bounds
computed_view_bounds=70,941 218x281
```

The webtui and Roamium side traces confirmed the expected setup:

```text
webtui_send_set_overlay pane_id=0 viewport=(x=1 y=1 width=158 height=66)
roamium_tab_ready pane_id=0 tab_id=1 handle=0xaeac42000
```

No `NativeWidgetNSWindowBridge` lines fired in this run. That means the clicked
controls did not reach the Views native-widget bridge that Experiment 8
instrumented, or the specific reproduction path used popup `RenderWidgetHost`
views instead. This keeps the experiment from being a full pass.

#### Conclusion

The useful finding is that Wezboard displays the browser at screen
`x=1653, y=307`, while Chromium still believes the browser's AppKit host window
lives around `x=0, y=90`. Native popup placement is therefore anchored to
Chromium's hidden Shell `NSWindow`, not to the CALayerHost's actual on-screen
location inside Wezboard.

The next fix should make Chromium's host-window/screen bounds match the Wezboard
overlay screen rect. A size-only update is insufficient. The likely fix is to
either move/resize the hidden content Shell `NSWindow` to the overlay screen
rect when Roamium receives Wezboard's bounds, or explicitly update the relevant
`RenderWidgetHostViewMac` `window_frame_in_screen_dip_` from that rect.

### Experiment 9: Move Hidden Shell Window to Overlay Rect

#### Description

Use the coordinate mismatch from Experiment 8 to fix native popup placement.

The root cause is now specific: Wezboard composites the Chromium CALayerHost at
the actual terminal pane screen rect, but Chromium's hidden content Shell
`NSWindow` remains at a default screen-origin-ish frame. AppKit native popups
anchor to Chromium's `NSWindow`/`NSView` coordinate space, not to the
CALayerHost's composited location in Wezboard.

The fix should make the hidden Shell host window track the Wezboard overlay
screen rect whenever Roamium receives a resize/screen-rect update. This is
preferable to only forcing `RenderWidgetHostViewMac` cached bounds because
native AppKit popup paths ultimately depend on real `NSWindow`/`NSView`
placement.

This experiment is a narrow behavioral fix:

```text
Wezboard overlay screen rect
  -> Resize message screen_x/screen_y/screen_width/screen_height
  -> Roamium dispatch
  -> libtermsurf_chromium ResizeTab
  -> hidden content Shell NSWindow frame
```

After the fix, Chromium's Shell window frame and Chromium's computed webview
bounds should match the Wezboard webview screen rect. Native popups should then
open inside or adjacent to the visible webview instead of near the old hidden
Shell window origin.

#### Changes

1. **Keep Experiment 8 trace logs in place while implementing.**

   Do not remove the trace logs yet. They are the verification tool for this
   fix. The logs must still be opt-in behind `TERMSURF_ISSUE_779_TRACE=1`.

2. **Document and preserve the Shell window hiding mechanism first.**

   Before changing `ResizeTab`, identify why the content Shell `NSWindow` is not
   visible today. The trace must log, at Shell window construction and after the
   first resize:
   - `alphaValue`;
   - `isVisible`;
   - `level`;
   - `styleMask`;
   - `ignoresMouseEvents`;
   - `collectionBehavior`;
   - `screen.frame`;
   - whether the window is key/main;
   - whether the hidden mode depends on z-order or screen position.

   The initial Experiment 8 trace already showed `alpha=0`, which suggests the
   Shell is hidden by transparency rather than merely by being behind Wezboard.
   Experiment 9 must confirm that moving the Shell window does not make it
   visible, focusable, or mouse-interactive. If invisibility depends on the
   window's old position or z-order, the implementation must hide first, then
   move, and must not ship until the hidden state is stable after the move.

3. **Use the existing Resize screen rect as the source of truth.**

   In `roamium/src/dispatch.rs`, keep forwarding the full Resize information
   from the TermSurf protocol to the Chromium FFI:
   - pixel width/height;
   - screen x/y;
   - screen width/height;
   - screen scale.

   Do not introduce a second protocol message unless the existing Resize path
   cannot carry the needed values.

4. **Update the Chromium FFI resize entrypoint.**

   In `roamium/src/ffi.rs` and the matching C API in
   `chromium/src/content/libtermsurf_chromium/`, make sure the resize function
   passes both size and screen rect into `TsBrowserMainParts::ResizeTab`.

   The API should distinguish:
   - backing-pixel size used for rendering;
   - logical/DIP size used by Chromium;
   - screen rect used for AppKit window placement.

5. **Move the hidden Shell `NSWindow` from `ResizeTab`.**

   In `chromium/src/content/libtermsurf_chromium/ts_browser_main_parts.*`, when
   `ResizeTab` receives a non-empty screen rect:
   - find the `TabState` for the tab;
   - get the content Shell native window for that tab;
   - convert the Resize screen rect into the coordinate convention expected by
     AppKit's `setFrame:display:`;
   - skip the move if the target frame equals the current Shell window frame
     within `0.5` AppKit points on each axis;
   - set the hidden Shell `NSWindow` frame to the overlay screen rect with
     `setFrame:display:NO`;
   - keep the window transparent/hidden as before, without ordering it in front
     of Wezboard;
   - resize the WebContents view to the content area inside that Shell window.

   The Resize message carries top-left screen coordinates in Chromium-style
   screen DIPs. AppKit `NSWindow` frames use bottom-left screen coordinates in
   screen points. Convert explicitly:

   ```text
   appkit_x = screen_x
   appkit_y = primary_screen_frame.height - screen_y - screen_height
   appkit_width = screen_width
   appkit_height = screen_height
   ```

   Use the primary `NSScreen` frame for this first implementation. Multi-screen
   support can be refined later, but the conversion must be explicit and logged.

   Order matters: apply the Shell window `setFrame:display:NO` first, then call
   the existing WebContents resize path such as
   `Shell::ResizeWebContentForTests(logical_size)`. Verify after both calls that
   the WebContents native view frame has origin `(0, 0)` and size
   `content_width x content_height` inside the moved Shell window.

   The Shell window should move; it should not become visible chrome.

6. **Confirm Chromium's normal window-move propagation fires.**

   Keep or add an opt-in `RenderWidgetHostViewMac::OnWindowFrameInScreenChanged`
   trace during this experiment. After moving the Shell `NSWindow`, this trace
   should fire through Chromium's normal AppKit observer path and update
   `window_frame_in_screen_dip_`.

   If moving the hidden Shell window does not trigger
   `OnWindowFrameInScreenChanged`, the implementation must record that in the
   result before considering any fallback.

7. **Prove popup-side anchoring reads the moved Shell window.**

   Keep the Experiment 7/8 `<select>` traces, especially
   `WebMenuRunner::runMenuInView` and
   `RenderWidgetHostNSViewBridge::DisplayPopupMenu`. After the fix,
   `view.window.frame` in the select-menu path must match the Wezboard overlay
   screen rect within `1` DIP.

   This is the direct proof that AppKit popup placement is reading the moved
   Shell host window instead of the old default-origin frame.

8. **Preserve rendering and webtui behavior.**

   The fix must not:
   - break the `web` TUI;
   - cover the terminal chrome;
   - change CALayerHost compositing;
   - change input routing;
   - alter focus behavior beyond what native popup anchoring requires;
   - make the Shell toolbar visible.

9. **Avoid repeating the failed synthetic-bounds approach.**

   Experiment 3 already tried synthetic bounds and broke the webtui. Do not
   casually retry that path in this experiment.

   The specific failure to avoid is double-applying screen coordinates by
   pushing a screen rect through APIs that also update
   `view_bounds_in_window_dip_`, which made Chromium's view geometry and the TUI
   overlay disagree.

   If moving the Shell `NSWindow` proves unsafe or ineffective, the result
   should say so and the next experiment should investigate that failure. A
   synthetic `OnWindowFrameInScreenChanged(screen_rect)` method is only
   acceptable as a separate, explicitly designed follow-up that explains how it
   differs from the failed Experiment 3 approach.

#### Verification

1. Build local components:

   ```bash
   scripts/build.sh chromium
   scripts/build.sh roamium
   scripts/build.sh webtui --release
   scripts/build.sh wezboard
   ```

2. Start the reproduction server:

   ```bash
   bun test-html/server.ts
   ```

3. Start Wezboard with trace enabled:

   ```bash
   mkdir -p logs/issue-779-exp9-state/termsurf
   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-779-exp9-state" \
   RUST_LOG=info \
     ./wezboard/target/debug/wezboard-gui \
     2>&1 | tee logs/issue-779-exp9-wezboard.log
   ```

4. Inside Wezboard, run:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/release/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     ryanxcharles.com
   ```

5. Navigate inside the TUI to:

   ```text
   http://localhost:9616/test-native-popups.html
   ```

6. Confirm the Shell hiding mechanism before testing popups:

   ```text
   content_shell_window alphaValue
   content_shell_window isVisible
   content_shell_window level
   content_shell_window styleMask
   content_shell_window ignoresMouseEvents
   content_shell_window screen.frame
   ```

   The Shell window must remain visually hidden/transparent after the first
   move. Confirm visually that no duplicate Chromium window flashes or peeks
   over the Wezboard overlay.

7. Confirm the trace shows the core geometry now agrees:

   ```text
   wezboard_webview_screen_rect.screen_rect
   content_shell_window.window_frame
   chromium_webview_bounds.computed_view_bounds
   ```

   These should match within `1` DIP. A difference of hundreds or thousands of
   pixels is failure.

8. Confirm Chromium's move propagation:

   ```text
   chromium_webview_bounds event=OnWindowFrameInScreenChanged.after
   ```

   The logged `window_frame_in_screen` and `computed_view_bounds` should reflect
   the moved Shell window. If this trace does not fire after `setFrame`, the
   experiment should be treated as incomplete until that propagation path is
   understood.

9. Click the native controls:
   - `<select>`;
   - `date`;
   - datalist;
   - `time`;
   - `datetime-local`.

   Native popups must open inside or adjacent to the visible browser pane, not
   near the old screen origin and not outside the Wezboard window.

10. For `<select>`, confirm the popup-side AppKit anchor:

    ```text
    WebMenuRunner::runMenuInView view.window.frame
    RenderWidgetHostNSViewBridge::DisplayPopupMenu
    ```

    The `view.window.frame` must match the Wezboard overlay screen rect within
    `1` DIP.

11. Check multiple panes:

    Open two browser panes in different Wezboard splits. In each pane, navigate
    to the reproduction page and click `<select>`. Each popup must anchor to its
    own pane's overlay rect, not the other pane and not the old Shell origin.

12. Check DevTools:

    Open DevTools for a browser pane. Verify that the browser pane still anchors
    native popups correctly after DevTools opens. If DevTools has its own
    overlay pane, its Shell window should also track its own overlay rect.

13. Check window movement and resize:

    With trace off, drag or resize the Wezboard window for roughly three
    seconds. The browser overlay should not flicker, the TUI should remain
    visible, and CPU/frame behavior should be comparable to the baseline before
    the fix.

14. Extract trace logs:

    ```bash
    rg -a "\\[issue-779-trace\\]" \
      logs/issue-779-exp9-wezboard.log \
      logs/issue-779-exp9-state/termsurf/webtui-trace.log \
      logs/issue-779-exp9-state/termsurf/roamium-trace.log \
      logs/issue-779-exp9-state/termsurf/chromium-server.log
    ```

15. Pass criteria:
    - `web` remains visible and usable;
    - Chromium's hidden Shell `NSWindow` frame tracks the Wezboard overlay
      screen rect within `1` DIP;
    - Chromium's computed webview bounds track the Wezboard overlay screen rect;
    - `OnWindowFrameInScreenChanged` fires after the Shell move;
    - `WebMenuRunner` / select-menu logs show `view.window.frame` matching the
      overlay rect;
    - native controls no longer open near the old `x=0` Shell-window origin;
    - native controls no longer open outside the visible webview/Wezboard
      window;
    - the Shell window remains visually hidden/transparent;
    - two browser panes at different split positions both anchor popups
      correctly;
    - DevTools does not regress browser popup anchoring;
    - no new flicker, focus regression, or overlay disappearance appears.

16. Fail criteria:
    - `web` breaks or disappears;
    - the browser overlay covers the TUI chrome incorrectly;
    - the hidden Shell window remains at the old origin;
    - Chromium computed bounds still disagree with Wezboard's screen rect by
      more than rounding tolerance;
    - native popups still open outside the visible webview;
    - moving the Shell window makes the hidden Chromium window visible;
    - moving the Shell window does not trigger Chromium's normal
      `OnWindowFrameInScreenChanged` path;
    - two browser panes fight over one Shell window position;
    - Roamium crashes during normal tab close or app shutdown.

**Result:** Partial

Experiment 9 made the fundamental improvement: native widgets no longer opened
near the old hidden Shell window origin. In the manual run on 2026-05-21, the
native widgets appeared inside the visible webview instead of outside the
Wezboard window. However, their y position was still wrong: the widgets appeared
roughly 100 pixels too low.

The logs show the main coordinate path now works:

- `webtui_first_draw` fired for both runs, and `webtui_chrome_overlap` reported
  `overlaps_chrome=false`. The `web` TUI did not disappear in this run.
- Wezboard computed the visible webview at `screen_rect=(1595,373 1106x1056)` in
  AppKit bottom-left coordinates and `top_left_screen=(1595,371 1106x1056)` in
  Chromium-style top-left coordinates.
- Roamium received and forwarded that rect:
  `roamium_resize_to_ffi ... screen=(1595.0,371.0 1106.0x1056.0) scale=2.00`.
- The hidden Shell window moved from the old default frame
  `{{0, 90}, {800, 656}}` to `{{1595, 373}, {1106, 1112}}` while preserving the
  hidden state: `alphaValue=0`, `isVisible=true`, `ignoresMouseEvents=true`.
- Chromium's normal window movement path fired. After the move,
  `OnWindowFrameInScreenChanged` updated `window_frame_in_screen`, and the main
  webview computed bounds became `1595,371 1106x1056`, matching Wezboard's
  top-left webview rect.

That means the original root cause was correct: Chromium's hidden Shell window
was anchored near the wrong screen origin, and moving it makes native widgets
use the right broad screen region.

The remaining y-axis error is narrower. The logs show the Shell window frame is
not the same object as the browser content rect:

- target webview frame: `{{1595, 373}, {1106, 1056}}`;
- target content frame: `{{1595, 373}, {1106, 1080}}`;
- target Shell window frame: `{{1595, 373}, {1106, 1112}}`;
- main Chromium webview bounds after the move:
  `window_frame_in_screen=1595,315 1106x1112` plus
  `view_bounds_in_window=0,56 1106x1056` produces
  `computed_view_bounds=1595,371 1106x1056`.

The code correctly accounted for that `56` DIP internal webview offset for the
main webview, but the visible native widgets were still too low. This suggests
there is still an inconsistent y correction between at least two of these
spaces:

- Wezboard's AppKit bottom-left overlay rect;
- the top-left screen rect sent through `Resize`;
- the hidden Shell outer window frame;
- the Shell content view frame;
- Chromium's webview frame at y `56`;
- the popup anchor/final native widget window path.

This does not look like the old "wrong screen origin" bug anymore. It also does
not look like a simple physical-vs-logical pixel bug: the backing size
`2212x2112` maps cleanly to the logical webview size `1106x1056` at scale `2`.
The remaining error is an internal y-offset correction, likely involving the
Shell/content/webview vertical offset or a second top-left/bottom-left
conversion in the popup path.

The current trace is still missing the exact comparison needed to compute the
remaining correction mechanically: for the clicked control, it does not log the
control's anchor rect, Chromium's intended popup rect, and the final native
popup window rect in the same coordinate space. The popup-like transient
`chromium_webview_bounds` lines now appear inside the webview region, but they
do not identify the originating control or expected anchor position.

#### Conclusion

Moving the hidden Shell `NSWindow` is the correct fix direction, but the
implementation is not complete. Experiment 9 converts a total placement failure
into an inside-the-webview y-offset bug. The next step should keep the Shell
window move and add a focused y-coordinate trace at the popup anchor boundary:
log the control anchor rect, intended popup rect, final popup window rect, Shell
window frame, content view frame, and webview frame for the same native widget
open event. That should reveal whether the remaining offset is the Shell
content/webview y offset, a top-left/bottom-left conversion error, or an
additional popup-path adjustment.

### Experiment 10: Trace Popup Anchor Y Offset

#### Description

Experiment 9 proved that moving the hidden Shell `NSWindow` fixes the broad
screen-origin bug. Native widgets now open inside the visible webview, but they
are still vertically offset too low. The remaining work is to identify the exact
y delta between the clicked control's intended popup anchor and the final native
popup window/menu frame.

This experiment must be logs-only. It must not change Shell window movement,
popup placement, bounds calculation, protocol semantics, webview sizing, or TUI
behavior. Its only purpose is to put the expected anchor rect and the actual
native widget rect in the same coordinate space for one popup-open event.

The success condition is not visual correctness. The success condition is a log
that makes the next code fix mechanical by answering:

```text
final_popup_y - expected_anchor_y = ?
```

and identifying which coordinate conversion introduced that delta.

#### Changes

1. **Keep the existing Experiment 9 trace enabled.**

   Leave the current `TERMSURF_ISSUE_779_TRACE=1` gated logs in place:
   - `wezboard_webview_screen_rect`;
   - `roamium_resize_to_ffi`;
   - `TermSurfMoveShellWindow.before/after`;
   - `chromium_webview_bounds`;
   - `content_shell_window` construction and resize logs;
   - `webtui_first_draw` and `webtui_chrome_overlap`.

   These logs define the baseline webview, Shell, and TUI geometry.

2. **Use a non-invasive per-popup join key.**

   Do not add fields to Mojo structs or other Chromium protocols just to carry a
   trace id. For this manual experiment, the user clicks exactly one control at
   a time, so `WebContents*`, relevant NSView/NSWindow pointers, and timestamp
   are enough to join the log lines.

   Every popup-open log line should include:
   - the relevant `WebContents*` pointer when available;
   - the relevant view/window pointer;
   - a timestamp or monotonic sequence value local to the process;
   - a path label such as `select`, `views-popup`, `date`, or `datalist`.

   The log prefix remains `[issue-779-trace]`.

3. **Trace `<select>` at the AppKit menu boundary.**

   In the macOS `<select>` path, add a focused log at
   `WebMenuRunner::runMenuInView` in:

   ```text
   chromium/src/content/app_shim_remote_cocoa/web_menu_runner_mac.mm
   ```

   Log:
   - the per-popup join key fields from step 2;
   - `view` pointer;
   - `view.frame`;
   - `view.bounds`;
   - `view.window.frame`;
   - `view.window.contentView.frame`;
   - `view.window.screen.frame`;
   - `view.window.screen.visibleFrame`;
   - the `bounds` / anchor rect passed into `runMenuInView`;
   - that anchor rect converted with `[view convertRect:toView:nil]`;
   - that window rect converted with `[view.window convertRectToScreen:]`;
   - the final frame/rect passed to `NSPopUpButtonCell attachPopUpWithFrame`;
   - the selected item/control point if available.

   The log must clearly label the coordinate space of every rect:

   ```text
   view-local
   window
   appkit-screen-bottom-left
   chromium-screen-top-left
   ```

   Important limitation: for `<select>`, Chromium does not directly observe the
   final NSMenu/NSPopUpButtonCell window frame after AppKit places the menu.
   This path can log the expected anchor rect. The final visible menu position
   must be measured from a screenshot and compared to that anchor. This is not a
   failure of the trace; it is an AppKit ownership boundary.

4. **Trace the RenderWidgetHost popup-menu entry.**

   In the browser/app-shim boundary for popup menus:

   ```text
   chromium/src/content/app_shim_remote_cocoa/render_widget_host_ns_view_bridge.mm
   chromium/src/content/browser/renderer_host/render_frame_host_impl.cc
   chromium/src/content/browser/web_contents/web_contents_view_mac.mm
   ```

   Keep or add logs for:
   - incoming element/control bounds from Blink/renderer;
   - `RenderWidgetHostViewMac::GetViewBounds()`;
   - `RenderWidgetHostViewMac::GetBoundsInRootWindow()` if available at that
     point;
   - any bounds passed across Mojo to the app shim;
   - the per-popup join key fields from step 2.

   Do not modify Mojo IDL or serialized message fields for this experiment.

5. **Trace Views-based native widget popups.**

   For date/time/page-popup/datalist/autofill-style popups, add a final window
   placement log at:

   ```text
   chromium/src/components/remote_cocoa/app_shim/native_widget_ns_window_bridge.mm
   ```

   Specifically log in `NativeWidgetNSWindowBridge::SetBounds` and the
   visibility/order-front path, especially `SetVisibilityState` if that is the
   method that orders the popup window onscreen:
   - the per-popup join key fields from step 2 when available, otherwise
     bridge/window pointer and timestamp;
   - `this` pointer;
   - `window_` pointer;
   - requested Chromium `new_bounds` / `bounds` rect;
   - `window_.frame` before and after;
   - `window_.contentView.frame`;
   - `window_.parentWindow` pointer and frame;
   - `window_.screen.frame`;
   - `window_.screen.visibleFrame`;
   - `window_.level`;
   - `window_.isVisible`;
   - `window_.alphaValue`;
   - whether the window is child/parented to the Shell window.

   These logs are required even if the manual test starts with `<select>`,
   because Experiment 8 showed that not every native control uses the same path.

6. **Log all rects in both AppKit and Chromium screen conventions.**

   For every final native popup rect, log:
   - AppKit screen rect, bottom-left origin;
   - Chromium-style screen rect, top-left origin;
   - the `NSScreen.frame` used for conversion.
   - the `NSScreen.visibleFrame` used to detect AppKit menu-bar/dock
     constraints.

   Use the same conversion formula already used by Wezboard:

   ```text
   top_left_y = screen_frame.origin.y
              + screen_frame.height
              - appkit_rect.origin.y
              - appkit_rect.height
   ```

   This avoids another ambiguous "same numbers, different origin" trace.

7. **Add one computed delta line per Views popup.**

   At the lowest point where both expected anchor and final native rect are
   known, emit one summary line:

   ```text
   popup_y_delta join_key=...
     expected_anchor_top_left=(x,y w x h)
     final_popup_top_left=(x,y w x h)
     delta_x=...
     delta_y=...
     shell_window_top_left=...
     content_view_frame=...
     web_view_frame=...
     rwhv_computed_view_bounds=...
     screen_frame=...
     visible_frame=...
   ```

   If no single function has both expected and final rects, emit the partial
   values with the same join key and explicitly state in the result that the
   delta was computed from joined log lines.

   For `<select>`, `final_popup_top_left` is expected to be unavailable from
   Chromium. Record the anchor rect in logs and compute the delta against a
   screenshot-measured final menu position in the result.

8. **Use the numeric decision tree from Experiment 9.**

   Interpret the measured `delta_y` against these candidate causes:
   - `delta_y ~= 0`: Shell move and popup anchor are correct; remaining visual
     issue is likely AppKit menu behavior or human measurement error.
   - `delta_y ~= 24`: Shell content chrome height from Experiment 9
     (`web_chrome_height=24`) is being added or skipped incorrectly.
   - `delta_y ~= 32`: AppKit title/frame height is being mixed with content
     coordinates.
   - `delta_y ~= 56`: the exact webview-in-window offset from Experiment 9
     (`view_bounds_in_window.y=56`) is being applied on the wrong side of the
     conversion.
   - `delta_y ~= 100`: likely a composition of multiple offsets, such as `56`
     plus AppKit menu positioning, selected-item alignment, menu bar, or visible
     frame clamping.
   - `delta_y ~= screen.frame/visibleFrame delta`: AppKit is constraining the
     popup to `NSScreen.visibleFrame`.

   The result must name which bucket the measured value falls into, or state
   that it does not match any known candidate.

9. **Keep logging opt-in and low volume.**

   All new logs must be gated by `TERMSURF_ISSUE_779_TRACE=1`.

   Do not log from hot getters on every frame. Logging should happen only when a
   popup is requested, positioned, shown, or dismissed.

10. **Do not modify popup behavior.**

    This experiment must not:
    - move the Shell window differently;
    - change `SetBounds`;
    - change `setFrame:display:`;
    - change popup anchor math;
    - change protocol fields;
    - change Wezboard overlay placement;
    - change webtui layout.

#### Verification

0. Run a trace-off baseline.

   Before enabling `TERMSURF_ISSUE_779_TRACE`, run the current build once with
   no trace env var. Confirm:
   - `web` is fully visible and usable;
   - the browser overlay appears normally;
   - no `[issue-779-trace]` lines are emitted to the log paths.

   If trace-off is broken, stop. The logging experiment cannot diagnose popup
   placement until the baseline works.

1. Build the same components as Experiment 9:

   ```bash
   scripts/build.sh chromium
   scripts/build.sh roamium
   scripts/build.sh webtui --release
   scripts/build.sh wezboard
   ```

2. Start the reproduction page server:

   ```bash
   cd /Users/ryan/dev/termsurf/test-html
   bun run server.ts
   ```

3. Start Wezboard with tracing enabled:

   ```bash
   cd /Users/ryan/dev/termsurf
   mkdir -p logs/issue-779-exp10-state/termsurf

   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-779-exp10-state" \
   RUST_LOG=info \
   ./wezboard/target/debug/wezboard-gui \
   2>&1 | tee logs/issue-779-exp10-wezboard.log
   ```

4. Inside Wezboard, launch:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/release/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     https://ryanxcharles.com
   ```

5. Navigate inside `web` to:

   ```text
   http://localhost:9616/test-native-popups.html
   ```

6. Keep the layout deterministic.

   Use the same single-pane layout shape as the Experiment 9 run unless there is
   a reason to test splits. Before clicking a control, take or visually inspect
   a screenshot and note the clicked control's position relative to the top-left
   of the visible webview. This verifies that the logged expected anchor is
   itself correct.

7. Click exactly one `<select>` control. Close it. Then click exactly one
   date/time-style control if available. Avoid clicking every control at once;
   this experiment needs clean, joinable popup events.

8. Extract the trace:

   ```bash
   rg -a "\[issue-779-trace\]|popup_y_delta|WebMenuRunner|NativeWidgetNSWindowBridge" \
     logs/issue-779-exp10-wezboard.log \
     logs/issue-779-exp10-state/termsurf/webtui-trace.log \
     logs/issue-779-exp10-state/termsurf/roamium-trace.log \
     logs/issue-779-exp10-state/termsurf/chromium-server.log \
     > logs/issue-779-exp10-trace.log
   ```

9. Pass criteria:
   - `web` remains visible and usable;
   - native widgets still appear inside the webview, preserving Experiment 9's
     improvement;
   - each clicked popup produces one joinable pointer/timestamp chain;
   - for `<select>`, the trace includes the expected anchor rect in top-left
     screen coordinates, and that anchor is within `4` DIP of the
     screenshot-measured clicked control position;
   - for Views-based popups, the trace includes expected anchor rect and final
     native popup rect in the same top-left screen coordinate system;
   - the trace computes or allows computing `delta_x` and `delta_y`; for
     `<select>`, the final menu position may come from screenshot measurement;
   - the trace shows whether the remaining y offset matches the Shell
     content/webview offset, the AppKit frame/titlebar offset, or a separate
     popup-path conversion.
   - `NSScreen.frame` and `NSScreen.visibleFrame` are present on the relevant
     popup lines.

10. Fail criteria:
    - `web` disappears or the browser covers the TUI chrome;
    - native widgets return to the old outside-the-window placement;
    - logs do not include the clicked control anchor rect;
    - logs do not include the final native popup rect for Views-based popups;
    - logs cannot join expected and actual rects for one Views popup-open event;
    - the `<select>` anchor cannot be compared to a screenshot-measured menu
      position;
    - the logs add noise without producing a concrete y delta.

**Result:** Fail

The manual trace run showed that the experiment did not produce the information
needed to explain the remaining date picker y-axis error.

The base overlay geometry was stable. WebTUI sent one viewport, Roamium resized
the browser to `screen=(1956.0,485.0 1106.0x1056.0)`, Wezboard reported the same
webview top-left, and Chromium's main `RenderWidgetHostViewMac` resolved to
`computed_view_bounds=1956,485 1106x1056`.

The `<select>` path produced a complete expected anchor chain. The select
control had `input_bounds=407,385 292x43`; adding that to the webview top-left
gives `2363,870 292x43`, exactly matching the traced
`bounds_top_left_screen={{2363.0, 870.0}, {292.0, 43.0}}`. This means the
`<select>` anchor rect was correct in both x and y. However, Chromium still did
not expose AppKit's final menu window frame, so the trace could not compare the
visible menu position to Chromium's anchor without an external screenshot
measurement.

The date picker did not hit the expected `NativeWidgetNSWindowBridge` trace
path. No `NativeWidgetNSWindowBridge::SetBounds path=views-popup` lines were
emitted. Instead, the date interaction produced transient
`RenderWidgetHostViewMac` views with `webcontents=0`, including bounds such as
`2026,772 218x281` and `2364,774 172x260`. Those rects showed that date-related
popup views were inside the webview, but the run did not log the date input's
expected anchor rect before those popup views were created. Therefore the trace
could not compute the required date picker `delta_y`.

A separate regression or previously unnoticed bug appeared during the run: after
the `<select>` interaction, native widgets stopped appearing. The log contains
many later `CursorChanged` messages, which indicates Chromium was still
receiving hover/cursor movement, but there were no later
`RenderFrameHostImpl::ShowPopupMenu`, `WebContentsViewMac::ShowPopupMenu`,
`DisplayPopupMenu`, `WebMenuRunner`, `NativeWidgetNSWindowBridge`, or date popup
view creation logs. That means subsequent clicks did not reach any popup request
path at all. This is upstream of popup placement and was not diagnosed by this
experiment.

#### Conclusion

Experiment 10 failed because it did not teach the needed fact: the exact y delta
between the date input's expected anchor and the date popup's final rect. It
confirmed that the base webview placement is stable and that the `<select>`
anchor calculation is correct, but it did not identify why the date picker y
position is wrong. The next experiment must trace the date/page-popup anchor
before the transient `webcontents=0` popup `RenderWidgetHostViewMac` is created,
and it must also trace click delivery/menu lifecycle state after a popup closes
to explain why later native widgets stop opening.

### Experiment 11: Trace Date PagePopup Anchor

#### Description

Experiment 10 failed because it traced the wrong final boundary for the date
picker. The date picker did not use `NativeWidgetNSWindowBridge`; it used
Blink's `DateTimeChooserImpl` and `PagePopup` path, then created transient
`RenderWidgetHostViewMac` views with `webcontents=0`.

This experiment is still logs-only. It must not change popup positioning, window
movement, Mojo fields, layout, event routing, or webview geometry. Its purpose
is to put the date input's expected anchor, the `PagePopup` requested window
rect, and the transient popup view's actual first screen rect into one trace
chain.

The success condition is one date-popup summary line that answers:

```text
date_popup_final_y - date_input_anchor_y = ?
```

and identifies whether the bad y value was introduced before `OpenPagePopup`, at
the browser `ShowCreatedWidget` IPC boundary, inside
`RenderWidgetHostViewMac::InitAsPopup`, or while the transient
`RenderWidgetHostViewMac` view was attached to the Shell window.

#### Changes

1. **Keep all Experiment 10 logs.**

   The existing logs remain useful baselines:
   - Wezboard webview top-left screen rect;
   - Roamium resize rect;
   - Shell window move before/after;
   - main `RenderWidgetHostViewMac` `computed_view_bounds`;
   - `<select>` anchor trace.

   Do not remove or reinterpret those logs. Experiment 11 adds the missing
   date/page-popup chain.

2. **Trace `DateTimeChooserImpl` at creation time.**

   Add a `TERMSURF_ISSUE_779_TRACE=1` gated log in:

   ```text
   chromium/src/third_party/blink/renderer/core/html/forms/date_time_chooser_impl.cc
   ```

   Log when `DateTimeChooserImpl` is constructed and calls `OpenPagePopup`.
   Include:
   - `path=date-page-popup`;
   - `this` pointer;
   - `frame_` pointer;
   - `client_` pointer;
   - `OwnerElement()` pointer;
   - owner element tag/type if cheap to access;
   - owner element bounds in local document/frame coordinates;
   - owner element bounds after any available absolute, viewport, or screen
     conversion in Blink;
   - frame view scroll offset;
   - visual viewport offset and scale if available;
   - device scale factor if available;
   - the `PagePopup*` returned by `OpenPagePopup`.

   The log must label every coordinate space. If Blink cannot produce screen
   coordinates at this point without invasive code, log the most precise
   renderer/frame/viewport coordinates and the scroll/viewport values needed to
   compute the screen anchor later.

3. **Trace `ChromeClientImpl::OpenPagePopup`.**

   Add a focused log in:

   ```text
   chromium/src/third_party/blink/renderer/core/page/chrome_client_impl.cc
   ```

   Log:
   - `path=date-page-popup`;
   - `PagePopupClient*`;
   - `web_view_`;
   - the returned `WebPagePopupImpl*`;
   - the opener `WebViewImpl` page-popup pointer before and after opening;
   - any available owner element pointer from the `PagePopupClient`.

   This confirms whether the date popup reached the same renderer-side
   `PagePopup` entry every time.

4. **Trace `WebViewImpl::OpenPagePopup` and close lifecycle.**

   Add logs in:

   ```text
   chromium/src/third_party/blink/renderer/core/exported/web_view_impl.cc
   ```

   Log:
   - before opening a page popup;
   - after `page_popup_` is assigned;
   - `ClosePagePopup`;
   - `CleanupPagePopup`;
   - `CancelPagePopup`.

   Include the opener `WebViewImpl*`, `PagePopupClient*`, `WebPagePopupImpl*`,
   `page_popup_`, and whether another popup was already open. This is the
   minimal lifecycle trace needed to diagnose the "later widgets stop opening"
   failure without distracting from the date y-axis question.

5. **Trace `WebPagePopupImpl::SetWindowRect`.**

   Add the primary date popup bounds log in:

   ```text
   chromium/src/third_party/blink/renderer/core/exported/web_page_popup_impl.cc
   ```

   In `WebPagePopupImpl::SetWindowRect`, log:
   - `path=date-page-popup`;
   - `this` pointer;
   - `opener_web_view_`;
   - `popup_client_`;
   - popup handle;
   - incoming `rect_in_screen`;
   - `OwnerWindowRectInScreen()`;
   - `GetAnchorRectInScreen()`;
   - `WindowRectInScreen()` before and after if available;
   - `ViewRect()`;
   - `ViewportVisibleRect()`;
   - `GetScreenInfo()` rect and device scale factor;
   - the callback point for `DidSetBounds`.

   This line shows whether Blink requested the wrong top-left y before the popup
   reaches browser-process widget creation.

6. **Trace `WebContentsImpl::ShowCreatedWidget`.**

   Add the primary browser-process IPC boundary log in:

   ```text
   chromium/src/content/browser/web_contents/web_contents_impl.cc
   ```

   In `WebContentsImpl::ShowCreatedWidget`, log:
   - `path=date-page-popup`;
   - `this` `WebContentsImpl*`;
   - `process_id`;
   - `widget_route_id`;
   - the `initial_rect` received from the renderer;
   - the `initial_anchor_rect` received from the renderer;
   - `needs_transform`;
   - `transformed_rect`;
   - `transformed_anchor_rect`;
   - `GetRenderWidgetHostView()` for `this`;
   - the `outermost_web_contents`;
   - the parent `view` passed to `InitAsPopup`;
   - the `widget_host_view` returned by `GetCreatedWidget`;
   - the popup `RenderWidgetHostImpl*`;
   - `GetVisibility()`;
   - `rwh->is_active()`;
   - whether the popup is destroyed by the unfocused-window guard,
     background-tab guard, or permission exclusion-area guard.

   This line answers whether the renderer IPC already supplied a bad popup rect,
   or whether the y error appears later in the macOS popup view code.

7. **Trace `RenderWidgetHostViewMac::InitAsPopup`.**

   Add the highest-priority macOS popup placement log in:

   ```text
   chromium/src/content/browser/renderer_host/render_widget_host_view_mac.mm
   ```

   In `RenderWidgetHostViewMac::InitAsPopup`, log:
   - `path=date-page-popup`;
   - `this` popup `RenderWidgetHostViewMac*`;
   - `host()` pointer if available;
   - `parent_host_view`;
   - `popup_parent_host_view_`;
   - incoming `pos`;
   - incoming `anchor_rect`;
   - parent `GetViewBounds()`;
   - parent `GetBoundsInRootWindow()`;
   - existing `popup_child_host_view_` before replacement;
   - `ns_view_`;
   - parent `ns_view_id_`;
   - `window_frame_in_screen_dip_`;
   - `view_bounds_in_window_dip_`;
   - `computed_view_bounds` using the same formula as the existing
     `chromium_webview_bounds` trace.

   This is the smoking-gun log for date/page-popup placement. `pos` is the
   popup's intended screen rect, `anchor_rect` is the element anchor, and the
   parent bounds establish whether the rects are relative to the visible webview
   or to the hidden Shell window/frame.

8. **Trace transient popup `RenderWidgetHostViewMac` bounds updates.**

   Extend the existing `chromium_webview_bounds` log in:

   ```text
   chromium/src/content/browser/renderer_host/render_widget_host_view_mac.mm
   ```

   For `webcontents=0` views, add a page-popup-specific line when possible:
   - `path=date-page-popup-or-transient`;
   - view pointer;
   - widget pointer if available;
   - parent/root view pointer if available;
   - `input` rect;
   - `view_bounds_in_window`;
   - `window_frame_in_screen`;
   - `computed_view_bounds`;
   - `attached_to_window`;
   - `is_headless`;
   - whether the view has a `WebContents` pointer.

   If a direct `WebPagePopupImpl*` join key is unavailable at this layer, use
   timestamp plus the unique `webcontents=0` view pointer and the incoming rect
   to join with `WebContentsImpl::ShowCreatedWidget` and
   `RenderWidgetHostViewMac::InitAsPopup`.

9. **Trace `<select>` menu close only if needed.**

   Keep the `<select>` regression trace narrow. Add or keep logs around:
   - `WebMenuRunner` return/cancel/dismissal;
   - `PopupMenuHelper` close/destruction if present;
   - the top of `RenderFrameHostImpl::ShowPopupMenu`, including whether a popup
     helper is already alive.

   This is not the main date y-axis path. Its only job is to explain the
   Experiment 10 symptom where later widgets stopped opening after a select menu
   closed.

10. **Add one computed `date_popup_y_delta` summary.**

At the lowest point where enough values are available, or in the result by
joining log lines, produce this summary:

```text
date_popup_y_delta join_key=...
  input_anchor_top_left=(x,y w x h)
  page_popup_requested_top_left=(x,y w x h)
  show_created_widget_initial=(x,y w x h)
  show_created_widget_anchor=(x,y w x h)
  init_as_popup_pos=(x,y w x h)
  init_as_popup_anchor=(x,y w x h)
  transient_rwhv_top_left=(x,y w x h)
  delta_input_to_requested_x=...
  delta_input_to_requested_y=...
  delta_requested_to_show_created_x=...
  delta_requested_to_show_created_y=...
  delta_show_created_to_init_as_popup_x=...
  delta_show_created_to_init_as_popup_y=...
  delta_init_as_popup_to_transient_x=...
  delta_init_as_popup_to_transient_y=...
  webview_top_left=...
  shell_window_frame=...
  webview_in_window_offset=...
  screen_info=...
```

If the summary cannot be emitted in code without behavior risk, the result must
compute the same fields manually from joined trace lines.

11. **Keep click lifecycle logs minimal but sufficient.**

To diagnose the "native widgets stop opening" failure only if it recurs, add
logs for:

- date/page-popup close and cleanup;
- `<select>` close/cancel return from `WebMenuRunner`;
- whether `WebViewImpl` still has `page_popup_` set after close;
- whether the next popup request reaches `DateTimeChooserImpl`, `OpenPagePopup`,
  or `ShowPopupMenu`.

Do not add high-volume mouse-move logging. Cursor movement was already visible
in Experiment 10 and did not identify the missing popup request.

12. **Do not change behavior.**

This experiment must not:

- change `SetWindowRect`;
- change `ShowCreatedWidget`;
- change `InitAsPopup`;
- change `SetBounds`;
- change `OpenPagePopup`;
- change popup anchor math;
- change event dispatch;
- change Mojo structs or protocol fields;
- change Shell window placement;
- change Wezboard or webtui layout.

#### Verification

0. Run a trace-off baseline with the same binaries if possible. Confirm `web`
   remains usable and no `[issue-779-trace]` lines are emitted.

1. Build the same components:

   ```bash
   scripts/build.sh chromium
   scripts/build.sh roamium
   scripts/build.sh webtui --release
   scripts/build.sh wezboard
   ```

2. Start the reproduction page server:

   ```bash
   cd /Users/ryan/dev/termsurf/test-html
   bun run server.ts
   ```

3. Start Wezboard with Experiment 11 log paths:

   ```bash
   cd /Users/ryan/dev/termsurf
   mkdir -p logs/issue-779-exp11-state/termsurf

   TERMSURF_ISSUE_779_TRACE=1 \
   XDG_STATE_HOME="$PWD/logs/issue-779-exp11-state" \
   RUST_LOG=info \
   ./wezboard/target/debug/wezboard-gui \
   2>&1 | tee logs/issue-779-exp11-wezboard.log
   ```

4. Inside Wezboard, launch:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/release/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     https://ryanxcharles.com
   ```

5. Navigate to:

   ```text
   http://localhost:9616/test-native-popups.html
   ```

6. Keep the layout deterministic and click exactly one date/time control first.
   Do not click a `<select>` before the date control in the primary run, because
   Experiment 10 showed that the `<select>` path may leave later popup requests
   stuck.

7. If the date popup appears, close it and click the same date control one more
   time. This checks whether the date popup lifecycle cleans up.

8. In a separate run, click one `<select>`, close it, then click one date
   control. This isolates whether the stop-opening bug is caused by the select
   menu lifecycle.

9. Extract the trace:

   ```bash
   rg -a "\[issue-779-trace\]|date_popup_y_delta|DateTimeChooserImpl|OpenPagePopup|WebPagePopupImpl|ShowCreatedWidget|InitAsPopup|webcontents=0|WebViewImpl::ClosePagePopup|WebViewImpl::CleanupPagePopup|WebMenuRunner|PopupMenuHelper" \
     logs/issue-779-exp11-wezboard.log \
     logs/issue-779-exp11-state/termsurf/webtui-trace.log \
     logs/issue-779-exp11-state/termsurf/roamium-trace.log \
     logs/issue-779-exp11-state/termsurf/chromium-server.log \
     > logs/issue-779-exp11-trace.log
   ```

10. Pass criteria:
    - the first date click produces a joined
      `DateTimeChooserImpl -> OpenPagePopup -> WebPagePopupImpl::SetWindowRect -> WebContentsImpl::ShowCreatedWidget -> RenderWidgetHostViewMac::InitAsPopup -> webcontents=0 RWHV`
      chain;
    - the trace includes the date input anchor or enough renderer coordinates
      plus scroll/viewport values to compute it;
    - the trace includes the page popup requested rect in screen coordinates;
    - the trace includes the browser `initial_rect` and `initial_anchor_rect`
      from `ShowCreatedWidget`;
    - the trace includes the `InitAsPopup` `pos` and `anchor_rect`;
    - the trace includes the transient popup view's first computed screen rect;
    - the result can compute `delta_input_to_requested_y` and
      `delta_requested_to_show_created_y`,
      `delta_show_created_to_init_as_popup_y`, and
      `delta_init_as_popup_to_transient_y`;
    - the measured delta maps to one likely cause: renderer anchor, page-popup
      rect calculation, `ShowCreatedWidget` transform, `InitAsPopup` macOS
      conversion, `56` DIP webview-in-window offset, top-left/bottom-left
      conversion, or transient RWHV attachment.

11. Fail criteria:
    - the date click does not produce a `DateTimeChooserImpl` or `OpenPagePopup`
      log;
    - the date click does not produce `ShowCreatedWidget` or `InitAsPopup` logs;
    - the trace still only shows transient `webcontents=0` bounds without the
      upstream date input anchor;
    - the log cannot join the page-popup request to the transient popup view;
    - the experiment adds high-volume event noise without a computable y delta;
    - the logging changes alter popup behavior.

**Result:** Partial

Experiment 11 produced the missing joined PagePopup chain and identified where
the date/time/color-family y value first becomes wrong. The user tested date,
time, date-time, color, dropdown, and datalist controls. Date, time, date-time,
and color all appeared with the wrong y value and stayed visible after alt-tab
when the TermSurf window was no longer active. The dropdown behaved differently:
its y value appeared correct, but its x value was wrong. After interacting with
dropdown/datalist controls, native widgets eventually stopped appearing for the
rest of the session.

The date/PagePopup trace is now complete. A representative date click produced:

```text
anchor_rect = 1916,680 291x43
WebPagePopupImpl::SetWindowRect rect_in_screen = 1916,723 218x281
WebContentsImpl::ShowCreatedWidget initial_rect = 1916,723 218x281
RenderWidgetHostViewMac::InitAsPopup pos = 1916,723 218x281
transient RenderWidgetHostViewMac bounds = 1916,723 218x281
```

The requested popup y is exactly the input anchor y plus the input anchor
height: `723 = 680 + 43`. This same pattern repeated for the other
PagePopup-style controls:

- date: `delta_input_to_requested_y = 43`;
- time: `delta_input_to_requested_y = 45`;
- date-time/color-family PagePopup controls: `delta_input_to_requested_y` was
  the control height, typically `43` or `44`.

There was no additional y drift at the browser-process IPC boundary or in the
macOS popup view code:

```text
delta_requested_to_show_created_y = 0
delta_show_created_to_init_as_popup_y = 0
delta_init_as_popup_to_transient_y = 0
```

That means the wrong y value is already present in Blink's PagePopup placement
request. `ShowCreatedWidget`, `RenderWidgetHostViewMac::InitAsPopup`, and the
transient `webcontents=0` RWHV preserve the requested rect exactly. The y-axis
bug for date/time/color is therefore not a Wezboard overlay, Shell window,
`ShowCreatedWidget`, or `InitAsPopup` conversion bug. It is upstream in Blink's
PagePopup positioning decision.

The dropdown followed the separate `<select>` AppKit menu path:

```text
RenderFrameHostImpl::ShowPopupMenu
PopupMenuHelper::ShowPopupMenu
RenderWidgetHostNSViewBridge::DisplayPopupMenu
WebMenuRunner::runMenuInView
NSPopUpButtonCell
```

For the observed dropdown, Chromium computed the select anchor as:

```text
bounds_top_left_screen = 2253,821 292x43
```

This agrees with the user's observation that the dropdown y value looked right.
The wrong x value is a separate AppKit menu placement problem. Chromium still
does not observe the final `NSPopUpButtonCell` menu window rect, so the final
dropdown x delta remains visually observed rather than fully log-computable.

The "all native widgets stop opening" failure also appears separate from stale
PagePopup or stale `PopupMenuHelper` state. After the PagePopup controls closed,
the trace showed clean PagePopup cleanup:

```text
WebViewImpl::CleanupPagePopup page_popup_after=0
```

After the dropdown closed, the trace showed:

```text
WebMenuRunner::runMenuInView.return
PopupMenuHelper::PopupMenuClosed
PopupMenuHelper::~PopupMenuHelper
PopupMenuHelper::Hide
```

Later clicks still produced `CursorChanged` messages, so Chromium continued to
receive some pointer movement, but the clicks no longer reached
`DateTimeChooserImpl`, `WebPagePopupImpl::SetWindowRect`, or
`RenderFrameHostImpl::ShowPopupMenu`. This suggests the stuck state is upstream
of the popup implementations, likely in activation, focus, event dispatch, or
AppKit menu tracking state, not in an uncleared popup object.

The alt-tab persistence is another distinct bug. PagePopup widgets are separate
transient popup RWHV/NSWindow-style views, and the trace did not show any
hide/close tied to TermSurf window deactivation. Date/time/color popups can
therefore remain visible after the hosting TermSurf window loses focus.

#### Conclusion

Experiment 11 succeeded at identifying the y-axis fault boundary for the primary
date input problem: Blink requests the PagePopup at the bottom edge of the input
control, and every downstream layer preserves that rect. The next experiment
should focus on Blink's PagePopup placement calculation, specifically why
`WebPagePopupImpl` requests `anchor.y + anchor.height` when the visual popup
should be aligned differently in the embedded TermSurf window.

Keep the other findings as separate issues or later experiments:

- dropdown uses a different AppKit `<select>` path and has a wrong x value, not
  the PagePopup y bug;
- PagePopup widgets remain visible after alt-tab/window deactivation;
- after dropdown/datalist interactions, native popup activation can stop
  reaching the popup-open paths even though cursor events still arrive.
