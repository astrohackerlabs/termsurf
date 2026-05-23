+++
status = "closed"
opened = "2026-04-17"
closed = "2026-05-23"
+++

# Issue 780: Link drag freezes the browser

## Goal

When the user clicks a link and accidentally drags the mouse, the browser should
behave normally — either cancel the drag or complete it cleanly. Currently, any
drag initiated on a link (or other draggable element) puts the webview into a
stuck state that appears as a freeze.

## Background

Web pages use the HTML5 drag-and-drop API for links, images, and any element
with `draggable=true`. On a regular Chromium browser, clicking a link and moving
the mouse before release starts a drag: the cursor changes, a drag "ghost"
follows the mouse, and the drag ends on mouse-up (drop, or cancel if released
outside a drop target).

In TermSurf, the user reports that doing this freezes the browser — it stops
responding to subsequent input until some recovery action is taken. That
strongly suggests we start a drag session (Chromium enters drag-state
internally) but never deliver the events that would end it, so Chromium sits
forever waiting for a drop/cancel.

## Analysis

Our mouse input pipeline forwards `mousedown`, `mousemove`, and `mouseup` into
Chromium via the TermSurf protocol (see Issues 514, 515, 695). That pipeline is
sufficient for text selection and ordinary clicks, but HTML5 drag-and-drop on
macOS requires a separate interaction model:

1. Chromium detects a drag gesture and calls its platform delegate to **start a
   native drag session**. On macOS this normally goes through
   `NSView`/`NSDraggingSource` APIs on the `RenderWidgetHostView`.
2. While dragging, the OS drives the interaction: it sends `draggingEntered:` /
   `draggingUpdated:` / `draggingExited:` / `performDragOperation:` to potential
   targets.
3. On mouse-up, the OS delivers a drop or cancel to the source, which ends the
   drag session inside Chromium.

Because the webview is composited via CALayerHost and is not a normal `NSView`
in Wezboard's window (the underlying `RenderWidgetHostView` lives in the Roamium
process), the native macOS drag machinery cannot see the correct window or
hit-test. Possible failure modes:

- **Drag starts but never ends.** Chromium starts a drag session, but the
  mouse-up we forward as a protocol message doesn't end the drag because
  Chromium is waiting on the native drag loop, not on our synthetic `mouseup`.
  Until a cancel/drop arrives, input is effectively frozen.
- **The drag ghost window never appears** because the `NSDraggingSource` is in
  the Roamium process, not in Wezboard's window.
- **Synthetic mouse events during drag are ignored** because Chromium has
  switched to the drag-event state machine.

## Proposed Solutions

Options to investigate, roughly in order of effort:

1. **Suppress drag-start entirely.** The simplest fix: stop Chromium from
   initiating a drag for draggable elements in overlay mode. The page still sees
   `click` on release, which is what the user wanted anyway. This keeps us in a
   known-good state but loses drag-and-drop as a feature.

2. **Synthesize a drag cancel on mouse-up.** If Chromium has entered a drag
   state, deliver a cancel signal (ESC-equivalent / cancel drag) when we see
   `mouseup`, so the drag session always ends with the button release. This
   prevents the freeze without implementing full drag support.

3. **Implement drag-and-drop properly.** Bridge the macOS drag machinery across
   the Wezboard/Roamium process boundary: Roamium initiates the drag, Wezboard
   receives the `NSDraggingSource` events and forwards them over the protocol,
   and Roamium drives Chromium's drag state with them. This is the "real" fix
   but a substantial chunk of work.

Start with option 1 or 2 to unfreeze the browser; file a follow-up for option 3
if/when we want real drag-and-drop support.

## Reproduction

1. Build and run Wezboard + Roamium.
2. Load any page with links (e.g., a news site).
3. Press the mouse button on a link, move the cursor a few pixels, release.
4. Observe: the overlay stops responding to clicks/scrolls/keyboard until
   recovered (reload, tab switch, etc.).

## Experiments

### Experiment 1: Trace the Link-Drag Code Path

#### Description

