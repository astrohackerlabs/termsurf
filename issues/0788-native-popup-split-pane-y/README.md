+++
status = "open"
opened = "2026-05-27"
+++

# Issue 788: Native Popup Y Is Wrong in Top Split Webviews

## Goal

Native browser popups should anchor to controls inside a webview regardless of
which Wezboard split pane contains the webview.

In particular, when a webview is in the top pane and a terminal pane is split
below it, native popups such as `<select>` menus must open at the clicked
control, not far down in the lower pane.

## Background

Issue 779 fixed the original native popup positioning problem for full-pane and
right-split webviews. The work established that Chromium native popups are not
drawn into the CALayerHost texture. They are native Chromium/AppKit popup
windows positioned from Chromium's internal view/window screen geometry.

That issue added the current geometry pipeline:

1. Wezboard computes the webview overlay's local frame from the painted pane.
2. Wezboard converts that local frame to a screen rect.
3. Wezboard sends that rect to Roamium through the existing `Resize` message.
4. Roamium forwards the rect to `libtermsurf_chromium`.
5. Chromium moves the hidden TermSurf Shell window to that screen rect so native
   popup code sees the same origin as the visible CALayerHost overlay.

The full-pane case still looks acceptable. The new failure appears when the
webview is placed in the top split pane with another terminal pane below it. The
visible webview remains in the top pane, but a native `<select>` menu opens far
down in the lower pane.

That means the visible CALayerHost overlay and Chromium's hidden native
coordinate proxy disagree about the webview's screen Y position.

## Analysis

The likely source of the mismatch is the conversion in:

```text
wezboard/wezboard-gui/src/termsurf/conn.rs::webview_screen_rect_desc()
```

That function receives `local_frame` from the CALayer positioning layer. The
CALayer tree used for webview overlays is explicitly flipped:

```text
setGeometryFlipped:YES
```

The function then passes that layer frame directly into AppKit view conversion:

```text
view convertRect:local_frame toView:nil
window convertRectToScreen:...
```

This is suspicious because a flipped CALayer frame uses the overlay layer's
top-left-style coordinate system, while `NSView convertRect:toView:` expects the
view's coordinate system. In a full-height webview the mistake is mostly hidden
because the overlay height nearly equals the view height. In a top split pane,
the same mistake can map the top pane's layer rect into the lower part of the
window, exactly matching the screenshot.

The previous fixes did not catch this because Issue 779 focused on:

- moving Chromium's hidden Shell window to the reported screen rect; and
- correcting Blink PagePopup y from `anchor.y + anchor.height()` to `anchor.y`.

Those fixes assume the reported webview screen rect is already correct. This
issue is about the source rect being converted from the wrong local coordinate
space before Chromium ever sees it.

There is also a secondary suspicious path in:

```text
wezboard/wezboard-gui/src/termsurf/conn.rs::create_pending_ca_layer_host()
```

The initial screen-rect resize for newly created overlays appears to be gated
behind `TERMSURF_ISSUE_779_TRACE`. That should not be trace-gated if Chromium's
hidden Shell window must always track the visible overlay. This is probably not
the main split-pane y bug, because later `set_overlay_frame()` calls do send the
screen rect unconditionally, but the experiment should audit it while in this
area.

## Proposed Solution

Fix the screen-rect conversion in Wezboard so it converts from the flipped layer
coordinate space into the AppKit view coordinate space before calling AppKit's
view/window conversion APIs.

The expected shape is:

```text
layer-local top-left y
  -> view-local bottom-left y
  -> window rect
  -> screen rect
  -> Chromium-style top-left screen y
```

The visible CALayerHost frame and the screen rect sent to Chromium must be
derived from the same geometry, but each must be expressed in the coordinate
space expected by its consumer.

Do not change Chromium's PagePopup y correction, direct `NSMenu` select path,
Shell mouse transparency, protocol structs, or browser input forwarding unless
instrumentation proves the Wezboard conversion is not the source of the bug.

## Experiments

### Experiment 1: Convert Flipped Overlay Frames Before Screen Rect Reporting

#### Description