Before choosing a fix, trace the current input and drag paths from Wezboard to
Chromium. The goal is to determine whether the freeze is plausibly caused by
TermSurf dropping a normal `mouseup`, or by Chromium entering a native macOS
drag session that TermSurf does not know how to complete.

This experiment is analysis-only. It does not change runtime behavior.

#### Changes

No code changes.

Code inspected:

- `wezboard/wezboard-gui/src/termsurf/input.rs`
- `proto/termsurf.proto`
- `roamium/src/dispatch.rs`
- `roamium/src/ffi.rs`
- `chromium/src/content/libtermsurf_chromium/libtermsurf_chromium.cc`
- `chromium/src/content/libtermsurf_chromium/ts_browser_main_parts.cc`
- `chromium/src/content/browser/renderer_host/render_widget_host_impl.cc`
- `chromium/src/content/browser/web_contents/web_contents_view_mac.mm`
- `chromium/src/content/browser/web_contents/web_contents_impl.cc`

Findings:

1. Wezboard forwards only ordinary mouse events into browser overlays:
   `MouseEvent` for down/up, `MouseMove` for movement, and scroll/key/focus
   messages. During a drag, Wezboard keeps a local `drag_pane` marker only so it
   can keep forwarding move/release events to the same overlay and clamp
   coordinates when the cursor leaves the overlay. That state is not an HTML or
   native drag session.
2. The TermSurf protobuf has no drag-and-drop protocol surface. There is no
   `DragStart`, `DragUpdate`, `Drop`, `DragCancel`, or native-drag completion
   message.
3. Roamium is a pass-through for this path. `Msg::MouseEvent` calls
   `ts_forward_mouse_event(...)`; `Msg::MouseMove` calls
   `ts_forward_mouse_move(...)`. There is no Roamium-side drag state and no FFI
   call for canceling or ending a Chromium drag source.
4. The Chromium TermSurf bridge converts protocol input into synthetic
   `blink::WebMouseEvent` values and sends them directly through
   `RenderWidgetHost::ForwardMouseEvent(...)`. That is sufficient for normal
   click, move, selection, and scroll behavior, but it is not a native AppKit
   drag/drop bridge.
5. Chromium's drag path is separate from that synthetic mouse stream. When Blink
   decides a drag gesture has started,
   `RenderWidgetHostImpl::StartDragging(...)` filters the renderer-provided drag
   data and delegates to the platform view. On macOS,
   `WebContentsViewMac::StartDragging(...)` calls
   `drag_dest_ initiateDragWithRenderWidgetHost:dropData:`, stores
   `drag_source_start_rwh_`, and then starts a native drag via either
   `remote_ns_view_->StartDrag(...)` or
   `in_process_ns_view_bridge_->StartDrag(...)`.
6. Chromium expects the native drag session to finish by calling back into
   `WebContentsViewMac::EndDrag(...)` / `PerformEndDrag(...)`, which then calls
   `SystemDragEnded(...)` and `DragSourceEndedAt(...)`. TermSurf has no protocol
   message or FFI path that can drive that completion sequence from Wezboard.

#### Verification

The analysis was verified by static code inspection and symbol search:

- `rg "MouseEvent|MouseMove" proto roamium/src wezboard/wezboard-gui/src/termsurf`
  shows the protocol and Rust forwarding path.
- `rg "ForwardMouseEvent|ForwardMouseMove" chromium/src/content/libtermsurf_chromium`
  shows the Chromium bridge converts those messages into `blink::WebMouseEvent`.
- `rg "StartDragging|SystemDragEnded|DragSourceEndedAt" chromium/src/content/browser`
  shows Chromium's native drag lifecycle and the macOS platform entry point.

**Result:** Pass

The code path supports the issue's theory. The most plausible freeze is not that
TermSurf forgot to forward a normal `mouseup`; Wezboard already forwards release
events while its local overlay drag marker is active. The more likely failure is
that Chromium starts a native macOS drag session from
`WebContentsViewMac::StartDragging(...)`, but the visible browser surface is a
CALayerHost overlay inside Wezboard, not the Chromium `NSView` running the
AppKit drag loop. Once Chromium enters that native drag state, TermSurf's
synthetic `MouseEvent(type="up")` is not the completion signal Chromium is
waiting for.