Fix the likely coordinate-space bug in Wezboard's screen rect reporting for
browser overlays.

The CALayerHost visual placement can continue using the existing flipped layer
frame. Only the screen rect sent to Chromium should change: before using
`NSView convertRect:toView:`, convert the flipped layer frame into the hosting
view's coordinate space.

This experiment should also remove any trace-only gate that prevents the initial
overlay screen rect from reaching Chromium during normal operation.

#### Changes

1. **Audit the existing coordinate path.**

   Inspect:

   ```text
   wezboard/wezboard-gui/src/termwindow/render/paint.rs
   wezboard/wezboard-gui/src/termsurf/conn.rs
   roamium/src/dispatch.rs
   chromium/src/content/libtermsurf_chromium/ts_browser_main_parts.cc
   chromium/src/content/libtermsurf_chromium/ts_shell_window_mac.mm
   chromium/src/third_party/blink/renderer/core/exported/web_page_popup_impl.cc
   chromium/src/content/app_shim_remote_cocoa/web_menu_runner_mac.mm
   ```

   Confirm the intended data flow:

   ```text
   paint_pane() pane origin
     -> set_overlay_frame()/create_pending_ca_layer_host()
     -> webview_screen_rect_desc()
     -> Resize(screen_x, screen_y, screen_width, screen_height)
     -> Roamium ts_set_view_size()
     -> TsBrowserMainParts::ResizeTab()
     -> MoveShellWindowToTermSurfScreenRect()
   ```

2. **Fix `webview_screen_rect_desc()`.**

   In:

   ```text
   wezboard/wezboard-gui/src/termsurf/conn.rs
   ```

   Convert `local_frame` from flipped layer coordinates into the `NSView`
   coordinate space before calling `convertRect:toView:`.

   The conversion should use the hosting view's bounds height:

   ```text
   ns_view_y = view_bounds.height - local_frame.y - local_frame.height
   ```

   Then call:

   ```text
   view convertRect:ns_view_frame toView:nil
   window convertRectToScreen:window_rect
   ```

   Keep the final Chromium-style top-left screen y conversion:

   ```text
   top_left_screen_y =
     screen_frame.origin.y + screen_frame.height
       - screen_rect.origin.y
       - screen_rect.height
   ```

3. **Keep visual CALayer placement unchanged.**

   Do not change the frame assigned to the positioning CALayer in
   `set_overlay_frame()` or `create_pending_ca_layer_host()`. The visible
   overlay is already in the correct pane in the screenshots; only Chromium's
   reported native coordinate proxy is wrong.

4. **Remove trace-only initial screen-rect gating if present.**

   In `create_pending_ca_layer_host()`, sending the screen rect to Chromium must
   not depend on `TERMSURF_ISSUE_779_TRACE`.

   If the pane has a tab id and a screen rect can be computed, the resize with
   screen rect should be sent through the same helper used by
   `set_overlay_frame()`.

5. **Preserve prior native popup fixes.**

   Do not modify:
   - `WebPagePopupImpl::SetWindowRect` PagePopup y correction;
   - direct `NSMenu` select placement in `WebMenuRunner`;
   - Shell window mouse transparency;
   - `SetGuiActive`;
   - protocol message definitions;
   - Roamium mouse/key/scroll forwarding.

#### Verification

1. Build the debug components normally:

   ```bash
   cd /Users/ryan/dev/termsurf
   ./scripts/build.sh wezboard
   ./scripts/build.sh roamium
   ./scripts/build.sh webtui
   ```

   If Chromium was not changed, do not rebuild Chromium for this experiment.

2. Run debug Wezboard directly:

   ```bash
   cd /Users/ryan/dev/termsurf
   ./wezboard/target/debug/wezboard-gui
   ```

3. Inside Wezboard, launch the debug `web` binary with the repo-built Roamium:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/debug/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     http://localhost:9616/test-native-popups.html
   ```

4. Verify the original good case:
   - keep the webview as one large pane;
   - open the native `<select>`;
   - confirm the menu opens at the select field, matching the current acceptable
     behavior.

5. Verify the failing top-split case:
   - split a terminal pane below the webview;
   - keep the webview in the top pane;
   - open the native `<select>`;
   - confirm the menu opens at the select field in the top pane, not in the
     lower terminal pane.

6. Verify PagePopup-family controls:
   - in the same top-split layout, click date, time, date-time, and color;
   - confirm each popup anchors to its control;
   - confirm the Issue 779 PagePopup y correction still applies.

7. Verify other split positions:
   - webview in bottom pane;
   - webview in left pane;
   - webview in right pane;
   - nested split with webview not at window origin.

   Native popups should anchor to controls in every layout.

8. Verify ordinary overlay behavior:
   - browser content remains visually in the correct pane;
   - clicks still hit the correct web controls;
   - scrolling still affects the webview;
   - resizing splits updates the overlay and popup anchors.

#### Pass Criteria

- The top-split screenshot failure is fixed: native popups open in the top pane
  at the clicked control.
- Full-pane native popup placement remains acceptable.
- PagePopup y correction, select x alignment, Cmd-Tab dismissal, and post-select
  popup behavior from Issues 779, 782, and 783 do not regress.
- The visible CALayerHost overlay and Chromium's hidden Shell window use the
  same effective screen rect after split changes.

#### Partial Criteria

- The top-split `<select>` y is fixed, but another split orientation still has a
  smaller coordinate error. Record which orientation and whether the remaining
  error is x, y, scale, or size.
- PagePopup controls improve but select does not, or select improves but
  PagePopup does not. Record which popup family still disagrees and inspect
  whether it uses a different coordinate path.

#### Failure Criteria

- The native popup still opens in the lower pane when the webview is in the top
  pane.
- The visible webview moves or resizes incorrectly.
- The fix changes terminal pane layout, PTY dimensions, or mux split geometry.
- The fix modifies Chromium popup placement as a workaround without correcting
  Wezboard's reported screen rect.
- Any prior native popup fix from Issues 779, 782, or 783 regresses.

**Result:** Partial

The Wezboard-side screen-rect conversion fix partially succeeded.

What worked:

- the visible webview remained in the correct pane;
- the severe split-pane y error was fixed or substantially reduced;
- when the webview is in the top pane with a terminal pane below it, native
  popups no longer open far down in the lower terminal pane;
- the fix stayed in Wezboard's coordinate reporting path and did not require
  Chromium changes.

What did not work:

- PagePopup-family controls still appear vertically offset by approximately one
  control height;
- the popup is now near the right webview and pane, but it is not anchored at
  the same y used by the clicked input;
- this looks like the Issue 779 PagePopup height correction is missing,
  bypassed, or failing to match after the Wezboard screen-rect fix.

The implementation changed `webview_screen_rect_desc()` so the flipped CALayer
frame is converted into the hosting overlay `NSView` coordinate space before
calling AppKit's `convertRect:toView:`. It also removed the
`TERMSURF_ISSUE_779_TRACE` gate around the initial screen-rect send in
`create_pending_ca_layer_host()`.

#### Conclusion

Experiment 1 fixed the larger coordinate-space error: Chromium's hidden native
coordinate proxy now tracks the top split webview closely enough that native
popups appear in the correct pane instead of the lower terminal pane.

The remaining bug is narrower. The screenshots show a popup y offset that looks
approximately equal to the height of the input/control that opened the popup.
That matches the original Issue 779 PagePopup failure mode:

```text
popup_y = anchor.y + anchor.height
```

instead of:

```text
popup_y = anchor.y
```

The next experiment should focus on the Chromium PagePopup correction path. The
leading hypothesis is that `WebPagePopupImpl::SetWindowRect()` still receives a
rect anchored at `anchor.bottom()`, but the current exact-match predicate no
longer fires after the Wezboard screen-rect correction because of rounding,
scale, or small DIP differences. If the trace shows
`page_popup_y_fix applied=false` with a y delta near the input height, the fix
should add a small tolerance to the `anchor.bottom()` predicate. If the trace
does not appear at all, then the failing popup is bypassing
`WebPagePopupImpl::SetWindowRect()` and the next experiment should identify the
alternate popup placement path.