#### Conclusion

The first implementation experiment should suppress Chromium native drag start
in TermSurf overlay mode rather than trying to add a partial drag/drop protocol.
The narrow target is the Chromium macOS drag-start path, most likely
`WebContentsViewMac::StartDragging(...)`.

The suppression path should clear Chromium's drag state the same way Chromium
already does when a drag is disallowed by policy: call
`web_contents_->SystemDragEnded(source_rwh)` and return before
`initiateDragWithRenderWidgetHost` or `StartDrag(...)` runs. That should prevent
the stuck native-drag state while preserving ordinary clicks, mouse movement,
scrolling, and text selection. Full cross-process drag-and-drop should remain a
separate future feature.

### Experiment 2: Suppress Native Drag Start in Roamium

#### Description

Prevent Chromium from entering native macOS drag-and-drop while running as
TermSurf's Roamium overlay engine. Experiment 1 showed that the freeze most
likely starts when synthetic mouse input triggers Chromium's normal HTML drag
pipeline, which then calls into AppKit drag APIs that cannot complete correctly
across the Roamium/Wezboard process boundary.

This experiment chooses the narrow product behavior for now: Roamium does not
support native web drag-and-drop. Accidental drags on links, images, or other
draggable elements should be canceled cleanly instead of starting AppKit drag.
Ordinary clicks, text selection, scrolling, keyboard input, browser overlays,
and popup behavior must remain unchanged.

The implementation should suppress drag at the macOS Chromium drag-start point,
not in Wezboard. Wezboard cannot reliably know whether a mouse drag is over a
link, image, text selection, JavaScript draggable element, or ordinary page
content. Chromium already knows this because Blink has decided to call
`StartDragging(...)`.

#### Changes

1. Create a Chromium branch for this issue.

   First verify that `148.0.7778.97` is still the current TermSurf Chromium
   version by checking `chromium/README.md`. If a newer Chromium version has
   already become the current TermSurf fork version, branch from that version
   instead.
   - From `chromium/src`, fork the most relevant current TermSurf Chromium
     branch to `148.0.7778.97-issue-780`.
   - Add the branch to the Branches table in `chromium/README.md`.
   - Do not modify an older issue branch directly.

2. Add a TermSurf-only native-drag suppression path.

   Target file:
   - `chromium/src/content/browser/web_contents/web_contents_view_mac.mm`

   In `WebContentsViewMac::StartDragging(...)`, add an early TermSurf/Roamium
   suppression branch after `source_rwh` is available and before this code runs:
   - `[drag_dest_ initiateDragWithRenderWidgetHost:source_rwh dropData:drop_data]`
   - `drag_source_start_rwh_ = source_rwh->GetWeakPtr()`
   - `remote_ns_view_->StartDrag(...)`
   - `in_process_ns_view_bridge_->StartDrag(...)`

   The branch should:
   - call `web_contents_->SystemDragEnded(source_rwh)`;
   - return immediately;
   - not assign `drag_source_start_rwh_`;
   - not call `initiateDragWithRenderWidgetHost`;
   - not call AppKit/RemoteCocoa `StartDrag`.

   This mirrors Chromium's existing cleanup behavior when a drag is disallowed
   by policy or cannot be started.

   Confirm the exact `SystemDragEnded(...)` signature in Chromium 148 before
   editing. If the function expects `RenderWidgetHost*` rather than
   `RenderWidgetHostImpl*`, pass `source_rwh` through the compatible base type
   instead of adding unnecessary casts.

   Platform scope: this experiment fixes the macOS path only. Do not modify
   `web_contents_view_aura.cc` or other non-mac drag implementations. TermSurf
   currently ships Roamium on macOS; future Windows/Linux overlay support should
   get platform-equivalent drag suppression in a separate issue.

3. Scope the suppression to TermSurf/Roamium.

   Before choosing the scoping mechanism, search for an existing TermSurf
   marker:

   ```bash
   rg "termsurf|TermSurf|TERMSURF|IsTermSurf" \
     chromium/src/content/libtermsurf_chromium \
     chromium/src/content/shell
   ```

   Use an existing marker if one is already available and appropriate.

   Do not make an unguarded upstream-wide behavior change if a narrower
   TermSurf-only condition already exists or can be added simply. Acceptable
   scoping options, in order of preference:
   - a TermSurf-specific compile-time build flag if one already exists;
   - a TermSurf-specific runtime marker already present in the
     `libtermsurf_chromium` embedder path;
   - a minimal new TermSurf-only runtime flag set by `libtermsurf_chromium`
     during initialization and read by the drag-start code.

   If no clean TermSurf-only marker exists, document that finding before
   choosing a broader patch. The expected behavior for the product is still:
   native drag is suppressed in Roamium.

   If a new marker is required, prefer a small product-level marker set by
   `libtermsurf_chromium` during initialization. Do not use an environment
   variable for this behavior; native drag suppression is Roamium product
   behavior, not a diagnostic toggle.

4. Do not add new TermSurf protocol messages in this experiment.

   This is intentionally not a cross-process drag-and-drop implementation. Do
   not add drag/drop/cancel protobuf messages, Roamium FFI calls, or Wezboard
   drag-session state unless the suppression path proves impossible.

5. Preserve normal synthetic mouse input.

   Do not change:
   - `proto/termsurf.proto`;
   - `wezboard/wezboard-gui/src/termsurf/input.rs`;
   - `roamium/src/dispatch.rs`;
   - `roamium/src/ffi.rs`;
   - `TsBrowserMainParts::ForwardMouseEvent(...)`;
   - `TsBrowserMainParts::ForwardMouseMove(...)`.

   Text selection depends on the normal down/move/up stream and should not go
   through Chromium native `StartDragging(...)`.

6. Build and archive the Chromium patch.
   - Build `libtermsurf_chromium` with `autoninja`.
   - Regenerate the Issue 780 Chromium patch archive after committing the
     Chromium branch.
   - Commit the updated patch archive and docs in the main TermSurf repo.

   Document the patch location in the Chromium commit message so future Chromium
   upgrades can re-apply it deliberately:
   - file: `content/browser/web_contents/web_contents_view_mac.mm`;
   - function: `WebContentsViewMac::StartDragging(...)`;
   - guard: the TermSurf/Roamium marker chosen in step 3;
   - purpose: prevent stuck native drag state in Roamium overlay mode (Issue
     780).

#### Non-Negotiable Invariants

- A normal click on a link still navigates.
- Click-and-drag text selection still works.
- Scrolling, keyboard input, focus, and normal mouse movement still work.
- Browser overlay positioning and CALayerHost compositing are unchanged.
- Native popup fixes from Issues 779-784 are not touched.
- DevTools targeting from Issue 775 is not touched.
- Grid-native split border work from Issue 786 is not touched.
- No partial drag/drop protocol is introduced.
- Roamium must not enter a stuck drag state after an accidental link/image drag.

#### Verification

1. Build Chromium:

   ```bash
   cd chromium/src
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   autoninja -C out/Default libtermsurf_chromium
   ```

2. Build and run TermSurf components needed for manual testing.

3. Link click:
   - Open a page with normal links.
   - Click a link without dragging.
   - Expected: navigation/click behavior still works.

4. Accidental link drag:
   - Press on a link.
   - Move the mouse enough to normally start a link drag.
   - Release.
   - Expected: no native drag ghost appears, the browser does not freeze, and
     subsequent clicks/scrolls/keyboard input still work.
   - Immediately after release, click another element on the same page.
   - Expected: the first post-drag click is processed normally. It must not be
     eaten by leftover Blink drag state. Repeat this immediate next-input check
     with another link click, a button click, and a scroll.

5. Image drag:
   - Press on an image.
   - Move the mouse enough to normally start an image drag.
   - Release.
   - Expected: no freeze and no stuck drag state.

6. Draggable JavaScript element:
   - Test a page with an element using `draggable=true` or a simple HTML5 drag
     demo.
   - Attempt to drag it.
   - Expected: Roamium suppresses the native drag; browser input remains
     responsive afterward.

7. Text selection:
   - Drag across selectable text in a web page.
   - Expected: text selection still works. The suppression must not convert
     ordinary text selection into a canceled drag.

8. Basic browser input regression:
   - Scroll the page.
   - Type in a text field.
   - Click buttons.
   - Expected: all still work.

9. Popup regression smoke test:
   - Open a `<select>` popup and a datalist popup.
   - Expected: popup behavior from Issues 779-784 still works.
   - If time allows, also smoke-test a date picker and app switch/return while a
     native popup is involved, because those were the most recent native popup
     regression surfaces.

#### Pass Criteria

- Accidental drags on links/images/draggable elements no longer freeze Roamium.
- Chromium native drag start is suppressed before AppKit `StartDrag(...)` runs.
- Chromium drag state is cleared with `SystemDragEnded(source_rwh)`.
- Ordinary link clicks and text selection still work.
- No TermSurf protocol changes are required.
- The Chromium branch and patch archive are updated for Issue 780.

#### Partial Criteria

- The freeze is fixed, but one secondary browser behavior regresses, such as a
  specific drag demo failing in an unexpected way beyond simple suppression.
  Record the regression and decide whether it blocks closing or becomes a
  follow-up.
- The suppression works only with a broader-than-desired Chromium hook because
  no clean TermSurf-only marker exists. Record the scoping tradeoff and tighten
  it in a follow-up if needed.

#### Failure Criteria

- Link/image drag can still freeze the browser.
- A normal link click no longer works.
- Text selection no longer works.
- The patch starts native AppKit drag and tries to cancel it later instead of
  suppressing it before `StartDrag(...)`.
- The patch adds a partial cross-process drag/drop protocol.
- The patch touches unrelated popup, DevTools, split-border, or normal input
  behavior.

#### Expected Conclusion

If this passes, Issue 780 can treat native web drag-and-drop as intentionally
unsupported in Roamium for now. A future issue can design real cross-process
drag-and-drop if that becomes a product requirement.

**Result:** Pass

Manual debug testing passed after running `web` with an explicit `--browser`
path to the repo-built Roamium binary:

```bash
/Users/ryan/dev/termsurf/webtui/target/debug/web \
  --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
  https://example.com
```

The earlier failed test was invalid because it likely used the installed stable
Roamium. Running the repo-built `web` binary alone is not enough to select the
repo-built browser engine: if `--browser` is omitted, Wezboard may resolve
`roamium` to `/usr/local/roamium/roamium` or a Homebrew-installed Roamium. That
would bypass the modified Chromium branch entirely.

With the patched Roamium selected explicitly, dragging a link no longer hangs
the browser. The suppression in `WebContentsViewMac::StartDragging(...)` is
sufficient for the reported freeze.

#### Conclusion

Experiment 2 fixes the link-drag freeze by suppressing native macOS drag start
in Roamium overlay mode. Roamium intentionally does not support native web
drag-and-drop for now; accidental drags on links should be canceled cleanly
instead of entering AppKit drag state.

The testing workflow must always pass
`--browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium` when
validating Chromium/Roamium changes without installing. Otherwise tests can
accidentally exercise the installed stable Roamium and produce misleading
results.

## Conclusion

Issue 780 is closed by Experiment 2. Roamium now suppresses Chromium's native
macOS drag start while running in TermSurf overlay mode, which prevents
accidental link/image drags from freezing the browser.

This is intentionally a freeze fix, not full drag-and-drop support. Native HTML5
drag-and-drop and file drag/drop remain unsupported and are tracked separately
in `TODO.md`. Normal mouse-driven web interactions that do not use Chromium's
native drag pipeline should continue to work.
